use ropey::Rope;
use std::sync::Arc;
use tree_sitter::InputEdit;

use super::syntax::{merge_adjacent_highlight_spans, HighlightSpan};

#[derive(Debug, Clone)]
pub(crate) struct AsyncSyntaxHighlightCache {
    line_count: usize,
    byte_len: usize,
    lines: Vec<Option<Arc<Vec<HighlightSpan>>>>,
    dirty: Vec<bool>,
}

impl AsyncSyntaxHighlightCache {
    pub(crate) fn new_for_rope(rope: &Rope) -> Self {
        let total_lines = rope.len_lines().max(1);
        Self {
            line_count: total_lines,
            byte_len: rope.len_bytes(),
            lines: vec![None; total_lines],
            dirty: vec![true; total_lines],
        }
    }

    pub(crate) fn ensure_shape_for_rope(&mut self, rope: &Rope) {
        let total_lines = rope.len_lines().max(1);
        match self.lines.len().cmp(&total_lines) {
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Less => self.lines.resize_with(total_lines, || None),
            std::cmp::Ordering::Greater => self.lines.truncate(total_lines),
        }

        match self.dirty.len().cmp(&total_lines) {
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Less => self.dirty.resize_with(total_lines, || true),
            std::cmp::Ordering::Greater => self.dirty.truncate(total_lines),
        }

        self.line_count = total_lines;
        self.byte_len = rope.len_bytes();
    }

    pub(crate) fn reset_for_rope(&mut self, rope: &Rope) {
        *self = Self::new_for_rope(rope);
    }

    pub(crate) fn apply_edit_shape_shift(&mut self, rope: &Rope, edit: &InputEdit) {
        if self.lines.len() != self.dirty.len() || self.lines.is_empty() {
            self.reset_for_rope(rope);
            return;
        }

        let start_row = edit.start_position.row;
        let old_end_row = edit.old_end_position.row;
        let new_end_row = edit.new_end_position.row;

        let is_single_line_edit = old_end_row == start_row && new_end_row == start_row;

        let Some(old_len) = old_end_row
            .checked_sub(start_row)
            .and_then(|n| n.checked_add(1))
        else {
            self.reset_for_rope(rope);
            return;
        };
        let Some(new_len) = new_end_row
            .checked_sub(start_row)
            .and_then(|n| n.checked_add(1))
        else {
            self.reset_for_rope(rope);
            return;
        };

        if start_row > self.lines.len() || start_row.saturating_add(old_len) > self.lines.len() {
            self.reset_for_rope(rope);
            return;
        }

        if old_len == new_len {
            for line in start_row..start_row.saturating_add(old_len) {
                if let Some(d) = self.dirty.get_mut(line) {
                    *d = true;
                }
            }

            if is_single_line_edit && old_len == 1 {
                let local_start_byte = edit.start_position.column;
                let deleted_len = edit.old_end_byte.saturating_sub(edit.start_byte);
                let inserted_len = edit.new_end_byte.saturating_sub(edit.start_byte);
                self.apply_byte_edit_to_line_spans(
                    start_row,
                    local_start_byte,
                    deleted_len,
                    inserted_len,
                );
            } else {
                for line in start_row..start_row.saturating_add(old_len) {
                    if let Some(slot) = self.lines.get_mut(line) {
                        *slot = None;
                    }
                }
            }

            self.ensure_shape_for_rope(rope);
            return;
        }

        self.lines.splice(
            start_row..start_row.saturating_add(old_len),
            std::iter::repeat_with(|| None).take(new_len),
        );
        self.dirty.splice(
            start_row..start_row.saturating_add(old_len),
            std::iter::repeat_with(|| true).take(new_len),
        );

        self.ensure_shape_for_rope(rope);
    }

    fn apply_byte_edit_to_line_spans(
        &mut self,
        line: usize,
        local_start_byte: usize,
        deleted_len: usize,
        inserted_len: usize,
    ) {
        let Some(existing) = self.lines.get(line).and_then(|spans| spans.as_ref()) else {
            return;
        };
        if existing.is_empty() {
            return;
        }

        let mut next: Vec<HighlightSpan> = Vec::with_capacity(existing.len());
        if deleted_len == 0 {
            next.extend(
                existing
                    .iter()
                    .copied()
                    .filter_map(|span| shift_span_for_insert(span, local_start_byte, inserted_len)),
            );
        } else {
            let deleted_end = local_start_byte.saturating_add(deleted_len);
            let delta = inserted_len as isize - deleted_len as isize;
            next.extend(existing.iter().copied().filter_map(|span| {
                shift_span_for_delete_or_replace(span, local_start_byte, deleted_end, delta)
            }));
        }

        next.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        merge_adjacent_highlight_spans(&mut next);
        self.lines[line] = Some(Arc::new(next));
    }

    pub(crate) fn mark_dirty_from_changed_ranges(
        &mut self,
        rope: &Rope,
        changed: &[tree_sitter::Range],
    ) {
        if self.lines.len() != self.dirty.len() {
            self.reset_for_rope(rope);
            return;
        }

        let rope_byte_len = rope.len_bytes();
        for range in changed {
            let mut start_byte = range.start_byte.min(rope_byte_len);
            let mut end_byte = range.end_byte.min(rope_byte_len);
            if start_byte >= end_byte {
                continue;
            }

            if start_byte < rope_byte_len && rope.byte(start_byte) == b'\n' {
                start_byte = start_byte.saturating_add(1);
            }
            while end_byte > start_byte && rope.byte(end_byte.saturating_sub(1)) == b'\n' {
                end_byte = end_byte.saturating_sub(1);
            }
            if start_byte >= end_byte {
                continue;
            }

            let start_line = rope.byte_to_line(start_byte);
            let end_line = rope.byte_to_line(end_byte.saturating_sub(1));
            for line in start_line..=end_line {
                if let Some(d) = self.dirty.get_mut(line) {
                    *d = true;
                }
            }
        }
    }

    pub(crate) fn dirty_segments(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut segment_start: Option<usize> = None;
        for (idx, dirty) in self.dirty.iter().copied().enumerate() {
            if dirty {
                if segment_start.is_none() {
                    segment_start = Some(idx);
                }
                continue;
            }

            if let Some(start) = segment_start.take() {
                out.push((start, idx));
            }
        }
        if let Some(start) = segment_start.take() {
            out.push((start, self.dirty.len()));
        }
        out
    }

    pub(crate) fn dirty_segments_with_budget(
        &self,
        center_line: usize,
        max_total_lines: usize,
    ) -> Vec<(usize, usize)> {
        let max_total_lines = max_total_lines.max(1);
        let segments = self.dirty_segments();
        if segments.is_empty() {
            return Vec::new();
        }

        let center_line = center_line.min(self.dirty.len().saturating_sub(1));

        let mut best_idx = 0usize;
        let mut best_dist = usize::MAX;
        for (idx, &(start, end)) in segments.iter().enumerate() {
            let dist = if center_line < start {
                start.saturating_sub(center_line)
            } else if center_line >= end {
                center_line.saturating_sub(end.saturating_sub(1))
            } else {
                0
            };

            if dist < best_dist {
                best_idx = idx;
                best_dist = dist;
                if dist == 0 {
                    break;
                }
            }
        }

        let (start, end) = segments[best_idx];
        let seg_len = end.saturating_sub(start);
        if seg_len <= max_total_lines {
            return vec![(start, end)];
        }

        let chunk_len = max_total_lines;
        let mut chunk_start = if center_line <= start {
            start
        } else if center_line >= end {
            end.saturating_sub(chunk_len)
        } else {
            let half = chunk_len / 2;
            center_line.saturating_sub(half)
        };

        chunk_start = chunk_start.max(start);
        chunk_start = chunk_start.min(end.saturating_sub(chunk_len));

        vec![(chunk_start, chunk_start.saturating_add(chunk_len).min(end))]
    }

    pub(crate) fn line(&self, line: usize) -> Option<&Arc<Vec<HighlightSpan>>> {
        self.lines.get(line).and_then(|v| v.as_ref())
    }

    pub(crate) fn is_line_dirty(&self, line: usize) -> bool {
        self.dirty.get(line).copied().unwrap_or(true)
    }

    pub(crate) fn apply_patch(&mut self, start_line: usize, lines: Vec<Vec<HighlightSpan>>) {
        if self.lines.len() != self.dirty.len() {
            self.lines.resize_with(self.dirty.len(), || None);
        }

        for (idx, spans) in lines.into_iter().enumerate() {
            let line = start_line.saturating_add(idx);
            if line >= self.lines.len() || line >= self.dirty.len() {
                break;
            }
            self.lines[line] = Some(Arc::new(spans));
            self.dirty[line] = false;
        }
    }
}

fn shift_span_for_insert(
    mut span: HighlightSpan,
    local_start_byte: usize,
    inserted_len: usize,
) -> Option<HighlightSpan> {
    if span.start >= local_start_byte {
        span.start = span.start.saturating_add(inserted_len);
    }
    if span.end > local_start_byte {
        span.end = span.end.saturating_add(inserted_len);
    }
    (span.end > span.start).then_some(span)
}

fn shift_span_for_delete_or_replace(
    mut span: HighlightSpan,
    local_start_byte: usize,
    deleted_end: usize,
    delta: isize,
) -> Option<HighlightSpan> {
    if span.start >= deleted_end {
        span.start = shift_with_delta(span.start, delta)?;
    } else if span.start >= local_start_byte {
        span.start = local_start_byte;
    }

    if span.end >= deleted_end {
        span.end = shift_with_delta(span.end, delta)?;
    } else if span.end > local_start_byte {
        span.end = local_start_byte;
    }

    (span.end > span.start).then_some(span)
}

fn shift_with_delta(value: usize, delta: isize) -> Option<usize> {
    if delta >= 0 {
        value.checked_add(delta as usize)
    } else {
        value.checked_sub(delta.unsigned_abs())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/syntax_highlight_cache.rs"]
mod tests;
