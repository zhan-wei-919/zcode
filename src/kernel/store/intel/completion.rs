use crate::kernel::editor::EditorTabState;
use crate::kernel::editor::SnippetTabstop;
use crate::kernel::language::adapter::{
    expand_snippet as expand_fallback_snippet, CompletionContext, CompletionProtocolAdapter,
    CompletionRecord, CompletionReplacePolicy, LanguageInteractionPolicy, LanguageRuntimeContext,
    TextEditPlan,
};
use crate::kernel::language::{CompletionEntry, LanguageId};
use crate::kernel::services::ports::{LspCompletionItem, LspPositionEncoding, LspRange};
use crate::kernel::state::CompletionPopupState;
use crate::kernel::{AppState, EditorAction};
use crate::models::{Granularity, Selection};
use rustc_hash::FxHashMap;

use super::completion_rank::CompletionRanker;

pub(in crate::kernel::store) use crate::kernel::language::adapter::SnippetExpansion;

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

pub(in crate::kernel::store) fn language_runtime_context<'a>(
    state: &'a AppState,
    tab: &'a EditorTabState,
    adapter: &dyn crate::kernel::language::LanguageAdapter,
) -> LanguageRuntimeContext<'a> {
    let server_caps = tab
        .path
        .as_deref()
        .and_then(|path| super::lsp::lsp_server_capabilities_for_path(state, path));
    let server = tab
        .path
        .as_deref()
        .and_then(|path| super::lsp::lsp_client_key_for_path(state, path).map(|key| key.server))
        .or(adapter.features().lsp_server);

    LanguageRuntimeContext::new(tab.language(), tab, adapter.syntax().syntax_facts(tab))
        .with_server(server, server_caps)
}

pub(in crate::kernel::store) fn completion_runtime_context<'a>(
    state: &'a AppState,
    tab: &'a EditorTabState,
    adapter: &dyn crate::kernel::language::LanguageAdapter,
) -> LanguageRuntimeContext<'a> {
    language_runtime_context(state, tab, adapter)
}

pub(in crate::kernel::store) fn normalize_completion_record(
    runtime: &LanguageRuntimeContext<'_>,
    adapter: &dyn CompletionProtocolAdapter,
    raw: LspCompletionItem,
) -> CompletionRecord {
    let entry = adapter.normalize_completion(&CompletionContext {
        runtime: runtime.clone(),
        item: &raw,
    });
    CompletionRecord { entry, raw }
}

pub(in crate::kernel::store) fn sort_completion_items(
    items: &mut [CompletionRecord],
    ranker: &CompletionRanker,
    language: Option<LanguageId>,
) {
    let mut score_by_id = FxHashMap::default();
    for item in items.iter() {
        score_by_id.insert(
            item.entry.id,
            ranker.score(language, &item.entry.label, item.entry.kind),
        );
    }

    items.sort_by(|a, b| {
        let a_score = score_by_id.get(&a.entry.id).copied().unwrap_or(0.0);
        let b_score = score_by_id.get(&b.entry.id).copied().unwrap_or(0.0);
        b_score
            .partial_cmp(&a_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_key = a
                    .entry
                    .sort_text
                    .as_deref()
                    .unwrap_or(a.entry.label.as_str());
                let b_key = b
                    .entry
                    .sort_text
                    .as_deref()
                    .unwrap_or(b.entry.label.as_str());
                a_key
                    .cmp(b_key)
                    .then_with(|| a.entry.label.cmp(&b.entry.label))
                    .then_with(|| a.entry.detail.cmp(&b.entry.detail))
            })
    });
}

pub(in crate::kernel::store) fn filtered_completion_indices(
    runtime: &LanguageRuntimeContext<'_>,
    items: &[CompletionRecord],
    interaction: &dyn LanguageInteractionPolicy,
) -> Vec<usize> {
    if items.is_empty() {
        return Vec::new();
    }

    let prefix = completion_prefix_at_cursor(runtime, interaction);
    if prefix.is_empty() {
        return (0..items.len()).collect();
    }

    let mut filtered = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        if completion_item_matches_prefix(&item.entry, &prefix) {
            filtered.push(idx);
        }
    }

    filtered
}

fn collect_matching_indices(
    all_items: &[CompletionRecord],
    prefix: &str,
    base_indices: impl Iterator<Item = usize>,
) -> Vec<usize> {
    let mut matched = Vec::new();
    for idx in base_indices {
        if completion_item_matches_prefix(&all_items[idx].entry, prefix) {
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
        .is_none_or(|item| item.entry.id != id)
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
    runtime: &LanguageRuntimeContext<'_>,
    interaction: &dyn LanguageInteractionPolicy,
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

    let selected_id = if completion.selection_locked {
        completion.selected_item().map(|item| item.id)
    } else {
        None
    };

    let prefix = completion_prefix_at_cursor(runtime, interaction);
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
        completion.visible = !cached_indices.is_empty();
        return items_changed;
    }

    let can_use_cached_base = completion.filter_cache_valid
        && completion.filter_cache_source_len == source_len
        && prefix.starts_with(&completion.filter_cache_prefix);

    let new_indices = if prefix.is_empty() {
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
    completion.visible = completion.visible_len() > 0;
    items_changed
}

fn completion_item_matches_prefix(item: &CompletionEntry, prefix: &str) -> bool {
    let candidate = item.filter_text.as_deref().unwrap_or(item.label.as_str());
    starts_with_ignore_ascii_case(candidate, prefix)
}

fn starts_with_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .get(..needle.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(needle))
}

pub(in crate::kernel::store) fn completion_prefix_at_cursor(
    runtime: &LanguageRuntimeContext<'_>,
    interaction: &dyn LanguageInteractionPolicy,
) -> String {
    let rope = runtime.tab.buffer.rope();
    let (start_char, end_char) = interaction.completion_prefix_bounds(runtime);
    rope.slice(start_char..end_char).to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::kernel::store) struct CompletionInsertion {
    pub(in crate::kernel::store) text: String,
    pub(in crate::kernel::store) cursor: Option<usize>,
    pub(in crate::kernel::store) selection: Option<(usize, usize)>,
    pub(in crate::kernel::store) tabstops: Vec<SnippetTabstop>,
}

impl CompletionInsertion {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(in crate::kernel::store) fn from_plain_text(text: String) -> Self {
        Self::from_plan(TextEditPlan::from_plain_text(text))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(in crate::kernel::store) fn from_snippet(snippet: &str) -> Self {
        Self::from_plan(TextEditPlan::from_snippet(snippet))
    }

    pub(in crate::kernel::store) fn from_plan(plan: TextEditPlan) -> Self {
        Self {
            text: plan.text,
            cursor: plan.cursor,
            selection: plan.selection,
            tabstops: plan
                .tabstops
                .into_iter()
                .map(|tabstop| SnippetTabstop {
                    index: tabstop.index,
                    start: tabstop.start,
                    end: tabstop.end,
                })
                .collect(),
        }
    }

    pub(in crate::kernel::store) fn has_cursor_or_selection(&self) -> bool {
        self.cursor.is_some() || self.selection.is_some()
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::kernel::store) fn resolve_completion_insertion(
    tab: &EditorTabState,
    adapter: &dyn crate::kernel::language::LanguageAdapter,
    item: &LspCompletionItem,
) -> CompletionInsertion {
    let runtime =
        LanguageRuntimeContext::new(tab.language(), tab, adapter.syntax().syntax_facts(tab));
    let plan = adapter
        .completion_protocol()
        .normalize_completion_text(&CompletionContext { runtime, item });
    CompletionInsertion::from_plan(plan)
}

pub(in crate::kernel::store) fn completion_replace_range(
    tab: &EditorTabState,
    requested_version: u64,
    replace: &CompletionReplacePolicy,
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

    let fallback_range = compute_range();
    if tab.edit_version != requested_version {
        return fallback_range;
    }

    let CompletionReplacePolicy::ServerRange {
        insert_range,
        replace_range,
        anchor_to_prefix,
    } = replace
    else {
        return fallback_range;
    };

    let Some(mut item_range) = replace_range.or(*insert_range) else {
        return fallback_range;
    };

    if !anchor_to_prefix {
        return item_range;
    }

    let to_char_offset = |line: u32, character: u32| {
        let byte = super::lsp::lsp_position_to_byte_offset(tab, line, character, encoding);
        tab.buffer
            .rope()
            .byte_to_char(byte.min(tab.buffer.rope().len_bytes()))
    };

    let fallback_start_char =
        to_char_offset(fallback_range.start.line, fallback_range.start.character);
    let fallback_end_char = to_char_offset(fallback_range.end.line, fallback_range.end.character);
    let item_start_char = to_char_offset(item_range.start.line, item_range.start.character);
    let item_end_char = to_char_offset(item_range.end.line, item_range.end.character);

    if item_start_char > item_end_char || item_end_char < fallback_end_char {
        return fallback_range;
    }

    if item_start_char < fallback_start_char {
        item_range.start = fallback_range.start;
    }

    item_range
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

#[cfg_attr(not(test), allow(dead_code))]
pub(in crate::kernel::store) fn expand_snippet(snippet: &str) -> SnippetExpansion {
    expand_fallback_snippet(snippet)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::editor::TabId;
    use crate::kernel::language::adapter::adapter_for_tab;
    use crate::kernel::language::LanguageId;
    use crate::kernel::services::ports::{
        EditorConfig, LspInsertTextFormat, LspPosition, LspRange,
    };
    use crate::kernel::state::CompletionPopupState;
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

    fn completion_record(id: u64, label: &str) -> crate::kernel::language::CompletionRecord {
        completion_item(id, label).into()
    }

    fn runtime_for<'a>(
        tab: &'a crate::kernel::editor::EditorTabState,
    ) -> crate::kernel::language::LanguageRuntimeContext<'a> {
        let adapter = adapter_for_tab(tab);
        crate::kernel::language::LanguageRuntimeContext::new(
            tab.language(),
            tab,
            adapter.syntax().syntax_facts(tab),
        )
    }

    fn tab_with_cursor(content: &str, col: usize) -> crate::kernel::editor::EditorTabState {
        tab_with_cursor_at_path("test.rs", content, col)
    }

    fn tab_with_cursor_at_path(
        path: &str,
        content: &str,
        col: usize,
    ) -> crate::kernel::editor::EditorTabState {
        let config = EditorConfig::default();
        let mut tab = crate::kernel::editor::EditorTabState::from_file(
            TabId::new(1),
            PathBuf::from(path),
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
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_record(1, "piano"),
                completion_record(2, "print"),
                completion_record(3, "private"),
                completion_record(4, "probe"),
            ],
            visible: true,
            ..Default::default()
        };

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
        ));
        assert_eq!(
            completion_labels(&completion),
            ["piano", "print", "private", "probe"]
        );
        assert_eq!(completion.filter_cache_prefix, "p");

        tab.buffer.set_cursor(0, 2);
        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
        ));
        assert_eq!(
            completion_labels(&completion),
            ["print", "private", "probe"]
        );
        assert_eq!(completion.filter_cache_prefix, "pr");

        tab.buffer.set_cursor(0, 3);
        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
        ));
        assert_eq!(completion_labels(&completion), ["print", "private"]);
        assert_eq!(completion.filter_cache_prefix, "pri");
    }

    #[test]
    fn sync_completion_items_prefix_shrink_recomputes_correct_result() {
        let mut tab = tab_with_cursor("pri", 3);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_record(1, "piano"),
                completion_record(2, "print"),
                completion_record(3, "private"),
                completion_record(4, "probe"),
            ],
            visible: true,
            ..Default::default()
        };

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
        ));
        assert_eq!(completion_labels(&completion), ["print", "private"]);

        tab.buffer.set_cursor(0, 2);
        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
        ));
        assert_eq!(
            completion_labels(&completion),
            ["print", "private", "probe"]
        );
        assert_eq!(completion.filter_cache_prefix, "pr");
    }

    #[test]
    fn sync_completion_items_no_match_hides_popup() {
        let tab = tab_with_cursor("zzz", 3);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_record(1, "piano"),
                completion_record(2, "print"),
                completion_record(3, "private"),
            ],
            visible: true,
            ..Default::default()
        };

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
        ));
        assert_eq!(completion_labels(&completion), Vec::<String>::new());
        assert_eq!(completion.filter_cache_indices, Vec::<usize>::new());
        assert!(!completion.visible);
    }

    #[test]
    fn sync_completion_items_keeps_selected_id_stable() {
        let mut tab = tab_with_cursor("pri", 1);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_record(1, "piano"),
                completion_record(2, "print"),
                completion_record(3, "private"),
                completion_record(4, "probe"),
            ],
            visible: true,
            ..Default::default()
        };

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
        ));
        completion.selected = 2;
        completion.selection_locked = true;
        let selected_id = completion
            .selected_item()
            .map(|item| item.id)
            .expect("selected item");

        tab.buffer.set_cursor(0, 2);
        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
        ));
        assert_eq!(
            completion.selected_item().map(|item| item.id),
            Some(selected_id)
        );

        tab.buffer.set_cursor(0, 3);
        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
        ));
        assert_eq!(
            completion.selected_item().map(|item| item.id),
            Some(selected_id)
        );
    }

    #[test]
    fn sync_completion_items_keeps_selected_id_stable_with_large_visible_indices() {
        let mut tab = tab_with_cursor("pri", 1);
        let mut completion = CompletionPopupState {
            all_items: (0..5_000u64)
                .map(|id| completion_record(id + 1, &format!("print_{id:04}")))
                .collect(),
            visible: true,
            ..Default::default()
        };

        let _ = sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction(),
        );
        completion.selected = 3_200;
        completion.selection_locked = true;
        let selected_id = completion
            .selected_item()
            .map(|item| item.id)
            .expect("selected item");

        tab.buffer.set_cursor(0, 2);
        let _ = sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction(),
        );
        assert_eq!(
            completion.selected_item().map(|item| item.id),
            Some(selected_id)
        );

        tab.buffer.set_cursor(0, 3);
        let _ = sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction(),
        );
        assert_eq!(
            completion.selected_item().map(|item| item.id),
            Some(selected_id)
        );
    }

    #[test]
    fn sync_completion_items_selected_lookup_fallback_handles_unsorted_visible_indices() {
        let tab = tab_with_cursor("", 0);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_record(1, "alpha"),
                completion_record(2, "beta"),
                completion_record(3, "gamma"),
            ],
            visible: true,
            visible_indices: vec![2, 0, 1],
            selected: 1,
            selection_locked: true,
            filter_cache_prefix: String::new(),
            filter_cache_indices: vec![2, 0, 1],
            filter_cache_source_len: 3,
            filter_cache_valid: true,
            ..Default::default()
        };
        completion.rebuild_index_by_id();

        let selected_id = completion.selected_item().map(|item| item.id);
        let changed = sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction(),
        );

        assert!(!changed);
        assert_eq!(completion.selected, 1);
        assert_eq!(completion.selected_item().map(|item| item.id), selected_id);
    }

    #[test]
    fn sync_completion_items_respects_cache_invalidation_after_source_replace() {
        let tab = tab_with_cursor("pr", 2);
        let mut completion = CompletionPopupState {
            all_items: vec![
                completion_record(1, "print"),
                completion_record(2, "private"),
                completion_record(3, "probe"),
            ],
            visible: true,
            ..Default::default()
        };

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
        ));
        assert_eq!(
            completion_labels(&completion),
            ["print", "private", "probe"]
        );

        completion.all_items = vec![
            completion_record(100, "prism"),
            completion_record(101, "proto"),
        ];
        completion.invalidate_filter_cache();

        assert!(sync_completion_items_from_cache(
            &mut completion,
            &runtime_for(&tab),
            adapter_for_tab(&tab).interaction()
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

        let tab = tab_with_cursor("", 0);
        let adapter = adapter_for_tab(&tab);
        let insertion = resolve_completion_insertion(&tab, adapter, &item);
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

        let items: Vec<LspCompletionItem> = (0..128)
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
            .collect::<Vec<_>>();
        let mut items: Vec<crate::kernel::language::CompletionRecord> =
            items.into_iter().map(Into::into).collect();

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

        let tab = tab_with_cursor("", 0);
        let adapter = adapter_for_tab(&tab);
        let insertion = resolve_completion_insertion(&tab, adapter, &item);
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

        let replace = crate::kernel::language::CompletionRecord::from(item.clone())
            .entry
            .commit
            .replace;
        let range =
            completion_replace_range(&tab, tab.edit_version, &replace, LspPositionEncoding::Utf16);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 2);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 5);
    }

    #[test]
    fn completion_replace_range_keeps_boundary_characters_before_prefix() {
        let config = EditorConfig::default();
        let mut tab = crate::kernel::editor::EditorTabState::from_file(
            TabId::new(1),
            PathBuf::from("test.rs"),
            "&s",
            &config,
        );
        tab.buffer.set_cursor(0, 2);

        let item = LspCompletionItem {
            id: 1,
            label: "self".to_string(),
            detail: None,
            kind: Some(5),
            documentation: None,
            insert_text: "self".to_string(),
            insert_text_format: LspInsertTextFormat::PlainText,
            insert_range: Some(LspRange {
                start: LspPosition {
                    line: 0,
                    character: 1,
                },
                end: LspPosition {
                    line: 0,
                    character: 2,
                },
            }),
            replace_range: Some(LspRange {
                start: LspPosition {
                    line: 0,
                    character: 0,
                },
                end: LspPosition {
                    line: 0,
                    character: 2,
                },
            }),
            sort_text: None,
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        };

        let replace = crate::kernel::language::CompletionRecord::from(item.clone())
            .entry
            .commit
            .replace;
        let range =
            completion_replace_range(&tab, tab.edit_version, &replace, LspPositionEncoding::Utf16);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 1);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 2);
    }

    #[test]
    fn resolve_completion_insertion_uses_adapter_behavior_per_language() {
        let item = LspCompletionItem {
            id: 1,
            label: "push_back".to_string(),
            detail: None,
            kind: Some(3),
            documentation: None,
            insert_text: "push_back".to_string(),
            insert_text_format: LspInsertTextFormat::PlainText,
            insert_range: None,
            replace_range: None,
            sort_text: None,
            filter_text: None,
            additional_text_edits: Vec::new(),
            command: None,
            data: None,
        };

        let rust_tab = tab_with_cursor_at_path("main.rs", "", 0);
        let cpp_tab = tab_with_cursor_at_path("main.cpp", "obj->", "obj->".chars().count());

        let rust_insertion =
            resolve_completion_insertion(&rust_tab, adapter_for_tab(&rust_tab), &item);
        let cpp_insertion =
            resolve_completion_insertion(&cpp_tab, adapter_for_tab(&cpp_tab), &item);

        assert_eq!(rust_insertion.text, "push_back()");
        assert_eq!(cpp_insertion.text, "push_back");
        assert_eq!(cpp_insertion.cursor, None);
    }
}
