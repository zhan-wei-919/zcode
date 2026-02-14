use super::convert::{
    code_actions_from_lsp, command_from_lsp, completion_items, decode_semantic_tokens,
    definition_location, diagnostics_from_params, documentation_text, hover_text,
    inlay_hints_from_lsp, insert_text_format, push_document_symbols, range_from_lsp,
    server_capabilities_from_lsp, signature_help_text, symbol_item_from_symbol_information,
    symbol_item_from_workspace_symbol, workspace_edit_from_lsp,
};
use super::LspClient;
use crate::kernel::locations::LocationItem;
use crate::kernel::services::ports::{
    LspFoldingRange, LspRange, LspServerKind, LspTextEdit, LspWorkspaceEdit, LspWorkspaceFileEdit,
};
use crate::kernel::services::KernelServiceContext;
use crate::kernel::Action;
use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use rustc_hash::FxHashMap;
use serde_json::Value;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) struct LspProcess {
    pub(super) tx: mpsc::Sender<Message>,
    pub(super) pending: Arc<Mutex<LspPending>>,
    pub(super) child: Arc<Mutex<std::process::Child>>,
}

impl Drop for LspProcess {
    fn drop(&mut self) {
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        let _ = child.kill();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InitState {
    Starting,
    Ready,
    Failed,
}

pub(super) struct LspPending {
    pub(super) state: InitState,
    pub(super) queue: VecDeque<Message>,
}

#[derive(Debug, Clone)]
pub(super) enum LspRequestKind {
    Hover,
    Definition,
    References,
    DocumentSymbols {
        path: PathBuf,
    },
    WorkspaceSymbols,
    CodeAction,
    Completion,
    CompletionResolve {
        item_id: u64,
    },
    SemanticTokens {
        path: PathBuf,
        version: u64,
    },
    SemanticTokensRange {
        path: PathBuf,
        version: u64,
        range: LspRange,
    },
    InlayHints {
        path: PathBuf,
        version: u64,
        range: LspRange,
    },
    FoldingRange {
        path: PathBuf,
        version: u64,
    },
    SignatureHelp,
    Rename,
    Format {
        path: PathBuf,
    },
    ExecuteCommand,
    Shutdown,
}

impl LspClient {
    pub(super) fn next_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    pub(super) fn send_message(&mut self, msg: Message, requires_init: bool) {
        let Some(process) = self.process.as_ref() else {
            return;
        };

        if requires_init {
            if let Ok(mut pending) = process.pending.lock() {
                match pending.state {
                    InitState::Starting => {
                        pending.queue.push_back(msg);
                        return;
                    }
                    InitState::Failed => return,
                    InitState::Ready => {
                        if !pending.queue.is_empty() {
                            pending.queue.push_back(msg);
                            drop(pending);
                            Self::flush_pending(process);
                            return;
                        }
                    }
                }
            }
        }

        if process.tx.send(msg).is_err() {
            tracing::warn!("lsp writer channel closed");
            mark_failed(&process.pending);
            return;
        }

        if requires_init {
            Self::flush_pending(process);
        }
    }

    fn flush_pending(process: &LspProcess) {
        let queued = {
            let Ok(mut pending) = process.pending.lock() else {
                return;
            };
            if pending.state != InitState::Ready {
                return;
            }
            pending.queue.drain(..).collect::<Vec<_>>()
        };

        for msg in queued {
            if process.tx.send(msg).is_err() {
                mark_failed(&process.pending);
                break;
            }
        }
    }

    pub(super) fn track_request(&mut self, id: i32, kind: LspRequestKind) {
        if let Ok(mut map) = self.pending_requests.lock() {
            map.insert(RequestId::from(id), kind);
        }
    }

    pub(super) fn untrack_request(&mut self, id: i32) {
        if let Ok(mut map) = self.pending_requests.lock() {
            map.remove(&RequestId::from(id));
        }
    }
}

fn mark_failed(pending: &Arc<Mutex<LspPending>>) {
    if let Ok(mut pending) = pending.lock() {
        pending.queue.clear();
        pending.state = InitState::Failed;
    }
}

pub(super) fn child_watch_loop(
    child: Arc<Mutex<std::process::Child>>,
    pending: Arc<Mutex<LspPending>>,
) {
    loop {
        let status = {
            let Ok(mut child) = child.lock() else {
                break;
            };
            child.try_wait()
        };

        match status {
            Ok(Some(status)) => {
                tracing::warn!(status = ?status, "lsp process exited");
                break;
            }
            Ok(None) => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                tracing::warn!(error = %e, "lsp process wait failed");
                break;
            }
        }
    }

    mark_failed(&pending);
}

pub(super) fn writer_loop(
    stdin: std::process::ChildStdin,
    rx: mpsc::Receiver<Message>,
    pending: Arc<Mutex<LspPending>>,
) {
    let mut writer = BufWriter::new(stdin);
    while let Ok(msg) = rx.recv() {
        if msg.write(&mut writer).is_err() {
            break;
        }
    }
    mark_failed(&pending);
}

pub(super) struct ReaderLoopArgs {
    pub(super) server: LspServerKind,
    pub(super) root: PathBuf,
    pub(super) stdout: std::process::ChildStdout,
    pub(super) ctx: KernelServiceContext,
    pub(super) init_id: i32,
    pub(super) pending: Arc<Mutex<LspPending>>,
    pub(super) pending_requests: Arc<Mutex<FxHashMap<RequestId, LspRequestKind>>>,
    pub(super) latest_hover: Arc<AtomicI32>,
    pub(super) latest_definition: Arc<AtomicI32>,
    pub(super) latest_references: Arc<AtomicI32>,
    pub(super) latest_document_symbols: Arc<AtomicI32>,
    pub(super) latest_workspace_symbols: Arc<AtomicI32>,
    pub(super) latest_code_action: Arc<AtomicI32>,
    pub(super) latest_completion: Arc<AtomicI32>,
    pub(super) latest_completion_resolve: Arc<AtomicI32>,
    pub(super) latest_semantic_tokens_by_path: Arc<Mutex<FxHashMap<PathBuf, i32>>>,
    pub(super) latest_inlay_hints: Arc<AtomicI32>,
    pub(super) latest_folding_range: Arc<AtomicI32>,
    pub(super) latest_signature_help: Arc<AtomicI32>,
    pub(super) latest_format: Arc<AtomicI32>,
    pub(super) latest_rename: Arc<AtomicI32>,
    pub(super) latest_shutdown: Arc<AtomicI32>,
    pub(super) tx: mpsc::Sender<Message>,
    pub(super) workspace_folders: Option<Vec<lsp_types::WorkspaceFolder>>,
}

pub(super) fn reader_loop(args: ReaderLoopArgs) {
    let ReaderLoopArgs {
        server,
        root,
        stdout,
        ctx,
        init_id,
        pending,
        pending_requests,
        latest_hover,
        latest_definition,
        latest_references,
        latest_document_symbols,
        latest_workspace_symbols,
        latest_code_action,
        latest_completion,
        latest_completion_resolve,
        latest_semantic_tokens_by_path,
        latest_inlay_hints,
        latest_folding_range,
        latest_signature_help,
        latest_format,
        latest_rename,
        latest_shutdown,
        tx,
        workspace_folders,
    } = args;
    let mut reader = BufReader::new(stdout);
    let init_req_id = RequestId::from(init_id);

    loop {
        let msg = match Message::read(&mut reader) {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                mark_failed(&pending);
                return;
            }
            Err(e) => {
                tracing::warn!(error = %e, "lsp read failed");
                mark_failed(&pending);
                return;
            }
        };

        match msg {
            Message::Request(req) => {
                tracing::debug!(method = %req.method, "lsp server request");
                let resp = handle_server_request(req, &workspace_folders, &ctx);
                let _ = tx.send(Message::Response(resp));
            }
            Message::Notification(not) => {
                tracing::debug!(method = %not.method, "lsp notification");
                if not.method == lsp_types::notification::PublishDiagnostics::METHOD {
                    if let Ok(params) =
                        serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(not.params)
                    {
                        if let Some((path, items)) = diagnostics_from_params(params) {
                            ctx.dispatch(Action::LspDiagnostics { path, items });
                        }
                    }
                } else if not.method == lsp_types::notification::LogMessage::METHOD {
                    if let Ok(params) =
                        serde_json::from_value::<lsp_types::LogMessageParams>(not.params.clone())
                    {
                        tracing::debug!(message = %params.message, "lsp log message");
                    }
                } else if not.method == "$/progress" {
                    if let Ok(params) =
                        serde_json::from_value::<lsp_types::ProgressParams>(not.params)
                    {
                        if let lsp_types::ProgressParamsValue::WorkDone(
                            lsp_types::WorkDoneProgress::End(_),
                        ) = params.value
                        {
                            tracing::info!(token = ?params.token, "lsp progress end");
                            ctx.dispatch(Action::LspProgressEnd);
                        }
                    }
                }
            }
            Message::Response(resp) => {
                if resp.id == init_req_id {
                    if let Some(err) = resp.error {
                        tracing::error!(error = %err.message, code = err.code, "lsp initialize failed");
                        mark_failed(&pending);
                        continue;
                    }

                    if let Some(result) = resp.result.as_ref() {
                        if let Ok(result) =
                            serde_json::from_value::<lsp_types::InitializeResult>(result.clone())
                        {
                            let caps = server_capabilities_from_lsp(&result.capabilities);
                            ctx.dispatch(Action::LspServerCapabilities {
                                server,
                                root: root.clone(),
                                capabilities: caps,
                            });
                        }
                    }

                    let queued = match pending.lock() {
                        Ok(mut pending) => {
                            let queued: Vec<Message> = pending.queue.drain(..).collect();
                            pending.state = InitState::Ready;
                            queued
                        }
                        Err(_) => Vec::new(),
                    };

                    let init_not = Message::Notification(Notification::new(
                        lsp_types::notification::Initialized::METHOD.to_string(),
                        lsp_types::InitializedParams {},
                    ));
                    let _ = tx.send(init_not);
                    for msg in queued {
                        let _ = tx.send(msg);
                    }
                    continue;
                }

                if let Some(kind) = pending_requests
                    .lock()
                    .ok()
                    .and_then(|mut map| map.remove(&resp.id))
                {
                    let is_latest = match &kind {
                        LspRequestKind::Hover => {
                            resp.id == RequestId::from(latest_hover.load(Ordering::Relaxed))
                        }
                        LspRequestKind::Definition => {
                            resp.id == RequestId::from(latest_definition.load(Ordering::Relaxed))
                        }
                        LspRequestKind::References => {
                            resp.id == RequestId::from(latest_references.load(Ordering::Relaxed))
                        }
                        LspRequestKind::DocumentSymbols { .. } => {
                            resp.id
                                == RequestId::from(latest_document_symbols.load(Ordering::Relaxed))
                        }
                        LspRequestKind::WorkspaceSymbols => {
                            resp.id
                                == RequestId::from(latest_workspace_symbols.load(Ordering::Relaxed))
                        }
                        LspRequestKind::CodeAction => {
                            resp.id == RequestId::from(latest_code_action.load(Ordering::Relaxed))
                        }
                        LspRequestKind::Completion => {
                            resp.id == RequestId::from(latest_completion.load(Ordering::Relaxed))
                        }
                        LspRequestKind::CompletionResolve { .. } => {
                            resp.id
                                == RequestId::from(
                                    latest_completion_resolve.load(Ordering::Relaxed),
                                )
                        }
                        LspRequestKind::SemanticTokens { .. }
                        | LspRequestKind::SemanticTokensRange { .. } => {
                            let latest = match &kind {
                                LspRequestKind::SemanticTokens { path, .. }
                                | LspRequestKind::SemanticTokensRange { path, .. } => {
                                    latest_semantic_tokens_by_path
                                        .lock()
                                        .ok()
                                        .and_then(|map| map.get(path).copied())
                                }
                                _ => None,
                            };
                            latest.is_some_and(|id| resp.id == RequestId::from(id))
                        }
                        LspRequestKind::InlayHints { .. } => {
                            resp.id == RequestId::from(latest_inlay_hints.load(Ordering::Relaxed))
                        }
                        LspRequestKind::FoldingRange { .. } => {
                            resp.id == RequestId::from(latest_folding_range.load(Ordering::Relaxed))
                        }
                        LspRequestKind::SignatureHelp => {
                            resp.id
                                == RequestId::from(latest_signature_help.load(Ordering::Relaxed))
                        }
                        LspRequestKind::Rename => {
                            resp.id == RequestId::from(latest_rename.load(Ordering::Relaxed))
                        }
                        LspRequestKind::Format { .. } => {
                            resp.id == RequestId::from(latest_format.load(Ordering::Relaxed))
                        }
                        LspRequestKind::ExecuteCommand => true,
                        LspRequestKind::Shutdown => {
                            resp.id == RequestId::from(latest_shutdown.load(Ordering::Relaxed))
                        }
                    };
                    tracing::debug!(id = ?resp.id, kind = ?kind, is_latest, "lsp response matched");
                    if is_latest {
                        if matches!(kind, LspRequestKind::Shutdown) {
                            let exit = Message::Notification(Notification::new(
                                lsp_types::notification::Exit::METHOD.to_string(),
                                (),
                            ));
                            let _ = tx.send(exit);
                        } else {
                            handle_response(kind, resp, &ctx);
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn stderr_loop(stderr: std::process::ChildStderr) {
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    let mut window_started = Instant::now();
    let mut emitted = 0usize;
    let mut dropped = 0usize;
    let window = Duration::from_secs(1);
    let max_lines = 20usize;
    loop {
        if window_started.elapsed() >= window {
            if dropped > 0 {
                tracing::warn!(dropped, "lsp stderr rate-limited");
            }
            window_started = Instant::now();
            emitted = 0;
            dropped = 0;
        }

        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    continue;
                }

                if emitted < max_lines {
                    tracing::warn!("lsp: {}", trimmed);
                    emitted += 1;
                } else {
                    dropped += 1;
                }
            }
            Err(_) => break,
        }
    }

    if dropped > 0 {
        tracing::warn!(dropped, "lsp stderr rate-limited");
    }
}

fn handle_server_request(
    req: Request,
    workspace_folders: &Option<Vec<lsp_types::WorkspaceFolder>>,
    ctx: &KernelServiceContext,
) -> Response {
    match req.method.as_str() {
        m if m == lsp_types::request::WorkspaceConfiguration::METHOD => {
            let params = serde_json::from_value::<lsp_types::ConfigurationParams>(req.params)
                .unwrap_or_default();
            let items = vec![Value::Null; params.items.len()];
            Response::new_ok(req.id, items)
        }
        m if m == lsp_types::request::ApplyWorkspaceEdit::METHOD => {
            let params =
                match serde_json::from_value::<lsp_types::ApplyWorkspaceEditParams>(req.params) {
                    Ok(params) => params,
                    Err(err) => {
                        return Response::new_err(
                            req.id,
                            ErrorCode::InvalidParams as i32,
                            format!("invalid params: {err}"),
                        );
                    }
                };

            let edit = workspace_edit_from_lsp(params.edit);
            if !edit.changes.is_empty() {
                ctx.dispatch(Action::LspApplyWorkspaceEdit { edit });
            }

            Response::new_ok(
                req.id,
                lsp_types::ApplyWorkspaceEditResponse {
                    applied: true,
                    failure_reason: None,
                    failed_change: None,
                },
            )
        }
        m if m == lsp_types::request::WorkspaceFoldersRequest::METHOD => {
            Response::new_ok(req.id, workspace_folders.clone())
        }
        m if m == lsp_types::request::WorkDoneProgressCreate::METHOD => {
            Response::new_ok(req.id, ())
        }
        m if m == lsp_types::request::RegisterCapability::METHOD => Response::new_ok(req.id, ()),
        m if m == lsp_types::request::UnregisterCapability::METHOD => Response::new_ok(req.id, ()),
        m if m == lsp_types::request::ShowMessageRequest::METHOD => {
            Response::new_ok(req.id, Option::<lsp_types::MessageActionItem>::None)
        }
        _ => Response::new_err(
            req.id,
            ErrorCode::MethodNotFound as i32,
            "Method not found".to_string(),
        ),
    }
}

pub(super) fn handle_response(kind: LspRequestKind, resp: Response, ctx: &KernelServiceContext) {
    let response_start = Instant::now();
    let kind_label = match &kind {
        LspRequestKind::Hover => "hover",
        LspRequestKind::Definition => "definition",
        LspRequestKind::References => "references",
        LspRequestKind::DocumentSymbols { .. } => "documentSymbols",
        LspRequestKind::WorkspaceSymbols => "workspaceSymbols",
        LspRequestKind::CodeAction => "codeAction",
        LspRequestKind::Completion => "completion",
        LspRequestKind::CompletionResolve { .. } => "completionResolve",
        LspRequestKind::SemanticTokens { .. } => "semanticTokens",
        LspRequestKind::SemanticTokensRange { .. } => "semanticTokensRange",
        LspRequestKind::InlayHints { .. } => "inlayHints",
        LspRequestKind::FoldingRange { .. } => "foldingRange",
        LspRequestKind::SignatureHelp => "signatureHelp",
        LspRequestKind::Rename => "rename",
        LspRequestKind::Format { .. } => "format",
        LspRequestKind::ExecuteCommand => "executeCommand",
        LspRequestKind::Shutdown => "shutdown",
    };

    if let Some(err) = resp.error {
        let is_optional_method = matches!(
            kind,
            LspRequestKind::SemanticTokens { .. }
                | LspRequestKind::SemanticTokensRange { .. }
                | LspRequestKind::InlayHints { .. }
                | LspRequestKind::FoldingRange { .. }
        );
        if err.code == ErrorCode::MethodNotFound as i32 && is_optional_method {
            tracing::debug!(code = err.code, error = %err.message, "lsp method not supported");
        } else {
            tracing::warn!(code = err.code, error = %err.message, "lsp request failed");
        }
        match &kind {
            LspRequestKind::Hover => ctx.dispatch(Action::LspHover {
                text: String::new(),
            }),
            LspRequestKind::References => ctx.dispatch(Action::LspReferences { items: Vec::new() }),
            LspRequestKind::DocumentSymbols { .. } | LspRequestKind::WorkspaceSymbols => {
                ctx.dispatch(Action::LspSymbols { items: Vec::new() })
            }
            LspRequestKind::CodeAction => {
                ctx.dispatch(Action::LspCodeActions { items: Vec::new() })
            }
            LspRequestKind::Completion => ctx.dispatch(Action::LspCompletion {
                items: Vec::new(),
                is_incomplete: false,
            }),
            LspRequestKind::SemanticTokens { .. } | LspRequestKind::SemanticTokensRange { .. } => {}
            LspRequestKind::InlayHints { .. } => {}
            LspRequestKind::FoldingRange { path, version } => {
                ctx.dispatch(Action::LspFoldingRanges {
                    path: path.clone(),
                    version: *version,
                    ranges: Vec::new(),
                })
            }
            LspRequestKind::SignatureHelp => ctx.dispatch(Action::LspSignatureHelp {
                text: String::new(),
            }),
            LspRequestKind::Format { path } => {
                ctx.dispatch(Action::LspFormatCompleted { path: path.clone() })
            }
            _ => {}
        }
        return;
    }

    let Some(result) = resp.result else {
        match &kind {
            LspRequestKind::Hover => ctx.dispatch(Action::LspHover {
                text: String::new(),
            }),
            LspRequestKind::References => ctx.dispatch(Action::LspReferences { items: Vec::new() }),
            LspRequestKind::DocumentSymbols { .. } | LspRequestKind::WorkspaceSymbols => {
                ctx.dispatch(Action::LspSymbols { items: Vec::new() })
            }
            LspRequestKind::CodeAction => {
                ctx.dispatch(Action::LspCodeActions { items: Vec::new() })
            }
            LspRequestKind::Completion => ctx.dispatch(Action::LspCompletion {
                items: Vec::new(),
                is_incomplete: false,
            }),
            LspRequestKind::SemanticTokens { .. } | LspRequestKind::SemanticTokensRange { .. } => {}
            LspRequestKind::InlayHints { .. } => {}
            LspRequestKind::FoldingRange { path, version } => {
                ctx.dispatch(Action::LspFoldingRanges {
                    path: path.clone(),
                    version: *version,
                    ranges: Vec::new(),
                })
            }
            LspRequestKind::SignatureHelp => ctx.dispatch(Action::LspSignatureHelp {
                text: String::new(),
            }),
            LspRequestKind::Format { path } => {
                ctx.dispatch(Action::LspFormatCompleted { path: path.clone() })
            }
            _ => {}
        }
        return;
    };

    match kind {
        LspRequestKind::Hover => {
            let hover = serde_json::from_value::<Option<lsp_types::Hover>>(result)
                .ok()
                .flatten();
            let text = hover.and_then(|h| hover_text(&h)).unwrap_or_default();
            tracing::debug!(text_len = text.len(), "lsp hover response");
            ctx.dispatch(Action::LspHover { text });
        }
        LspRequestKind::Definition => {
            let resp = serde_json::from_value::<Option<lsp_types::GotoDefinitionResponse>>(result)
                .ok()
                .flatten();
            if let Some((path, line, column)) = resp.and_then(definition_location) {
                ctx.dispatch(Action::LspDefinition { path, line, column });
            }
        }
        LspRequestKind::References => {
            let resp = serde_json::from_value::<Option<Vec<lsp_types::Location>>>(result)
                .ok()
                .flatten()
                .unwrap_or_default();

            let mut items = Vec::with_capacity(resp.len());
            for loc in resp {
                let Ok(path) = loc.uri.to_file_path() else {
                    continue;
                };
                items.push(LocationItem {
                    path,
                    line: loc.range.start.line,
                    column: loc.range.start.character,
                });
            }

            ctx.dispatch(Action::LspReferences { items });
        }
        LspRequestKind::DocumentSymbols { path } => {
            let resp = serde_json::from_value::<Option<lsp_types::DocumentSymbolResponse>>(result)
                .ok()
                .flatten();

            let mut items = Vec::new();
            if let Some(resp) = resp {
                match resp {
                    lsp_types::DocumentSymbolResponse::Nested(symbols) => {
                        push_document_symbols(&path, &symbols, 0, &mut items);
                    }
                    lsp_types::DocumentSymbolResponse::Flat(symbols) => {
                        for sym in symbols {
                            if let Some(item) =
                                symbol_item_from_symbol_information(Some(&path), sym)
                            {
                                items.push(item);
                            }
                        }
                    }
                }
            }

            ctx.dispatch(Action::LspSymbols { items });
        }
        LspRequestKind::WorkspaceSymbols => {
            let resp = serde_json::from_value::<Option<lsp_types::WorkspaceSymbolResponse>>(result)
                .ok()
                .flatten();

            let mut items = Vec::new();
            if let Some(resp) = resp {
                match resp {
                    lsp_types::WorkspaceSymbolResponse::Flat(symbols) => {
                        for sym in symbols {
                            if let Some(item) = symbol_item_from_symbol_information(None, sym) {
                                items.push(item);
                            }
                        }
                    }
                    lsp_types::WorkspaceSymbolResponse::Nested(symbols) => {
                        for sym in symbols {
                            if let Some(item) = symbol_item_from_workspace_symbol(sym) {
                                items.push(item);
                            }
                        }
                    }
                }
            }

            ctx.dispatch(Action::LspSymbols { items });
        }
        LspRequestKind::CodeAction => {
            let resp =
                serde_json::from_value::<Option<Vec<lsp_types::CodeActionOrCommand>>>(result)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
            let items = code_actions_from_lsp(resp);
            ctx.dispatch(Action::LspCodeActions { items });
        }
        LspRequestKind::Completion => {
            let resp = serde_json::from_value::<Option<lsp_types::CompletionResponse>>(result)
                .ok()
                .flatten();
            let (items, is_incomplete) = resp.map(completion_items).unwrap_or_default();
            tracing::debug!(
                items_len = items.len(),
                is_incomplete,
                "lsp completion response"
            );
            ctx.dispatch(Action::LspCompletion {
                items,
                is_incomplete,
            });
        }
        LspRequestKind::CompletionResolve { item_id } => {
            let resp = serde_json::from_value::<lsp_types::CompletionItem>(result).ok();
            let Some(resp) = resp else {
                return;
            };

            let detail = resp
                .detail
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty());
            let documentation = resp
                .documentation
                .as_ref()
                .and_then(documentation_text)
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty());
            let additional_text_edits = resp
                .additional_text_edits
                .unwrap_or_default()
                .into_iter()
                .map(|edit| LspTextEdit {
                    range: range_from_lsp(edit.range),
                    new_text: edit.new_text,
                })
                .collect::<Vec<_>>();
            let command = resp.command.map(command_from_lsp);

            let mut insert_range = None;
            let mut replace_range = None;
            let mut insert_text = None;

            if let Some(text_edit) = resp.text_edit {
                match text_edit {
                    lsp_types::CompletionTextEdit::Edit(edit) => {
                        let range = range_from_lsp(edit.range);
                        insert_range = Some(range);
                        replace_range = Some(range);
                        insert_text = Some(edit.new_text);
                    }
                    lsp_types::CompletionTextEdit::InsertAndReplace(edit) => {
                        insert_range = Some(range_from_lsp(edit.insert));
                        replace_range = Some(range_from_lsp(edit.replace));
                        insert_text = Some(edit.new_text);
                    }
                }
            }

            if insert_text.is_none() {
                insert_text = resp.insert_text;
            }

            let insert_text_format = if insert_text.is_some() {
                Some(insert_text_format(resp.insert_text_format))
            } else {
                None
            };

            ctx.dispatch(Action::LspCompletionResolved {
                id: item_id,
                detail,
                documentation,
                insert_text,
                insert_text_format,
                insert_range,
                replace_range,
                additional_text_edits,
                command,
            });
        }
        LspRequestKind::SemanticTokens { path, version } => {
            let resp = serde_json::from_value::<Option<lsp_types::SemanticTokensResult>>(result)
                .ok()
                .flatten();
            let Some(resp) = resp else {
                return;
            };
            let tokens = match resp {
                lsp_types::SemanticTokensResult::Tokens(tokens) => tokens.data,
                lsp_types::SemanticTokensResult::Partial(tokens) => tokens.data,
            };
            let tokens = decode_semantic_tokens(tokens);
            ctx.dispatch(Action::LspSemanticTokens {
                path,
                version,
                tokens,
            });
        }
        LspRequestKind::SemanticTokensRange {
            path,
            version,
            range,
        } => {
            let resp =
                serde_json::from_value::<Option<lsp_types::SemanticTokensRangeResult>>(result)
                    .ok()
                    .flatten();
            let Some(resp) = resp else {
                return;
            };
            let tokens = match resp {
                lsp_types::SemanticTokensRangeResult::Tokens(tokens) => tokens.data,
                lsp_types::SemanticTokensRangeResult::Partial(tokens) => tokens.data,
            };
            let tokens = decode_semantic_tokens(tokens);
            ctx.dispatch(Action::LspSemanticTokensRange {
                path,
                version,
                range,
                tokens,
            });
        }
        LspRequestKind::InlayHints {
            path,
            version,
            range,
        } => {
            let resp = serde_json::from_value::<Option<Vec<lsp_types::InlayHint>>>(result)
                .ok()
                .flatten();
            let Some(resp) = resp else {
                tracing::debug!(
                    path = %path.display(),
                    version,
                    start_line = range.start.line,
                    start_char = range.start.character,
                    end_line = range.end.line,
                    end_char = range.end.character,
                    "lsp inlay hints response: null"
                );
                return;
            };
            tracing::debug!(
                path = %path.display(),
                version,
                start_line = range.start.line,
                start_char = range.start.character,
                end_line = range.end.line,
                end_char = range.end.character,
                hint_count = resp.len(),
                "lsp inlay hints response"
            );
            let hints = inlay_hints_from_lsp(resp);
            ctx.dispatch(Action::LspInlayHints {
                path,
                version,
                range,
                hints,
            });
        }
        LspRequestKind::FoldingRange { path, version } => {
            let resp = serde_json::from_value::<Option<Vec<lsp_types::FoldingRange>>>(result)
                .ok()
                .flatten()
                .unwrap_or_default();

            let mut ranges = Vec::with_capacity(resp.len().min(512));
            for range in resp.into_iter().take(2048) {
                let start = range.start_line;
                let end = range.end_line;
                if end <= start {
                    continue;
                }
                ranges.push(LspFoldingRange {
                    start_line: start,
                    end_line: end,
                });
            }

            ranges.sort_by(|a, b| {
                a.start_line
                    .cmp(&b.start_line)
                    .then(a.end_line.cmp(&b.end_line))
            });
            ranges.dedup();

            ctx.dispatch(Action::LspFoldingRanges {
                path,
                version,
                ranges,
            });
        }
        LspRequestKind::SignatureHelp => {
            let resp = serde_json::from_value::<Option<lsp_types::SignatureHelp>>(result)
                .ok()
                .flatten();
            let text = resp
                .as_ref()
                .and_then(signature_help_text)
                .unwrap_or_default();
            ctx.dispatch(Action::LspSignatureHelp { text });
        }
        LspRequestKind::Rename => {
            let edit = serde_json::from_value::<Option<lsp_types::WorkspaceEdit>>(result)
                .ok()
                .flatten();
            let Some(edit) = edit else {
                return;
            };

            let edit = workspace_edit_from_lsp(edit);
            if edit.changes.is_empty() {
                return;
            }

            ctx.dispatch(Action::LspApplyWorkspaceEdit { edit });
        }
        LspRequestKind::Format { path } => {
            let resp = serde_json::from_value::<Option<Vec<lsp_types::TextEdit>>>(result)
                .ok()
                .flatten()
                .unwrap_or_default();
            if !resp.is_empty() {
                let edits = resp
                    .into_iter()
                    .map(|edit| LspTextEdit {
                        range: range_from_lsp(edit.range),
                        new_text: edit.new_text,
                    })
                    .collect();

                ctx.dispatch(Action::LspApplyWorkspaceEdit {
                    edit: LspWorkspaceEdit {
                        changes: vec![LspWorkspaceFileEdit {
                            path: path.clone(),
                            edits,
                        }],
                        ..Default::default()
                    },
                });
            }

            ctx.dispatch(Action::LspFormatCompleted { path });
        }
        LspRequestKind::ExecuteCommand => {}
        LspRequestKind::Shutdown => {}
    }

    let elapsed = response_start.elapsed();
    if elapsed.as_millis() > 1 {
        tracing::debug!(
            elapsed_ms = elapsed.as_millis() as u64,
            kind = ?kind_label,
            target = "lsp.pipeline",
            "lsp response processing"
        );
    }
}
