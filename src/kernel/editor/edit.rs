use crate::core::Command;
use crate::kernel::services::ports::EditorConfig;
use crate::kernel::services::ports::Match;
use crate::models::cursor_set;
use crate::models::edit_op::BatchEdit;
use crate::models::{
    slice_to_cow, EditOp, Granularity, OpId, OpKind, SecondaryCursor, Selection, TextBuffer,
};
use compact_str::CompactString;
use unicode_segmentation::UnicodeSegmentation;

use super::state::EditorTabState;
use super::viewport;
use super::LanguageId;

#[derive(Default)]
struct DryExecution {
    changed: bool,
    ops: Vec<EditOp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorSlot {
    Primary,
    Secondary(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerticalCursorDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug)]
struct SelectionChars {
    anchor_char: usize,
    cursor_char: usize,
}

#[derive(Clone, Debug)]
struct CursorEditRecord {
    slot: CursorSlot,
    cursor_char: usize,
    selection: Option<SelectionChars>,
    goal_col: Option<usize>,
}

#[derive(Clone, Debug)]
struct EmptyPairReplacePlan {
    start_char: usize,
    end_char: usize,
    inserted: String,
    cursor_after: (usize, usize),
    cursor_after_char_offset: usize,
}

fn empty_pair_replace_plan(
    buffer: &TextBuffer,
    cursor: (usize, usize),
    open: &str,
    close: &str,
    tab_size: u8,
) -> Option<EmptyPairReplacePlan> {
    let (row, col) = cursor;
    let slice = buffer.line_slice(row)?;

    let line_cow = slice_to_cow(slice);
    let line = line_cow.strip_suffix('\n').unwrap_or(&line_cow);
    let line = line.strip_suffix('\r').unwrap_or(line);
    let graphemes: Vec<&str> = line.graphemes(true).collect();
    let len = graphemes.len();
    let col = col.min(len);

    let is_ws = |g: &str| g.chars().all(|c| c.is_whitespace());

    let left = graphemes[..col].iter().rposition(|&g| !is_ws(g));
    let right = graphemes[col..]
        .iter()
        .position(|&g| !is_ws(g))
        .map(|i| i + col);

    let (left, right) = match (left, right) {
        (Some(l), Some(r)) => (l, r),
        _ => return None,
    };

    if graphemes[left] != open || graphemes[right] != close || left >= right {
        return None;
    }
    if !(left + 1..right).all(|i| is_ws(graphemes[i])) {
        return None;
    }

    let indent_end = line
        .bytes()
        .position(|b| b != b' ' && b != b'\t')
        .unwrap_or(line.len());
    let base_indent = &line[..indent_end];
    let base_indent_chars = base_indent.chars().count();
    let indent_spaces = tab_size as usize;

    let mut inserted =
        String::with_capacity(1 + base_indent.len() + indent_spaces + 1 + base_indent.len());
    inserted.push('\n');
    inserted.push_str(base_indent);
    inserted.push_str(&" ".repeat(indent_spaces));
    inserted.push('\n');
    inserted.push_str(base_indent);

    let start_char = buffer.pos_to_char((row, left + 1));
    let end_char = buffer.pos_to_char((row, right));

    let cursor_after = (row.saturating_add(1), base_indent_chars + indent_spaces);
    let cursor_after_char_offset = start_char + 1 + base_indent_chars + indent_spaces;

    Some(EmptyPairReplacePlan {
        start_char,
        end_char,
        inserted,
        cursor_after,
        cursor_after_char_offset,
    })
}

fn batch_edit_from_op(op: &EditOp) -> BatchEdit {
    match &op.kind {
        OpKind::Insert { char_offset, text } => BatchEdit {
            start: *char_offset,
            end: *char_offset,
            deleted: CompactString::default(),
            inserted: text.clone(),
        },
        OpKind::Delete {
            start,
            end,
            deleted,
        } => BatchEdit {
            start: *start,
            end: *end,
            deleted: deleted.clone(),
            inserted: CompactString::default(),
        },
        OpKind::Replace {
            start,
            end,
            deleted,
            inserted,
        } => BatchEdit {
            start: *start,
            end: *end,
            deleted: deleted.clone(),
            inserted: inserted.clone(),
        },
        OpKind::Batch { .. } => {
            panic!("execute_single_dry should not emit OpKind::Batch");
        }
    }
}

fn adjust_offset_after_edit(offset: usize, start: usize, end: usize, inserted_len: usize) -> usize {
    if offset < start {
        return offset;
    }
    if offset < end {
        return start.saturating_add(inserted_len);
    }

    let deleted_len = end.saturating_sub(start);
    if inserted_len >= deleted_len {
        offset.saturating_add(inserted_len.saturating_sub(deleted_len))
    } else {
        offset.saturating_sub(deleted_len.saturating_sub(inserted_len))
    }
}

fn apply_edit_to_records(records: &mut [CursorEditRecord], current: usize, op: &EditOp) {
    let (start, end, inserted_len) = match &op.kind {
        OpKind::Insert { char_offset, text } => (*char_offset, *char_offset, text.chars().count()),
        OpKind::Delete { start, end, .. } => (*start, *end, 0usize),
        OpKind::Replace {
            start,
            end,
            inserted,
            ..
        } => (*start, *end, inserted.chars().count()),
        OpKind::Batch { .. } => return,
    };

    for (idx, record) in records.iter_mut().enumerate() {
        if idx == current {
            continue;
        }

        record.cursor_char = adjust_offset_after_edit(record.cursor_char, start, end, inserted_len);
        if let Some(sel) = record.selection.as_mut() {
            sel.anchor_char = adjust_offset_after_edit(sel.anchor_char, start, end, inserted_len);
            sel.cursor_char = adjust_offset_after_edit(sel.cursor_char, start, end, inserted_len);
            if sel.anchor_char == sel.cursor_char {
                record.selection = None;
            }
        }
    }
}

fn is_word_boundary_char(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            c,
            '（' | '）' | '【' | '】' | '「' | '」' | '，' | '。' | '：' | '；'
        )
}

fn supports_auto_pairs(language: Option<LanguageId>) -> bool {
    matches!(
        language,
        Some(
            LanguageId::Rust
                | LanguageId::Go
                | LanguageId::Python
                | LanguageId::JavaScript
                | LanguageId::TypeScript
                | LanguageId::Jsx
                | LanguageId::Tsx
                | LanguageId::C
                | LanguageId::Cpp
                | LanguageId::Java
        )
    )
}

fn supports_brace_electric_enter(language: Option<LanguageId>) -> bool {
    matches!(
        language,
        Some(
            LanguageId::Rust
                | LanguageId::Go
                | LanguageId::JavaScript
                | LanguageId::TypeScript
                | LanguageId::Jsx
                | LanguageId::Tsx
                | LanguageId::C
                | LanguageId::Cpp
                | LanguageId::Java
        )
    )
}

fn supports_paren_electric_enter(language: Option<LanguageId>) -> bool {
    language == Some(LanguageId::Go)
}

fn supports_python_colon_indent(language: Option<LanguageId>) -> bool {
    language == Some(LanguageId::Python)
}

impl EditorTabState {
    pub fn apply_command(
        &mut self,
        command: Command,
        pane: usize,
        config: &EditorConfig,
    ) -> (bool, Vec<crate::kernel::Effect>) {
        use crate::kernel::Effect;

        if matches!(
            command,
            Command::Undo
                | Command::Redo
                | Command::AddCursorAbove
                | Command::AddCursorBelow
                | Command::AddCursorAtNextMatch
                | Command::AddCursorAtAllMatches
                | Command::RemoveSecondaryCursors
                | Command::CursorLeft
                | Command::CursorRight
                | Command::CursorUp
                | Command::CursorDown
                | Command::CursorLineStart
                | Command::CursorLineEnd
                | Command::CursorFileStart
                | Command::CursorFileEnd
                | Command::CursorWordLeft
                | Command::CursorWordRight
                | Command::ClearSelection
                | Command::SelectAll
                | Command::SelectWord
                | Command::SelectLine
                | Command::ExtendSelectionLeft
                | Command::ExtendSelectionRight
                | Command::ExtendSelectionUp
                | Command::ExtendSelectionDown
                | Command::ExtendSelectionLineStart
                | Command::ExtendSelectionLineEnd
                | Command::ExtendSelectionWordLeft
                | Command::ExtendSelectionWordRight
        ) {
            self.cancel_snippet_session();
        }

        self.viewport.follow_cursor = true;
        let tab_size = config.tab_size;

        match command {
            Command::Undo => {
                let changed = self.undo(tab_size);
                (changed, Vec::new())
            }
            Command::Redo => {
                let changed = self.redo(tab_size);
                (changed, Vec::new())
            }
            Command::AddCursorAbove => {
                let changed = self.add_cursor_above(tab_size);
                (changed, Vec::new())
            }
            Command::AddCursorBelow => {
                let changed = self.add_cursor_below(tab_size);
                (changed, Vec::new())
            }
            Command::AddCursorAtNextMatch => {
                let changed = self.add_cursor_at_next_match(tab_size);
                (changed, Vec::new())
            }
            Command::AddCursorAtAllMatches => {
                let changed = self.add_cursor_at_all_matches(tab_size);
                (changed, Vec::new())
            }
            Command::RemoveSecondaryCursors => {
                let changed = self.remove_secondary_cursors(tab_size);
                (changed, Vec::new())
            }
            Command::Copy => self.copy(),
            Command::Cut => self.cut(config),
            Command::Paste => (false, vec![Effect::RequestClipboardText { pane }]),
            Command::EditorFoldToggle | Command::EditorFold | Command::EditorUnfold => {
                let changed = self.execute(command, config);
                (changed, Vec::new())
            }
            cmd if cmd.is_cursor_command() => {
                if self.is_multi_cursor() {
                    let changed = self.execute_on_all_cursors(cmd, config);
                    (changed, Vec::new())
                } else {
                    self.clear_empty_selection();
                    let changed = self.execute(cmd, config);
                    (changed, Vec::new())
                }
            }
            cmd if cmd.is_selection_command() => {
                let changed = if self.is_multi_cursor() {
                    self.execute_on_all_cursors(cmd, config)
                } else {
                    self.execute(cmd, config)
                };
                (changed, Vec::new())
            }
            cmd if cmd.is_edit_command() => {
                let changed = if self.is_multi_cursor() {
                    self.execute_on_all_cursors(cmd, config)
                } else {
                    self.execute(cmd, config)
                };
                (changed, Vec::new())
            }
            _ => (false, Vec::new()),
        }
    }

    fn clear_empty_selection(&mut self) {
        if self
            .buffer
            .selection()
            .is_some_and(|selection| selection.is_empty())
        {
            self.buffer.clear_selection();
        }
    }

    fn execute(&mut self, command: Command, config: &EditorConfig) -> bool {
        let result = self.execute_single_dry(command, config);
        let tab_size = config.tab_size;
        for op in result.ops {
            self.commit_op(op, tab_size);
        }
        result.changed
    }

    fn remove_secondary_cursors(&mut self, tab_size: u8) -> bool {
        if self.secondary_cursors.is_empty() {
            return false;
        }
        self.secondary_cursors.clear();
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    fn add_cursor_above(&mut self, tab_size: u8) -> bool {
        self.cancel_snippet_session();
        self.add_cursor_vertical(tab_size, VerticalCursorDirection::Up)
    }

    fn add_cursor_below(&mut self, tab_size: u8) -> bool {
        self.cancel_snippet_session();
        self.add_cursor_vertical(tab_size, VerticalCursorDirection::Down)
    }

    fn add_cursor_vertical(&mut self, tab_size: u8, direction: VerticalCursorDirection) -> bool {
        let old_primary_pos = self.buffer.cursor();
        let old_primary_selection = self.buffer.selection().cloned();
        let old_primary_goal_col = self.cursor_goal_col;

        let old_secondaries = self.secondary_cursors.clone();

        let new_primary =
            self.vertical_cursor_target(old_primary_pos, old_primary_goal_col, direction);

        let mut added_any = false;

        let mut new_secondaries: Vec<SecondaryCursor> =
            Vec::with_capacity(old_secondaries.len().saturating_mul(2).saturating_add(1));

        // Keep existing secondary cursors.
        new_secondaries.extend(old_secondaries.iter().cloned());

        // New cursors for existing secondaries.
        for cursor in &old_secondaries {
            if let Some(new_cursor) =
                self.vertical_cursor_target(cursor.pos, cursor.goal_col, direction)
            {
                new_secondaries.push(new_cursor);
                added_any = true;
            }
        }

        // New cursor for primary becomes the new primary when available.
        if let Some(new_primary) = new_primary {
            new_secondaries.push(SecondaryCursor {
                pos: old_primary_pos,
                selection: old_primary_selection,
                goal_col: old_primary_goal_col,
            });
            self.buffer.set_cursor(new_primary.pos.0, new_primary.pos.1);
            self.buffer.clear_selection();
            self.cursor_goal_col = new_primary.goal_col;
            added_any = true;
        }

        if !added_any {
            return false;
        }

        self.secondary_cursors = new_secondaries;

        let merged = cursor_set::merge_overlapping(
            self.buffer.cursor(),
            self.buffer.selection(),
            &mut self.secondary_cursors,
        );
        self.buffer
            .set_cursor(merged.primary_pos.0, merged.primary_pos.1);
        self.buffer.set_selection(merged.primary_selection);

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    fn vertical_cursor_target(
        &self,
        pos: (usize, usize),
        goal_col: Option<usize>,
        direction: VerticalCursorDirection,
    ) -> Option<SecondaryCursor> {
        let target_row = match direction {
            VerticalCursorDirection::Up => self.prev_visible_row_before(pos.0)?,
            VerticalCursorDirection::Down => {
                self.next_visible_row_at_or_after(pos.0.saturating_add(1))?
            }
        };

        let goal_col = goal_col.unwrap_or(pos.1);
        let line_len = self.buffer.line_grapheme_len(target_row);
        let col = goal_col.min(line_len);

        Some(SecondaryCursor {
            pos: (target_row, col),
            selection: None,
            goal_col: Some(goal_col),
        })
    }

    fn add_cursor_at_next_match(&mut self, tab_size: u8) -> bool {
        self.cancel_snippet_session();

        if self
            .buffer
            .selection()
            .is_none_or(|selection| selection.is_empty())
        {
            return self.select_word_under_cursor(tab_size);
        }

        let Some(needle) = self.buffer.get_selection_text() else {
            return false;
        };
        if needle.is_empty() {
            return false;
        }

        let Some(selection) = self.buffer.selection() else {
            return false;
        };
        let (_, sel_end_pos) = selection.range();
        let sel_end_char = self.buffer.pos_to_char(sel_end_pos);

        let needle_chars = needle.chars().count();
        let rope = self.buffer.rope();
        let text = rope.to_string();

        let mut existing: Vec<(usize, usize)> =
            Vec::with_capacity(self.secondary_cursors.len().saturating_add(1));
        if let Some(sel) = self.buffer.selection().filter(|s| !s.is_empty()) {
            let (start_pos, end_pos) = sel.range();
            existing.push((
                self.buffer.pos_to_char(start_pos),
                self.buffer.pos_to_char(end_pos),
            ));
        }
        for c in &self.secondary_cursors {
            let Some(sel) = c.selection.as_ref().filter(|s| !s.is_empty()) else {
                continue;
            };
            let (start_pos, end_pos) = sel.range();
            existing.push((
                self.buffer.pos_to_char(start_pos),
                self.buffer.pos_to_char(end_pos),
            ));
        }
        existing.sort_unstable();
        existing.dedup();

        let start_byte = rope.char_to_byte(sel_end_char.min(rope.len_chars()));
        let mut found: Option<(usize, usize)> = None;

        for (range_start, range_end) in [(start_byte, text.len()), (0, start_byte)] {
            let mut offset = range_start;
            while offset <= range_end && offset <= text.len() {
                let hay = &text[offset..range_end];
                let Some(rel) = hay.find(&needle) else {
                    break;
                };
                let match_byte = offset + rel;
                let match_start_char = rope.byte_to_char(match_byte);
                let match_end_char = match_start_char.saturating_add(needle_chars);
                let range = (match_start_char, match_end_char);
                if existing.binary_search(&range).is_err() {
                    found = Some(range);
                    break;
                }
                offset = match_byte.saturating_add(needle.len().max(1));
            }
            if found.is_some() {
                break;
            }
        }

        let Some((match_start_char, match_end_char)) = found else {
            return false;
        };

        let old_primary = SecondaryCursor {
            pos: self.buffer.cursor(),
            selection: self.buffer.selection().cloned(),
            goal_col: self.cursor_goal_col,
        };
        self.secondary_cursors.push(old_primary);

        let start_pos = self
            .buffer
            .cursor_pos_from_char_offset(match_start_char.min(rope.len_chars()));
        let end_pos = self
            .buffer
            .cursor_pos_from_char_offset(match_end_char.min(rope.len_chars()));
        let mut new_sel = Selection::new(start_pos, Granularity::Char);
        new_sel.update_cursor(end_pos, rope);

        self.buffer.set_selection(Some(new_sel));
        self.buffer.set_cursor(end_pos.0, end_pos.1);
        self.reset_cursor_goal_col();

        let merged = cursor_set::merge_overlapping(
            self.buffer.cursor(),
            self.buffer.selection(),
            &mut self.secondary_cursors,
        );
        self.buffer
            .set_cursor(merged.primary_pos.0, merged.primary_pos.1);
        self.buffer.set_selection(merged.primary_selection);

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    fn add_cursor_at_all_matches(&mut self, tab_size: u8) -> bool {
        self.cancel_snippet_session();

        let mut before_positions = cursor_set::secondary_cursor_positions(&self.secondary_cursors);
        before_positions.sort_unstable();

        let mut changed = false;
        if self
            .buffer
            .selection()
            .is_none_or(|selection| selection.is_empty())
        {
            changed |= self.select_word_under_cursor(tab_size);
        }

        let Some(needle) = self.buffer.get_selection_text() else {
            return changed;
        };
        if needle.is_empty() {
            return changed;
        }

        let Some(primary_selection) = self.buffer.selection().filter(|s| !s.is_empty()) else {
            return changed;
        };
        let (primary_start_pos, primary_end_pos) = primary_selection.range();
        let primary_start_char = self.buffer.pos_to_char(primary_start_pos);
        let primary_end_char = self.buffer.pos_to_char(primary_end_pos);

        let rope = self.buffer.rope();
        let text = rope.to_string();
        let needle_chars = needle.chars().count();

        let mut next_secondaries: Vec<SecondaryCursor> = Vec::new();
        for (match_byte, _) in text.match_indices(&needle) {
            let match_start_char = rope.byte_to_char(match_byte);
            let match_end_char = match_start_char.saturating_add(needle_chars);
            if match_start_char == primary_start_char && match_end_char == primary_end_char {
                continue;
            }

            let start_pos = self
                .buffer
                .cursor_pos_from_char_offset(match_start_char.min(rope.len_chars()));
            let end_pos = self
                .buffer
                .cursor_pos_from_char_offset(match_end_char.min(rope.len_chars()));
            let mut sel = Selection::new(start_pos, Granularity::Char);
            sel.update_cursor(end_pos, rope);

            next_secondaries.push(SecondaryCursor {
                pos: end_pos,
                selection: Some(sel),
                goal_col: None,
            });
        }

        let mut after_positions: Vec<(usize, usize)> =
            next_secondaries.iter().map(|c| c.pos).collect();
        after_positions.sort_unstable();
        if before_positions != after_positions {
            changed = true;
        }
        self.secondary_cursors = next_secondaries;

        let merged = cursor_set::merge_overlapping(
            self.buffer.cursor(),
            self.buffer.selection(),
            &mut self.secondary_cursors,
        );
        self.buffer
            .set_cursor(merged.primary_pos.0, merged.primary_pos.1);
        self.buffer.set_selection(merged.primary_selection);

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        changed
    }

    fn select_word_under_cursor(&mut self, tab_size: u8) -> bool {
        let pos = self.buffer.cursor();
        let selection = Selection::from_pos(pos, Granularity::Word, self.buffer.rope());
        if selection.is_empty() {
            return false;
        }
        let cursor = selection.cursor();
        self.buffer.set_selection(Some(selection));
        self.buffer.set_cursor(cursor.0, cursor.1);
        self.reset_cursor_goal_col();
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    fn execute_on_all_cursors(&mut self, command: Command, config: &EditorConfig) -> bool {
        let tab_size = config.tab_size;

        let merged = cursor_set::merge_overlapping(
            self.buffer.cursor(),
            self.buffer.selection(),
            &mut self.secondary_cursors,
        );
        self.buffer
            .set_cursor(merged.primary_pos.0, merged.primary_pos.1);
        self.buffer.set_selection(merged.primary_selection);

        if command.is_edit_command() {
            return self.execute_edit_on_all_cursors(command, config);
        }

        let mut changed_any = false;

        let primary = SecondaryCursor {
            pos: self.buffer.cursor(),
            selection: self.buffer.selection().cloned(),
            goal_col: self.cursor_goal_col,
        };

        let mut cursors: Vec<SecondaryCursor> =
            Vec::with_capacity(self.secondary_cursors.len().saturating_add(1));
        cursors.push(primary);
        cursors.append(&mut self.secondary_cursors);

        for cursor in &mut cursors {
            self.buffer.set_cursor(cursor.pos.0, cursor.pos.1);
            self.buffer.set_selection(cursor.selection.clone());
            self.cursor_goal_col = cursor.goal_col;
            if command.is_cursor_command() {
                self.clear_empty_selection();
            }

            let result = self.execute_single_dry(command.clone(), config);
            changed_any |= result.changed;

            cursor.pos = self.buffer.cursor();
            cursor.selection = self.buffer.selection().cloned();
            cursor.goal_col = self.cursor_goal_col;
        }

        let mut cursors = cursors.into_iter();
        let primary = cursors.next().unwrap_or(SecondaryCursor {
            pos: self.buffer.cursor(),
            selection: self.buffer.selection().cloned(),
            goal_col: self.cursor_goal_col,
        });
        self.secondary_cursors = cursors.collect();

        self.buffer.set_cursor(primary.pos.0, primary.pos.1);
        self.buffer.set_selection(primary.selection);
        self.cursor_goal_col = primary.goal_col;

        let merged = cursor_set::merge_overlapping(
            self.buffer.cursor(),
            self.buffer.selection(),
            &mut self.secondary_cursors,
        );
        self.buffer
            .set_cursor(merged.primary_pos.0, merged.primary_pos.1);
        self.buffer.set_selection(merged.primary_selection);

        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);

        changed_any
    }

    fn execute_single_dry(&mut self, command: Command, config: &EditorConfig) -> DryExecution {
        let tab_size = config.tab_size;

        let mut parent = self.history.head();
        let mut ops: Vec<EditOp> = Vec::new();
        let mut changed = false;

        match command {
            // ==================== cursor movement ====================
            Command::CursorLeft => changed = self.cursor_left(tab_size),
            Command::CursorRight => changed = self.cursor_right(tab_size),
            Command::CursorUp => changed = self.cursor_up(tab_size),
            Command::CursorDown => changed = self.cursor_down(tab_size),
            Command::CursorWordLeft => changed = self.cursor_word_left(tab_size),
            Command::CursorWordRight => changed = self.cursor_word_right(tab_size),
            Command::CursorLineStart => {
                let prev = self.buffer.cursor();
                let (row, _) = prev;
                self.buffer.set_cursor(row, 0);
                changed = self.buffer.cursor() != prev;
                if changed {
                    self.reset_cursor_goal_col();
                    self.buffer.update_selection_cursor(self.buffer.cursor());
                    viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
                }
            }
            Command::CursorLineEnd => {
                let prev = self.buffer.cursor();
                let (row, _) = prev;
                let len = self.buffer.line_grapheme_len(row);
                self.buffer.set_cursor(row, len);
                changed = self.buffer.cursor() != prev;
                if changed {
                    self.reset_cursor_goal_col();
                    self.buffer.update_selection_cursor(self.buffer.cursor());
                    viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
                }
            }
            Command::CursorFileStart => {
                let prev = self.buffer.cursor();
                self.buffer.set_cursor(0, 0);
                changed = self.buffer.cursor() != prev;
                if changed {
                    self.reset_cursor_goal_col();
                    self.buffer.update_selection_cursor(self.buffer.cursor());
                    viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
                }
            }
            Command::CursorFileEnd => {
                let prev = self.buffer.cursor();
                let last = self.buffer.len_lines().saturating_sub(1);
                let row = self.prev_visible_row_at_or_before(last).unwrap_or(last);
                let len = self.buffer.line_grapheme_len(row);
                self.buffer.set_cursor(row, len);
                changed = self.buffer.cursor() != prev;
                if changed {
                    self.reset_cursor_goal_col();
                    self.buffer.update_selection_cursor(self.buffer.cursor());
                    viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
                }
            }

            // ==================== selection ====================
            Command::ClearSelection => {
                if self.buffer.selection().is_some() {
                    self.buffer.clear_selection();
                    changed = true;
                }
            }
            Command::ExtendSelectionLeft => changed = self.extend_selection_left(tab_size),
            Command::ExtendSelectionRight => changed = self.extend_selection_right(tab_size),
            Command::ExtendSelectionUp => changed = self.extend_selection_up(tab_size),
            Command::ExtendSelectionDown => changed = self.extend_selection_down(tab_size),
            Command::ExtendSelectionLineStart => {
                changed = self.extend_selection_to_line_start(tab_size)
            }
            Command::ExtendSelectionLineEnd => {
                changed = self.extend_selection_to_line_end(tab_size)
            }
            Command::ExtendSelectionWordLeft => changed = self.extend_selection_word_left(tab_size),
            Command::ExtendSelectionWordRight => {
                changed = self.extend_selection_word_right(tab_size)
            }
            Command::SelectAll => changed = self.select_all(tab_size),

            // ==================== edit ====================
            Command::InsertChar(c) => {
                let had_selection = self.buffer.has_selection();
                if let Some(op) = self.buffer.delete_selection_op(parent) {
                    parent = op.id;
                    ops.push(op);
                } else {
                    self.buffer.clear_selection();
                }

                if !had_selection && self.try_skip_closing(c, tab_size) {
                    return DryExecution { changed: true, ops };
                }

                if matches!(c, '{' | '(' | '[' | '"' | '\'') {
                    let language = self.language();
                    let can_pair = config.auto_indent
                        && supports_auto_pairs(language)
                        && !self.in_string_or_comment();
                    if can_pair {
                        let op = match c {
                            '{' => self.insert_brace_pair_op(parent),
                            '(' => self.insert_pair_op('(', ')', parent),
                            '[' => self.insert_pair_op('[', ']', parent),
                            '"' => self.insert_pair_op('"', '"', parent),
                            '\'' => self.insert_pair_op('\'', '\'', parent),
                            _ => unreachable!("matches! gate ensures auto-pair chars"),
                        };
                        ops.push(op);
                        self.reset_cursor_goal_col();
                        return DryExecution { changed: true, ops };
                    }
                }

                ops.push(self.buffer.insert_char_op(c, parent));
                self.reset_cursor_goal_col();
                changed = true;
            }
            Command::InsertNewline => {
                let language = self.language();
                if config.auto_indent && !self.in_string_or_comment() {
                    if supports_brace_electric_enter(language) {
                        if let Some(op) = self.expand_empty_pair_op("{", "}", tab_size, parent) {
                            ops.push(op);
                            self.reset_cursor_goal_col();
                            return DryExecution { changed: true, ops };
                        }
                    }
                    if supports_paren_electric_enter(language) {
                        if let Some(op) = self.expand_empty_pair_op("(", ")", tab_size, parent) {
                            ops.push(op);
                            self.reset_cursor_goal_col();
                            return DryExecution { changed: true, ops };
                        }
                    }
                }

                if let Some(op) = self.buffer.delete_selection_op(parent) {
                    parent = op.id;
                    ops.push(op);
                } else {
                    self.buffer.clear_selection();
                }

                let op = if config.auto_indent {
                    self.insert_newline_with_indent_op(tab_size, parent)
                } else {
                    self.buffer.insert_char_op('\n', parent)
                };
                ops.push(op);
                self.reset_cursor_goal_col();
                changed = true;
            }
            Command::InsertTab => {
                if self.snippet_move_next(tab_size) {
                    return DryExecution { changed: true, ops };
                }

                if let Some(op) = self.buffer.delete_selection_op(parent) {
                    parent = op.id;
                    ops.push(op);
                } else {
                    self.buffer.clear_selection();
                }

                ops.push(self.buffer.insert_char_op('\t', parent));
                self.reset_cursor_goal_col();
                changed = true;
            }
            Command::DeleteBackward => {
                if let Some(op) = self.buffer.delete_selection_op(parent) {
                    ops.push(op);
                    self.reset_cursor_goal_col();
                    changed = true;
                } else {
                    self.buffer.clear_selection();
                    if let Some(op) = self.buffer.delete_backward_op(parent) {
                        ops.push(op);
                        self.reset_cursor_goal_col();
                        changed = true;
                    }
                }
            }
            Command::DeleteForward => {
                if let Some(op) = self.buffer.delete_selection_op(parent) {
                    ops.push(op);
                    self.reset_cursor_goal_col();
                    changed = true;
                } else {
                    self.buffer.clear_selection();
                    if let Some(op) = self.buffer.delete_forward_op(parent) {
                        ops.push(op);
                        self.reset_cursor_goal_col();
                        changed = true;
                    }
                }
            }
            Command::DeleteSelection => {
                if let Some(op) = self.buffer.delete_selection_op(parent) {
                    ops.push(op);
                    self.reset_cursor_goal_col();
                    changed = true;
                } else {
                    self.buffer.clear_selection();
                }
            }
            Command::DeleteToLineEnd => {
                if let Some(op) = self.buffer.delete_selection_op(parent) {
                    ops.push(op);
                    self.reset_cursor_goal_col();
                    changed = true;
                } else {
                    self.buffer.clear_selection();
                    if let Some(op) = self.delete_to_line_end_op(parent) {
                        ops.push(op);
                        self.reset_cursor_goal_col();
                        changed = true;
                    }
                }
            }
            Command::DeleteLine => {
                if let Some(op) = self.buffer.delete_selection_op(parent) {
                    ops.push(op);
                    self.reset_cursor_goal_col();
                    changed = true;
                } else {
                    self.buffer.clear_selection();
                    if let Some(op) = self.delete_line_op(parent) {
                        ops.push(op);
                        self.reset_cursor_goal_col();
                        changed = true;
                    }
                }
            }

            // ==================== folding ====================
            Command::EditorFoldToggle => changed = self.fold_toggle_at_cursor(tab_size),
            Command::EditorFold => changed = self.fold_close_at_cursor(tab_size),
            Command::EditorUnfold => changed = self.fold_open_at_cursor(tab_size),

            // ==================== snippets ====================
            Command::SnippetPrevPlaceholder => changed = self.snippet_move_prev(tab_size),

            _ => {}
        }

        DryExecution { changed, ops }
    }

    fn execute_edit_on_all_cursors(&mut self, command: Command, config: &EditorConfig) -> bool {
        let tab_size = config.tab_size;

        let primary_before = self.buffer.cursor();
        let extra_before = cursor_set::secondary_cursor_positions(&self.secondary_cursors);
        let parent = self.history.head();

        let mut records: Vec<CursorEditRecord> =
            Vec::with_capacity(self.secondary_cursors.len().saturating_add(1));

        let primary_char = self.buffer.cursor_char_offset();
        records.push(CursorEditRecord {
            slot: CursorSlot::Primary,
            cursor_char: primary_char,
            selection: self.selection_chars(self.buffer.selection()),
            goal_col: self.cursor_goal_col,
        });

        for (idx, cursor) in self.secondary_cursors.iter().enumerate() {
            records.push(CursorEditRecord {
                slot: CursorSlot::Secondary(idx),
                cursor_char: self.buffer.pos_to_char(cursor.pos),
                selection: self.selection_chars(cursor.selection.as_ref()),
                goal_col: cursor.goal_col,
            });
        }

        let mut order: Vec<usize> = (0..records.len()).collect();
        order.sort_by(|&a, &b| {
            let a_key = self.edit_start_char_for_record(&records[a], &command, config);
            let b_key = self.edit_start_char_for_record(&records[b], &command, config);
            b_key
                .cmp(&a_key)
                .then_with(|| records[b].cursor_char.cmp(&records[a].cursor_char))
                .then_with(|| match (records[a].slot, records[b].slot) {
                    (CursorSlot::Primary, CursorSlot::Primary) => std::cmp::Ordering::Equal,
                    (CursorSlot::Primary, _) => std::cmp::Ordering::Greater,
                    (_, CursorSlot::Primary) => std::cmp::Ordering::Less,
                    (CursorSlot::Secondary(a_idx), CursorSlot::Secondary(b_idx)) => {
                        b_idx.cmp(&a_idx)
                    }
                })
        });

        let mut batch_edits: Vec<BatchEdit> = Vec::new();
        let mut changed_any = false;

        for idx in order {
            let record = records[idx].clone();
            self.load_cursor_record(&record);

            let result = self.execute_single_dry(command.clone(), config);
            changed_any |= result.changed;

            records[idx].cursor_char = self.buffer.cursor_char_offset();
            records[idx].selection = self.selection_chars(self.buffer.selection());
            records[idx].goal_col = self.cursor_goal_col;

            if result.ops.is_empty() {
                continue;
            }

            for op in &result.ops {
                batch_edits.push(batch_edit_from_op(op));
            }
            for op in &result.ops {
                apply_edit_to_records(&mut records, idx, op);
            }
        }

        let Some(primary_record) = records.iter().find(|r| r.slot == CursorSlot::Primary) else {
            return false;
        };

        self.cursor_goal_col = primary_record.goal_col;
        let primary_pos = self
            .buffer
            .cursor_pos_from_char_offset(primary_record.cursor_char);
        self.buffer.set_cursor(primary_pos.0, primary_pos.1);
        self.buffer
            .set_cursor_char_offset_cache(primary_record.cursor_char);
        self.buffer
            .set_selection(self.selection_from_chars(primary_record.selection));

        self.secondary_cursors = records
            .iter()
            .filter_map(|r| match r.slot {
                CursorSlot::Secondary(_) => {
                    let pos = self.buffer.cursor_pos_from_char_offset(r.cursor_char);
                    Some(SecondaryCursor {
                        pos,
                        selection: self.selection_from_chars(r.selection),
                        goal_col: r.goal_col,
                    })
                }
                CursorSlot::Primary => None,
            })
            .collect();

        let merged = cursor_set::merge_overlapping(
            self.buffer.cursor(),
            self.buffer.selection(),
            &mut self.secondary_cursors,
        );
        self.buffer
            .set_cursor(merged.primary_pos.0, merged.primary_pos.1);
        self.buffer.set_selection(merged.primary_selection);

        if batch_edits.is_empty() {
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            return changed_any;
        }

        batch_edits.sort_by(|a, b| b.start.cmp(&a.start));
        let extra_after = cursor_set::secondary_cursor_positions(&self.secondary_cursors);

        let batch_op = EditOp {
            id: OpId::new(),
            parent,
            kind: OpKind::Batch { edits: batch_edits },
            cursor_before: primary_before,
            cursor_after: self.buffer.cursor(),
            extra_cursors_before: Some(extra_before),
            extra_cursors_after: Some(extra_after),
        };

        self.commit_op(batch_op, tab_size);
        true
    }

    fn selection_chars(&self, selection: Option<&Selection>) -> Option<SelectionChars> {
        let selection = selection?;
        if selection.is_empty() {
            return None;
        }
        Some(SelectionChars {
            anchor_char: self.buffer.pos_to_char(selection.anchor()),
            cursor_char: self.buffer.pos_to_char(selection.cursor()),
        })
    }

    fn selection_from_chars(&self, selection: Option<SelectionChars>) -> Option<Selection> {
        let selection = selection?;
        if selection.anchor_char == selection.cursor_char {
            return None;
        }
        let anchor = self
            .buffer
            .cursor_pos_from_char_offset(selection.anchor_char);
        let cursor = self
            .buffer
            .cursor_pos_from_char_offset(selection.cursor_char);
        let mut sel = Selection::new(anchor, Granularity::Char);
        sel.update_cursor(cursor, self.buffer.rope());
        Some(sel)
    }

    fn load_cursor_record(&mut self, record: &CursorEditRecord) {
        let pos = self.buffer.cursor_pos_from_char_offset(record.cursor_char);
        self.buffer.set_cursor(pos.0, pos.1);
        self.buffer.set_cursor_char_offset_cache(record.cursor_char);
        self.buffer
            .set_selection(self.selection_from_chars(record.selection));
        self.cursor_goal_col = record.goal_col;
    }

    fn edit_start_char_for_record(
        &self,
        record: &CursorEditRecord,
        command: &Command,
        config: &EditorConfig,
    ) -> usize {
        let Some(sel) = record.selection else {
            return match command {
                Command::DeleteBackward => {
                    if record.cursor_char == 0 {
                        return 0;
                    }
                    let (row, col) = self.buffer.cursor_pos_from_char_offset(record.cursor_char);
                    if col > 0 {
                        self.buffer.pos_to_char((row, col - 1))
                    } else {
                        record.cursor_char.saturating_sub(1)
                    }
                }
                Command::InsertNewline => {
                    let language = self.language();
                    if config.auto_indent && !self.in_string_or_comment_at(record.cursor_char) {
                        let (row, col) =
                            self.buffer.cursor_pos_from_char_offset(record.cursor_char);
                        if supports_brace_electric_enter(language) {
                            if let Some(plan) =
                                self.empty_pair_replace_plan((row, col), "{", "}", config.tab_size)
                            {
                                return plan.start_char;
                            }
                        }
                        if supports_paren_electric_enter(language) {
                            if let Some(plan) =
                                self.empty_pair_replace_plan((row, col), "(", ")", config.tab_size)
                            {
                                return plan.start_char;
                            }
                        }
                    }
                    record.cursor_char
                }
                Command::DeleteLine => {
                    let row = self.buffer.rope().char_to_line(record.cursor_char);
                    self.buffer.rope().line_to_char(row)
                }
                _ => record.cursor_char,
            };
        };

        sel.anchor_char.min(sel.cursor_char)
    }

    fn paste_start_char_for_record(&self, record: &CursorEditRecord) -> usize {
        record
            .selection
            .map(|sel| sel.anchor_char.min(sel.cursor_char))
            .unwrap_or(record.cursor_char)
    }

    fn in_string_or_comment_at(&self, char_offset: usize) -> bool {
        let byte_offset = self.buffer.rope().char_to_byte(char_offset);
        self.syntax()
            .is_some_and(|syntax| syntax.is_in_string_or_comment(byte_offset))
    }

    fn empty_pair_replace_plan(
        &self,
        cursor: (usize, usize),
        open: &str,
        close: &str,
        tab_size: u8,
    ) -> Option<EmptyPairReplacePlan> {
        empty_pair_replace_plan(&self.buffer, cursor, open, close, tab_size)
    }

    fn delete_to_line_end_op(&mut self, parent: OpId) -> Option<EditOp> {
        let (row, col) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);

        let start_char = self.buffer.cursor_char_offset();
        let end_char = if col < line_len {
            self.buffer.pos_to_char((row, line_len))
        } else {
            let rope = self.buffer.rope();
            if start_char < rope.len_chars() && rope.char(start_char) == '\n' {
                start_char.saturating_add(1)
            } else {
                return None;
            }
        };

        (start_char < end_char).then(|| {
            self.buffer
                .replace_range_op_auto_cursor(start_char, end_char, "", parent)
        })
    }

    fn delete_line_op(&mut self, parent: OpId) -> Option<EditOp> {
        let rope = self.buffer.rope();
        if rope.len_chars() == 0 {
            return None;
        }

        let (row, _) = self.buffer.cursor();
        let start_char = rope.line_to_char(row);
        let end_char = if row + 1 < rope.len_lines() {
            rope.line_to_char(row + 1)
        } else {
            rope.len_chars()
        };

        (start_char < end_char).then(|| {
            self.buffer
                .replace_range_op_auto_cursor(start_char, end_char, "", parent)
        })
    }

    fn in_string_or_comment(&mut self) -> bool {
        let char_offset = self.buffer.cursor_char_offset();
        let byte_offset = self.buffer.rope().char_to_byte(char_offset);
        self.syntax()
            .is_some_and(|syntax| syntax.is_in_string_or_comment(byte_offset))
    }

    fn cursor_left(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        if col > 0 {
            self.buffer.set_cursor(row, col - 1);
        } else if let Some(prev_row) = self.prev_visible_row_before(row) {
            let prev_len = self.buffer.line_grapheme_len(prev_row);
            self.buffer.set_cursor(prev_row, prev_len);
        }
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.reset_cursor_goal_col();
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn cursor_right(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let line_len = self.buffer.line_grapheme_len(row);
        if col < line_len {
            self.buffer.set_cursor(row, col + 1);
        } else if let Some(next_row) = self.next_visible_row_after(row) {
            self.buffer.set_cursor(next_row, 0);
        }
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.reset_cursor_goal_col();
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn cursor_up(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let goal_col = self.cursor_goal_col_or_current();
        let Some(prev_row) = self.prev_visible_row_before(row) else {
            return false;
        };
        let prev = (row, col);
        let new_len = self.buffer.line_grapheme_len(prev_row);
        self.buffer.set_cursor(prev_row, goal_col.min(new_len));
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.set_cursor_goal_col(goal_col);
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn cursor_down(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let goal_col = self.cursor_goal_col_or_current();
        let Some(next_row) = self.next_visible_row_after(row) else {
            return false;
        };
        let prev = (row, col);
        let new_len = self.buffer.line_grapheme_len(next_row);
        self.buffer.set_cursor(next_row, goal_col.min(new_len));
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.set_cursor_goal_col(goal_col);
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn cursor_word_left(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);

        if col == 0 {
            if let Some(prev_row) = self.prev_visible_row_before(row) {
                let prev_len = self.buffer.line_grapheme_len(prev_row);
                self.buffer.set_cursor(prev_row, prev_len);
            }
            let changed = self.buffer.cursor() != prev;
            if changed {
                self.reset_cursor_goal_col();
                self.buffer.update_selection_cursor(self.buffer.cursor());
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            }
            return changed;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return false,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();

        let mut pos = col.min(graphemes.len());

        while pos > 0 && graphemes[pos - 1].chars().all(|c| c.is_whitespace()) {
            pos -= 1;
        }

        while pos > 0
            && !graphemes[pos - 1]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos -= 1;
        }

        self.buffer.set_cursor(row, pos);
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.reset_cursor_goal_col();
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn cursor_word_right(&mut self, tab_size: u8) -> bool {
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let line_len = self.buffer.line_grapheme_len(row);

        if col >= line_len {
            if let Some(next_row) = self.next_visible_row_after(row) {
                self.buffer.set_cursor(next_row, 0);
            }
            let changed = self.buffer.cursor() != prev;
            if changed {
                self.reset_cursor_goal_col();
                self.buffer.update_selection_cursor(self.buffer.cursor());
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            }
            return changed;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return false,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();
        let len = graphemes.len();

        let mut pos = col;

        while pos < len
            && !graphemes[pos]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos += 1;
        }

        while pos < len
            && graphemes[pos]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos += 1;
        }

        self.buffer.set_cursor(row, pos.min(line_len));
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.reset_cursor_goal_col();
            self.buffer.update_selection_cursor(self.buffer.cursor());
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn select_all(&mut self, tab_size: u8) -> bool {
        let last_line = self.buffer.len_lines().saturating_sub(1);
        let last_line = self
            .prev_visible_row_at_or_before(last_line)
            .unwrap_or(last_line);
        let last_col = self.buffer.line_grapheme_len(last_line);

        let mut selection = Selection::new((0, 0), Granularity::Char);
        selection.update_cursor((last_line, last_col), self.buffer.rope());
        self.buffer.set_selection(Some(selection));
        self.buffer.set_cursor(last_line, last_col);
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        true
    }

    fn ensure_selection(&mut self) {
        if self.buffer.selection().is_none() {
            let pos = self.buffer.cursor();
            self.buffer
                .set_selection(Some(Selection::new(pos, Granularity::Char)));
        }
    }

    fn extend_selection_left(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let new_pos = if col > 0 {
            (row, col - 1)
        } else if let Some(prev_row) = self.prev_visible_row_before(row) {
            let prev_len = self.buffer.line_grapheme_len(prev_row);
            (prev_row, prev_len)
        } else {
            (row, col)
        };
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.reset_cursor_goal_col();
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_right(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let line_len = self.buffer.line_grapheme_len(row);
        let new_pos = if col < line_len {
            (row, col + 1)
        } else if let Some(next_row) = self.next_visible_row_after(row) {
            (next_row, 0)
        } else {
            (row, col)
        };
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.reset_cursor_goal_col();
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_up(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let goal_col = self.cursor_goal_col_or_current();
        let Some(prev_row) = self.prev_visible_row_before(row) else {
            return false;
        };
        let prev = (row, col);
        let new_len = self.buffer.line_grapheme_len(prev_row);
        let new_pos = (prev_row, goal_col.min(new_len));
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.set_cursor_goal_col(goal_col);
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_down(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let goal_col = self.cursor_goal_col_or_current();
        let Some(next_row) = self.next_visible_row_after(row) else {
            return false;
        };
        let prev = (row, col);
        let new_len = self.buffer.line_grapheme_len(next_row);
        let new_pos = (next_row, goal_col.min(new_len));
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.set_cursor_goal_col(goal_col);
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_to_line_start(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let new_pos = (row, 0);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.reset_cursor_goal_col();
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_to_line_end(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let len = self.buffer.line_grapheme_len(row);
        let new_pos = (row, len);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.reset_cursor_goal_col();
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_word_left(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);

        if col == 0 {
            if let Some(prev_row) = self.prev_visible_row_before(row) {
                let prev_len = self.buffer.line_grapheme_len(prev_row);
                let new_pos = (prev_row, prev_len);
                self.buffer.update_selection_cursor(new_pos);
                self.buffer.set_cursor(new_pos.0, new_pos.1);
            }
            let changed = self.buffer.cursor() != prev;
            if changed {
                self.reset_cursor_goal_col();
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            }
            return changed;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return false,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();

        let mut pos = col.min(graphemes.len());

        while pos > 0 && graphemes[pos - 1].chars().all(|c| c.is_whitespace()) {
            pos -= 1;
        }

        while pos > 0
            && !graphemes[pos - 1]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos -= 1;
        }

        let new_pos = (row, pos);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.reset_cursor_goal_col();
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn extend_selection_word_right(&mut self, tab_size: u8) -> bool {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let prev = (row, col);
        let line_len = self.buffer.line_grapheme_len(row);

        if col >= line_len {
            if let Some(next_row) = self.next_visible_row_after(row) {
                let new_pos = (next_row, 0);
                self.buffer.update_selection_cursor(new_pos);
                self.buffer.set_cursor(new_pos.0, new_pos.1);
            }
            let changed = self.buffer.cursor() != prev;
            if changed {
                self.reset_cursor_goal_col();
                viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            }
            return changed;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return false,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();
        let len = graphemes.len();

        let mut pos = col;

        while pos < len
            && !graphemes[pos]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos += 1;
        }

        while pos < len
            && graphemes[pos]
                .chars()
                .all(|c| c.is_whitespace() || is_word_boundary_char(c))
        {
            pos += 1;
        }

        let new_pos = (row, pos.min(line_len));
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
        let changed = self.buffer.cursor() != prev;
        if changed {
            self.reset_cursor_goal_col();
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        }
        changed
    }

    fn commit_op(&mut self, op: EditOp, tab_size: u8) {
        self.snippet_apply_edit(&op);
        self.apply_syntax_edit(&op);
        self.invalidate_semantic_highlight_on_edit(&op);
        self.last_edit_op_id = Some(op.id);
        self.reset_cursor_goal_col();
        self.history.push(op, self.buffer.rope());
        self.dirty = true;
        viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
        self.bump_version();
    }

    pub(super) fn apply_edit_op(&mut self, op: EditOp, tab_size: u8) {
        self.commit_op(op, tab_size);
    }

    fn insert_brace_pair_op(&mut self, parent: OpId) -> EditOp {
        let (row, col) = self.buffer.cursor();
        let cursor_char_offset = self.buffer.cursor_char_offset();
        self.buffer.insert_str_op_with_cursor_after_char_offset(
            "{}",
            (row, col.saturating_add(1)),
            cursor_char_offset.saturating_add(1),
            parent,
        )
    }

    fn try_skip_closing(&mut self, c: char, tab_size: u8) -> bool {
        if !matches!(c, ')' | ']' | '}' | '"' | '\'') {
            return false;
        }

        let cursor_char_offset = self.buffer.cursor_char_offset();
        let rope = self.buffer.rope();
        if cursor_char_offset >= rope.len_chars() {
            return false;
        }

        let next = rope.char(cursor_char_offset);
        if next != c {
            return false;
        }

        self.cursor_right(tab_size)
    }

    fn insert_pair_op(&mut self, open: char, close: char, parent: OpId) -> EditOp {
        let (row, col) = self.buffer.cursor();
        let cursor_char_offset = self.buffer.cursor_char_offset();

        let mut text = String::with_capacity(2);
        text.push(open);
        text.push(close);

        self.buffer.insert_str_op_with_cursor_after_char_offset(
            &text,
            (row, col.saturating_add(1)),
            cursor_char_offset.saturating_add(1),
            parent,
        )
    }

    fn insert_newline_with_indent_op(&mut self, tab_size: u8, parent: OpId) -> EditOp {
        let row = self.buffer.cursor().0;
        let cursor_char_offset = self.buffer.cursor_char_offset();
        let in_string_or_comment = self.in_string_or_comment();
        let rope = self.buffer.rope();
        let line_start = rope.line_to_char(row);
        let before_cursor = slice_to_cow(rope.slice(line_start..cursor_char_offset));
        let before_cursor = before_cursor.as_ref();

        let mut indent = String::new();
        for ch in before_cursor.chars() {
            if ch == ' ' || ch == '\t' {
                indent.push(ch);
            } else {
                break;
            }
        }

        let trimmed = before_cursor.trim_end_matches([' ', '\t']);
        let language = self.language();
        if supports_brace_electric_enter(language)
            && trimmed.ends_with('{')
            && !in_string_or_comment
        {
            indent.push_str(&" ".repeat(tab_size as usize));
        }
        if supports_python_colon_indent(language) && trimmed.ends_with(':') && !in_string_or_comment
        {
            indent.push_str(&" ".repeat(tab_size as usize));
        }

        let mut text = String::with_capacity(1 + indent.len());
        text.push('\n');
        text.push_str(&indent);

        self.buffer.insert_str_op(&text, parent)
    }

    fn expand_empty_pair_op(
        &mut self,
        open: &str,
        close: &str,
        tab_size: u8,
        parent: OpId,
    ) -> Option<EditOp> {
        if self.buffer.has_selection() {
            return None;
        }

        let (row, col) = self.buffer.cursor();
        let plan = empty_pair_replace_plan(&self.buffer, (row, col), open, close, tab_size)?;
        Some(self.buffer.replace_range_op(
            plan.start_char,
            plan.end_char,
            &plan.inserted,
            plan.cursor_after,
            plan.cursor_after_char_offset,
            parent,
        ))
    }

    pub fn insert_text(&mut self, text: &str, tab_size: u8) -> bool {
        const PASTE_MAX_SIZE: usize = 10 * 1024 * 1024;
        if text.is_empty() || text.len() > PASTE_MAX_SIZE {
            return false;
        }

        if self.is_multi_cursor() {
            return self.insert_text_multi_cursor(text, tab_size);
        }

        if self.buffer.has_selection() {
            let parent = self.history.head();
            let Some(selection) = self.buffer.selection() else {
                return false;
            };
            let (start_pos, end_pos) = selection.range();
            let start_char = self.buffer.pos_to_char(start_pos);
            let end_char = self.buffer.pos_to_char(end_pos);
            let op = self
                .buffer
                .replace_range_op_auto_cursor(start_char, end_char, text, parent);
            self.commit_op(op, tab_size);
            return true;
        }

        let parent = self.history.head();
        let op = self.buffer.insert_str_op(text, parent);
        self.commit_op(op, tab_size);
        true
    }

    fn insert_text_multi_cursor(&mut self, text: &str, tab_size: u8) -> bool {
        let merged = cursor_set::merge_overlapping(
            self.buffer.cursor(),
            self.buffer.selection(),
            &mut self.secondary_cursors,
        );
        self.buffer
            .set_cursor(merged.primary_pos.0, merged.primary_pos.1);
        self.buffer.set_selection(merged.primary_selection);

        let primary_before = self.buffer.cursor();
        let extra_before = cursor_set::secondary_cursor_positions(&self.secondary_cursors);
        let parent = self.history.head();

        let mut records: Vec<CursorEditRecord> =
            Vec::with_capacity(self.secondary_cursors.len().saturating_add(1));
        let primary_char = self.buffer.cursor_char_offset();
        records.push(CursorEditRecord {
            slot: CursorSlot::Primary,
            cursor_char: primary_char,
            selection: self.selection_chars(self.buffer.selection()),
            goal_col: self.cursor_goal_col,
        });
        for (idx, cursor) in self.secondary_cursors.iter().enumerate() {
            records.push(CursorEditRecord {
                slot: CursorSlot::Secondary(idx),
                cursor_char: self.buffer.pos_to_char(cursor.pos),
                selection: self.selection_chars(cursor.selection.as_ref()),
                goal_col: cursor.goal_col,
            });
        }

        let cursor_count = records.len();
        let paste_lines: Vec<&str> = text.split('\n').collect();
        let distribute = !text.ends_with('\n') && paste_lines.len() == cursor_count;
        let mut insertions: Vec<&str> = vec![text; cursor_count];
        if distribute {
            let mut dist_order: Vec<usize> = (0..cursor_count).collect();
            dist_order.sort_by(|&a, &b| {
                let a_key = self.paste_start_char_for_record(&records[a]);
                let b_key = self.paste_start_char_for_record(&records[b]);
                a_key
                    .cmp(&b_key)
                    .then_with(|| records[a].cursor_char.cmp(&records[b].cursor_char))
            });
            for (line_idx, record_idx) in dist_order.into_iter().enumerate() {
                insertions[record_idx] = paste_lines[line_idx]
                    .strip_suffix('\r')
                    .unwrap_or(paste_lines[line_idx]);
            }
        }

        let mut order: Vec<usize> = (0..cursor_count).collect();
        order.sort_by(|&a, &b| {
            let a_key = self.paste_start_char_for_record(&records[a]);
            let b_key = self.paste_start_char_for_record(&records[b]);
            b_key
                .cmp(&a_key)
                .then_with(|| records[b].cursor_char.cmp(&records[a].cursor_char))
                .then_with(|| match (records[a].slot, records[b].slot) {
                    (CursorSlot::Primary, CursorSlot::Primary) => std::cmp::Ordering::Equal,
                    (CursorSlot::Primary, _) => std::cmp::Ordering::Greater,
                    (_, CursorSlot::Primary) => std::cmp::Ordering::Less,
                    (CursorSlot::Secondary(a_idx), CursorSlot::Secondary(b_idx)) => {
                        b_idx.cmp(&a_idx)
                    }
                })
        });

        let mut batch_edits: Vec<BatchEdit> = Vec::new();
        for idx in order {
            let record = records[idx].clone();
            self.load_cursor_record(&record);

            let inserted = insertions[idx];
            let (start, end) = record
                .selection
                .map(|sel| {
                    (
                        sel.anchor_char.min(sel.cursor_char),
                        sel.anchor_char.max(sel.cursor_char),
                    )
                })
                .unwrap_or((record.cursor_char, record.cursor_char));

            let op = self
                .buffer
                .replace_range_op_adjust_cursor(start, end, inserted, parent);

            records[idx].cursor_char = self.buffer.cursor_char_offset();
            records[idx].selection = self.selection_chars(self.buffer.selection());
            records[idx].goal_col = self.cursor_goal_col;

            batch_edits.push(batch_edit_from_op(&op));
            apply_edit_to_records(&mut records, idx, &op);
        }

        let Some(primary_record) = records.iter().find(|r| r.slot == CursorSlot::Primary) else {
            return false;
        };

        self.cursor_goal_col = primary_record.goal_col;
        let primary_pos = self
            .buffer
            .cursor_pos_from_char_offset(primary_record.cursor_char);
        self.buffer.set_cursor(primary_pos.0, primary_pos.1);
        self.buffer
            .set_cursor_char_offset_cache(primary_record.cursor_char);
        self.buffer
            .set_selection(self.selection_from_chars(primary_record.selection));

        self.secondary_cursors = records
            .iter()
            .filter_map(|r| match r.slot {
                CursorSlot::Secondary(_) => {
                    let pos = self.buffer.cursor_pos_from_char_offset(r.cursor_char);
                    Some(SecondaryCursor {
                        pos,
                        selection: self.selection_from_chars(r.selection),
                        goal_col: r.goal_col,
                    })
                }
                CursorSlot::Primary => None,
            })
            .collect();

        let merged = cursor_set::merge_overlapping(
            self.buffer.cursor(),
            self.buffer.selection(),
            &mut self.secondary_cursors,
        );
        self.buffer
            .set_cursor(merged.primary_pos.0, merged.primary_pos.1);
        self.buffer.set_selection(merged.primary_selection);

        batch_edits.sort_by(|a, b| b.start.cmp(&a.start));
        let extra_after = cursor_set::secondary_cursor_positions(&self.secondary_cursors);

        let batch_op = EditOp {
            id: OpId::new(),
            parent,
            kind: OpKind::Batch { edits: batch_edits },
            cursor_before: primary_before,
            cursor_after: self.buffer.cursor(),
            extra_cursors_before: Some(extra_before),
            extra_cursors_after: Some(extra_after),
        };

        self.commit_op(batch_op, tab_size);
        true
    }

    fn delete_selection(&mut self, tab_size: u8) -> bool {
        let parent = self.history.head();
        if let Some(op) = self.buffer.delete_selection_op(parent) {
            self.commit_op(op, tab_size);
            return true;
        }
        self.buffer.clear_selection();
        false
    }

    fn undo(&mut self, tab_size: u8) -> bool {
        self.cancel_snippet_session();
        if let Some(result) = self.history.undo(self.buffer.rope()) {
            self.buffer.set_rope(result.rope);
            self.buffer.set_cursor(result.cursor.0, result.cursor.1);
            if let Some(secondary) = result.secondary_cursors {
                self.secondary_cursors = secondary
                    .into_iter()
                    .map(|pos| crate::models::SecondaryCursor {
                        pos,
                        selection: None,
                        goal_col: None,
                    })
                    .collect();
            }
            self.reset_cursor_goal_col();
            self.reparse_syntax();
            self.dirty = self.history.is_dirty();
            self.last_edit_op_id = None;
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            self.bump_version();
            return true;
        }
        false
    }

    fn redo(&mut self, tab_size: u8) -> bool {
        self.cancel_snippet_session();
        if let Some(result) = self.history.redo(self.buffer.rope()) {
            self.buffer.set_rope(result.rope);
            self.buffer.set_cursor(result.cursor.0, result.cursor.1);
            if let Some(secondary) = result.secondary_cursors {
                self.secondary_cursors = secondary
                    .into_iter()
                    .map(|pos| crate::models::SecondaryCursor {
                        pos,
                        selection: None,
                        goal_col: None,
                    })
                    .collect();
            }
            self.reset_cursor_goal_col();
            self.reparse_syntax();
            self.dirty = self.history.is_dirty();
            self.last_edit_op_id = None;
            viewport::clamp_and_follow(&mut self.viewport, &self.buffer, tab_size);
            self.bump_version();
            return true;
        }
        false
    }

    fn copy(&mut self) -> (bool, Vec<crate::kernel::Effect>) {
        use crate::kernel::Effect;

        if !self.is_multi_cursor() {
            let Some(text) = self.buffer.get_selection_text() else {
                return (false, Vec::new());
            };
            return (false, vec![Effect::SetClipboardText(text)]);
        }

        let mut pieces: Vec<(usize, String)> = Vec::new();
        let rope = self.buffer.rope();

        if let Some(sel) = self.buffer.selection().filter(|s| !s.is_empty()) {
            let (start_pos, end_pos) = sel.range();
            let start_char = self.buffer.pos_to_char(start_pos);
            let end_char = self.buffer.pos_to_char(end_pos);
            pieces.push((start_char, rope.slice(start_char..end_char).to_string()));
        }
        for cursor in &self.secondary_cursors {
            let Some(sel) = cursor.selection.as_ref().filter(|s| !s.is_empty()) else {
                continue;
            };
            let (start_pos, end_pos) = sel.range();
            let start_char = self.buffer.pos_to_char(start_pos);
            let end_char = self.buffer.pos_to_char(end_pos);
            pieces.push((start_char, rope.slice(start_char..end_char).to_string()));
        }

        if pieces.is_empty() {
            return (false, Vec::new());
        }

        pieces.sort_by_key(|(start, _)| *start);
        let mut out = String::new();
        for (idx, (_, piece)) in pieces.into_iter().enumerate() {
            if idx > 0 {
                out.push('\n');
            }
            out.push_str(&piece);
        }

        (false, vec![Effect::SetClipboardText(out)])
    }

    fn cut(&mut self, config: &EditorConfig) -> (bool, Vec<crate::kernel::Effect>) {
        use crate::kernel::Effect;
        let tab_size = config.tab_size;

        if !self.is_multi_cursor() {
            let Some(text) = self.buffer.get_selection_text() else {
                return (false, Vec::new());
            };
            let changed = self.delete_selection(tab_size);
            return (changed, vec![Effect::SetClipboardText(text)]);
        }

        let merged = cursor_set::merge_overlapping(
            self.buffer.cursor(),
            self.buffer.selection(),
            &mut self.secondary_cursors,
        );
        self.buffer
            .set_cursor(merged.primary_pos.0, merged.primary_pos.1);
        self.buffer.set_selection(merged.primary_selection);

        let (copied, eff) = self.copy();
        let copied = copied || !eff.is_empty();
        if !copied {
            return (false, Vec::new());
        }

        let changed = self.execute_on_all_cursors(Command::DeleteSelection, config);
        (changed, eff)
    }

    pub fn replace_current_match(&mut self, m: &Match, replace: &str, tab_size: u8) -> bool {
        let rope = self.buffer.rope();
        if m.start >= m.end || m.start >= rope.len_bytes() {
            return false;
        }

        let start_char = rope.byte_to_char(m.start);
        let end_char = rope.byte_to_char(m.end.min(rope.len_bytes()));
        if start_char >= end_char {
            return false;
        }

        let parent = self.history.head();
        let op = self
            .buffer
            .replace_range_op_auto_cursor(start_char, end_char, replace, parent);
        self.commit_op(op, tab_size);
        true
    }

    pub fn on_saved(&mut self) {
        self.history.on_save(self.buffer.rope());
        self.dirty = false;
        self.disk_state = super::state::DiskState::InSync;
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/editor/edit.rs"]
mod tests;
