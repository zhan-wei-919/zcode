use super::*;
use crate::kernel::editor::HighlightKind;
use crate::kernel::services::ports::{
    EditorConfig, LspCommand, LspCompletionItem, LspFoldingRange, LspInlayHint,
    LspInsertTextFormat, LspPosition, LspRange, LspSemanticToken, LspSemanticTokensLegend,
    LspServerCapabilities, LspServerKind, LspTextEdit,
};
use crate::models::FileTree;
use std::ffi::OsString;
use std::time::Instant;

fn new_store() -> Store {
    let root = std::env::temp_dir();
    let tree = FileTree::new_with_root_for_test(OsString::from("root"), root.clone());
    Store::new(AppState::new(root, tree, EditorConfig::default()))
}

fn rust_content(lines: usize) -> String {
    let mut out = String::new();
    for i in 0..lines {
        out.push_str(&format!("fn item_{i:04}() {{ let value_{i} = {i}; }}\n"));
    }
    out
}

fn install_rust_lsp_caps(store: &mut Store) {
    install_rust_lsp_caps_with_triggers(store, Vec::new(), Vec::new());
}

fn install_rust_lsp_caps_with_triggers(
    store: &mut Store,
    completion_triggers: Vec<char>,
    signature_help_triggers: Vec<char>,
) {
    let action = Action::LspServerCapabilities {
        server: LspServerKind::RustAnalyzer,
        root: store.state.workspace_root.clone(),
        capabilities: LspServerCapabilities {
            semantic_tokens: true,
            semantic_tokens_full: true,
            semantic_tokens_range: true,
            semantic_tokens_legend: Some(LspSemanticTokensLegend {
                token_types: vec!["function".to_string(), "variable".to_string()],
                token_modifiers: vec![],
            }),
            completion: true,
            completion_resolve: true,
            completion_triggers,
            signature_help: true,
            signature_help_triggers,
            inlay_hints: true,
            folding_range: true,
            ..Default::default()
        },
    };
    let _ = store.dispatch(action);
}

fn open_shared_path_tabs(
    store: &mut Store,
    panes: usize,
    path: &std::path::Path,
    content: &str,
) -> u64 {
    store.state.editor.ensure_panes(panes);
    for pane in 0..panes {
        let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
            pane,
            path: path.to_path_buf(),
            content: content.to_string(),
        }));
    }

    let version = store
        .state
        .editor
        .pane(0)
        .and_then(|p| p.active_tab())
        .map(|tab| tab.edit_version)
        .unwrap_or(0);

    for pane in 0..panes {
        let tab = store
            .state
            .editor
            .pane(pane)
            .and_then(|p| p.active_tab())
            .expect("tab exists");
        assert_eq!(tab.edit_version, version);
        assert_eq!(tab.path.as_ref(), Some(&path.to_path_buf()));
    }

    version
}

fn test_completion_item(id: u64) -> LspCompletionItem {
    LspCompletionItem {
        id,
        label: format!("item_{id:05}"),
        detail: None,
        kind: Some(3),
        documentation: None,
        insert_text: format!("item_{id:05}"),
        insert_text_format: LspInsertTextFormat::PlainText,
        insert_range: None,
        replace_range: None,
        sort_text: None,
        filter_text: None,
        additional_text_edits: Vec::new(),
        command: None,
        data: Some(serde_json::json!({"id": id})),
    }
}

fn open_single_rust_tab_for_input(store: &mut Store, filename: &str) -> (std::path::PathBuf, u64) {
    let path = store.state.workspace_root.join(filename);
    let content = "let alpha_beta = 12345;\nlet gamma = alpha_beta + 1;\n".to_string();
    let _ = store.dispatch(Action::Editor(EditorAction::OpenFile {
        pane: 0,
        path: path.clone(),
        content,
    }));

    {
        let tab = store
            .state
            .editor
            .pane_mut(0)
            .and_then(|pane| pane.active_tab_mut())
            .expect("tab exists");
        tab.buffer.set_cursor(0, 5);
    }

    let version = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .map(|tab| tab.edit_version)
        .unwrap_or(0);

    (path, version)
}

fn run_input_key_cycle(store: &mut Store, loops: usize) -> (std::time::Duration, usize) {
    let mut changed = 0usize;
    let start = Instant::now();
    for _ in 0..loops {
        let r = store.dispatch(Action::RunCommand(Command::InsertChar('a')));
        changed += usize::from(r.state_changed);

        let r = store.dispatch(Action::RunCommand(Command::DeleteBackward));
        changed += usize::from(r.state_changed);

        let r = store.dispatch(Action::RunCommand(Command::CursorRight));
        changed += usize::from(r.state_changed);

        let r = store.dispatch(Action::RunCommand(Command::CursorLeft));
        changed += usize::from(r.state_changed);
    }
    (start.elapsed(), changed)
}

#[test]
fn experiment_semantic_tokens_fanout_scale_baseline() {
    let mut store = new_store();
    install_rust_lsp_caps(&mut store);

    let panes = 4usize;
    let lines = 1200usize;
    let path = store.state.workspace_root.join("fanout_semantic.rs");
    let content = rust_content(lines);
    let version = open_shared_path_tabs(&mut store, panes, &path, &content);

    let tokens: Vec<LspSemanticToken> = (0..lines)
        .map(|line| LspSemanticToken {
            line: line as u32,
            start: 3,
            length: 8,
            token_type: (line % 2) as u32,
            modifiers: 0,
        })
        .collect();

    let _ = store.dispatch(Action::LspSemanticTokens {
        path: path.clone(),
        version,
        tokens: tokens.clone(),
    });

    let loops = 120usize;
    let mut changed_count = 0usize;
    let start = Instant::now();
    for _ in 0..loops {
        let result = store.dispatch(Action::LspSemanticTokens {
            path: path.clone(),
            version,
            tokens: tokens.clone(),
        });
        if result.state_changed {
            changed_count += 1;
        }
    }
    let elapsed = start.elapsed();
    let avg_us = elapsed.as_secs_f64() * 1_000_000.0 / loops as f64;

    eprintln!(
        "[experiment] semantic_tokens_fanout loops={} tabs={} tokens={} elapsed_ms={} avg_us={:.2} changed_count={}",
        loops,
        panes,
        tokens.len(),
        elapsed.as_millis(),
        avg_us,
        changed_count
    );

    for pane in 0..panes {
        let tab = store
            .state
            .editor
            .pane(pane)
            .and_then(|p| p.active_tab())
            .expect("tab exists");
        let line = tab
            .semantic_highlight_lines(0, 1)
            .and_then(|slice| slice.first())
            .expect("semantic line exists");
        assert!(!line.is_empty());
    }
}

#[test]
fn experiment_inlay_hints_fanout_scale_baseline() {
    let mut store = new_store();

    let panes = 4usize;
    let lines = 1000usize;
    let path = store.state.workspace_root.join("fanout_inlay.rs");
    let content = rust_content(lines);
    let version = open_shared_path_tabs(&mut store, panes, &path, &content);

    let range = LspRange {
        start: LspPosition {
            line: 0,
            character: 0,
        },
        end: LspPosition {
            line: (lines - 1) as u32,
            character: 0,
        },
    };

    let hints: Vec<LspInlayHint> = (0..lines)
        .map(|line| LspInlayHint {
            position: LspPosition {
                line: line as u32,
                character: 8,
            },
            label: format!(": i64 // {line}"),
            padding_left: true,
            padding_right: false,
        })
        .collect();

    let _ = store.dispatch(Action::LspInlayHints {
        path: path.clone(),
        version,
        range,
        hints: hints.clone(),
    });

    let loops = 120usize;
    let mut changed_count = 0usize;
    let start = Instant::now();
    for _ in 0..loops {
        let result = store.dispatch(Action::LspInlayHints {
            path: path.clone(),
            version,
            range,
            hints: hints.clone(),
        });
        if result.state_changed {
            changed_count += 1;
        }
    }
    let elapsed = start.elapsed();
    let avg_us = elapsed.as_secs_f64() * 1_000_000.0 / loops as f64;

    eprintln!(
        "[experiment] inlay_hints_fanout loops={} tabs={} hints={} elapsed_ms={} avg_us={:.2} changed_count={}",
        loops,
        panes,
        hints.len(),
        elapsed.as_millis(),
        avg_us,
        changed_count
    );

    for pane in 0..panes {
        let tab = store
            .state
            .editor
            .pane(pane)
            .and_then(|p| p.active_tab())
            .expect("tab exists");
        let line = tab
            .inlay_hint_lines(0, 1)
            .and_then(|slice| slice.first())
            .expect("inlay line exists");
        assert!(!line.is_empty());
    }
}

#[test]
fn experiment_inlay_hints_apply_scale_baseline() {
    let mut store = new_store();

    let panes = 4usize;
    let line_count = 100usize;
    let hints_per_line = 10usize;
    let hint_count = line_count * hints_per_line;
    let loops = 120usize;
    let path = store.state.workspace_root.join("apply_inlay.rs");
    let content = rust_content(line_count);
    let version = open_shared_path_tabs(&mut store, panes, &path, &content);

    let range = LspRange {
        start: LspPosition {
            line: 0,
            character: 0,
        },
        end: LspPosition {
            line: (line_count - 1) as u32,
            character: 0,
        },
    };

    let base_hints: Vec<LspInlayHint> = (0..line_count)
        .flat_map(|line| {
            (0..hints_per_line).map(move |slot| LspInlayHint {
                position: LspPosition {
                    line: line as u32,
                    character: (slot * 3 + 2) as u32,
                },
                label: format!(": hint_{line}_{slot}"),
                padding_left: true,
                padding_right: false,
            })
        })
        .collect();

    let _ = store.dispatch(Action::LspInlayHints {
        path: path.clone(),
        version,
        range,
        hints: base_hints.clone(),
    });

    let payloads: Vec<Vec<LspInlayHint>> = (0..loops)
        .map(|i| {
            let mut hints = base_hints.clone();
            let idx = i % hint_count;
            hints[idx].label.push_str(&format!("_update_{i}"));
            hints
        })
        .collect();

    let mut changed_count = 0usize;
    let start = Instant::now();
    for hints in payloads {
        let result = store.dispatch(Action::LspInlayHints {
            path: path.clone(),
            version,
            range,
            hints,
        });
        if result.state_changed {
            changed_count += 1;
        }
    }

    let elapsed = start.elapsed();
    let avg_us = elapsed.as_secs_f64() * 1_000_000.0 / loops as f64;

    eprintln!(
        "[experiment] inlay_hints_apply loops={} tabs={} hints={} elapsed_ms={} avg_us={:.2} changed_count={}",
        loops,
        panes,
        hint_count,
        elapsed.as_millis(),
        avg_us,
        changed_count
    );

    assert!(changed_count > 0);
}

#[test]
fn lsp_semantic_tokens_identical_payload_second_dispatch_is_noop() {
    let mut store = new_store();
    install_rust_lsp_caps(&mut store);

    let path = store.state.workspace_root.join("semantic_fast_path.rs");
    let content = rust_content(64);
    let version = open_shared_path_tabs(&mut store, 2, &path, &content);

    let tokens: Vec<LspSemanticToken> = (0..64)
        .map(|line| LspSemanticToken {
            line: line as u32,
            start: 3,
            length: 8,
            token_type: (line % 2) as u32,
            modifiers: 0,
        })
        .collect();

    let first = store.dispatch(Action::LspSemanticTokens {
        path: path.clone(),
        version,
        tokens: tokens.clone(),
    });
    assert!(first.state_changed);

    let second = store.dispatch(Action::LspSemanticTokens {
        path,
        version,
        tokens,
    });
    assert!(!second.state_changed);
}

#[test]
fn lsp_inlay_hints_identical_payload_second_dispatch_is_noop() {
    let mut store = new_store();

    let path = store.state.workspace_root.join("inlay_fast_path.rs");
    let content = rust_content(64);
    let version = open_shared_path_tabs(&mut store, 2, &path, &content);

    let range = LspRange {
        start: LspPosition {
            line: 0,
            character: 0,
        },
        end: LspPosition {
            line: 64,
            character: 0,
        },
    };
    let hints: Vec<LspInlayHint> = (0..64)
        .map(|line| LspInlayHint {
            position: LspPosition {
                line: line as u32,
                character: 8,
            },
            label: format!(": i64 // {line}"),
            padding_left: true,
            padding_right: false,
        })
        .collect();

    let first = store.dispatch(Action::LspInlayHints {
        path: path.clone(),
        version,
        range,
        hints: hints.clone(),
    });
    assert!(first.state_changed);

    let second = store.dispatch(Action::LspInlayHints {
        path,
        version,
        range,
        hints,
    });
    assert!(!second.state_changed);
}

#[test]
fn lsp_inlay_hints_unsorted_payload_still_orders_by_column_then_label() {
    let mut store = new_store();

    let path = store.state.workspace_root.join("inlay_unsorted_payload.rs");
    let content = rust_content(4);
    let version = open_shared_path_tabs(&mut store, 1, &path, &content);

    let range = LspRange {
        start: LspPosition {
            line: 0,
            character: 0,
        },
        end: LspPosition {
            line: 1,
            character: 0,
        },
    };
    let hints = vec![
        LspInlayHint {
            position: LspPosition {
                line: 0,
                character: 12,
            },
            label: "z12".to_string(),
            padding_left: false,
            padding_right: false,
        },
        LspInlayHint {
            position: LspPosition {
                line: 0,
                character: 4,
            },
            label: "b4".to_string(),
            padding_left: false,
            padding_right: false,
        },
        LspInlayHint {
            position: LspPosition {
                line: 0,
                character: 4,
            },
            label: "a4".to_string(),
            padding_left: false,
            padding_right: false,
        },
    ];

    let result = store.dispatch(Action::LspInlayHints {
        path,
        version,
        range,
        hints,
    });
    assert!(result.state_changed);

    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|pane| pane.active_tab())
        .expect("tab exists");
    let line = tab
        .inlay_hint_lines(0, 1)
        .and_then(|rows| rows.first())
        .expect("inlay hint line");
    assert_eq!(
        line,
        &vec!["a4".to_string(), "b4".to_string(), "z12".to_string()]
    );
}

#[test]
fn lsp_semantic_tokens_legend_change_misses_fast_path() {
    let mut store = new_store();
    install_rust_lsp_caps(&mut store);

    let path = store.state.workspace_root.join("semantic_legend_change.rs");
    let content = "let value = 1;\n";
    let version = open_shared_path_tabs(&mut store, 1, &path, content);
    let tokens = vec![LspSemanticToken {
        line: 0,
        start: 4,
        length: 5,
        token_type: 1,
        modifiers: 0,
    }];

    let first = store.dispatch(Action::LspSemanticTokens {
        path: path.clone(),
        version,
        tokens: tokens.clone(),
    });
    assert!(first.state_changed);

    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|p| p.active_tab())
        .expect("tab exists");
    let first_kind = tab
        .semantic_highlight_lines(0, 1)
        .and_then(|rows| rows.first())
        .and_then(|line| line.first())
        .map(|span| span.kind);
    assert_eq!(first_kind, Some(HighlightKind::Variable));

    let _ = store.dispatch(Action::LspServerCapabilities {
        server: LspServerKind::RustAnalyzer,
        root: store.state.workspace_root.clone(),
        capabilities: LspServerCapabilities {
            semantic_tokens: true,
            semantic_tokens_full: true,
            semantic_tokens_range: true,
            semantic_tokens_legend: Some(LspSemanticTokensLegend {
                token_types: vec!["function".to_string(), "keyword".to_string()],
                token_modifiers: vec![],
            }),
            completion: true,
            completion_resolve: true,
            signature_help: true,
            inlay_hints: true,
            folding_range: true,
            ..Default::default()
        },
    });

    let second = store.dispatch(Action::LspSemanticTokens {
        path,
        version,
        tokens,
    });
    assert!(second.state_changed);

    let tab = store
        .state
        .editor
        .pane(0)
        .and_then(|p| p.active_tab())
        .expect("tab exists");
    let second_kind = tab
        .semantic_highlight_lines(0, 1)
        .and_then(|rows| rows.first())
        .and_then(|line| line.first())
        .map(|span| span.kind);
    assert_eq!(second_kind, Some(HighlightKind::Keyword));
}

#[test]
fn lsp_completion_resolve_updates_large_list_item_with_index_map() {
    let mut store = new_store();
    let items_count = 20_000usize;
    let target_id = (items_count.saturating_sub(1)) as u64;
    store.state.ui.completion.visible = true;
    store.state.ui.completion.all_items =
        (0..items_count as u64).map(test_completion_item).collect();
    store.state.ui.completion.visible_indices = (0..items_count).collect();
    store.state.ui.completion.rebuild_index_by_id();
    store.state.ui.completion.resolve_inflight = Some(target_id);

    let result = store.dispatch(Action::LspCompletionResolved {
        id: target_id,
        detail: Some("resolved".to_string()),
        documentation: Some("doc".to_string()),
        insert_text: None,
        insert_text_format: None,
        insert_range: None,
        replace_range: None,
        additional_text_edits: Vec::new(),
        command: None,
    });

    assert!(result.state_changed);
    assert!(store
        .state
        .ui
        .completion
        .all_items
        .iter()
        .any(|item| item.id == target_id && item.detail.as_deref() == Some("resolved")));
}

#[test]
fn experiment_folding_ranges_fanout_scale_baseline() {
    let mut store = new_store();

    let panes = 4usize;
    let lines = 1200usize;
    let path = store.state.workspace_root.join("fanout_folding.rs");
    let content = rust_content(lines);
    let version = open_shared_path_tabs(&mut store, panes, &path, &content);

    let ranges: Vec<LspFoldingRange> = (0..(lines / 3))
        .map(|i| {
            let start = (i * 3) as u32;
            LspFoldingRange {
                start_line: start,
                end_line: start.saturating_add(2),
            }
        })
        .collect();

    let _ = store.dispatch(Action::LspFoldingRanges {
        path: path.clone(),
        version,
        ranges: ranges.clone(),
    });

    let loops = 160usize;
    let mut changed_count = 0usize;
    let start = Instant::now();
    for _ in 0..loops {
        let result = store.dispatch(Action::LspFoldingRanges {
            path: path.clone(),
            version,
            ranges: ranges.clone(),
        });
        if result.state_changed {
            changed_count += 1;
        }
    }
    let elapsed = start.elapsed();
    let avg_us = elapsed.as_secs_f64() * 1_000_000.0 / loops as f64;

    eprintln!(
        "[experiment] folding_ranges_fanout loops={} tabs={} ranges={} elapsed_ms={} avg_us={:.2} changed_count={}",
        loops,
        panes,
        ranges.len(),
        elapsed.as_millis(),
        avg_us,
        changed_count
    );

    for pane in 0..panes {
        let tab = store
            .state
            .editor
            .pane(pane)
            .and_then(|p| p.active_tab())
            .expect("tab exists");
        assert!(tab.has_folding_ranges());
    }
}

#[test]
fn experiment_completion_resolve_apply_scale_baseline() {
    let mut store = new_store();

    let items_count = 8000usize;
    let loops = 200usize;
    let all_items: Vec<LspCompletionItem> =
        (0..items_count as u64).map(test_completion_item).collect();
    let visible_items = all_items[..1500].to_vec();

    store.state.ui.completion.visible = true;
    store.state.ui.completion.all_items = all_items;
    store.state.ui.completion.visible_indices = (0..visible_items.len()).collect();
    store.state.ui.completion.rebuild_index_by_id();
    store.state.ui.completion.resolve_inflight = Some(1);

    let insert_range = LspRange {
        start: LspPosition {
            line: 0,
            character: 2,
        },
        end: LspPosition {
            line: 0,
            character: 5,
        },
    };
    let replace_range = LspRange {
        start: LspPosition {
            line: 0,
            character: 2,
        },
        end: LspPosition {
            line: 0,
            character: 8,
        },
    };
    let additional_text_edits = vec![LspTextEdit {
        range: replace_range,
        new_text: "resolved_text".to_string(),
    }];
    let command = Some(LspCommand {
        command: "editor.action.resolve".to_string(),
        arguments: Vec::new(),
    });

    let start = Instant::now();
    let mut changed_count = 0usize;
    for i in 0..loops {
        let id = ((i % items_count) + 1) as u64;
        let result = store.dispatch(Action::LspCompletionResolved {
            id,
            detail: Some("detail".to_string()),
            documentation: Some("doc".to_string()),
            insert_text: Some("resolved_text".to_string()),
            insert_text_format: Some(LspInsertTextFormat::Snippet),
            insert_range: Some(insert_range),
            replace_range: Some(replace_range),
            additional_text_edits: additional_text_edits.clone(),
            command: command.clone(),
        });
        if result.state_changed {
            changed_count += 1;
        }
    }
    let elapsed = start.elapsed();
    let avg_us = elapsed.as_secs_f64() * 1_000_000.0 / loops as f64;

    eprintln!(
        "[experiment] completion_resolve_apply loops={} all_items={} visible_items={} elapsed_ms={} avg_us={:.2} changed_count={}",
        loops,
        items_count,
        store.state.ui.completion.visible_len(),
        elapsed.as_millis(),
        avg_us,
        changed_count
    );

    assert!(changed_count >= 1);
    assert!(store
        .state
        .ui
        .completion
        .all_items
        .iter()
        .any(|item| item.documentation.as_deref() == Some("doc")));
}

#[test]
fn experiment_input_key_cycle_hot_path_baseline() {
    let mut store = new_store();
    store.state.ui.focus = FocusTarget::Editor;
    let _ = open_single_rust_tab_for_input(&mut store, "input_cycle.rs");

    let loops = 10_000usize;
    let (elapsed, changed) = run_input_key_cycle(&mut store, loops);
    let commands = loops * 4;
    let avg_ns = elapsed.as_secs_f64() * 1_000_000_000.0 / commands as f64;

    eprintln!(
        "[experiment] input_key_cycle loops={} commands={} elapsed_ms={} avg_ns_per_cmd={:.1} changed_count={}",
        loops,
        commands,
        elapsed.as_millis(),
        avg_ns,
        changed
    );

    assert!(changed > 0);
}

#[test]
fn experiment_lsp_completion_reuse_cache_burden() {
    fn setup_store(items: usize, visible_items: usize) -> Store {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        let (path, version) = open_single_rust_tab_for_input(&mut store, "completion_reuse.rs");

        let mut all_items: Vec<LspCompletionItem> =
            (0..items as u64).map(test_completion_item).collect();
        for item in &mut all_items {
            item.documentation = Some("doc".to_string());
            item.data = None;
        }

        let take = visible_items.min(all_items.len());
        store.state.ui.completion.visible = true;
        store.state.ui.completion.all_items = all_items;
        store.state.ui.completion.visible_indices = (0..take).collect();
        store.state.ui.completion.request = Some(crate::kernel::state::CompletionRequestContext {
            pane: 0,
            path,
            version,
        });
        store.state.ui.completion.pending_request = None;
        store.state.ui.completion.is_incomplete = false;
        store
    }

    fn run_reuse_loop(store: &mut Store, loops: usize) -> (std::time::Duration, usize) {
        let mut changed = 0usize;
        let start = Instant::now();
        for _ in 0..loops {
            store.state.ui.completion.session_started_at = Some(Instant::now());
            let result = store.dispatch(Action::RunCommand(Command::LspCompletion));
            changed += usize::from(result.state_changed);
        }
        (start.elapsed(), changed)
    }

    let loops = 1200usize;
    let mut small = setup_store(256, 128);
    let mut large = setup_store(10_000, 3000);

    let (small_elapsed, small_changed) = run_reuse_loop(&mut small, loops);
    let (large_elapsed, large_changed) = run_reuse_loop(&mut large, loops);

    let small_avg_ns = small_elapsed.as_secs_f64() * 1_000_000_000.0 / loops as f64;
    let large_avg_ns = large_elapsed.as_secs_f64() * 1_000_000_000.0 / loops as f64;
    let burden_pct = if small_avg_ns > 0.0 {
        ((large_avg_ns - small_avg_ns) / small_avg_ns) * 100.0
    } else {
        0.0
    };

    eprintln!(
        "[experiment] completion_reuse_cache_burden loops={} small_items=256 large_items=10000 small_avg_ns={:.1} large_avg_ns={:.1} burden_pct={:.2} small_changed={} large_changed={}",
        loops,
        small_avg_ns,
        large_avg_ns,
        burden_pct,
        small_changed,
        large_changed
    );

    assert!(small_changed > 0 || large_changed > 0);
}

#[test]
fn experiment_insert_char_capability_clone_burden() {
    fn trigger_vec(len: usize) -> Vec<char> {
        const POOL: [char; 18] = [
            '.', ':', ';', ',', '(', ')', '[', ']', '{', '}', '+', '-', '*', '/', '=', '<', '>',
            '?',
        ];
        (0..len).map(|i| POOL[i % POOL.len()]).collect()
    }

    fn setup_with_trigger_len(trigger_len: usize) -> Store {
        let mut store = new_store();
        store.state.ui.focus = FocusTarget::Editor;
        let _ = open_single_rust_tab_for_input(&mut store, "caps_clone.rs");
        install_rust_lsp_caps_with_triggers(
            &mut store,
            trigger_vec(trigger_len),
            trigger_vec(trigger_len),
        );
        store
    }

    let loops = 9000usize;
    let mut small = setup_with_trigger_len(0);
    let mut large = setup_with_trigger_len(512);

    let (small_elapsed, small_changed) = run_input_key_cycle(&mut small, loops);
    let (large_elapsed, large_changed) = run_input_key_cycle(&mut large, loops);

    let commands = (loops * 4) as f64;
    let small_avg_ns = small_elapsed.as_secs_f64() * 1_000_000_000.0 / commands;
    let large_avg_ns = large_elapsed.as_secs_f64() * 1_000_000_000.0 / commands;
    let burden_pct = if small_avg_ns > 0.0 {
        ((large_avg_ns - small_avg_ns) / small_avg_ns) * 100.0
    } else {
        0.0
    };

    eprintln!(
        "[experiment] insert_char_caps_clone_burden loops={} commands={} trigger_len_small=0 trigger_len_large=512 small_avg_ns={:.1} large_avg_ns={:.1} burden_pct={:.2} small_changed={} large_changed={}",
        loops,
        loops * 4,
        small_avg_ns,
        large_avg_ns,
        burden_pct,
        small_changed,
        large_changed
    );

    assert!(small_changed > 0);
    assert!(large_changed > 0);
}
