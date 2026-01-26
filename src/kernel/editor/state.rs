use crate::kernel::services::ports::{EditorConfig, LspFoldingRange, Match};
use crate::models::{EditHistory, EditOp, Granularity, OpKind, TextBuffer};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::PathBuf;
use std::time::Instant;

use super::syntax::SyntaxDocument;
use super::{viewport, HighlightSpan, LanguageId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBarMode {
    Search,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBarField {
    Search,
    Replace,
}

#[derive(Debug)]
pub struct SearchBarState {
    pub visible: bool,
    pub mode: SearchBarMode,
    pub focused_field: SearchBarField,
    pub search_text: String,
    pub replace_text: String,
    pub cursor_pos: usize,
    pub case_sensitive: bool,
    pub use_regex: bool,
    pub matches: Vec<Match>,
    pub current_match_index: Option<usize>,
    pub searching: bool,
    pub active_search_id: Option<u64>,
    pub last_error: Option<String>,
}

impl Default for SearchBarState {
    fn default() -> Self {
        Self {
            visible: false,
            mode: SearchBarMode::Search,
            focused_field: SearchBarField::Search,
            search_text: String::new(),
            replace_text: String::new(),
            cursor_pos: 0,
            case_sensitive: false,
            use_regex: false,
            matches: Vec::new(),
            current_match_index: None,
            searching: false,
            active_search_id: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EditorViewportState {
    pub line_offset: usize,
    pub height: usize,
    pub horiz_offset: u32,
    pub width: usize,
    pub follow_cursor: bool,
}

impl Default for EditorViewportState {
    fn default() -> Self {
        Self {
            line_offset: 0,
            height: 20,
            horiz_offset: 0,
            width: 80,
            follow_cursor: true,
        }
    }
}

#[derive(Debug)]
pub struct EditorMouseState {
    pub last_click: Option<(u16, u16, Instant)>,
    pub click_count: u8,
    pub dragging: bool,
    pub granularity: Granularity,
}

impl EditorMouseState {
    pub fn new() -> Self {
        Self {
            last_click: None,
            click_count: 0,
            dragging: false,
            granularity: Granularity::Char,
        }
    }
}

impl Default for EditorMouseState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EditorTabState {
    pub title: String,
    pub path: Option<PathBuf>,
    pub buffer: TextBuffer,
    pub viewport: EditorViewportState,
    pub history: EditHistory,
    pub dirty: bool,
    pub edit_version: u64,
    pub last_edit_op: Option<EditOp>,
    pub mouse: EditorMouseState,
    semantic_highlight: Option<SemanticHighlightState>,
    inlay_hints: Option<InlayHintsState>,
    folding: Option<FoldingState>,
    syntax: Option<SyntaxDocument>,
}

impl std::fmt::Debug for EditorTabState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorTabState")
            .field("title", &self.title)
            .field("path", &self.path)
            .field("dirty", &self.dirty)
            .field("cursor", &self.buffer.cursor())
            .field("lines", &self.buffer.len_lines())
            .finish()
    }
}

impl EditorTabState {
    pub fn untitled(config: &EditorConfig) -> Self {
        let buffer = TextBuffer::new();
        let history = EditHistory::new(buffer.rope().clone());
        Self {
            title: "Untitled".to_string(),
            path: None,
            buffer,
            viewport: EditorViewportState {
                height: config.default_viewport_height,
                ..EditorViewportState::default()
            },
            history,
            dirty: false,
            edit_version: 0,
            last_edit_op: None,
            mouse: EditorMouseState::new(),
            semantic_highlight: None,
            inlay_hints: None,
            folding: None,
            syntax: None,
        }
    }

    pub fn from_file(path: PathBuf, content: &str, config: &EditorConfig) -> Self {
        let title = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let buffer = TextBuffer::from_text(content);
        let history = EditHistory::new(buffer.rope().clone());
        let syntax = SyntaxDocument::for_path(&path, buffer.rope());

        Self {
            title,
            path: Some(path),
            buffer,
            viewport: EditorViewportState {
                height: config.default_viewport_height,
                ..EditorViewportState::default()
            },
            history,
            dirty: false,
            edit_version: 0,
            last_edit_op: None,
            mouse: EditorMouseState::new(),
            semantic_highlight: None,
            inlay_hints: None,
            folding: None,
            syntax,
        }
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.title = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        self.path = Some(path.clone());
        self.syntax = SyntaxDocument::for_path(&path, self.buffer.rope());
        self.semantic_highlight = None;
        self.inlay_hints = None;
        self.clear_folding();
    }

    pub fn display_title(&self) -> String {
        if self.dirty {
            format!("● {}", self.title)
        } else {
            self.title.clone()
        }
    }

    pub fn language(&self) -> Option<LanguageId> {
        self.syntax.as_ref().map(|s| s.language())
    }

    pub fn highlight_lines(
        &self,
        start_line: usize,
        end_line_exclusive: usize,
    ) -> Option<Vec<Vec<HighlightSpan>>> {
        let syntax = self.syntax.as_ref()?;
        Some(syntax.highlight_lines(self.buffer.rope(), start_line, end_line_exclusive))
    }

    pub fn semantic_highlight_lines(
        &self,
        start_line: usize,
        end_line_exclusive: usize,
    ) -> Option<&[Vec<HighlightSpan>]> {
        let semantic = self.semantic_highlight.as_ref()?;
        let start = start_line.min(semantic.lines.len());
        let end = end_line_exclusive.min(semantic.lines.len());
        if start >= end {
            return None;
        }
        Some(&semantic.lines[start..end])
    }

    pub fn set_semantic_highlight(
        &mut self,
        version: u64,
        lines: Vec<Vec<HighlightSpan>>,
    ) -> bool {
        let same = self
            .semantic_highlight
            .as_ref()
            .is_some_and(|s| s.version == version && s.lines == lines);
        if same {
            return false;
        }

        self.semantic_highlight = Some(SemanticHighlightState { version, lines });
        true
    }

    pub(super) fn invalidate_semantic_highlight_on_edit(&mut self, op: &EditOp) {
        let Some(semantic) = self.semantic_highlight.as_mut() else {
            return;
        };

        fn is_word_byte(b: u8) -> bool {
            matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
        }

        fn shift_byte_offset(value: usize, delta: isize) -> Option<usize> {
            if delta >= 0 {
                value.checked_add(delta as usize)
            } else {
                value.checked_sub(delta.wrapping_abs() as usize)
            }
        }

        let (start_char, inserted_text, deleted_text, invalidates_following_lines) = match &op.kind {
            OpKind::Insert { char_offset, text } => {
                (*char_offset, text.as_str(), "", text.contains('\n'))
            }
            OpKind::Delete { start, deleted, .. } => (*start, "", deleted.as_str(), deleted.contains('\n')),
            OpKind::Replace {
                start,
                deleted,
                inserted,
                ..
            } => (
                *start,
                inserted.as_str(),
                deleted.as_str(),
                deleted.contains('\n') || inserted.contains('\n'),
            ),
        };

        if semantic.lines.is_empty() {
            return;
        }

        let rope = self.buffer.rope();
        let start_char = start_char.min(rope.len_chars());
        let start_byte = rope.char_to_byte(start_char);
        let start_line = rope.byte_to_line(start_byte);

        if invalidates_following_lines {
            let start = start_line.min(semantic.lines.len());
            for line in semantic.lines.iter_mut().skip(start) {
                line.clear();
            }
            return;
        }

        let Some(spans) = semantic.lines.get_mut(start_line) else {
            return;
        };

        let line_start_byte = rope.line_to_byte(start_line);
        let local_start_byte = start_byte.saturating_sub(line_start_byte);
        let inserted_len = inserted_text.len();
        let deleted_len = deleted_text.len();
        let delta = inserted_len as isize - deleted_len as isize;

        let mut line = rope.line(start_line).to_string();
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        } else if line.ends_with('\r') {
            line.pop();
        }

        let line_bytes = line.as_bytes();
        let line_len = line_bytes.len();
        let local_start_byte = local_start_byte.min(line_len);
        let right_index = local_start_byte
            .saturating_add(inserted_len)
            .min(line_len);

        let inserted_has_word = inserted_text.bytes().any(is_word_byte);
        let deleted_has_word = deleted_text.bytes().any(is_word_byte);

        let find_word_bounds = |anchor: usize| -> (usize, usize) {
            let mut start = anchor;
            while start > 0 && is_word_byte(line_bytes[start - 1]) {
                start = start.saturating_sub(1);
            }
            let mut end = anchor.saturating_add(1);
            while end < line_len && is_word_byte(line_bytes[end]) {
                end = end.saturating_add(1);
            }
            (start, end)
        };

        let invalidate_range: Option<(usize, usize)> = if inserted_has_word {
            let mut anchor = None;
            if local_start_byte < right_index {
                anchor = (local_start_byte..right_index)
                    .find(|&idx| is_word_byte(line_bytes[idx]));
            }

            let anchor = anchor.or_else(|| {
                if local_start_byte < line_len && is_word_byte(line_bytes[local_start_byte]) {
                    Some(local_start_byte)
                } else if local_start_byte > 0 && is_word_byte(line_bytes[local_start_byte - 1]) {
                    Some(local_start_byte - 1)
                } else {
                    None
                }
            });

            anchor.map(find_word_bounds)
        } else if deleted_has_word {
            let anchor = if local_start_byte < line_len && is_word_byte(line_bytes[local_start_byte]) {
                Some(local_start_byte)
            } else if local_start_byte > 0 && is_word_byte(line_bytes[local_start_byte - 1]) {
                Some(local_start_byte - 1)
            } else {
                None
            };

            anchor.map(find_word_bounds)
        } else {
            let left_is_word =
                local_start_byte > 0 && is_word_byte(line_bytes[local_start_byte - 1]);
            let right_is_word = right_index < line_len && is_word_byte(line_bytes[right_index]);
            if !left_is_word || !right_is_word {
                None
            } else {
                let mut start = local_start_byte.saturating_sub(1);
                while start > 0 && is_word_byte(line_bytes[start - 1]) {
                    start = start.saturating_sub(1);
                }
                let mut end = right_index;
                while end < line_len && is_word_byte(line_bytes[end]) {
                    end = end.saturating_add(1);
                }
                Some((start, end))
            }
        };

        let old_edit_end = local_start_byte.saturating_add(deleted_len);

        let mut next: Vec<HighlightSpan> = Vec::with_capacity(spans.len());
        for span in spans.iter().copied() {
            if span.end > local_start_byte && span.start < old_edit_end {
                continue;
            }

            let mut start = span.start;
            let mut end = span.end;

            if span.start >= old_edit_end {
                let Some(new_start) = shift_byte_offset(start, delta) else {
                    continue;
                };
                let Some(new_end) = shift_byte_offset(end, delta) else {
                    continue;
                };
                start = new_start;
                end = new_end;
            }

            if end <= start {
                continue;
            }

            if let Some((invalidate_start, invalidate_end)) = invalidate_range {
                if end > invalidate_start && start < invalidate_end {
                    continue;
                }
            }

            next.push(HighlightSpan {
                start,
                end,
                kind: span.kind,
            });
        }

        *spans = next;
    }

    pub(crate) fn semantic_highlight_line(&self, line: usize) -> Option<&[HighlightSpan]> {
        let semantic = self.semantic_highlight.as_ref()?;
        semantic.lines.get(line).map(|spans| spans.as_slice())
    }

    pub fn inlay_hint_lines(&self, start_line: usize, end_line_exclusive: usize) -> Option<&[Vec<String>]> {
        let hints = self.inlay_hints.as_ref()?;
        if start_line < hints.start_line || end_line_exclusive > hints.end_line_exclusive {
            return None;
        }
        let start = start_line.saturating_sub(hints.start_line);
        let end = end_line_exclusive.saturating_sub(hints.start_line).min(hints.lines.len());
        if start >= end {
            return None;
        }
        Some(&hints.lines[start..end])
    }

    pub fn set_inlay_hints(
        &mut self,
        version: u64,
        start_line: usize,
        end_line_exclusive: usize,
        lines: Vec<Vec<String>>,
    ) -> bool {
        let same = self.inlay_hints.as_ref().is_some_and(|s| {
            s.version == version
                && s.start_line == start_line
                && s.end_line_exclusive == end_line_exclusive
                && s.lines == lines
        });
        if same {
            return false;
        }

        self.inlay_hints = Some(InlayHintsState {
            version,
            start_line,
            end_line_exclusive,
            lines,
        });
        true
    }

    pub(crate) fn inlay_hint_line(&self, line: usize) -> Option<&[String]> {
        let hints = self.inlay_hints.as_ref()?;
        if line < hints.start_line || line >= hints.end_line_exclusive {
            return None;
        }
        hints
            .lines
            .get(line.saturating_sub(hints.start_line))
            .map(|row| row.as_slice())
    }

    pub(crate) fn fold_marker_char(&self, line: u32) -> Option<char> {
        self.folding
            .as_ref()
            .and_then(|s| s.marker_char(line))
    }

    pub(crate) fn folding_version(&self) -> Option<u64> {
        self.folding.as_ref().map(|s| s.version)
    }

    pub(crate) fn has_folding_ranges(&self) -> bool {
        self.folding.as_ref().is_some_and(|s| !s.ranges.is_empty())
    }

    pub(crate) fn visible_lines_in_viewport(&self, start_line: usize, height: usize) -> Vec<usize> {
        let total_lines = self.buffer.len_lines().max(1);
        if height == 0 {
            return Vec::new();
        }

        let start_line = start_line.min(total_lines.saturating_sub(1));

        let Some(folding) = self.folding.as_ref().filter(|s| !s.hidden_ranges.is_empty()) else {
            let end = (start_line + height).min(total_lines);
            return (start_line..end).collect();
        };

        let total = total_lines.min(u32::MAX as usize) as u32;

        let mut out = Vec::with_capacity(height.min(256));
        let line = start_line.min(u32::MAX as usize) as u32;
        let Some(mut line) = folding.next_visible_line_at_or_after(line, total) else {
            return out;
        };

        while out.len() < height && (line as usize) < total_lines {
            out.push(line as usize);
            let next = line.saturating_add(1);
            let Some(next) = folding.next_visible_line_at_or_after(next, total) else {
                break;
            };
            line = next;
        }

        out
    }

    pub(crate) fn next_visible_row_after(&self, row: usize) -> Option<usize> {
        let total_lines = self.buffer.len_lines().max(1);
        if row + 1 >= total_lines {
            return None;
        }

        let Some(folding) = self.folding.as_ref() else {
            return Some(row + 1);
        };

        let total = total_lines.min(u32::MAX as usize) as u32;
        folding
            .next_visible_line_at_or_after((row + 1).min(u32::MAX as usize) as u32, total)
            .map(|line| line as usize)
    }

    pub(crate) fn next_visible_row_at_or_after(&self, row: usize) -> Option<usize> {
        let total_lines = self.buffer.len_lines().max(1);
        if row >= total_lines {
            return None;
        }

        let Some(folding) = self.folding.as_ref() else {
            return Some(row);
        };

        let total = total_lines.min(u32::MAX as usize) as u32;
        folding
            .next_visible_line_at_or_after(row.min(u32::MAX as usize) as u32, total)
            .map(|line| line as usize)
    }

    pub(crate) fn prev_visible_row_before(&self, row: usize) -> Option<usize> {
        if row == 0 {
            return None;
        }

        let Some(folding) = self.folding.as_ref() else {
            return Some(row - 1);
        };

        folding
            .prev_visible_line_at_or_before((row - 1).min(u32::MAX as usize) as u32)
            .map(|line| line as usize)
    }

    pub(crate) fn prev_visible_row_at_or_before(&self, row: usize) -> Option<usize> {
        let total_lines = self.buffer.len_lines().max(1);
        if row >= total_lines {
            return None;
        }

        let Some(folding) = self.folding.as_ref() else {
            return Some(row);
        };

        folding
            .prev_visible_line_at_or_before(row.min(u32::MAX as usize) as u32)
            .map(|line| line as usize)
    }

    pub(crate) fn fold_toggle_at_cursor(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let cursor_line = row.min(u32::MAX as usize) as u32;
        let Some(state) = self.folding.as_mut() else {
            return false;
        };
        let Some(start_line) = state.target_start_line_for(cursor_line) else {
            return false;
        };

        let end_line = state.fold_end(start_line);
        let toggled = state.toggle(start_line);
        if !toggled {
            return false;
        }

        if state.is_folded(start_line)
            && end_line.is_some_and(|end| cursor_line > start_line && cursor_line <= end)
        {
            let target_row = start_line.min(u32::MAX) as usize;
            let len = self.buffer.line_grapheme_len(target_row);
            self.buffer.set_cursor(target_row, col.min(len));
            self.buffer.update_selection_cursor(self.buffer.cursor());
        }

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub(crate) fn fold_close_at_cursor(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let cursor_line = row.min(u32::MAX as usize) as u32;
        let Some(state) = self.folding.as_mut() else {
            return false;
        };
        let Some(start_line) = state.target_start_line_for(cursor_line) else {
            return false;
        };

        let end_line = state.fold_end(start_line);
        let changed = state.collapse(start_line);
        if !changed {
            return false;
        }

        if end_line.is_some_and(|end| cursor_line > start_line && cursor_line <= end) {
            let target_row = start_line.min(u32::MAX) as usize;
            let len = self.buffer.line_grapheme_len(target_row);
            self.buffer.set_cursor(target_row, col.min(len));
            self.buffer.update_selection_cursor(self.buffer.cursor());
        }

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub(crate) fn fold_open_at_cursor(&mut self, tab_size: u8) -> bool {
        let (row, _col) = self.buffer.cursor();
        let cursor_line = row.min(u32::MAX as usize) as u32;
        let Some(state) = self.folding.as_mut() else {
            return false;
        };
        let Some(start_line) = state.target_start_line_for(cursor_line) else {
            return false;
        };

        let changed = state.expand(start_line);
        if !changed {
            return false;
        }

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    pub fn set_folding_ranges(
        &mut self,
        version: u64,
        ranges: Vec<LspFoldingRange>,
    ) -> bool {
        let mut collapsed = self
            .folding
            .as_ref()
            .map(|s| s.collapsed.clone())
            .unwrap_or_default();

        let fold_starts = FoldingState::build_fold_starts(&ranges);
        collapsed.retain(|start| fold_starts.contains_key(start));
        let hidden_ranges = FoldingState::build_hidden_ranges(&collapsed, &fold_starts);

        let next = FoldingState {
            version,
            ranges,
            fold_starts,
            collapsed,
            hidden_ranges,
        };

        let same = self.folding.as_ref().is_some_and(|s| s == &next);
        if same {
            return false;
        }

        self.folding = Some(next);
        true
    }

    pub(super) fn syntax(&self) -> Option<&SyntaxDocument> {
        self.syntax.as_ref()
    }

    pub(super) fn clear_folding(&mut self) {
        self.folding = None;
    }

    pub(super) fn apply_syntax_edit(&mut self, op: &EditOp) {
        let Some(syntax) = self.syntax.as_mut() else {
            return;
        };
        syntax.apply_edit(self.buffer.rope(), op);
    }

    pub(super) fn bump_version(&mut self) {
        self.edit_version = self.edit_version.saturating_add(1);
    }

    pub(super) fn reparse_syntax(&mut self) {
        let Some(syntax) = self.syntax.as_mut() else {
            return;
        };
        syntax.reparse(self.buffer.rope());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticHighlightState {
    pub version: u64,
    pub lines: Vec<Vec<HighlightSpan>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlayHintsState {
    pub version: u64,
    pub start_line: usize,
    pub end_line_exclusive: usize,
    pub lines: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldingState {
    pub version: u64,
    pub ranges: Vec<LspFoldingRange>,
    fold_starts: FxHashMap<u32, u32>,
    pub collapsed: FxHashSet<u32>,
    hidden_ranges: Vec<(u32, u32)>,
}

impl FoldingState {
    fn build_fold_starts(ranges: &[LspFoldingRange]) -> FxHashMap<u32, u32> {
        let mut out: FxHashMap<u32, u32> = FxHashMap::default();
        out.reserve(ranges.len().min(1024));

        for range in ranges {
            out.entry(range.start_line)
                .and_modify(|end| *end = (*end).max(range.end_line))
                .or_insert(range.end_line);
        }

        out
    }

    fn build_hidden_ranges(
        collapsed: &FxHashSet<u32>,
        fold_starts: &FxHashMap<u32, u32>,
    ) -> Vec<(u32, u32)> {
        let mut ranges = Vec::with_capacity(collapsed.len().min(512));

        for start in collapsed {
            let Some(&end) = fold_starts.get(start) else {
                continue;
            };
            let hidden_start = start.saturating_add(1);
            if end < hidden_start {
                continue;
            }
            ranges.push((hidden_start, end));
        }

        ranges.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
        for (start, end) in ranges {
            let Some((_, last_end)) = merged.last_mut() else {
                merged.push((start, end));
                continue;
            };

            if start <= last_end.saturating_add(1) {
                *last_end = (*last_end).max(end);
            } else {
                merged.push((start, end));
            }
        }

        merged
    }

    fn marker_char(&self, line: u32) -> Option<char> {
        if !self.fold_starts.contains_key(&line) {
            return None;
        }
        if self.collapsed.contains(&line) {
            Some('▸')
        } else {
            Some('▾')
        }
    }

    fn is_folded(&self, start_line: u32) -> bool {
        self.collapsed.contains(&start_line)
    }

    fn fold_end(&self, start_line: u32) -> Option<u32> {
        self.fold_starts.get(&start_line).copied()
    }

    fn target_start_line_for(&self, cursor_line: u32) -> Option<u32> {
        if self.fold_starts.contains_key(&cursor_line) {
            return Some(cursor_line);
        }

        let mut best: Option<u32> = None;
        for range in &self.ranges {
            if range.start_line < cursor_line && range.end_line >= cursor_line {
                if best.is_none_or(|b| range.start_line > b) {
                    best = Some(range.start_line);
                }
            }
        }
        best
    }

    fn toggle(&mut self, start_line: u32) -> bool {
        if !self.fold_starts.contains_key(&start_line) {
            return false;
        }
        if !self.collapsed.insert(start_line) {
            self.collapsed.remove(&start_line);
        }
        self.hidden_ranges = Self::build_hidden_ranges(&self.collapsed, &self.fold_starts);
        true
    }

    fn collapse(&mut self, start_line: u32) -> bool {
        if !self.fold_starts.contains_key(&start_line) {
            return false;
        }
        let changed = self.collapsed.insert(start_line);
        if changed {
            self.hidden_ranges = Self::build_hidden_ranges(&self.collapsed, &self.fold_starts);
        }
        changed
    }

    fn expand(&mut self, start_line: u32) -> bool {
        let changed = self.collapsed.remove(&start_line);
        if changed {
            self.hidden_ranges = Self::build_hidden_ranges(&self.collapsed, &self.fold_starts);
        }
        changed
    }

    fn hidden_range_containing(&self, line: u32) -> Option<(u32, u32)> {
        if self.hidden_ranges.is_empty() {
            return None;
        }

        let ranges = self.hidden_ranges.as_slice();
        let idx = ranges.partition_point(|(_, end)| *end < line);
        let (start, end) = *ranges.get(idx)?;
        if line < start {
            return None;
        }
        Some((start, end))
    }

    fn next_visible_line_at_or_after(&self, mut line: u32, total_lines: u32) -> Option<u32> {
        while line < total_lines {
            let Some((_start, end)) = self.hidden_range_containing(line) else {
                return Some(line);
            };
            line = end.saturating_add(1);
        }
        None
    }

    fn prev_visible_line_at_or_before(&self, mut line: u32) -> Option<u32> {
        loop {
            let Some((start, _end)) = self.hidden_range_containing(line) else {
                return Some(line);
            };
            if start == 0 {
                return None;
            }
            line = start.saturating_sub(1);
        }
    }
}

#[derive(Debug)]
pub struct EditorPaneState {
    pub tabs: Vec<EditorTabState>,
    pub active: usize,
    pub search_bar: SearchBarState,
}

impl EditorPaneState {
    pub fn new(_config: &EditorConfig) -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            search_bar: SearchBarState::default(),
        }
    }

    pub fn active_tab(&self) -> Option<&EditorTabState> {
        self.tabs.get(self.active)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut EditorTabState> {
        self.tabs.get_mut(self.active)
    }

    pub fn set_active(&mut self, index: usize) -> bool {
        let index = index.min(self.tabs.len().saturating_sub(1));
        if index == self.active {
            return false;
        }
        self.active = index;
        true
    }

    pub fn open_file(&mut self, path: PathBuf, content: &str, config: &EditorConfig) -> bool {
        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.path.as_ref() == Some(&path) {
                return self.set_active(i);
            }
        }

        self.tabs
            .push(EditorTabState::from_file(path, content, config));
        self.active = self.tabs.len().saturating_sub(1);
        true
    }

    pub fn close_active_tab(&mut self) -> bool {
        if self.tabs.is_empty() {
            return false;
        }

        let index = self.active.min(self.tabs.len().saturating_sub(1));
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
        true
    }

    pub fn close_tab_at(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        } else if self.active > index {
            self.active = self.active.saturating_sub(1);
        }
        true
    }

    pub fn is_tab_dirty(&self, index: usize) -> bool {
        self.tabs.get(index).is_some_and(|t| t.dirty)
    }

    pub fn next_tab(&mut self) -> bool {
        let len = self.tabs.len();
        if len <= 1 {
            return false;
        }
        let prev = self.active;
        self.active = (self.active + 1) % len;
        self.active != prev
    }

    pub fn prev_tab(&mut self) -> bool {
        let len = self.tabs.len();
        if len <= 1 {
            return false;
        }
        let prev = self.active;
        self.active = if self.active == 0 {
            len - 1
        } else {
            self.active - 1
        };
        self.active != prev
    }

    pub fn set_viewport_size(&mut self, width: usize, height: usize) -> bool {
        let width = width.max(1);
        let height = height.max(1);

        let mut changed = false;
        for tab in &mut self.tabs {
            if tab.viewport.width != width {
                tab.viewport.width = width;
                changed = true;
            }
            if tab.viewport.height != height {
                tab.viewport.height = height;
                changed = true;
            }
        }
        changed
    }
}

#[derive(Debug)]
pub struct EditorState {
    pub config: EditorConfig,
    pub panes: Vec<EditorPaneState>,
    pub open_paths_version: u64,
}

impl EditorState {
    pub fn new(config: EditorConfig) -> Self {
        Self {
            config: config.clone(),
            panes: vec![EditorPaneState::new(&config)],
            open_paths_version: 0,
        }
    }

    pub fn pane_mut(&mut self, pane: usize) -> Option<&mut EditorPaneState> {
        self.panes.get_mut(pane)
    }

    pub fn pane(&self, pane: usize) -> Option<&EditorPaneState> {
        self.panes.get(pane)
    }

    pub fn ensure_panes(&mut self, desired: usize) -> bool {
        let desired = desired.max(1);
        let current = self.panes.len();
        match desired.cmp(&current) {
            std::cmp::Ordering::Equal => false,
            std::cmp::Ordering::Less => {
                let dropped_has_paths = self
                    .panes
                    .iter()
                    .skip(desired)
                    .any(|pane| pane.tabs.iter().any(|tab| tab.path.is_some()));
                self.panes.truncate(desired);
                if dropped_has_paths {
                    self.open_paths_version = self.open_paths_version.saturating_add(1);
                }
                true
            }
            std::cmp::Ordering::Greater => {
                self.panes.reserve(desired - current);
                for _ in current..desired {
                    self.panes.push(EditorPaneState::new(&self.config));
                }
                true
            }
        }
    }
}
