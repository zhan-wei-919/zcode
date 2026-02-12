use crate::kernel::git::GitGutterMarks;
use crate::kernel::services::ports::{EditorConfig, LspFoldingRange, Match};
use crate::models::{EditHistory, EditOp, Granularity, OpKind, TextBuffer};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::PathBuf;
use std::time::{Instant, SystemTime};
use unicode_xid::UnicodeXID;

use super::markdown::MarkdownDocument;
use super::syntax::SyntaxDocument;
use super::{viewport, HighlightSpan, LanguageId};

#[derive(Debug, Clone)]
pub enum DiskState {
    InSync,
    ReloadedFromDisk { at: Instant },
    ConflictExternalModified,
    MissingOnDisk,
}

#[derive(Debug, Clone)]
pub struct DiskSnapshot {
    pub modified: Option<SystemTime>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadCause {
    ExternalSync,
    ManualCommand,
}

impl ReloadCause {
    pub fn allows_dirty_overwrite(self) -> bool {
        matches!(self, Self::ManualCommand)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadRequest {
    pub pane: usize,
    pub path: PathBuf,
    pub cause: ReloadCause,
    pub request_id: u64,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(u64);

impl TabId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
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
    pub id: TabId,
    pub title: String,
    pub path: Option<PathBuf>,
    pub buffer: TextBuffer,
    pub viewport: EditorViewportState,
    pub history: EditHistory,
    pub dirty: bool,
    pub edit_version: u64,
    pub last_edit_op: Option<EditOp>,
    pub(super) cursor_goal_col: Option<usize>,
    pub mouse: EditorMouseState,
    pub disk_state: DiskState,
    pub saved_snapshot: Option<DiskSnapshot>,
    pub last_reload_request_id: u64,
    pub last_applied_reload_request_id: u64,
    semantic_highlight: Option<SemanticHighlightState>,
    git_gutter: Option<GitGutterMarks>,
    inlay_hints: Option<InlayHintsState>,
    folding: Option<FoldingState>,
    syntax: Option<SyntaxDocument>,
    pub(super) markdown: Option<MarkdownDocument>,
}

impl std::fmt::Debug for EditorTabState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorTabState")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("path", &self.path)
            .field("dirty", &self.dirty)
            .field("cursor", &self.buffer.cursor())
            .field("lines", &self.buffer.len_lines())
            .finish()
    }
}

impl EditorTabState {
    pub fn untitled(id: TabId, config: &EditorConfig) -> Self {
        let buffer = TextBuffer::new();
        let history = EditHistory::new(buffer.rope().clone());
        Self {
            id,
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
            cursor_goal_col: None,
            mouse: EditorMouseState::new(),
            disk_state: DiskState::InSync,
            saved_snapshot: None,
            last_reload_request_id: 0,
            last_applied_reload_request_id: 0,
            semantic_highlight: None,
            git_gutter: None,
            inlay_hints: None,
            folding: None,
            syntax: None,
            markdown: None,
        }
    }

    pub fn from_file(id: TabId, path: PathBuf, content: &str, config: &EditorConfig) -> Self {
        let title = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let buffer = TextBuffer::from_text(content);
        let history = EditHistory::new(buffer.rope().clone());
        let syntax = SyntaxDocument::for_path(&path, buffer.rope());
        let markdown = if LanguageId::from_path(&path) == Some(LanguageId::Markdown) {
            Some(MarkdownDocument::new(buffer.rope()))
        } else {
            None
        };

        Self {
            id,
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
            cursor_goal_col: None,
            mouse: EditorMouseState::new(),
            disk_state: DiskState::InSync,
            saved_snapshot: None,
            last_reload_request_id: 0,
            last_applied_reload_request_id: 0,
            semantic_highlight: None,
            git_gutter: None,
            inlay_hints: None,
            folding: None,
            syntax,
            markdown,
        }
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.title = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        self.path = Some(path.clone());
        self.syntax = SyntaxDocument::for_path(&path, self.buffer.rope());
        self.markdown = if LanguageId::from_path(&path) == Some(LanguageId::Markdown) {
            Some(MarkdownDocument::new(self.buffer.rope()))
        } else {
            None
        };
        self.semantic_highlight = None;
        self.git_gutter = None;
        self.inlay_hints = None;
        self.clear_folding();
    }

    pub(crate) fn reset_cursor_goal_col(&mut self) {
        self.cursor_goal_col = None;
    }

    pub(crate) fn cursor_goal_col_or_current(&self) -> usize {
        self.cursor_goal_col.unwrap_or(self.buffer.cursor().1)
    }

    pub(crate) fn set_cursor_goal_col(&mut self, col: usize) {
        self.cursor_goal_col = Some(col);
    }

    pub fn next_reload_request_id(&mut self) -> u64 {
        self.last_reload_request_id = self.last_reload_request_id.saturating_add(1);
        self.last_reload_request_id
    }

    pub fn issue_reload_request(
        &mut self,
        pane: usize,
        cause: ReloadCause,
    ) -> Option<ReloadRequest> {
        let path = self.path.clone()?;
        let request_id = self.next_reload_request_id();
        Some(ReloadRequest {
            pane,
            path,
            cause,
            request_id,
        })
    }

    pub fn can_apply_reload(&mut self, request: &ReloadRequest) -> bool {
        if request.request_id < self.last_reload_request_id {
            return false;
        }
        if request.request_id == self.last_applied_reload_request_id {
            return false;
        }
        self.last_reload_request_id = request.request_id;
        if self.dirty && !request.cause.allows_dirty_overwrite() {
            return false;
        }
        self.last_applied_reload_request_id = request.request_id;
        true
    }

    pub fn set_git_gutter(&mut self, gutter: Option<GitGutterMarks>) -> bool {
        if self.git_gutter == gutter {
            return false;
        }
        self.git_gutter = gutter;
        true
    }

    pub fn clear_git_gutter(&mut self) -> bool {
        self.set_git_gutter(None)
    }

    pub fn git_gutter_marker(&self, line: usize) -> Option<char> {
        let gutter = self.git_gutter.as_ref()?;
        if gutter.deletions.binary_search(&line).is_ok() {
            return Some('-');
        }

        let idx = gutter.ranges.partition_point(|r| r.start_line <= line);
        let range = idx.checked_sub(1).and_then(|i| gutter.ranges.get(i))?;
        if line < range.end_line_exclusive {
            return Some(range.kind.marker());
        }
        None
    }

    pub fn display_title(&self) -> String {
        let prefix = match &self.disk_state {
            DiskState::ConflictExternalModified => "\u{26a0} ",
            DiskState::MissingOnDisk => "\u{2717} ",
            _ if self.dirty => "\u{25cf} ",
            _ => "",
        };
        format!("{}{}", prefix, self.title)
    }

    pub fn language(&self) -> Option<LanguageId> {
        if self.markdown.is_some() {
            return Some(LanguageId::Markdown);
        }
        self.syntax.as_ref().map(|s| s.language())
    }

    pub fn is_markdown(&self) -> bool {
        self.markdown.is_some()
    }

    pub fn markdown(&self) -> Option<&MarkdownDocument> {
        self.markdown.as_ref()
    }

    /// Returns the nearest identifier position at or immediately before `pos`.
    ///
    /// This makes cursor-based actions (hover, etc.) work when the cursor is placed *after* an
    /// identifier, which is a very common editing state.
    pub fn identifier_pos_at_or_before(&self, pos: (usize, usize)) -> Option<(usize, usize)> {
        let (row, col) = pos;

        // Be forgiving: users often park the cursor on punctuation/whitespace right after a token
        // (e.g. `pred,` or `foo()`), but still expect hover/completion to resolve the identifier.
        const MAX_BACKTRACK: usize = 32;
        let max = col.min(MAX_BACKTRACK);

        for back in 0..=max {
            let candidate = (row, col.saturating_sub(back));
            if self.is_identifier_at_pos(candidate) {
                return Some(candidate);
            }
        }

        None
    }

    fn is_identifier_at_pos(&self, pos: (usize, usize)) -> bool {
        let rope = self.buffer.rope();
        let char_offset = self.buffer.pos_to_char(pos).min(rope.len_chars());
        if char_offset >= rope.len_chars() {
            return false;
        }

        let ch = rope.char(char_offset);
        ch == '_' || UnicodeXID::is_xid_continue(ch)
    }

    pub fn is_in_string_or_comment_at_char(&self, char_offset: usize) -> bool {
        let Some(syntax) = self.syntax.as_ref() else {
            return false;
        };
        let rope = self.buffer.rope();
        let char_offset = char_offset.min(rope.len_chars());
        let byte_offset = rope.char_to_byte(char_offset);
        syntax.is_in_string_or_comment(byte_offset)
    }

    pub fn is_in_string_or_comment_at_cursor(&self) -> bool {
        let (row, col) = self.buffer.cursor();
        let char_offset = self.buffer.pos_to_char((row, col));
        self.is_in_string_or_comment_at_char(char_offset)
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
        semantic.lines(start_line, end_line_exclusive)
    }

    pub fn set_semantic_highlight(&mut self, version: u64, lines: Vec<Vec<HighlightSpan>>) -> bool {
        let same = self
            .semantic_highlight
            .as_ref()
            .is_some_and(|s| s.matches_full(version, &lines));
        if same {
            return false;
        }

        self.semantic_highlight = Some(SemanticHighlightState::from_full(version, lines));
        true
    }

    pub fn set_semantic_highlight_from_slice(
        &mut self,
        version: u64,
        lines: &[Vec<HighlightSpan>],
    ) -> bool {
        let same = self
            .semantic_highlight
            .as_ref()
            .is_some_and(|s| s.matches_full(version, lines));
        if same {
            return false;
        }

        self.semantic_highlight = Some(SemanticHighlightState::from_full(version, lines.to_vec()));
        true
    }

    pub fn set_semantic_highlight_range(
        &mut self,
        version: u64,
        start_line: usize,
        lines: Vec<Vec<HighlightSpan>>,
    ) -> bool {
        self.set_semantic_highlight_range_from_slice(version, start_line, &lines)
    }

    pub fn set_semantic_highlight_range_from_slice(
        &mut self,
        version: u64,
        start_line: usize,
        lines: &[Vec<HighlightSpan>],
    ) -> bool {
        if lines.is_empty() {
            return false;
        }

        let end_line_exclusive = start_line.saturating_add(lines.len());
        if self.semantic_highlight.as_ref().is_some_and(|semantic| {
            semantic.version == version
                && semantic
                    .lines(start_line, end_line_exclusive)
                    .is_some_and(|current| current == lines)
        }) {
            return false;
        }

        let semantic = self
            .semantic_highlight
            .get_or_insert_with(|| SemanticHighlightState {
                version,
                segments: Vec::new(),
            });

        if semantic.version != version {
            semantic.version = version;
            semantic.segments.clear();
        }

        semantic.replace_range(start_line, end_line_exclusive, lines.to_vec());
        true
    }

    pub(super) fn invalidate_semantic_highlight_on_edit(&mut self, op: &EditOp) {
        let Some(semantic) = self.semantic_highlight.as_mut() else {
            return;
        };

        let (start_char, inserted_text, deleted_text) = match &op.kind {
            OpKind::Insert { char_offset, text } => (*char_offset, text.as_str(), ""),
            OpKind::Delete { start, deleted, .. } => (*start, "", deleted.as_str()),
            OpKind::Replace {
                start,
                deleted,
                inserted,
                ..
            } => (*start, inserted.as_str(), deleted.as_str()),
        };

        if semantic.segments.is_empty() {
            return;
        }

        let rope = self.buffer.rope();
        let start_char = start_char.min(rope.len_chars());
        let start_line = rope.char_to_line(start_char);
        let start_byte = rope.char_to_byte(start_char);
        let line_start_byte = rope.line_to_byte(start_line);
        let local_start_byte = start_byte.saturating_sub(line_start_byte);

        let inserted_lines = inserted_text.matches('\n').count();
        let deleted_lines = deleted_text.matches('\n').count();
        let delta_lines = inserted_lines as isize - deleted_lines as isize;

        if inserted_lines == 0 && deleted_lines == 0 {
            semantic.apply_byte_edit(
                start_line,
                local_start_byte,
                deleted_text.len(),
                inserted_text.len(),
            );
        } else {
            semantic.apply_newline_edit(start_line, delta_lines);
        }

        semantic.version = self.edit_version.saturating_add(1);
    }

    pub(crate) fn semantic_highlight_line(&self, line: usize) -> Option<&[HighlightSpan]> {
        let semantic = self.semantic_highlight.as_ref()?;
        semantic.line(line)
    }

    pub fn inlay_hint_lines(
        &self,
        start_line: usize,
        end_line_exclusive: usize,
    ) -> Option<&[Vec<String>]> {
        let hints = self.inlay_hints.as_ref()?;
        if start_line < hints.start_line || end_line_exclusive > hints.end_line_exclusive {
            return None;
        }
        let start = start_line.saturating_sub(hints.start_line);
        let end = end_line_exclusive
            .saturating_sub(hints.start_line)
            .min(hints.lines.len());
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

    pub fn set_inlay_hints_from_slice(
        &mut self,
        version: u64,
        start_line: usize,
        end_line_exclusive: usize,
        lines: &[Vec<String>],
    ) -> bool {
        let same = self.inlay_hints.as_ref().is_some_and(|s| {
            s.version == version
                && s.start_line == start_line
                && s.end_line_exclusive == end_line_exclusive
                && s.lines.as_slice() == lines
        });
        if same {
            return false;
        }

        self.inlay_hints = Some(InlayHintsState {
            version,
            start_line,
            end_line_exclusive,
            lines: lines.to_vec(),
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
        self.folding.as_ref().and_then(|s| s.marker_char(line))
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

        let Some(folding) = self
            .folding
            .as_ref()
            .filter(|s| !s.hidden_ranges.is_empty())
        else {
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
            let target_row = start_line as usize;
            let len = self.buffer.line_grapheme_len(target_row);
            self.buffer.set_cursor(target_row, col.min(len));
            self.set_cursor_goal_col(col);
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
            let target_row = start_line as usize;
            let len = self.buffer.line_grapheme_len(target_row);
            self.buffer.set_cursor(target_row, col.min(len));
            self.set_cursor_goal_col(col);
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

    pub fn set_folding_ranges(&mut self, version: u64, ranges: Vec<LspFoldingRange>) -> bool {
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

    pub fn set_folding_ranges_from_slice(
        &mut self,
        version: u64,
        ranges: &[LspFoldingRange],
    ) -> bool {
        let mut collapsed = self
            .folding
            .as_ref()
            .map(|s| s.collapsed.clone())
            .unwrap_or_default();

        let fold_starts = FoldingState::build_fold_starts(ranges);
        collapsed.retain(|start| fold_starts.contains_key(start));
        let hidden_ranges = FoldingState::build_hidden_ranges(&collapsed, &fold_starts);

        let next = FoldingState {
            version,
            ranges: ranges.to_vec(),
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

    pub fn reload_from_content(&mut self, content: &str, config: &EditorConfig) {
        use crate::models::{EditHistory, TextBuffer};
        self.buffer = TextBuffer::from_text(content);
        self.history = EditHistory::new(self.buffer.rope().clone());
        self.dirty = false;
        self.edit_version = self.edit_version.saturating_add(1);
        self.last_edit_op = None;
        self.disk_state = DiskState::ReloadedFromDisk { at: Instant::now() };
        self.syntax = self
            .path
            .as_ref()
            .and_then(|p| SyntaxDocument::for_path(p, self.buffer.rope()));
        self.markdown = self
            .path
            .as_ref()
            .filter(|p| LanguageId::from_path(p) == Some(LanguageId::Markdown))
            .map(|_| MarkdownDocument::new(self.buffer.rope()));
        self.semantic_highlight = None;
        self.git_gutter = None;
        self.inlay_hints = None;
        self.clear_folding();
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, config.tab_size);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticHighlightState {
    pub version: u64,
    segments: Vec<SemanticHighlightSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticHighlightSegment {
    start_line: usize,
    lines: Vec<Vec<HighlightSpan>>,
}

impl SemanticHighlightSegment {
    fn new(start_line: usize, lines: Vec<Vec<HighlightSpan>>) -> Self {
        Self { start_line, lines }
    }

    fn end_line_exclusive(&self) -> usize {
        self.start_line.saturating_add(self.lines.len())
    }
}

impl SemanticHighlightState {
    fn from_full(version: u64, lines: Vec<Vec<HighlightSpan>>) -> Self {
        Self {
            version,
            segments: vec![SemanticHighlightSegment::new(0, lines)],
        }
    }

    fn matches_full(&self, version: u64, lines: &[Vec<HighlightSpan>]) -> bool {
        self.version == version
            && self.segments.len() == 1
            && self.segments[0].start_line == 0
            && self.segments[0].lines.as_slice() == lines
    }

    fn replace_range(
        &mut self,
        start_line: usize,
        end_line_exclusive: usize,
        mut lines: Vec<Vec<HighlightSpan>>,
    ) {
        if start_line >= end_line_exclusive || lines.is_empty() {
            return;
        }

        debug_assert!(self
            .segments
            .windows(2)
            .all(|w| w[0].end_line_exclusive() <= w[1].start_line));

        let start_idx = self
            .segments
            .partition_point(|seg| seg.end_line_exclusive() <= start_line);
        let end_idx = self
            .segments
            .partition_point(|seg| seg.start_line < end_line_exclusive);

        let mut prefix_start: Option<usize> = None;
        let mut prefix_lines: Vec<Vec<HighlightSpan>> = Vec::new();
        let mut suffix_lines: Vec<Vec<HighlightSpan>> = Vec::new();

        {
            let placeholder = SemanticHighlightSegment::new(start_line, Vec::new());
            let mut removed = self
                .segments
                .splice(start_idx..end_idx, std::iter::once(placeholder));
            if let Some(mut first) = removed.next() {
                let mut last: Option<SemanticHighlightSegment> = None;
                for seg in removed {
                    last = Some(seg);
                }

                if let Some(mut last) = last {
                    if first.start_line < start_line {
                        let keep_len = start_line
                            .saturating_sub(first.start_line)
                            .min(first.lines.len());
                        first.lines.truncate(keep_len);
                        if !first.lines.is_empty() {
                            prefix_start = Some(first.start_line);
                            prefix_lines = first.lines;
                        }
                    }

                    if last.end_line_exclusive() > end_line_exclusive {
                        let split_at = end_line_exclusive
                            .saturating_sub(last.start_line)
                            .min(last.lines.len());
                        suffix_lines = last.lines.split_off(split_at);
                    }
                } else {
                    let seg_start = first.start_line;
                    let seg_end = first.end_line_exclusive();
                    let overlap_start = seg_start.max(start_line);
                    let overlap_end = seg_end.min(end_line_exclusive);
                    let left_keep = overlap_start
                        .saturating_sub(seg_start)
                        .min(first.lines.len());
                    let right_keep_start =
                        overlap_end.saturating_sub(seg_start).min(first.lines.len());

                    let mut seg_lines = first.lines;
                    let mut right_lines = seg_lines.split_off(right_keep_start);
                    seg_lines.truncate(left_keep);

                    if !seg_lines.is_empty() {
                        prefix_start = Some(seg_start);
                        prefix_lines = seg_lines;
                    }

                    if !right_lines.is_empty() {
                        suffix_lines.append(&mut right_lines);
                    }
                }
            }
        }

        let replacement_start = prefix_start.unwrap_or(start_line);
        let mut replacement_lines = prefix_lines;
        replacement_lines.reserve(lines.len() + suffix_lines.len());
        replacement_lines.append(&mut lines);
        replacement_lines.append(&mut suffix_lines);

        if replacement_lines.is_empty() {
            self.segments.remove(start_idx);
            return;
        }

        let seg = &mut self.segments[start_idx];
        seg.start_line = replacement_start;
        seg.lines = replacement_lines;

        self.merge_adjacent_segments(start_idx);
    }

    fn line(&self, line: usize) -> Option<&[HighlightSpan]> {
        let idx = self.segments.partition_point(|seg| seg.start_line <= line);
        let seg = self.segments.get(idx.checked_sub(1)?)?;
        if line < seg.end_line_exclusive() {
            seg.lines
                .get(line.saturating_sub(seg.start_line))
                .map(|spans| spans.as_slice())
        } else {
            None
        }
    }

    fn lines(&self, start_line: usize, end_line_exclusive: usize) -> Option<&[Vec<HighlightSpan>]> {
        if start_line >= end_line_exclusive {
            return None;
        }

        let idx = self
            .segments
            .partition_point(|seg| seg.start_line <= start_line);
        let seg = self.segments.get(idx.checked_sub(1)?)?;
        let seg_end = seg.end_line_exclusive();
        if start_line < seg.start_line || end_line_exclusive > seg_end {
            return None;
        }

        let start = start_line.saturating_sub(seg.start_line);
        let end = end_line_exclusive
            .saturating_sub(seg.start_line)
            .min(seg.lines.len());
        if start >= end {
            return None;
        }
        Some(&seg.lines[start..end])
    }

    fn apply_byte_edit(
        &mut self,
        line: usize,
        local_start_byte: usize,
        deleted_len: usize,
        inserted_len: usize,
    ) {
        let idx = self.segments.partition_point(|seg| seg.start_line <= line);
        let Some(seg) = idx
            .checked_sub(1)
            .and_then(|idx| self.segments.get_mut(idx))
        else {
            return;
        };
        if line >= seg.end_line_exclusive() {
            return;
        }

        let Some(spans) = seg.lines.get_mut(line.saturating_sub(seg.start_line)) else {
            return;
        };
        if spans.is_empty() {
            return;
        }

        let del_end = local_start_byte.saturating_add(deleted_len);
        let delta = inserted_len as isize - deleted_len as isize;

        fn shift(value: usize, delta: isize) -> Option<usize> {
            if delta >= 0 {
                value.checked_add(delta as usize)
            } else {
                value.checked_sub(delta.wrapping_abs() as usize)
            }
        }

        let mut next: Vec<HighlightSpan> = Vec::with_capacity(spans.len());
        for mut span in spans.iter().copied() {
            if deleted_len == 0 {
                if span.start >= local_start_byte {
                    span.start = span.start.saturating_add(inserted_len);
                }
                if span.end > local_start_byte {
                    span.end = span.end.saturating_add(inserted_len);
                }
            } else {
                if span.start >= del_end {
                    let Some(new_start) = shift(span.start, delta) else {
                        continue;
                    };
                    span.start = new_start;
                } else if span.start >= local_start_byte {
                    span.start = local_start_byte;
                }

                if span.end >= del_end {
                    let Some(new_end) = shift(span.end, delta) else {
                        continue;
                    };
                    span.end = new_end;
                } else if span.end > local_start_byte {
                    span.end = local_start_byte;
                }
            }

            if span.end <= span.start {
                continue;
            }
            next.push(span);
        }

        next.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
        merge_adjacent_highlight_spans(&mut next);
        *spans = next;
    }

    fn apply_newline_edit(&mut self, line: usize, delta_lines: isize) {
        if delta_lines == 0 {
            return;
        }

        let inserted = delta_lines.max(0) as usize;
        let deleted = delta_lines.min(0).wrapping_abs() as usize;

        for idx in 0..self.segments.len() {
            let seg_start = self.segments[idx].start_line;
            let seg_end = self.segments[idx].end_line_exclusive();

            if seg_start > line {
                self.segments[idx].start_line = if delta_lines >= 0 {
                    seg_start.saturating_add(inserted)
                } else {
                    seg_start.saturating_sub(deleted)
                };
                continue;
            }

            if line < seg_start || line >= seg_end {
                continue;
            }

            let rel = line.saturating_sub(seg_start);
            let insert_at = rel.saturating_add(1).min(self.segments[idx].lines.len());
            if inserted > 0 {
                self.segments[idx].lines.splice(
                    insert_at..insert_at,
                    std::iter::repeat_with(Vec::new).take(inserted),
                );
            } else if deleted > 0 {
                let remove_end = insert_at
                    .saturating_add(deleted)
                    .min(self.segments[idx].lines.len());
                self.segments[idx].lines.drain(insert_at..remove_end);
            }
        }

        self.segments.retain(|seg| !seg.lines.is_empty());
        self.segments
            .sort_by(|a, b| a.start_line.cmp(&b.start_line));
    }

    fn merge_adjacent_segments(&mut self, idx: usize) {
        let mut idx = idx;
        if idx > 0 && self.segments[idx - 1].end_line_exclusive() == self.segments[idx].start_line {
            let mut current = self.segments.remove(idx);
            self.segments[idx - 1].lines.append(&mut current.lines);
            idx = idx.saturating_sub(1);
        }

        while idx + 1 < self.segments.len()
            && self.segments[idx].end_line_exclusive() == self.segments[idx + 1].start_line
        {
            let mut next = self.segments.remove(idx + 1);
            self.segments[idx].lines.append(&mut next.lines);
        }
    }
}

fn merge_adjacent_highlight_spans(spans: &mut Vec<HighlightSpan>) {
    if spans.len() < 2 {
        return;
    }

    let mut write = 1usize;
    for read in 1..spans.len() {
        let span = spans[read];
        let prev = &mut spans[write - 1];
        if prev.kind == span.kind && prev.end >= span.start {
            prev.end = prev.end.max(span.end);
        } else {
            spans[write] = span;
            write += 1;
        }
    }
    spans.truncate(write);
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/state.rs"]
mod tests;

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
            if range.start_line < cursor_line
                && range.end_line >= cursor_line
                && best.is_none_or(|b| range.start_line > b)
            {
                best = Some(range.start_line);
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

    pub fn open_file(
        &mut self,
        tab_id: TabId,
        path: PathBuf,
        content: &str,
        config: &EditorConfig,
    ) -> bool {
        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.path.as_ref() == Some(&path) {
                return self.set_active(i);
            }
        }

        self.tabs
            .push(EditorTabState::from_file(tab_id, path, content, config));
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
    next_tab_id: u64,
}

impl EditorState {
    pub fn new(config: EditorConfig) -> Self {
        Self {
            config: config.clone(),
            panes: vec![EditorPaneState::new(&config)],
            open_paths_version: 0,
            next_tab_id: 1,
        }
    }

    pub(super) fn alloc_tab_id(&mut self) -> TabId {
        let id = TabId::new(self.next_tab_id);
        self.next_tab_id = self.next_tab_id.saturating_add(1);
        id
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
