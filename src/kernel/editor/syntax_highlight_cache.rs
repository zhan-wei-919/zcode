use ropey::Rope;
use std::sync::Arc;
use tree_sitter::InputEdit;

use super::syntax::HighlightSpan;

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

    pub(crate) fn line(&self, line: usize) -> Option<&Arc<Vec<HighlightSpan>>> {
        self.lines.get(line).and_then(|v| v.as_ref())
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

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/syntax_highlight_cache.rs"]
mod tests;
