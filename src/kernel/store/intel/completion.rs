use crate::kernel::editor::EditorTabState;
use crate::kernel::editor::SnippetTabstop;
use crate::kernel::language::LanguageId;
use crate::kernel::services::ports::{
    LspCompletionItem, LspInsertTextFormat, LspPositionEncoding, LspRange,
};
use crate::kernel::state::CompletionPopupState;
use crate::kernel::EditorAction;
use crate::models::{Granularity, Selection};
use rustc_hash::FxHashMap;

use super::completion_rank::CompletionRanker;
use super::completion_strategy::CompletionStrategy;

pub(in crate::kernel::store) fn should_close_completion_on_editor_action(
    action: &EditorAction,
) -> bool {
    !matches!(
        action,
        EditorAction::SetViewportSize { .. }
            | EditorAction::SearchStarted { .. }
            | EditorAction::SearchMessage { .. }
            | EditorAction::ApplySyntaxHighlightPatches { .. }
    )
}

pub(in crate::kernel::store) fn sort_completion_items(
    items: &mut [LspCompletionItem],
    ranker: &CompletionRanker,
    language: Option<LanguageId>,
) {
    let mut score_by_id = FxHashMap::default();
    for item in items.iter() {
        score_by_id.insert(item.id, ranker.score(language, &item.label, item.kind));
    }

    items.sort_by(|a, b| {
        let a_score = score_by_id.get(&a.id).copied().unwrap_or(0.0);
        let b_score = score_by_id.get(&b.id).copied().unwrap_or(0.0);
        // Higher frequency first.
        b_score
            .partial_cmp(&a_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_key = a.sort_text.as_deref().unwrap_or(a.label.as_str());
                let b_key = b.sort_text.as_deref().unwrap_or(b.label.as_str());
                a_key
                    .cmp(b_key)
                    .then_with(|| a.label.cmp(&b.label))
                    .then_with(|| a.detail.cmp(&b.detail))
            })
    });
}

pub(in crate::kernel::store) fn filtered_completion_indices(
    tab: &EditorTabState,
    items: &[LspCompletionItem],
    strategy: &dyn CompletionStrategy,
) -> Vec<usize> {
    if items.is_empty() {
        return Vec::new();
    }

    let prefix = completion_prefix_at_cursor(tab, strategy);
    if prefix.is_empty() {
        return (0..items.len()).collect();
    }

    let mut filtered = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        if completion_item_matches_prefix(item, &prefix) {
            filtered.push(idx);
        }
    }

    if filtered.is_empty() {
        (0..items.len()).collect()
    } else {
        filtered
    }
}

fn collect_matching_indices(
    all_items: &[LspCompletionItem],
    prefix: &str,
    base_indices: impl Iterator<Item = usize>,
) -> Vec<usize> {
    let mut matched = Vec::new();
    for idx in base_indices {
        if completion_item_matches_prefix(&all_items[idx], prefix) {
            matched.push(idx);
        }
    }
    matched
}

fn selected_visible_index(
    completion: &CompletionPopupState,
    visible_indices: &[usize],
    selected_id: Option<u64>,
) -> usize {
    let Some(id) = selected_id else {
        return 0;
    };

    let Some(item_idx) = completion.index_by_id.get(&id).copied() else {
        return 0;
    };

    if completion
        .all_items
        .get(item_idx)
        .is_none_or(|item| item.id != id)
    {
        return 0;
    }

    match visible_indices.binary_search(&item_idx) {
        Ok(visible_idx) => visible_idx,
        Err(_) => visible_indices
            .iter()
            .position(|idx| *idx == item_idx)
            .unwrap_or(0),
    }
}

fn debug_assert_monotonic_indices(indices: &[usize]) {
    debug_assert!(
        indices.windows(2).all(|pair| pair[0] <= pair[1]),
        "completion visible indices must be monotonic"
    );
}

pub(in crate::kernel::store) fn sync_completion_items_from_cache(
    completion: &mut CompletionPopupState,
    tab: &EditorTabState,
    strategy: &dyn CompletionStrategy,
) -> bool {
    if completion.all_items.is_empty() {
        return false;
    }

    completion.reset_filter_cache_if_source_changed();
    let source_len = completion.all_items.len();
    let source_changed = !completion.filter_cache_valid;
    if source_changed || completion.index_by_id.len() != source_len {
        completion.rebuild_index_by_id();
    }

    let selected_id = completion.selected_item().map(|item| item.id);

    let prefix = completion_prefix_at_cursor(tab, strategy);
    if completion.filter_cache_valid
        && completion.filter_cache_source_len == source_len
        && prefix == completion.filter_cache_prefix
    {
        let cached_indices = completion.filter_cache_indices.as_slice();
        let mut items_changed = source_changed;
        if source_changed || completion.visible_indices.len() != cached_indices.len() {
            completion.visible_indices = cached_indices.to_vec();
            items_changed = true;
        } else {
            debug_assert_eq!(
                completion.visible_indices, cached_indices,
                "cached completion indices diverged from visible indices"
            );
        }

        completion.selected = selected_visible_index(completion, cached_indices, selected_id)
            .min(cached_indices.len().saturating_sub(1));
        completion.visible = true;
        return items_changed;
    }

    let can_use_cached_base = completion.filter_cache_valid
        && completion.filter_cache_source_len == source_len
        && prefix.starts_with(&completion.filter_cache_prefix);

    let mut new_indices = if prefix.is_empty() {
        (0..source_len).collect()
    } else if can_use_cached_base {
        collect_matching_indices(
            &completion.all_items,
            &prefix,
            completion.filter_cache_indices.iter().copied(),
        )
    } else {
        collect_matching_indices(&completion.all_items, &prefix, 0..source_len)
    };

    if new_indices.is_empty() {
        new_indices.extend(0..source_len);
    }
    debug_assert_monotonic_indices(&new_indices);

    let items_changed = source_changed || completion.visible_indices != new_indices;

    if items_changed {
        completion.visible_indices = new_indices.clone();
        debug_assert_monotonic_indices(&completion.visible_indices);
    }

    completion.selected = selected_visible_index(completion, &new_indices, selected_id)
        .min(new_indices.len().saturating_sub(1));

    completion.filter_cache_prefix = prefix;
    completion.filter_cache_indices = new_indices;
    completion.filter_cache_source_len = source_len;
    completion.filter_cache_valid = true;
    completion.visible = true;
    items_changed
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

pub(in crate::kernel::store) fn completion_prefix_at_cursor(
    tab: &EditorTabState,
    strategy: &dyn CompletionStrategy,
) -> String {
    let rope = tab.buffer.rope();
    let (start_char, end_char) = strategy.prefix_bounds(tab);
    rope.slice(start_char..end_char).to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::kernel::store) struct SnippetExpansion {
    pub(in crate::kernel::store) text: String,
    pub(in crate::kernel::store) cursor: Option<usize>,
    pub(in crate::kernel::store) selection: Option<(usize, usize)>,
    pub(in crate::kernel::store) tabstops: Vec<SnippetTabstop>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::kernel::store) struct CompletionInsertion {
    pub(in crate::kernel::store) text: String,
    pub(in crate::kernel::store) cursor: Option<usize>,
    pub(in crate::kernel::store) selection: Option<(usize, usize)>,
    pub(in crate::kernel::store) tabstops: Vec<SnippetTabstop>,
}

impl CompletionInsertion {
    pub(in crate::kernel::store) fn from_plain_text(text: String) -> Self {
        let cursor = text
            .strip_suffix("()")
            .map(|prefix| prefix.chars().count().saturating_add(1));
        Self {
            text,
            cursor,
            selection: None,
            tabstops: Vec::new(),
        }
    }

    pub(in crate::kernel::store) fn from_snippet(snippet: &str) -> Self {
        let expanded = expand_snippet(snippet);
        Self {
            text: expanded.text,
            cursor: expanded.cursor,
            selection: expanded.selection,
            tabstops: expanded.tabstops,
        }
    }

    pub(in crate::kernel::store) fn has_cursor_or_selection(&self) -> bool {
        self.cursor.is_some() || self.selection.is_some()
    }
}

pub(in crate::kernel::store) fn resolve_completion_insertion(
    item: &LspCompletionItem,
) -> CompletionInsertion {
    let mut insertion = match item.insert_text_format {
        LspInsertTextFormat::PlainText => {
            let mut insertion = CompletionInsertion::from_plain_text(item.insert_text.clone());
            if insertion.cursor.is_none()
                && insertion.selection.is_none()
                && should_append_callable_parentheses(item)
            {
                insertion.text.push('(');
                insertion.text.push(')');
                insertion.cursor = Some(insertion.text.chars().count().saturating_sub(1));
            }
            insertion
        }
        LspInsertTextFormat::Snippet => CompletionInsertion::from_snippet(&item.insert_text),
    };

    if !insertion.has_cursor_or_selection()
        && should_append_trailing_space(item)
        && !insertion.text.ends_with(' ')
    {
        insertion.text.push(' ');
    }

    insertion
}

fn should_append_callable_parentheses(item: &LspCompletionItem) -> bool {
    if !completion_kind_is_callable(item.kind) {
        return false;
    }

    let text = item.insert_text.as_str();
    if text.is_empty()
        || text.contains('(')
        || text.contains('!')
        || text.chars().any(|ch| ch.is_whitespace())
    {
        return false;
    }

    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || unicode_xid::UnicodeXID::is_xid_start(first)) {
        return false;
    }

    chars.all(|ch| ch == '_' || unicode_xid::UnicodeXID::is_xid_continue(ch))
}

fn completion_kind_is_callable(kind: Option<u32>) -> bool {
    matches!(kind, Some(2..=4))
}

fn should_append_trailing_space(item: &LspCompletionItem) -> bool {
    // LSP CompletionItemKind::Keyword = 14
    if !matches!(item.kind, Some(14)) {
        return false;
    }

    let text = item.insert_text.as_str();
    if text.is_empty() || text.ends_with(' ') {
        return false;
    }

    // Only for simple identifiers — not values like operators or punctuation.
    text.chars().all(|ch| ch == '_' || ch.is_alphanumeric())
}

pub(in crate::kernel::store) fn completion_replace_range(
    tab: &EditorTabState,
    requested_version: u64,
    item: &LspCompletionItem,
    encoding: LspPositionEncoding,
) -> LspRange {
    let compute_range = || {
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

        LspRange {
            start: super::lsp::lsp_position_from_char_offset(tab, start_char, encoding),
            end: super::lsp::lsp_position_from_char_offset(tab, end_char, encoding),
        }
    };

    if tab.edit_version == requested_version {
        item.replace_range.unwrap_or_else(compute_range)
    } else {
        compute_range()
    }
}

pub(in crate::kernel::store) fn adjust_completion_multiline_indentation(
    tab: &EditorTabState,
    insertion_start_char: usize,
    insertion: CompletionInsertion,
) -> CompletionInsertion {
    if !insertion.text.contains('\n') {
        return insertion;
    }

    let base_indent = indentation_prefix_before_char(tab, insertion_start_char);
    if base_indent.is_empty() {
        return insertion;
    }

    if !multiline_text_needs_indent_adjustment(&insertion.text, &base_indent) {
        return insertion;
    }

    let base_indent_chars = base_indent.chars().count();
    let old_char_len = insertion.text.chars().count();
    let mut index_map = Vec::with_capacity(old_char_len.saturating_add(1));
    index_map.push(0usize);

    let mut adjusted = String::with_capacity(insertion.text.len() + base_indent.len() * 2);
    let mut adjusted_chars = 0usize;
    for ch in insertion.text.chars() {
        adjusted.push(ch);
        adjusted_chars = adjusted_chars.saturating_add(1);
        if ch == '\n' {
            adjusted.push_str(&base_indent);
            adjusted_chars = adjusted_chars.saturating_add(base_indent_chars);
        }
        index_map.push(adjusted_chars);
    }

    let remap = |idx: usize| {
        let bounded = idx.min(old_char_len);
        index_map
            .get(bounded)
            .copied()
            .unwrap_or(*index_map.last().unwrap_or(&0))
    };

    CompletionInsertion {
        text: adjusted,
        cursor: insertion.cursor.map(remap),
        selection: insertion
            .selection
            .map(|(start, end)| (remap(start), remap(end))),
        tabstops: insertion
            .tabstops
            .into_iter()
            .map(|tabstop| SnippetTabstop {
                index: tabstop.index,
                start: remap(tabstop.start),
                end: remap(tabstop.end),
            })
            .collect(),
    }
}

fn indentation_prefix_before_char(tab: &EditorTabState, char_offset: usize) -> String {
    let rope = tab.buffer.rope();
    let char_offset = char_offset.min(rope.len_chars());
    let line = rope.char_to_line(char_offset);
    let line_start = rope.line_to_char(line);

    let mut ws_end = line_start;
    while ws_end < char_offset {
        let ch = rope.char(ws_end);
        if ch == ' ' || ch == '\t' {
            ws_end = ws_end.saturating_add(1);
        } else {
            break;
        }
    }

    rope.slice(line_start..ws_end).to_string()
}

fn multiline_text_needs_indent_adjustment(text: &str, base_indent: &str) -> bool {
    for line in text.split('\n').skip(1) {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(base_indent) {
            return true;
        }
    }
    false
}

pub(in crate::kernel::store) fn apply_completion_insertion_cursor(
    tab: &mut EditorTabState,
    insertion: &CompletionInsertion,
    tab_size: u8,
) {
    if !insertion.has_cursor_or_selection() {
        return;
    }

    tab.cancel_snippet_session();

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
    if rope.slice(start_char..end_char) != insertion.text.as_str() {
        return;
    }

    tab.viewport.follow_cursor = true;

    if !insertion.tabstops.is_empty() {
        tab.begin_snippet_session(start_char, insertion.tabstops.clone());
    }

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
        tab.reset_cursor_goal_col();
    } else if let Some(cursor_rel) = insertion.cursor {
        let cursor_char = start_char.saturating_add(cursor_rel);
        let cursor = tab.buffer.cursor_pos_from_char_offset(cursor_char);
        tab.buffer.clear_selection();
        tab.buffer.set_cursor(cursor.0, cursor.1);
        tab.reset_cursor_goal_col();
    }

    crate::kernel::editor::clamp_and_follow(&mut tab.viewport, &tab.buffer, tab_size);
}

pub(in crate::kernel::store) fn expand_snippet(snippet: &str) -> SnippetExpansion {
    let mut out = String::with_capacity(snippet.len());
    let mut out_chars = 0usize;

    let mut tabstops = Vec::<SnippetTabstop>::new();
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
                    for c in it.by_ref() {
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

                            tabstops.push(SnippetTabstop { index, start, end });
                        } else if index == 0 {
                            final_cursor = Some(out_chars);
                            tabstops.push(SnippetTabstop {
                                index,
                                start: out_chars,
                                end: out_chars,
                            });
                        } else if index > 0 {
                            let replace = best_tabstop
                                .as_ref()
                                .is_none_or(|(best_idx, _)| index < *best_idx);
                            if replace {
                                best_tabstop = Some((index, out_chars));
                            }
                            tabstops.push(SnippetTabstop {
                                index,
                                start: out_chars,
                                end: out_chars,
                            });
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
                        tabstops.push(SnippetTabstop {
                            index: 0,
                            start: out_chars,
                            end: out_chars,
                        });
                    } else {
                        let replace = best_tabstop
                            .as_ref()
                            .is_none_or(|(best_idx, _)| num < *best_idx);
                        if replace {
                            best_tabstop = Some((num, out_chars));
                        }
                        tabstops.push(SnippetTabstop {
                            index: num,
                            start: out_chars,
                            end: out_chars,
                        });
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
        tabstops,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::editor::TabId;
    use crate::kernel::language::LanguageId;
    use crate::kernel::services::ports::{EditorConfig, LspPosition, LspRange};
    use crate::kernel::state::CompletionPopupState;
    use crate::kernel::store::intel::completion_strategy;
    use std::path::PathBuf;

    fn completion_item(id: u64, label: &str) -> LspCompletionItem {
        LspCompletionItem {
            id,
            label: label.to_string(),
            detail: None,
            kind: Some(3),
            documentation: None,
            insert_text: label.to_string(),
            insert_text_format: LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: None,
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        }
    }

    fn tab_with_cursor(content: &str, col: usize) -> crate::kernel::editor::EditorTabState {
        let config = EditorConfig::default();
        let mut tab = crate::kernel::editor::EditorTabState::from_file(
            TabId::new(1),
            PathBuf::from("test.rs"),
            content,
            &config,
        );
        tab.buffer.set_cursor(0, col);
        tab
    }

    fn completion_labels(completion: &CompletionPopupState) -> Vec<String> {
        (0..completion.visible_len())
            .filter_map(|i| completion.visible_item(i))
            .map(|item| item.label.clone())
            .collect()
    }

    #[test]
    fn sync_completion_items_incremental_prefix_matches_expected_items() {
        let mut tab = tab_with_cursor("pri", 1);
        let strategy = completion_strategy::strategy_for_tab(&tab);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_item(1, "piano"),
                completion_item(2, "print"),
                completion_item(3, "private"),
                completion_item(4, "probe"),
            ],
            visible: true,
            ..Default::default()
        };

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        assert_eq!(
            completion_labels(&completion),
            ["piano", "print", "private", "probe"]
        );
        assert_eq!(completion.filter_cache_prefix, "p");

        tab.buffer.set_cursor(0, 2);
        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        assert_eq!(
            completion_labels(&completion),
            ["print", "private", "probe"]
        );
        assert_eq!(completion.filter_cache_prefix, "pr");

        tab.buffer.set_cursor(0, 3);
        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        assert_eq!(completion_labels(&completion), ["print", "private"]);
        assert_eq!(completion.filter_cache_prefix, "pri");
    }

    #[test]
    fn sync_completion_items_prefix_shrink_recomputes_correct_result() {
        let mut tab = tab_with_cursor("pri", 3);
        let strategy = completion_strategy::strategy_for_tab(&tab);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_item(1, "piano"),
                completion_item(2, "print"),
                completion_item(3, "private"),
                completion_item(4, "probe"),
            ],
            visible: true,
            ..Default::default()
        };

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        assert_eq!(completion_labels(&completion), ["print", "private"]);

        tab.buffer.set_cursor(0, 2);
        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        assert_eq!(
            completion_labels(&completion),
            ["print", "private", "probe"]
        );
        assert_eq!(completion.filter_cache_prefix, "pr");
    }

    #[test]
    fn sync_completion_items_no_match_falls_back_to_full_list() {
        let tab = tab_with_cursor("zzz", 3);
        let strategy = completion_strategy::strategy_for_tab(&tab);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_item(1, "piano"),
                completion_item(2, "print"),
                completion_item(3, "private"),
            ],
            visible: true,
            ..Default::default()
        };

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        assert_eq!(
            completion_labels(&completion),
            ["piano", "print", "private"]
        );
        assert_eq!(completion.filter_cache_indices, vec![0, 1, 2]);
    }

    #[test]
    fn sync_completion_items_keeps_selected_id_stable() {
        let mut tab = tab_with_cursor("pri", 1);
        let strategy = completion_strategy::strategy_for_tab(&tab);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_item(1, "piano"),
                completion_item(2, "print"),
                completion_item(3, "private"),
                completion_item(4, "probe"),
            ],
            visible: true,
            ..Default::default()
        };

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        completion.selected = 2;
        let selected_id = completion
            .selected_item()
            .map(|item| item.id)
            .expect("selected item");

        tab.buffer.set_cursor(0, 2);
        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        assert_eq!(
            completion.selected_item().map(|item| item.id),
            Some(selected_id)
        );

        tab.buffer.set_cursor(0, 3);
        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        assert_eq!(
            completion.selected_item().map(|item| item.id),
            Some(selected_id)
        );
    }

    #[test]
    fn sync_completion_items_keeps_selected_id_stable_with_large_visible_indices() {
        let mut tab = tab_with_cursor("pri", 1);
        let strategy = completion_strategy::strategy_for_tab(&tab);
        let mut completion = CompletionPopupState {
            all_items: (0..5_000u64)
                .map(|id| completion_item(id + 1, &format!("print_{id:04}")))
                .collect(),
            visible: true,
            ..Default::default()
        };

        let _ = sync_completion_items_from_cache(&mut completion, &tab, strategy);
        completion.selected = 3_200;
        let selected_id = completion
            .selected_item()
            .map(|item| item.id)
            .expect("selected item");

        tab.buffer.set_cursor(0, 2);
        let _ = sync_completion_items_from_cache(&mut completion, &tab, strategy);
        assert_eq!(
            completion.selected_item().map(|item| item.id),
            Some(selected_id)
        );

        tab.buffer.set_cursor(0, 3);
        let _ = sync_completion_items_from_cache(&mut completion, &tab, strategy);
        assert_eq!(
            completion.selected_item().map(|item| item.id),
            Some(selected_id)
        );
    }

    #[test]
    fn sync_completion_items_selected_lookup_fallback_handles_unsorted_visible_indices() {
        let tab = tab_with_cursor("", 0);
        let strategy = completion_strategy::strategy_for_tab(&tab);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_item(1, "alpha"),
                completion_item(2, "beta"),
                completion_item(3, "gamma"),
            ],
            visible: true,
            visible_indices: vec![2, 0, 1],
            selected: 1,
            filter_cache_prefix: String::new(),
            filter_cache_indices: vec![2, 0, 1],
            filter_cache_source_len: 3,
            filter_cache_valid: true,
            ..Default::default()
        };
        completion.rebuild_index_by_id();

        let selected_id = completion.selected_item().map(|item| item.id);
        let changed = sync_completion_items_from_cache(&mut completion, &tab, strategy);

        assert!(!changed);
        assert_eq!(completion.selected, 1);
        assert_eq!(completion.selected_item().map(|item| item.id), selected_id);
    }

    #[test]
    fn sync_completion_items_respects_cache_invalidation_after_source_replace() {
        let tab = tab_with_cursor("pr", 2);
        let strategy = completion_strategy::strategy_for_tab(&tab);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_item(1, "print"),
                completion_item(2, "private"),
                completion_item(3, "probe"),
            ],
            visible: true,
            ..Default::default()
        };

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        assert_eq!(
            completion_labels(&completion),
            ["print", "private", "probe"]
        );

        completion.all_items = vec![completion_item(100, "prism"), completion_item(101, "proto")];
        completion.invalidate_filter_cache();

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &tab,
            strategy
        ));
        assert_eq!(completion_labels(&completion), ["prism", "proto"]);
        assert_eq!(completion.filter_cache_source_len, 2);
        assert!(completion.filter_cache_valid);
    }

    #[test]
    fn resolve_plain_callable_completion_adds_parentheses_and_cursor() {
        let item = LspCompletionItem {
            id: 1,
            label: "print".to_string(),
            detail: None,
            kind: Some(3),
            documentation: None,
            insert_text: "print".to_string(),
            insert_text_format: LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: None,
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        };

        let insertion = resolve_completion_insertion(&item);
        assert_eq!(insertion.text, "print()");
        assert_eq!(insertion.cursor, Some("print(".chars().count()));
        assert!(insertion.selection.is_none());
    }

    #[test]
    fn experiment_sort_scores_each_item_once() {
        let mut ranker = CompletionRanker::default();
        for i in 0..128 {
            ranker.record(Some(LanguageId::Rust), &format!("item_{i:03}"), Some(3));
        }

        CompletionRanker::reset_perf_counters();

        let mut items: Vec<LspCompletionItem> = (0..128)
            .map(|i| {
                let key = (i * 73) % 128;
                LspCompletionItem {
                    id: key as u64,
                    label: format!("item_{key:03}"),
                    detail: None,
                    kind: Some(3),
                    documentation: None,
                    insert_text: format!("item_{key:03}"),
                    insert_text_format: LspInsertTextFormat::PlainText,
                    insert_range: None,
                    replace_range: None,
                    sort_text: Some(format!("{key:03}")),
                    filter_text: None,
                    additional_text_edits: Vec::new(),
                    command: None,
                    data: None,
                }
            })
            .collect();

        sort_completion_items(&mut items, &ranker, Some(LanguageId::Rust));

        let counters = CompletionRanker::perf_counters();
        eprintln!(
            "[experiment] sort score_calls={} items={}",
            counters.score_calls,
            items.len()
        );
        assert!(
            counters.score_calls <= items.len().saturating_add(2),
            "score_calls={} items={}",
            counters.score_calls,
            items.len()
        );
    }

    #[test]
    fn resolve_plain_non_callable_completion_keeps_plain_text() {
        let item = LspCompletionItem {
            id: 1,
            label: "static".to_string(),
            detail: None,
            kind: Some(14),
            documentation: None,
            insert_text: "static".to_string(),
            insert_text_format: LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: None,
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        };

        let insertion = resolve_completion_insertion(&item);
        assert_eq!(insertion.text, "static ");
        assert!(insertion.cursor.is_none());
        assert!(insertion.selection.is_none());
    }

    #[test]
    fn adjust_multiline_completion_indentation_aligns_closing_brace() {
        let config = EditorConfig::default();
        let tab = crate::kernel::editor::EditorTabState::from_file(
            TabId::new(1),
            PathBuf::from("Main.java"),
            "	pub",
            &config,
        );

        let insertion = CompletionInsertion {
            text: "public static void main(String[] args) {
	
}"
            .to_string(),
            cursor: Some(
                "public static void main(String[] args) {
	"
                .chars()
                .count(),
            ),
            selection: None,
            tabstops: Vec::new(),
        };

        let adjusted = adjust_completion_multiline_indentation(&tab, 1, insertion);
        assert_eq!(
            adjusted.text,
            "public static void main(String[] args) {
		
	}"
            .to_string()
        );
        assert_eq!(
            adjusted.cursor,
            Some(
                "public static void main(String[] args) {
		"
                .chars()
                .count()
            )
        );
    }

    #[test]
    fn adjust_multiline_completion_indentation_skips_already_aligned_text() {
        let config = EditorConfig::default();
        let tab = crate::kernel::editor::EditorTabState::from_file(
            TabId::new(1),
            PathBuf::from("Main.java"),
            "	pub",
            &config,
        );

        let insertion = CompletionInsertion {
            text: "foo
	bar
	baz"
            .to_string(),
            cursor: Some(
                "foo
	bar"
                .chars()
                .count(),
            ),
            selection: None,
            tabstops: Vec::new(),
        };

        let adjusted = adjust_completion_multiline_indentation(&tab, 1, insertion.clone());
        assert_eq!(adjusted.text, insertion.text);
        assert_eq!(adjusted.cursor, insertion.cursor);
        assert_eq!(adjusted.selection, insertion.selection);
    }

    #[test]
    fn completion_replace_range_prefers_item_range_when_version_matches() {
        let config = EditorConfig::default();
        let mut tab = crate::kernel::editor::EditorTabState::from_file(
            TabId::new(1),
            PathBuf::from("test.py"),
            "print",
            &config,
        );
        tab.buffer.set_cursor(0, 5);

        let item = LspCompletionItem {
            id: 1,
            label: "print".to_string(),
            detail: None,
            kind: Some(3),
            documentation: None,
            insert_text: "print".to_string(),
            insert_text_format: LspInsertTextFormat::PlainText,
            insert_range: Some(LspRange {
                start: LspPosition {
                    line: 0,
                    character: 1,
                },
                end: LspPosition {
                    line: 0,
                    character: 5,
                },
            }),
            replace_range: Some(LspRange {
                start: LspPosition {
                    line: 0,
                    character: 2,
                },
                end: LspPosition {
                    line: 0,
                    character: 5,
                },
            }),
            sort_text: None,
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        };

        let range =
            completion_replace_range(&tab, tab.edit_version, &item, LspPositionEncoding::Utf16);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 2);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 5);
    }
}
