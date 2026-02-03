use crate::core::Command;
use crate::kernel::editor::EditorTabState;
use crate::kernel::services::ports::LspCompletionItem;
use crate::kernel::state::CompletionPopupState;
use crate::kernel::EditorAction;
use crate::models::{Granularity, Selection};

pub(super) fn should_close_completion_on_editor_action(action: &EditorAction) -> bool {
    match action {
        EditorAction::SetViewportSize { .. } => false,
        EditorAction::SearchStarted { .. } | EditorAction::SearchMessage { .. } => false,
        _ => true,
    }
}

pub(super) fn should_close_completion_on_command(cmd: &Command) -> bool {
    match cmd {
        Command::LspCompletion => false,
        Command::LspSemanticTokens | Command::LspInlayHints | Command::LspFoldingRange => false,
        Command::InsertChar(ch) => !completion_keeps_open_on_inserted_char(*ch),
        Command::DeleteBackward | Command::DeleteForward | Command::DeleteSelection => false,
        _ => true,
    }
}

fn completion_keeps_open_on_inserted_char(inserted: char) -> bool {
    inserted.is_alphanumeric() || inserted == '_' || inserted == '.'
}

pub(super) fn sort_completion_items(items: &mut Vec<LspCompletionItem>) {
    items.sort_by(|a, b| {
        let a_key = a.sort_text.as_deref().unwrap_or(a.label.as_str());
        let b_key = b.sort_text.as_deref().unwrap_or(b.label.as_str());
        a_key
            .cmp(b_key)
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.detail.cmp(&b.detail))
    });
}

pub(super) fn filtered_completion_items(
    tab: &EditorTabState,
    items: &[LspCompletionItem],
) -> Vec<LspCompletionItem> {
    if items.is_empty() {
        return Vec::new();
    }

    let prefix = completion_prefix_at_cursor(tab);
    if prefix.is_empty() {
        return items.to_vec();
    }

    if !items
        .iter()
        .any(|item| completion_item_matches_prefix(item, &prefix))
    {
        return items.to_vec();
    }

    items
        .iter()
        .filter(|item| completion_item_matches_prefix(item, &prefix))
        .cloned()
        .collect()
}

fn same_completion_item_ids(a: &[LspCompletionItem], b: &[LspCompletionItem]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (a, b) in a.iter().zip(b.iter()) {
        if a.id != b.id {
            return false;
        }
    }
    true
}

pub(super) fn sync_completion_items_from_cache(
    completion: &mut CompletionPopupState,
    tab: &EditorTabState,
) -> bool {
    if completion.all_items.is_empty() {
        return false;
    }

    let selected_id = completion
        .items
        .get(completion.selected)
        .map(|item| item.id);

    let new_items = filtered_completion_items(tab, &completion.all_items);
    if new_items.is_empty() {
        return false;
    }

    let changed = !same_completion_item_ids(&completion.items, &new_items);
    completion.items = new_items;
    completion.selected = selected_id
        .and_then(|id| completion.items.iter().position(|item| item.id == id))
        .unwrap_or(0)
        .min(completion.items.len().saturating_sub(1));
    completion.visible = true;
    changed
}

fn completion_item_matches_prefix(item: &LspCompletionItem, prefix: &str) -> bool {
    let candidate = item.filter_text.as_deref().unwrap_or(item.label.as_str());
    starts_with_ignore_ascii_case(candidate, prefix)
}

fn starts_with_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle))
}

pub(super) fn completion_prefix_at_cursor(tab: &EditorTabState) -> String {
    let rope = tab.buffer.rope();
    let (start_char, end_char) = completion_prefix_bounds_at_cursor(tab);
    rope.slice(start_char..end_char).to_string()
}

fn completion_prefix_bounds_at_cursor(tab: &EditorTabState) -> (usize, usize) {
    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col));
    let rope = tab.buffer.rope();
    let end_char = cursor_char_offset.min(rope.len_chars());

    let mut start_char = end_char;
    while start_char > 0 {
        let ch = rope.char(start_char - 1);
        if ch.is_ascii_alphanumeric() || ch == '_' {
            start_char = start_char.saturating_sub(1);
        } else {
            break;
        }
    }

    (start_char, end_char)
}

pub(super) fn completion_should_keep_open(tab: &EditorTabState) -> bool {
    if tab.is_in_string_or_comment_at_cursor() {
        return false;
    }

    let (start_char, end_char) = completion_prefix_bounds_at_cursor(tab);
    if start_char != end_char {
        return true;
    }

    let rope = tab.buffer.rope();
    if start_char > 0 && rope.char(start_char - 1) == '.' {
        return true;
    }
    if start_char >= 2 && rope.char(start_char - 1) == ':' && rope.char(start_char - 2) == ':' {
        return true;
    }

    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SnippetExpansion {
    pub(super) text: String,
    pub(super) cursor: Option<usize>,
    pub(super) selection: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CompletionInsertion {
    pub(super) text: String,
    pub(super) cursor: Option<usize>,
    pub(super) selection: Option<(usize, usize)>,
}

impl CompletionInsertion {
    pub(super) fn from_plain_text(text: String) -> Self {
        let cursor = text
            .strip_suffix("()")
            .map(|prefix| prefix.chars().count().saturating_add(1));
        Self {
            text,
            cursor,
            selection: None,
        }
    }

    pub(super) fn from_snippet(snippet: &str) -> Self {
        let expanded = expand_snippet(snippet);
        Self {
            text: expanded.text,
            cursor: expanded.cursor,
            selection: expanded.selection,
        }
    }

    pub(super) fn has_cursor_or_selection(&self) -> bool {
        self.cursor.is_some() || self.selection.is_some()
    }
}

pub(super) fn apply_completion_insertion_cursor(
    tab: &mut EditorTabState,
    insertion: &CompletionInsertion,
    tab_size: u8,
) {
    if !insertion.has_cursor_or_selection() {
        return;
    }

    let inserted_chars = insertion.text.chars().count();
    if inserted_chars == 0 {
        return;
    }

    let cursor_end = tab.buffer.cursor_char_offset();
    if cursor_end < inserted_chars {
        return;
    }

    let start_char = cursor_end.saturating_sub(inserted_chars);
    let rope = tab.buffer.rope();
    let end_char = cursor_end.min(rope.len_chars());
    let start_char = start_char.min(end_char);
    if rope.slice(start_char..end_char).to_string() != insertion.text {
        return;
    }

    tab.viewport.follow_cursor = true;

    if let Some((mut sel_start_rel, mut sel_end_rel)) = insertion.selection {
        if sel_start_rel > sel_end_rel {
            std::mem::swap(&mut sel_start_rel, &mut sel_end_rel);
        }
        let sel_start_char = start_char.saturating_add(sel_start_rel);
        let sel_end_char = start_char.saturating_add(sel_end_rel);
        let sel_start = tab.buffer.cursor_pos_from_char_offset(sel_start_char);
        let sel_end = tab.buffer.cursor_pos_from_char_offset(sel_end_char);

        tab.buffer
            .set_selection(Some(Selection::new(sel_start, Granularity::Char)));
        tab.buffer.update_selection_cursor(sel_end);
        tab.buffer.set_cursor(sel_end.0, sel_end.1);
    } else if let Some(cursor_rel) = insertion.cursor {
        let cursor_char = start_char.saturating_add(cursor_rel);
        let cursor = tab.buffer.cursor_pos_from_char_offset(cursor_char);
        tab.buffer.clear_selection();
        tab.buffer.set_cursor(cursor.0, cursor.1);
    }

    crate::kernel::editor::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
}

pub(super) fn expand_snippet(snippet: &str) -> SnippetExpansion {
    let mut out = String::with_capacity(snippet.len());
    let mut out_chars = 0usize;

    let mut best_placeholder: Option<(u32, usize, usize)> = None;
    let mut best_tabstop: Option<(u32, usize)> = None;
    let mut final_cursor: Option<usize> = None;

    let mut it = snippet.chars().peekable();

    while let Some(ch) = it.next() {
        match ch {
            '\\' => match it.next() {
                Some(next) => {
                    out.push(next);
                    out_chars = out_chars.saturating_add(1);
                }
                None => {
                    out.push('\\');
                    out_chars = out_chars.saturating_add(1);
                }
            },
            '$' => match it.peek().copied() {
                Some('{') => {
                    let _ = it.next();
                    let mut content = String::new();
                    let mut depth = 0usize;
                    while let Some(c) = it.next() {
                        match c {
                            '{' => {
                                depth = depth.saturating_add(1);
                                content.push(c);
                            }
                            '}' => {
                                if depth == 0 {
                                    break;
                                }
                                depth = depth.saturating_sub(1);
                                content.push(c);
                            }
                            _ => content.push(c),
                        }
                    }

                    let digits = content
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect::<String>();
                    let index: Option<u32> = if digits.is_empty() {
                        None
                    } else {
                        digits.parse().ok()
                    };

                    if let Some(index) = index {
                        let rest = content.get(digits.len()..).unwrap_or_default();
                        let (inserted, inserted_is_placeholder) = if let Some((_, text)) =
                            rest.split_once(':')
                        {
                            (text.to_string(), true)
                        } else if let (Some(start), Some(end)) = (rest.find('|'), rest.rfind('|')) {
                            if end > start.saturating_add(1) {
                                let opts = &rest[start + 1..end];
                                let first = opts.split(',').next().unwrap_or_default().to_string();
                                (first, true)
                            } else {
                                (String::new(), false)
                            }
                        } else {
                            (String::new(), false)
                        };

                        if !inserted.is_empty() {
                            let start = out_chars;
                            out.push_str(&inserted);
                            let inserted_chars = inserted.chars().count();
                            out_chars = out_chars.saturating_add(inserted_chars);
                            let end = out_chars;

                            if inserted_is_placeholder && index > 0 {
                                let replace = best_placeholder
                                    .as_ref()
                                    .is_none_or(|(best_idx, _, _)| index < *best_idx);
                                if replace {
                                    best_placeholder = Some((index, start, end));
                                }
                            }
                        } else if index == 0 {
                            final_cursor = Some(out_chars);
                        } else if index > 0 {
                            let replace = best_tabstop
                                .as_ref()
                                .is_none_or(|(best_idx, _)| index < *best_idx);
                            if replace {
                                best_tabstop = Some((index, out_chars));
                            }
                        }

                        continue;
                    }
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut num: u32 = 0;
                    while it.peek().is_some_and(|c| c.is_ascii_digit()) {
                        let digit = it.next().unwrap();
                        num = num
                            .saturating_mul(10)
                            .saturating_add((digit as u32).saturating_sub('0' as u32));
                    }
                    if num == 0 {
                        final_cursor = Some(out_chars);
                    } else {
                        let replace = best_tabstop
                            .as_ref()
                            .is_none_or(|(best_idx, _)| num < *best_idx);
                        if replace {
                            best_tabstop = Some((num, out_chars));
                        }
                    }
                }
                _ => {
                    out.push('$');
                    out_chars = out_chars.saturating_add(1);
                }
            },
            _ => {
                out.push(ch);
                out_chars = out_chars.saturating_add(1);
            }
        }
    }

    let (selection, cursor) = if let Some((_idx, start, end)) = best_placeholder {
        (Some((start, end)), Some(end))
    } else if let Some((_idx, pos)) = best_tabstop {
        (None, Some(pos))
    } else {
        (None, final_cursor)
    };

    SnippetExpansion {
        text: out,
        cursor,
        selection,
    }
}

pub(super) fn completion_triggered_by_insert(
    tab: &EditorTabState,
    inserted: char,
    triggers: &[char],
) -> bool {
    if triggers.is_empty() {
        return match inserted {
            '.' => true,
            ':' => {
                let (row, col) = tab.buffer.cursor();
                let cursor_char_offset = tab.buffer.pos_to_char((row, col));
                let rope = tab.buffer.rope();
                let cursor_char_offset = cursor_char_offset.min(rope.len_chars());
                if cursor_char_offset < 2 {
                    return false;
                }
                rope.char(cursor_char_offset - 1) == ':' && rope.char(cursor_char_offset - 2) == ':'
            }
            _ => false,
        };
    }

    match inserted {
        ':' => {
            if !triggers.contains(&':') {
                return false;
            }
            let (row, col) = tab.buffer.cursor();
            let cursor_char_offset = tab.buffer.pos_to_char((row, col));
            let rope = tab.buffer.rope();
            let cursor_char_offset = cursor_char_offset.min(rope.len_chars());
            if cursor_char_offset < 2 {
                return false;
            }
            rope.char(cursor_char_offset - 1) == ':' && rope.char(cursor_char_offset - 2) == ':'
        }
        ch => triggers.contains(&ch),
    }
}

pub(super) fn signature_help_triggered_by_insert(inserted: char, triggers: &[char]) -> bool {
    if triggers.is_empty() {
        matches!(inserted, '(' | ',')
    } else {
        triggers.contains(&inserted)
    }
}

pub(super) fn signature_help_closed_by_insert(inserted: char) -> bool {
    matches!(inserted, ')')
}

pub(super) fn signature_help_should_keep_open(tab: &EditorTabState) -> bool {
    if tab.is_in_string_or_comment_at_cursor() {
        return false;
    }

    let rope = tab.buffer.rope();
    let (row, col) = tab.buffer.cursor();
    let cursor_char_offset = tab.buffer.pos_to_char((row, col)).min(rope.len_chars());
    let start = cursor_char_offset.saturating_sub(4096);

    let mut depth: usize = 0;
    let mut idx = cursor_char_offset;
    while idx > start {
        idx = idx.saturating_sub(1);
        let ch = rope.char(idx);
        if ch != '(' && ch != ')' {
            continue;
        }

        if tab.is_in_string_or_comment_at_char(idx) {
            continue;
        }

        match ch {
            ')' => depth = depth.saturating_add(1),
            '(' => {
                if depth == 0 {
                    return true;
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    false
}
