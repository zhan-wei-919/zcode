use super::Workbench;
use crate::kernel::lsp_registry;
use crate::kernel::services::adapters::perf;
use crate::kernel::services::adapters::{
    ClipboardService, GlobalSearchService, LspService, SearchService,
};
use crate::kernel::services::ports::{LspPosition, LspPositionEncoding, LspRange, LspTextChange};
use crate::kernel::state::PendingAction;
use crate::kernel::{Action as KernelAction, EditorAction, Effect as KernelEffect};
use crate::models::OpKind;
use ropey::Rope;
use rustc_hash::FxHashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

impl Workbench {
    pub(super) fn dispatch_kernel(&mut self, action: KernelAction) -> bool {
        if let KernelAction::LspDefinition { path, line, .. } = &action {
            self.pending_definition_highlight = Some(super::PendingDefinitionHighlight {
                path: path.clone(),
                row: *line as usize,
                armed_at: Instant::now(),
            });
        }

        let _scope = perf::scope("kernel.dispatch");
        let result = {
            let _scope = perf::scope("kernel.reduce");
            self.store.dispatch(action)
        };
        self.sync_editor_search_slots();
        self.sync_lsp();
        self.sync_file_watcher();
        {
            let _scope = perf::scope("kernel.effects");
            for effect in result.effects {
                self.run_effect(effect);
            }
        }
        let mut state_changed = result.state_changed;
        state_changed |= self.try_activate_definition_jump_highlight();
        state_changed
    }

    fn active_editor_location(
        &self,
    ) -> Option<(usize, crate::kernel::editor::TabId, PathBuf, usize)> {
        let pane = self.store.state().ui.editor_layout.active_pane;
        let tab = self
            .store
            .state()
            .editor
            .pane(pane)
            .and_then(|pane_state| pane_state.active_tab())?;
        let path = tab.path.clone()?;
        let (row, _col) = tab.buffer.cursor();
        Some((pane, tab.id, path, row))
    }

    fn try_activate_definition_jump_highlight(&mut self) -> bool {
        let Some((target_path, target_row, armed_at)) = self
            .pending_definition_highlight
            .as_ref()
            .map(|pending| (pending.path.clone(), pending.row, pending.armed_at))
        else {
            return false;
        };

        if armed_at.elapsed() >= super::DEFINITION_JUMP_PENDING_TIMEOUT {
            self.pending_definition_highlight = None;
            return false;
        }

        let Some((pane, tab_id, path, row)) = self.active_editor_location() else {
            return false;
        };
        if path != target_path || row != target_row {
            return false;
        }

        self.pending_definition_highlight = None;
        self.definition_jump_highlight = Some(super::DefinitionJumpHighlight {
            pane,
            tab_id,
            row,
            started_at: Instant::now(),
        });
        true
    }

    fn sync_editor_search_slots(&mut self) {
        let desired = self.store.state().ui.editor_layout.panes.max(1);

        if desired < self.editor_search_tasks.len() {
            for task in self.editor_search_tasks.iter().skip(desired).flatten() {
                task.cancel();
            }
        }

        self.editor_search_tasks.truncate(desired);
        self.editor_search_rx.truncate(desired);

        let current = self.editor_search_tasks.len();
        if desired > current {
            self.editor_search_tasks
                .extend(std::iter::repeat_with(|| None).take(desired - current));
            self.editor_search_rx
                .extend(std::iter::repeat_with(|| None).take(desired - current));
        }

        self.editor_mouse
            .resize_with(desired, super::mouse_tracker::EditorMouseTracker::new);
        self.sync_markdown_views();
    }

    fn run_effect(&mut self, effect: KernelEffect) {
        match effect {
            KernelEffect::LoadFile(path) => {
                let _scope = perf::scope("effect.load_file");
                self.runtime.load_file(path)
            }
            KernelEffect::LoadDir(path) => {
                let _scope = perf::scope("effect.load_dir");
                self.runtime.load_dir(path)
            }
            KernelEffect::CreateFile(path) => {
                let _scope = perf::scope("effect.create_file");
                self.runtime.create_file(path)
            }
            KernelEffect::CreateDir(path) => {
                let _scope = perf::scope("effect.create_dir");
                self.runtime.create_dir(path)
            }
            KernelEffect::RenamePath {
                from,
                to,
                overwrite,
            } => {
                let _scope = perf::scope("effect.rename_path");
                let root = self.store.state().workspace_root.clone();
                let root = root.as_path();
                if from.as_path() == root
                    || to.as_path() == root
                    || !from.starts_with(root)
                    || !to.starts_with(root)
                {
                    self.push_log_line(format!(
                        "[fs:rename_path] rejected out-of-workspace path: {} -> {}",
                        from.display(),
                        to.display()
                    ));
                    return;
                }

                // Default behavior: do not overwrite the destination.
                if !overwrite {
                    if let Ok(meta) = std::fs::symlink_metadata(&to) {
                        let rel = to
                            .strip_prefix(root)
                            .ok()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| to.to_string_lossy().to_string());

                        let message = if meta.is_dir() {
                            format!("Overwrite folder \"{}\" and all contents?", rel)
                        } else {
                            format!("Overwrite file \"{}\"?", rel)
                        };

                        let _ = self.dispatch_kernel(KernelAction::ShowConfirmDialog {
                            message,
                            on_confirm: PendingAction::RenamePath {
                                from,
                                to,
                                overwrite: true,
                            },
                        });
                        return;
                    }
                }

                self.runtime.rename_path(from, to, overwrite)
            }
            KernelEffect::CopyPath {
                from,
                to,
                overwrite,
            } => {
                let _scope = perf::scope("effect.copy_path");
                let root = self.store.state().workspace_root.clone();
                let root = root.as_path();
                if from.as_path() == root
                    || to.as_path() == root
                    || !from.starts_with(root)
                    || !to.starts_with(root)
                {
                    self.push_log_line(format!(
                        "[fs:copy_path] rejected out-of-workspace path: {} -> {}",
                        from.display(),
                        to.display()
                    ));
                    return;
                }

                if !overwrite {
                    if let Ok(meta) = std::fs::symlink_metadata(&to) {
                        let rel = to
                            .strip_prefix(root)
                            .ok()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| to.to_string_lossy().to_string());

                        let message = if meta.is_dir() {
                            format!("Overwrite folder \"{}\" and all contents?", rel)
                        } else {
                            format!("Overwrite file \"{}\"?", rel)
                        };

                        let _ = self.dispatch_kernel(KernelAction::ShowConfirmDialog {
                            message,
                            on_confirm: PendingAction::CopyPath {
                                from,
                                to,
                                overwrite: true,
                            },
                        });
                        return;
                    }
                }

                self.runtime.copy_path(from, to, overwrite)
            }
            KernelEffect::DeletePath { path, is_dir } => {
                let _scope = perf::scope("effect.delete_path");
                let root = self.store.state().workspace_root.clone();
                let root = root.as_path();
                if path.as_path() == root || !path.starts_with(root) {
                    self.push_log_line(format!(
                        "[fs:delete_path] rejected out-of-workspace path: {}",
                        path.display()
                    ));
                    return;
                }

                self.runtime.delete_path(path, is_dir)
            }
            KernelEffect::ReloadSettings => {
                let _scope = perf::scope("effect.reload_settings");
                self.reload_settings();
            }
            KernelEffect::OpenSettings => {
                let _scope = perf::scope("effect.open_settings");
                self.open_settings();
            }
            KernelEffect::StartGlobalSearch {
                root,
                pattern,
                case_sensitive,
                use_regex,
            } => {
                let _scope = perf::scope("effect.global_search");
                if let Some(task) = self.global_search_task.take() {
                    task.cancel();
                }

                let (tx, rx) = mpsc::sync_channel(super::GLOBAL_SEARCH_CHANNEL_CAP);
                self.global_search_rx = Some(rx);

                if let Some(service) = self.kernel_services.get::<GlobalSearchService>() {
                    let task = service.search_in_dir(root, pattern, case_sensitive, use_regex, tx);
                    let search_id = task.id();
                    self.global_search_task = Some(task);
                    let _ = self.dispatch_kernel(KernelAction::SearchStarted { search_id });
                }
            }
            KernelEffect::StartEditorSearch {
                pane,
                rope,
                pattern,
                case_sensitive,
                use_regex,
            } => {
                let _scope = perf::scope("effect.editor_search");
                self.sync_editor_search_slots();
                if pane >= self.editor_search_tasks.len() {
                    return;
                }

                if let Some(task) = self.editor_search_tasks[pane].take() {
                    task.cancel();
                }
                self.editor_search_rx[pane] = None;

                let (tx, rx) = mpsc::sync_channel(super::EDITOR_SEARCH_CHANNEL_CAP);
                self.editor_search_rx[pane] = Some(rx);

                if let Some(service) = self.kernel_services.get::<SearchService>() {
                    let task = service.search_in_rope(rope, pattern, case_sensitive, use_regex, tx);
                    let search_id = task.id();
                    self.editor_search_tasks[pane] = Some(task);
                    let _ =
                        self.dispatch_kernel(KernelAction::Editor(EditorAction::SearchStarted {
                            pane,
                            search_id,
                        }));
                }
            }
            KernelEffect::CancelEditorSearch { pane } => {
                let _scope = perf::scope("effect.cancel_search");
                self.sync_editor_search_slots();
                if pane >= self.editor_search_tasks.len() {
                    return;
                }
                if let Some(task) = self.editor_search_tasks[pane].take() {
                    task.cancel();
                }
                self.editor_search_rx[pane] = None;
            }
            KernelEffect::WriteFile {
                pane,
                path,
                version,
            } => {
                let _scope = perf::scope("effect.write_file");
                let Some(pane_state) = self.store.state().editor.pane(pane) else {
                    return;
                };
                let Some(tab) = pane_state
                    .tabs
                    .iter()
                    .find(|t| t.path.as_deref() == Some(path.as_path()))
                else {
                    return;
                };

                let rope = tab.buffer.rope().clone();
                self.runtime.write_file(pane, path, version, rope);
            }
            KernelEffect::SetClipboardText(text) => {
                let _scope = perf::scope("effect.clipboard_set");
                self.set_clipboard_text(&text);
            }
            KernelEffect::RequestClipboardText { pane } => {
                let _scope = perf::scope("effect.clipboard_get");
                let get_result = self
                    .kernel_services
                    .get_mut::<ClipboardService>()
                    .map(|svc| svc.get_text());

                match get_result {
                    Some(Ok(text)) if !text.is_empty() => {
                        let _ =
                            self.dispatch_kernel(KernelAction::Editor(EditorAction::InsertText {
                                pane,
                                text,
                            }));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        self.maybe_warn_clipboard_unavailable();
                        self.push_log_line(format!("[clipboard] {err}"));
                    }
                    None => self.maybe_warn_clipboard_unavailable(),
                }
            }
            KernelEffect::LspHoverRequest { path, line, column } => {
                let _scope = perf::scope("effect.lsp_hover");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_hover(
                        &path,
                        LspPosition {
                            line,
                            character: column,
                        },
                    );
                }
            }
            KernelEffect::LspDefinitionRequest { path, line, column } => {
                let _scope = perf::scope("effect.lsp_definition");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_definition(
                        &path,
                        LspPosition {
                            line,
                            character: column,
                        },
                    );
                }
            }
            KernelEffect::LspReferencesRequest { path, line, column } => {
                let _scope = perf::scope("effect.lsp_references");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_references(
                        &path,
                        LspPosition {
                            line,
                            character: column,
                        },
                    );
                }
            }
            KernelEffect::LspCodeActionRequest { path, line, column } => {
                let _scope = perf::scope("effect.lsp_code_action");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_code_action(
                        &path,
                        LspPosition {
                            line,
                            character: column,
                        },
                    );
                }
            }
            KernelEffect::LspCompletionRequest {
                path,
                line,
                column,
                trigger,
            } => {
                let _scope = perf::scope("effect.lsp_completion");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_completion(
                        &path,
                        LspPosition {
                            line,
                            character: column,
                        },
                        trigger,
                    );
                }
            }
            KernelEffect::LspCompletionResolveRequest { item } => {
                let _scope = perf::scope("effect.lsp_completion_resolve");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_completion_resolve(*item);
                }
            }
            KernelEffect::LspSemanticTokensRequest { path, version } => {
                let _scope = perf::scope("effect.lsp_semantic_tokens");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_semantic_tokens(&path, version);
                }
            }
            KernelEffect::LspSemanticTokensRangeRequest {
                path,
                version,
                range,
            } => {
                let _scope = perf::scope("effect.lsp_semantic_tokens_range");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_semantic_tokens_range(&path, range, version);
                }
            }
            KernelEffect::LspInlayHintsRequest {
                path,
                version,
                range,
            } => {
                let _scope = perf::scope("effect.lsp_inlay_hints");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_inlay_hints(&path, range, version);
                }
            }
            KernelEffect::LspFoldingRangeRequest { path, version } => {
                let _scope = perf::scope("effect.lsp_folding_range");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_folding_range(&path, version);
                }
            }
            KernelEffect::LspSignatureHelpRequest { path, line, column } => {
                let _scope = perf::scope("effect.lsp_signature_help");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_signature_help(
                        &path,
                        LspPosition {
                            line,
                            character: column,
                        },
                    );
                }
            }
            KernelEffect::LspRenameRequest {
                path,
                line,
                column,
                new_name,
            } => {
                let _scope = perf::scope("effect.lsp_rename");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_rename(
                        &path,
                        LspPosition {
                            line,
                            character: column,
                        },
                        new_name,
                    );
                }
            }
            KernelEffect::LspFormatRequest { path } => {
                let _scope = perf::scope("effect.lsp_format");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_format(&path);
                }
            }
            KernelEffect::LspDocumentSymbolsRequest { path } => {
                let _scope = perf::scope("effect.lsp_document_symbols");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_document_symbols(&path);
                }
            }
            KernelEffect::LspWorkspaceSymbolsRequest { query } => {
                let _scope = perf::scope("effect.lsp_workspace_symbols");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_workspace_symbols(query);
                }
            }
            KernelEffect::LspRangeFormatRequest { path, range } => {
                let _scope = perf::scope("effect.lsp_range_format");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.request_range_format(&path, range);
                }
            }
            KernelEffect::LspExecuteCommand { command, arguments } => {
                let _scope = perf::scope("effect.lsp_execute_command");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.execute_command(command, arguments);
                }
            }
            KernelEffect::LspShutdown => {
                let _scope = perf::scope("effect.lsp_shutdown");
                if let Some(service) = self.kernel_services.get_mut::<LspService>() {
                    service.shutdown();
                }
            }
            KernelEffect::ApplyFileEdits {
                position_encoding,
                resource_ops,
                edits,
            } => {
                let _scope = perf::scope("effect.apply_file_edits");
                self.runtime
                    .apply_file_edits(position_encoding, resource_ops, edits);
            }
            KernelEffect::GitDetectRepo { workspace_root } => {
                let _scope = perf::scope("effect.git_detect");
                self.runtime.git_detect_repo(workspace_root);
            }
            KernelEffect::GitRefreshStatus { repo_root } => {
                let _scope = perf::scope("effect.git_status");
                self.runtime.git_refresh_status(repo_root);
            }
            KernelEffect::GitRefreshDiff { repo_root, path } => {
                let _scope = perf::scope("effect.git_diff");
                self.runtime.git_refresh_diff(repo_root, path);
            }
            KernelEffect::GitListWorktrees { repo_root } => {
                let _scope = perf::scope("effect.git_worktrees");
                self.runtime.git_list_worktrees(repo_root);
            }
            KernelEffect::GitListBranches { repo_root } => {
                let _scope = perf::scope("effect.git_branches");
                self.runtime.git_list_branches(repo_root);
            }
            KernelEffect::GitWorktreeAdd { repo_root, branch } => {
                let _scope = perf::scope("effect.git_worktree_add");
                self.runtime.git_worktree_add(repo_root, branch);
            }
            KernelEffect::GitWorktreeResolve { repo_root, branch } => {
                let _scope = perf::scope("effect.git_worktree_resolve");
                self.runtime.git_worktree_resolve(repo_root, branch);
            }
            KernelEffect::TerminalSpawn {
                id,
                cwd,
                shell,
                args,
                cols,
                rows,
            } => {
                let _scope = perf::scope("effect.terminal_spawn");
                self.runtime
                    .terminal_spawn(id, cwd, shell, args, cols, rows);
            }
            KernelEffect::TerminalWrite { id, bytes } => {
                let _scope = perf::scope("effect.terminal_write");
                self.runtime.terminal_write(id, bytes);
            }
            KernelEffect::TerminalResize { id, cols, rows } => {
                let _scope = perf::scope("effect.terminal_resize");
                self.runtime.terminal_resize(id, cols, rows);
            }
            KernelEffect::TerminalKill { id } => {
                let _scope = perf::scope("effect.terminal_kill");
                self.runtime.terminal_kill(id);
            }
            KernelEffect::Restart { path, hard } => {
                self.pending_restart = Some(super::PendingRestart { path, hard });
            }
            KernelEffect::SaveThemeSettings { .. } => {
                // Theme saving is handled via debounce in tick.rs
            }
            KernelEffect::ReloadFile(request) => {
                self.runtime.reload_file(request);
            }
        }
    }
}

impl Workbench {
    fn sync_file_watcher(&mut self) {
        let Some(watcher) = self.file_watcher.as_mut() else {
            return;
        };

        let mut current_paths = FxHashSet::default();
        for pane in &self.store.state().editor.panes {
            for tab in &pane.tabs {
                if let Some(path) = tab.path.as_ref() {
                    current_paths.insert(path.clone());
                }
            }
        }

        watcher.sync_open_files(current_paths.iter().map(|path| path.as_path()));
    }

    fn sync_lsp(&mut self) {
        let Some(service) = self.kernel_services.get_mut::<LspService>() else {
            return;
        };

        let open_paths_version = self.store.state().editor.open_paths_version;
        if open_paths_version != self.lsp_open_paths_version {
            let mut current_paths = FxHashSet::default();
            let mut newly_open = Vec::new();

            for pane in &self.store.state().editor.panes {
                for tab in &pane.tabs {
                    let Some(path) = tab.path.as_ref() else {
                        continue;
                    };
                    if !lsp_registry::is_lsp_source_path(path) {
                        continue;
                    }
                    current_paths.insert(path.clone());
                    if !self.lsp_open_paths.contains(path) {
                        newly_open.push((path.clone(), tab));
                    }
                }
            }

            for path in self.lsp_open_paths.difference(&current_paths) {
                service.close_document(path);
            }

            for (path, tab) in newly_open {
                service.sync_document(&path, tab.edit_version, None, || tab.buffer.text());
            }

            self.lsp_open_paths_version = open_paths_version;
            self.lsp_open_paths = current_paths;
        }

        let pane = self.store.state().ui.editor_layout.active_pane;
        let Some(pane_state) = self.store.state().editor.pane(pane) else {
            return;
        };
        let Some(tab) = pane_state.active_tab() else {
            return;
        };
        let Some(path) = tab.path.as_ref() else {
            return;
        };
        if !lsp_registry::is_lsp_source_path(path) {
            return;
        }

        if !service.needs_sync(path, tab.edit_version) {
            return;
        }

        let encoding = lsp_position_encoding_for_path(self.store.state(), path);
        let change = lsp_change_from_tab(tab, encoding);
        service.sync_document(path, tab.edit_version, change, || tab.buffer.text());
    }
}

fn lsp_position_encoding_for_path(
    state: &crate::kernel::AppState,
    path: &std::path::Path,
) -> LspPositionEncoding {
    let Some((_language, key)) = lsp_registry::client_key_for_path(&state.workspace_root, path)
    else {
        return LspPositionEncoding::Utf16;
    };
    state
        .lsp
        .server_capabilities
        .get(&key)
        .map(|c| c.position_encoding)
        .unwrap_or(LspPositionEncoding::Utf16)
}

fn lsp_change_from_tab(
    tab: &crate::kernel::editor::EditorTabState,
    encoding: LspPositionEncoding,
) -> Option<LspTextChange> {
    let op = tab.last_edit_op.as_ref()?;
    if op.id != tab.history.head() {
        return None;
    }

    match &op.kind {
        OpKind::Insert { char_offset, text } => {
            let start = lsp_position_at(tab.buffer.rope(), *char_offset, encoding);
            Some(LspTextChange {
                range: Some(LspRange { start, end: start }),
                text: text.clone(),
            })
        }
        OpKind::Delete { start, deleted, .. } => {
            let start_pos = lsp_position_at(tab.buffer.rope(), *start, encoding);
            let end_pos = lsp_position_after_text(start_pos, deleted, encoding);
            Some(LspTextChange {
                range: Some(LspRange {
                    start: start_pos,
                    end: end_pos,
                }),
                text: String::new(),
            })
        }
        OpKind::Replace {
            start,
            deleted,
            inserted,
            ..
        } => {
            let start_pos = lsp_position_at(tab.buffer.rope(), *start, encoding);
            let end_pos = lsp_position_after_text(start_pos, deleted, encoding);
            Some(LspTextChange {
                range: Some(LspRange {
                    start: start_pos,
                    end: end_pos,
                }),
                text: inserted.clone(),
            })
        }
    }
}

fn lsp_position_at(rope: &Rope, char_offset: usize, encoding: LspPositionEncoding) -> LspPosition {
    let line = rope.char_to_line(char_offset);
    let line_start = rope.line_to_char(line);
    let col_chars = char_offset.saturating_sub(line_start);
    let line_slice = rope.line(line);
    let col = match encoding {
        LspPositionEncoding::Utf8 => line_slice
            .chars()
            .take(col_chars)
            .map(|ch| ch.len_utf8() as u32)
            .sum(),
        LspPositionEncoding::Utf16 => line_slice
            .chars()
            .take(col_chars)
            .map(|ch| ch.len_utf16() as u32)
            .sum(),
        LspPositionEncoding::Utf32 => col_chars as u32,
    };
    LspPosition {
        line: line as u32,
        character: col,
    }
}

fn lsp_position_after_text(
    mut pos: LspPosition,
    text: &str,
    encoding: LspPositionEncoding,
) -> LspPosition {
    let mut line = pos.line;
    let mut col = pos.character;
    for ch in text.chars() {
        if ch == '\n' {
            line = line.saturating_add(1);
            col = 0;
            continue;
        }
        if ch == '\r' {
            continue;
        }
        col = col.saturating_add(match encoding {
            LspPositionEncoding::Utf8 => ch.len_utf8() as u32,
            LspPositionEncoding::Utf16 => ch.len_utf16() as u32,
            LspPositionEncoding::Utf32 => 1,
        });
    }
    pos.line = line;
    pos.character = col;
    pos
}
