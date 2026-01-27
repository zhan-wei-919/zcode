use crate::core::Service;
use crate::kernel::locations::LocationItem;
use crate::kernel::problems::{ProblemItem, ProblemRange, ProblemSeverity};
use crate::kernel::services::ports::{
    LspCodeAction, LspCommand, LspCompletionItem, LspFoldingRange, LspInlayHint,
    LspInsertTextFormat, LspPosition, LspPositionEncoding, LspRange, LspResourceOp,
    LspSemanticToken, LspSemanticTokensLegend, LspServerCapabilities, LspTextChange, LspTextEdit,
    LspWorkspaceEdit, LspWorkspaceFileEdit,
};
use crate::kernel::services::KernelServiceContext;
use crate::kernel::symbols::SymbolItem;
use crate::kernel::Action;
use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use rustc_hash::FxHashMap;
use serde_json::Value;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub struct LspService {
    root: PathBuf,
    command: String,
    args: Vec<String>,
    ctx: KernelServiceContext,
    process: Option<LspProcess>,
    exiting: bool,
    restart_attempts: u32,
    restart_backoff_until: Option<Instant>,
    next_id: i32,
    doc_versions: FxHashMap<PathBuf, u64>,
    pending_requests: Arc<Mutex<FxHashMap<RequestId, LspRequestKind>>>,
    latest_hover: Arc<AtomicI32>,
    latest_definition: Arc<AtomicI32>,
    latest_references: Arc<AtomicI32>,
    latest_document_symbols: Arc<AtomicI32>,
    latest_workspace_symbols: Arc<AtomicI32>,
    latest_code_action: Arc<AtomicI32>,
    latest_completion: Arc<AtomicI32>,
    latest_completion_resolve: Arc<AtomicI32>,
    latest_semantic_tokens: Arc<AtomicI32>,
    latest_inlay_hints: Arc<AtomicI32>,
    latest_folding_range: Arc<AtomicI32>,
    latest_signature_help: Arc<AtomicI32>,
    latest_format: Arc<AtomicI32>,
    latest_rename: Arc<AtomicI32>,
    latest_shutdown: Arc<AtomicI32>,
}

struct LspProcess {
    tx: mpsc::Sender<Message>,
    pending: Arc<Mutex<LspPending>>,
    child: Arc<Mutex<std::process::Child>>,
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
enum InitState {
    Starting,
    Ready,
    Failed,
}

struct LspPending {
    state: InitState,
    queue: VecDeque<Message>,
}

#[derive(Debug, Clone)]
enum LspRequestKind {
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

impl LspService {
    pub fn new(root: PathBuf, ctx: KernelServiceContext) -> Self {
        Self {
            root,
            command: "rust-analyzer".to_string(),
            args: Vec::new(),
            ctx,
            process: None,
            exiting: false,
            restart_attempts: 0,
            restart_backoff_until: None,
            next_id: 1,
            doc_versions: FxHashMap::default(),
            pending_requests: Arc::new(Mutex::new(FxHashMap::default())),
            latest_hover: Arc::new(AtomicI32::new(0)),
            latest_definition: Arc::new(AtomicI32::new(0)),
            latest_references: Arc::new(AtomicI32::new(0)),
            latest_document_symbols: Arc::new(AtomicI32::new(0)),
            latest_workspace_symbols: Arc::new(AtomicI32::new(0)),
            latest_code_action: Arc::new(AtomicI32::new(0)),
            latest_completion: Arc::new(AtomicI32::new(0)),
            latest_completion_resolve: Arc::new(AtomicI32::new(0)),
            latest_semantic_tokens: Arc::new(AtomicI32::new(0)),
            latest_inlay_hints: Arc::new(AtomicI32::new(0)),
            latest_folding_range: Arc::new(AtomicI32::new(0)),
            latest_signature_help: Arc::new(AtomicI32::new(0)),
            latest_format: Arc::new(AtomicI32::new(0)),
            latest_rename: Arc::new(AtomicI32::new(0)),
            latest_shutdown: Arc::new(AtomicI32::new(0)),
        }
    }

    pub fn command_config(&self) -> (&str, &[String]) {
        (&self.command, &self.args)
    }

    pub fn with_command(mut self, command: String, args: Vec<String>) -> Self {
        self.command = command;
        self.args = args;
        self
    }

    pub fn needs_sync(&self, path: &Path, version: u64) -> bool {
        self.doc_versions.get(path).is_none_or(|v| *v != version)
    }

    pub fn sync_document(
        &mut self,
        path: &Path,
        version: u64,
        change: Option<LspTextChange>,
        text: impl FnOnce() -> String,
    ) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            let text = text();
            self.did_open(path, &text, version);
            return;
        }

        if !self.needs_sync(path, version) {
            return;
        }

        match change {
            Some(change) => self.did_change(path, "", version, Some(change)),
            None => {
                let text = text();
                self.did_change(path, &text, version, None);
            }
        }
    }

    pub fn close_document(&mut self, path: &Path) {
        if !self.ensure_started() {
            return;
        }

        if self.doc_versions.remove(path).is_none() {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let params = lsp_types::DidCloseTextDocumentParams {
            text_document: lsp_types::TextDocumentIdentifier { uri },
        };
        let msg = Message::Notification(Notification::new(
            lsp_types::notification::DidCloseTextDocument::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn save_document(&mut self, path: &Path) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let params = lsp_types::DidSaveTextDocumentParams {
            text_document: lsp_types::TextDocumentIdentifier { uri },
            text: None,
        };
        let msg = Message::Notification(Notification::new(
            lsp_types::notification::DidSaveTextDocument::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_hover(&mut self, path: &Path, position: LspPosition) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_hover.swap(id, Ordering::Relaxed);
        self.track_request(id, LspRequestKind::Hover);
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::HoverParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri },
                position: lsp_types::Position {
                    line: position.line,
                    character: position.character,
                },
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::HoverRequest::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn cancel_hover(&mut self) {
        let prev = self.latest_hover.swap(0, Ordering::Relaxed);
        if prev != 0 {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }
    }

    pub fn request_definition(&mut self, path: &Path, position: LspPosition) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_definition.swap(id, Ordering::Relaxed);
        self.track_request(id, LspRequestKind::Definition);
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::GotoDefinitionParams {
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri },
                position: lsp_types::Position {
                    line: position.line,
                    character: position.character,
                },
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::GotoDefinition::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_references(&mut self, path: &Path, position: LspPosition) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_references.swap(id, Ordering::Relaxed);
        self.track_request(id, LspRequestKind::References);
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::ReferenceParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri },
                position: lsp_types::Position {
                    line: position.line,
                    character: position.character,
                },
            },
            context: lsp_types::ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::References::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_document_symbols(&mut self, path: &Path) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_document_symbols.swap(id, Ordering::Relaxed);
        self.track_request(
            id,
            LspRequestKind::DocumentSymbols {
                path: path.to_path_buf(),
            },
        );
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::DocumentSymbolParams {
            text_document: lsp_types::TextDocumentIdentifier { uri },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::DocumentSymbolRequest::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_workspace_symbols(&mut self, query: String) {
        if !self.ensure_started() {
            return;
        }

        let id = self.next_id();
        let prev = self.latest_workspace_symbols.swap(id, Ordering::Relaxed);
        self.track_request(id, LspRequestKind::WorkspaceSymbols);
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::WorkspaceSymbolParams {
            partial_result_params: lsp_types::PartialResultParams::default(),
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            query,
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::WorkspaceSymbolRequest::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_code_action(&mut self, path: &Path, position: LspPosition) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_code_action.swap(id, Ordering::Relaxed);
        self.track_request(id, LspRequestKind::CodeAction);
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let pos = lsp_types::Position {
            line: position.line,
            character: position.character,
        };

        let params = lsp_types::CodeActionParams {
            text_document: lsp_types::TextDocumentIdentifier { uri },
            range: lsp_types::Range {
                start: pos,
                end: pos,
            },
            context: lsp_types::CodeActionContext {
                diagnostics: Vec::new(),
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::CodeActionRequest::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_completion(&mut self, path: &Path, position: LspPosition) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_completion.swap(id, Ordering::Relaxed);
        self.track_request(id, LspRequestKind::Completion);
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::CompletionParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri },
                position: lsp_types::Position {
                    line: position.line,
                    character: position.character,
                },
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
            context: Some(lsp_types::CompletionContext {
                trigger_kind: lsp_types::CompletionTriggerKind::INVOKED,
                trigger_character: None,
            }),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::Completion::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_completion_resolve(&mut self, item: LspCompletionItem) {
        if !self.ensure_started() {
            return;
        }

        if item.data.is_none() {
            return;
        }

        let id = self.next_id();
        let prev = self.latest_completion_resolve.swap(id, Ordering::Relaxed);
        self.track_request(id, LspRequestKind::CompletionResolve { item_id: item.id });
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = completion_item_to_lsp(&item);

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::ResolveCompletionItem::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_semantic_tokens(&mut self, path: &Path, version: u64) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_semantic_tokens.swap(id, Ordering::Relaxed);
        self.track_request(
            id,
            LspRequestKind::SemanticTokens {
                path: path.to_path_buf(),
                version,
            },
        );
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::SemanticTokensParams {
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
            text_document: lsp_types::TextDocumentIdentifier { uri },
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::SemanticTokensFullRequest::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_semantic_tokens_range(&mut self, path: &Path, range: LspRange, version: u64) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_semantic_tokens.swap(id, Ordering::Relaxed);
        self.track_request(
            id,
            LspRequestKind::SemanticTokensRange {
                path: path.to_path_buf(),
                version,
                range,
            },
        );
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::SemanticTokensRangeParams {
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
            text_document: lsp_types::TextDocumentIdentifier { uri },
            range: lsp_types::Range::new(
                lsp_types::Position::new(range.start.line, range.start.character),
                lsp_types::Position::new(range.end.line, range.end.character),
            ),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::SemanticTokensRangeRequest::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_inlay_hints(&mut self, path: &Path, range: LspRange, version: u64) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_inlay_hints.swap(id, Ordering::Relaxed);
        self.track_request(
            id,
            LspRequestKind::InlayHints {
                path: path.to_path_buf(),
                version,
                range,
            },
        );
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::InlayHintParams {
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            text_document: lsp_types::TextDocumentIdentifier { uri },
            range: lsp_types::Range::new(
                lsp_types::Position::new(range.start.line, range.start.character),
                lsp_types::Position::new(range.end.line, range.end.character),
            ),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::InlayHintRequest::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_folding_range(&mut self, path: &Path, version: u64) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_folding_range.swap(id, Ordering::Relaxed);
        self.track_request(
            id,
            LspRequestKind::FoldingRange {
                path: path.to_path_buf(),
                version,
            },
        );
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::FoldingRangeParams {
            text_document: lsp_types::TextDocumentIdentifier { uri },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
            partial_result_params: lsp_types::PartialResultParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::FoldingRangeRequest::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_signature_help(&mut self, path: &Path, position: LspPosition) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_signature_help.swap(id, Ordering::Relaxed);
        self.track_request(id, LspRequestKind::SignatureHelp);
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::SignatureHelpParams {
            context: None,
            text_document_position_params: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri },
                position: lsp_types::Position {
                    line: position.line,
                    character: position.character,
                },
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::SignatureHelpRequest::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_rename(&mut self, path: &Path, position: LspPosition, new_name: String) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_rename.swap(id, Ordering::Relaxed);
        self.track_request(id, LspRequestKind::Rename);
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::RenameParams {
            text_document_position: lsp_types::TextDocumentPositionParams {
                text_document: lsp_types::TextDocumentIdentifier { uri },
                position: lsp_types::Position {
                    line: position.line,
                    character: position.character,
                },
            },
            new_name,
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::Rename::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_format(&mut self, path: &Path) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_format.swap(id, Ordering::Relaxed);
        self.track_request(
            id,
            LspRequestKind::Format {
                path: path.to_path_buf(),
            },
        );
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::DocumentFormattingParams {
            text_document: lsp_types::TextDocumentIdentifier { uri },
            options: lsp_types::FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::Formatting::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn request_range_format(&mut self, path: &Path, range: LspRange) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_format.swap(id, Ordering::Relaxed);
        self.track_request(
            id,
            LspRequestKind::Format {
                path: path.to_path_buf(),
            },
        );
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let params = lsp_types::DocumentRangeFormattingParams {
            text_document: lsp_types::TextDocumentIdentifier { uri },
            range: lsp_types::Range::new(
                lsp_types::Position::new(range.start.line, range.start.character),
                lsp_types::Position::new(range.end.line, range.end.character),
            ),
            options: lsp_types::FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::RangeFormatting::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn execute_command(&mut self, command: String, arguments: Vec<Value>) {
        if !self.ensure_started() {
            return;
        }

        let id = self.next_id();
        self.track_request(id, LspRequestKind::ExecuteCommand);

        let params = lsp_types::ExecuteCommandParams {
            command,
            arguments,
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::ExecuteCommand::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }

    pub fn shutdown(&mut self) {
        let Some(_) = self.process.as_ref() else {
            return;
        };
        if self.exiting {
            return;
        }
        self.exiting = true;

        let id = self.next_id();
        let prev = self.latest_shutdown.swap(id, Ordering::Relaxed);
        self.track_request(id, LspRequestKind::Shutdown);
        if prev != 0 && prev != id {
            self.cancel_request(prev);
            self.untrack_request(prev);
        }

        let msg = Message::Request(Request::new(
            RequestId::from(id),
            lsp_types::request::Shutdown::METHOD.to_string(),
            (),
        ));
        self.send_message(msg, true);
    }

    fn did_open(&mut self, path: &Path, text: &str, version: u64) {
        let Some(uri) = path_to_url(path) else {
            return;
        };

        let item = lsp_types::TextDocumentItem::new(
            uri,
            language_id_for_path(path).to_string(),
            lsp_version(version),
            text.to_string(),
        );

        let params = lsp_types::DidOpenTextDocumentParams {
            text_document: item,
        };
        let msg = Message::Notification(Notification::new(
            lsp_types::notification::DidOpenTextDocument::METHOD.to_string(),
            params,
        ));

        self.doc_versions.insert(path.to_path_buf(), version);
        self.send_message(msg, true);
    }

    fn did_change(&mut self, path: &Path, text: &str, version: u64, change: Option<LspTextChange>) {
        let Some(uri) = path_to_url(path) else {
            return;
        };

        let text_document = lsp_types::VersionedTextDocumentIdentifier {
            uri,
            version: lsp_version(version),
        };

        let changes = match change {
            Some(change) => vec![text_change_event(change)],
            None => vec![lsp_types::TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }],
        };

        let params = lsp_types::DidChangeTextDocumentParams {
            text_document,
            content_changes: changes,
        };

        let msg = Message::Notification(Notification::new(
            lsp_types::notification::DidChangeTextDocument::METHOD.to_string(),
            params,
        ));

        self.doc_versions.insert(path.to_path_buf(), version);
        self.send_message(msg, true);
    }

    fn schedule_restart_backoff(&mut self) {
        let attempt = self.restart_attempts.saturating_add(1);
        self.restart_attempts = attempt;

        let shift = attempt.saturating_sub(1).min(6);
        let delay_ms = 200u64.saturating_mul(1u64 << shift);
        let delay = Duration::from_millis(delay_ms.min(5_000));
        self.restart_backoff_until = Some(Instant::now() + delay);
    }

    fn ensure_started(&mut self) -> bool {
        if self.exiting {
            return false;
        }

        if self
            .restart_backoff_until
            .is_some_and(|until| Instant::now() < until)
        {
            return false;
        }

        if let Some(process) = self.process.as_ref() {
            let state = process.pending.lock().ok().map(|p| p.state);

            if matches!(state, Some(InitState::Ready)) {
                self.restart_attempts = 0;
                self.restart_backoff_until = None;
                return true;
            }

            if !matches!(state, Some(InitState::Failed)) {
                return true;
            }

            self.process = None;
            self.doc_versions.clear();
            if let Ok(mut map) = self.pending_requests.lock() {
                map.clear();
            }

            self.schedule_restart_backoff();
            return false;
        }

        self.restart_backoff_until = None;

        let Some(workspace_folders) = workspace_folders_for_root(&self.root) else {
            tracing::error!(root = %self.root.display(), "lsp root path is not a valid file:// uri");
            return false;
        };

        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .current_dir(&self.root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                tracing::error!(error = %e, "spawn lsp server failed");
                self.schedule_restart_backoff();
                return false;
            }
        };

        let stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                tracing::error!("lsp server stdin unavailable");
                let _ = child.kill();
                let _ = child.wait();
                self.schedule_restart_backoff();
                return false;
            }
        };

        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                tracing::error!("lsp server stdout unavailable");
                let _ = child.kill();
                let _ = child.wait();
                self.schedule_restart_backoff();
                return false;
            }
        };

        let stderr = child.stderr.take();
        let child = Arc::new(Mutex::new(child));

        let (tx, rx) = mpsc::channel::<Message>();

        let init_id = self.next_id();
        let pending = Arc::new(Mutex::new(LspPending {
            state: InitState::Starting,
            queue: VecDeque::new(),
        }));

        if let Err(e) = std::thread::Builder::new()
            .name("zcode-lsp-writer".to_string())
            .spawn({
                let pending = pending.clone();
                move || writer_loop(stdin, rx, pending)
            })
        {
            tracing::error!(error = %e, "spawn lsp writer thread failed");
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
            self.schedule_restart_backoff();
            return false;
        }

        if let Err(e) = std::thread::Builder::new()
            .name("zcode-lsp-reader".to_string())
            .spawn({
                let ctx = self.ctx.clone();
                let pending = pending.clone();
                let pending_requests = self.pending_requests.clone();
                let latest_hover = self.latest_hover.clone();
                let latest_definition = self.latest_definition.clone();
                let latest_references = self.latest_references.clone();
                let latest_document_symbols = self.latest_document_symbols.clone();
                let latest_workspace_symbols = self.latest_workspace_symbols.clone();
                let latest_code_action = self.latest_code_action.clone();
                let latest_completion = self.latest_completion.clone();
                let latest_completion_resolve = self.latest_completion_resolve.clone();
                let latest_semantic_tokens = self.latest_semantic_tokens.clone();
                let latest_inlay_hints = self.latest_inlay_hints.clone();
                let latest_folding_range = self.latest_folding_range.clone();
                let latest_signature_help = self.latest_signature_help.clone();
                let latest_format = self.latest_format.clone();
                let latest_rename = self.latest_rename.clone();
                let latest_shutdown = self.latest_shutdown.clone();
                let tx = tx.clone();
                let workspace_folders = Some(workspace_folders);
                move || {
                    reader_loop(ReaderLoopArgs {
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
                        latest_semantic_tokens,
                        latest_inlay_hints,
                        latest_folding_range,
                        latest_signature_help,
                        latest_format,
                        latest_rename,
                        latest_shutdown,
                        tx,
                        workspace_folders,
                    })
                }
            })
        {
            tracing::error!(error = %e, "spawn lsp reader thread failed");
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
            self.schedule_restart_backoff();
            return false;
        }

        if let Some(stderr) = stderr {
            if let Err(e) = std::thread::Builder::new()
                .name("zcode-lsp-stderr".to_string())
                .spawn(move || stderr_loop(stderr))
            {
                tracing::warn!(error = %e, "spawn lsp stderr thread failed");
            }
        }

        if let Err(e) = std::thread::Builder::new()
            .name("zcode-lsp-watch".to_string())
            .spawn({
                let child = child.clone();
                let pending = pending.clone();
                move || child_watch_loop(child, pending)
            })
        {
            tracing::error!(error = %e, "spawn lsp watch thread failed");
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
            self.schedule_restart_backoff();
            return false;
        }

        self.process = Some(LspProcess {
            tx: tx.clone(),
            pending,
            child,
        });

        self.send_initialize(init_id);
        true
    }

    fn next_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn send_initialize(&mut self, init_id: i32) {
        let Some(root_uri) = path_to_url(&self.root) else {
            return;
        };

        let name = self
            .root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("workspace")
            .to_string();

        let capabilities = client_capabilities();

        #[allow(deprecated)]
        let params = lsp_types::InitializeParams {
            process_id: Some(std::process::id()),
            root_path: None,
            root_uri: Some(root_uri.clone()),
            initialization_options: None,
            capabilities,
            trace: None,
            workspace_folders: Some(vec![lsp_types::WorkspaceFolder {
                uri: root_uri,
                name,
            }]),
            client_info: Some(lsp_types::ClientInfo {
                name: "zcode".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            locale: None,
            work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        };

        let msg = Message::Request(Request::new(
            RequestId::from(init_id),
            lsp_types::request::Initialize::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, false);
    }

    fn send_message(&mut self, msg: Message, requires_init: bool) {
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

    fn track_request(&mut self, id: i32, kind: LspRequestKind) {
        if let Ok(mut map) = self.pending_requests.lock() {
            map.insert(RequestId::from(id), kind);
        }
    }

    fn untrack_request(&mut self, id: i32) {
        if let Ok(mut map) = self.pending_requests.lock() {
            map.remove(&RequestId::from(id));
        }
    }

    fn cancel_request(&mut self, id: i32) {
        let params = lsp_types::CancelParams {
            id: lsp_types::NumberOrString::Number(id),
        };
        let msg = Message::Notification(Notification::new(
            lsp_types::notification::Cancel::METHOD.to_string(),
            params,
        ));
        self.send_message(msg, true);
    }
}

impl Service for LspService {
    fn name(&self) -> &'static str {
        "LspService"
    }
}

fn mark_failed(pending: &Arc<Mutex<LspPending>>) {
    if let Ok(mut pending) = pending.lock() {
        pending.queue.clear();
        pending.state = InitState::Failed;
    }
}

fn child_watch_loop(child: Arc<Mutex<std::process::Child>>, pending: Arc<Mutex<LspPending>>) {
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

fn writer_loop(
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

struct ReaderLoopArgs {
    stdout: std::process::ChildStdout,
    ctx: KernelServiceContext,
    init_id: i32,
    pending: Arc<Mutex<LspPending>>,
    pending_requests: Arc<Mutex<FxHashMap<RequestId, LspRequestKind>>>,
    latest_hover: Arc<AtomicI32>,
    latest_definition: Arc<AtomicI32>,
    latest_references: Arc<AtomicI32>,
    latest_document_symbols: Arc<AtomicI32>,
    latest_workspace_symbols: Arc<AtomicI32>,
    latest_code_action: Arc<AtomicI32>,
    latest_completion: Arc<AtomicI32>,
    latest_completion_resolve: Arc<AtomicI32>,
    latest_semantic_tokens: Arc<AtomicI32>,
    latest_inlay_hints: Arc<AtomicI32>,
    latest_folding_range: Arc<AtomicI32>,
    latest_signature_help: Arc<AtomicI32>,
    latest_format: Arc<AtomicI32>,
    latest_rename: Arc<AtomicI32>,
    latest_shutdown: Arc<AtomicI32>,
    tx: mpsc::Sender<Message>,
    workspace_folders: Option<Vec<lsp_types::WorkspaceFolder>>,
}

fn reader_loop(args: ReaderLoopArgs) {
    let ReaderLoopArgs {
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
        latest_semantic_tokens,
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
                let resp = handle_server_request(req, &workspace_folders, &ctx);
                let _ = tx.send(Message::Response(resp));
            }
            Message::Notification(not) => {
                if not.method == lsp_types::notification::PublishDiagnostics::METHOD {
                    if let Ok(params) =
                        serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(not.params)
                    {
                        if let Some((path, items)) = diagnostics_from_params(params) {
                            ctx.dispatch(Action::LspDiagnostics { path, items });
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
                            ctx.dispatch(Action::LspServerCapabilities { capabilities: caps });
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
                            resp.id
                                == RequestId::from(latest_semantic_tokens.load(Ordering::Relaxed))
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

fn stderr_loop(stderr: std::process::ChildStderr) {
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

fn server_capabilities_from_lsp(caps: &lsp_types::ServerCapabilities) -> LspServerCapabilities {
    fn one_of_bool<T>(v: &Option<lsp_types::OneOf<bool, T>>) -> bool {
        match v {
            Some(lsp_types::OneOf::Left(enabled)) => *enabled,
            Some(lsp_types::OneOf::Right(_)) => true,
            None => false,
        }
    }

    fn hover(v: &Option<lsp_types::HoverProviderCapability>) -> bool {
        match v {
            Some(lsp_types::HoverProviderCapability::Simple(enabled)) => *enabled,
            Some(lsp_types::HoverProviderCapability::Options(_)) => true,
            None => false,
        }
    }

    fn code_action(v: &Option<lsp_types::CodeActionProviderCapability>) -> bool {
        match v {
            Some(lsp_types::CodeActionProviderCapability::Simple(enabled)) => *enabled,
            Some(lsp_types::CodeActionProviderCapability::Options(_)) => true,
            None => false,
        }
    }

    fn folding(v: &Option<lsp_types::FoldingRangeProviderCapability>) -> bool {
        match v {
            Some(lsp_types::FoldingRangeProviderCapability::Simple(enabled)) => *enabled,
            Some(_) => true,
            None => false,
        }
    }

    fn triggers(v: &Option<Vec<String>>) -> Vec<char> {
        let mut out = Vec::new();
        let Some(v) = v else {
            return out;
        };
        for s in v {
            let mut it = s.chars();
            let Some(ch) = it.next() else {
                continue;
            };
            if it.next().is_some() {
                continue;
            }
            out.push(ch);
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    let encoding = match caps
        .position_encoding
        .clone()
        .unwrap_or(lsp_types::PositionEncodingKind::UTF16)
    {
        kind if kind == lsp_types::PositionEncodingKind::UTF8 => LspPositionEncoding::Utf8,
        kind if kind == lsp_types::PositionEncodingKind::UTF32 => LspPositionEncoding::Utf32,
        _ => LspPositionEncoding::Utf16,
    };

    let completion_triggers = triggers(
        &caps
            .completion_provider
            .as_ref()
            .and_then(|p| p.trigger_characters.clone()),
    );
    let completion_resolve = caps
        .completion_provider
        .as_ref()
        .and_then(|p| p.resolve_provider)
        .unwrap_or(false);
    let signature_help_triggers = triggers(
        &caps
            .signature_help_provider
            .as_ref()
            .and_then(|p| p.trigger_characters.clone()),
    );

    let semantic_tokens_legend = caps.semantic_tokens_provider.as_ref().and_then(|provider| {
        let options = match provider {
            lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(options) => options,
            lsp_types::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                options,
            ) => &options.semantic_tokens_options,
        };

        let token_types = options
            .legend
            .token_types
            .iter()
            .map(|t| t.as_str().to_string())
            .collect::<Vec<_>>();
        let token_modifiers = options
            .legend
            .token_modifiers
            .iter()
            .map(|t| t.as_str().to_string())
            .collect::<Vec<_>>();

        Some(LspSemanticTokensLegend {
            token_types,
            token_modifiers,
        })
    });

    let (semantic_tokens_range, semantic_tokens_full) = caps
        .semantic_tokens_provider
        .as_ref()
        .map(|provider| {
            let options = match provider {
                lsp_types::SemanticTokensServerCapabilities::SemanticTokensOptions(options) => {
                    options
                }
                lsp_types::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                    options,
                ) => &options.semantic_tokens_options,
            };

            let range = options.range.unwrap_or(false);
            let full = options.full.as_ref().is_some_and(|full| match full {
                lsp_types::SemanticTokensFullOptions::Bool(enabled) => *enabled,
                lsp_types::SemanticTokensFullOptions::Delta { .. } => true,
            });

            (range, full)
        })
        .unwrap_or((false, false));

    LspServerCapabilities {
        position_encoding: encoding,
        hover: hover(&caps.hover_provider),
        definition: one_of_bool(&caps.definition_provider),
        references: one_of_bool(&caps.references_provider),
        document_symbols: one_of_bool(&caps.document_symbol_provider),
        workspace_symbols: one_of_bool(&caps.workspace_symbol_provider),
        code_action: code_action(&caps.code_action_provider),
        completion: caps.completion_provider.is_some(),
        signature_help: caps.signature_help_provider.is_some(),
        rename: one_of_bool(&caps.rename_provider),
        format: one_of_bool(&caps.document_formatting_provider),
        range_format: one_of_bool(&caps.document_range_formatting_provider),
        semantic_tokens: caps.semantic_tokens_provider.is_some(),
        semantic_tokens_range,
        semantic_tokens_full,
        semantic_tokens_legend,
        inlay_hints: one_of_bool(&caps.inlay_hint_provider),
        folding_range: folding(&caps.folding_range_provider),
        completion_resolve,
        completion_triggers,
        signature_help_triggers,
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

fn handle_response(kind: LspRequestKind, resp: Response, ctx: &KernelServiceContext) {
    if let Some(err) = resp.error {
        tracing::warn!(code = err.code, error = %err.message, "lsp request failed");
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

            ctx.dispatch(Action::LspCompletionResolved {
                id: item_id,
                detail,
                documentation,
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
                return;
            };
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
}

fn decode_semantic_tokens(tokens: Vec<lsp_types::SemanticToken>) -> Vec<LspSemanticToken> {
    let mut out = Vec::with_capacity(tokens.len().min(2048));
    let mut line = 0u32;
    let mut start = 0u32;

    for token in tokens {
        line = line.saturating_add(token.delta_line);
        if token.delta_line == 0 {
            start = start.saturating_add(token.delta_start);
        } else {
            start = token.delta_start;
        }

        out.push(LspSemanticToken {
            line,
            start,
            length: token.length,
            token_type: token.token_type,
            modifiers: token.token_modifiers_bitset,
        });
    }

    out
}

fn inlay_hints_from_lsp(hints: Vec<lsp_types::InlayHint>) -> Vec<LspInlayHint> {
    let mut out = Vec::with_capacity(hints.len().min(128));

    for hint in hints.into_iter().take(512) {
        let label = match hint.label {
            lsp_types::InlayHintLabel::String(s) => s,
            lsp_types::InlayHintLabel::LabelParts(parts) => {
                parts.into_iter().map(|p| p.value).collect::<String>()
            }
        };

        let label = label.trim().to_string();
        if label.is_empty() {
            continue;
        }

        out.push(LspInlayHint {
            position: LspPosition {
                line: hint.position.line,
                character: hint.position.character,
            },
            label,
            padding_left: hint.padding_left.unwrap_or(false),
            padding_right: hint.padding_right.unwrap_or(false),
        });
    }

    out
}

fn push_document_symbols(
    path: &PathBuf,
    symbols: &[lsp_types::DocumentSymbol],
    level: usize,
    out: &mut Vec<SymbolItem>,
) {
    for sym in symbols {
        out.push(SymbolItem {
            name: sym.name.clone(),
            detail: sym.detail.clone(),
            kind: symbol_kind_u32(sym.kind),
            level,
            path: path.clone(),
            line: sym.selection_range.start.line,
            column: sym.selection_range.start.character,
        });

        if let Some(children) = sym.children.as_ref() {
            push_document_symbols(path, children, level.saturating_add(1), out);
        }
    }
}

fn symbol_item_from_symbol_information(
    fallback_path: Option<&PathBuf>,
    sym: lsp_types::SymbolInformation,
) -> Option<SymbolItem> {
    let path = sym
        .location
        .uri
        .to_file_path()
        .ok()
        .or_else(|| fallback_path.cloned())?;

    Some(SymbolItem {
        name: sym.name,
        detail: sym.container_name,
        kind: symbol_kind_u32(sym.kind),
        level: 0,
        path,
        line: sym.location.range.start.line,
        column: sym.location.range.start.character,
    })
}

fn symbol_item_from_workspace_symbol(sym: lsp_types::WorkspaceSymbol) -> Option<SymbolItem> {
    match sym.location {
        lsp_types::OneOf::Left(loc) => {
            let path = loc.uri.to_file_path().ok()?;
            Some(SymbolItem {
                name: sym.name,
                detail: sym.container_name,
                kind: symbol_kind_u32(sym.kind),
                level: 0,
                path,
                line: loc.range.start.line,
                column: loc.range.start.character,
            })
        }
        lsp_types::OneOf::Right(loc) => {
            let path = loc.uri.to_file_path().ok()?;
            Some(SymbolItem {
                name: sym.name,
                detail: sym.container_name,
                kind: symbol_kind_u32(sym.kind),
                level: 0,
                path,
                line: 0,
                column: 0,
            })
        }
    }
}

fn symbol_kind_u32(kind: lsp_types::SymbolKind) -> u32 {
    if kind == lsp_types::SymbolKind::FILE {
        1
    } else if kind == lsp_types::SymbolKind::MODULE {
        2
    } else if kind == lsp_types::SymbolKind::NAMESPACE {
        3
    } else if kind == lsp_types::SymbolKind::PACKAGE {
        4
    } else if kind == lsp_types::SymbolKind::CLASS {
        5
    } else if kind == lsp_types::SymbolKind::METHOD {
        6
    } else if kind == lsp_types::SymbolKind::PROPERTY {
        7
    } else if kind == lsp_types::SymbolKind::FIELD {
        8
    } else if kind == lsp_types::SymbolKind::CONSTRUCTOR {
        9
    } else if kind == lsp_types::SymbolKind::ENUM {
        10
    } else if kind == lsp_types::SymbolKind::INTERFACE {
        11
    } else if kind == lsp_types::SymbolKind::FUNCTION {
        12
    } else if kind == lsp_types::SymbolKind::VARIABLE {
        13
    } else if kind == lsp_types::SymbolKind::CONSTANT {
        14
    } else if kind == lsp_types::SymbolKind::STRING {
        15
    } else if kind == lsp_types::SymbolKind::NUMBER {
        16
    } else if kind == lsp_types::SymbolKind::BOOLEAN {
        17
    } else if kind == lsp_types::SymbolKind::ARRAY {
        18
    } else if kind == lsp_types::SymbolKind::OBJECT {
        19
    } else if kind == lsp_types::SymbolKind::KEY {
        20
    } else if kind == lsp_types::SymbolKind::NULL {
        21
    } else if kind == lsp_types::SymbolKind::ENUM_MEMBER {
        22
    } else if kind == lsp_types::SymbolKind::STRUCT {
        23
    } else if kind == lsp_types::SymbolKind::EVENT {
        24
    } else if kind == lsp_types::SymbolKind::OPERATOR {
        25
    } else if kind == lsp_types::SymbolKind::TYPE_PARAMETER {
        26
    } else {
        0
    }
}

fn hover_text(hover: &lsp_types::Hover) -> Option<String> {
    let mut parts = Vec::new();
    match &hover.contents {
        lsp_types::HoverContents::Scalar(s) => push_marked_string(s, &mut parts),
        lsp_types::HoverContents::Array(items) => {
            for s in items {
                push_marked_string(s, &mut parts);
            }
        }
        lsp_types::HoverContents::Markup(m) => parts.push(m.value.clone()),
    }

    let text = parts.join("\n").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn signature_help_text(help: &lsp_types::SignatureHelp) -> Option<String> {
    let active_sig = help.active_signature.unwrap_or(0) as usize;
    let sig = help.signatures.get(active_sig)?;

    let active_param = sig.active_parameter.or(help.active_parameter).unwrap_or(0) as usize;

    let mut label = sig.label.clone();
    if let Some(params) = sig.parameters.as_ref() {
        if let Some(param) = params.get(active_param) {
            if let Some((start, end)) = parameter_label_range(&label, &param.label) {
                if start < end && end <= label.len() {
                    label = format!(
                        "{}[{}]{}",
                        &label[..start],
                        &label[start..end],
                        &label[end..]
                    );
                }
            }
        }
    }

    let mut lines = Vec::new();
    if help.signatures.len() > 1 {
        lines.push(format!(
            "{label} ({}/{})",
            active_sig + 1,
            help.signatures.len()
        ));
    } else {
        lines.push(label);
    }

    if let Some(doc) = sig.documentation.as_ref().and_then(documentation_text) {
        let doc = doc.trim();
        if !doc.is_empty() {
            let mut it = doc.lines();
            let first = it.next().unwrap_or_default();
            if !first.trim().is_empty() {
                lines.push(first.trim().to_string());
            }
        }
    }

    let text = lines.join("\n").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn documentation_text(doc: &lsp_types::Documentation) -> Option<String> {
    match doc {
        lsp_types::Documentation::String(s) => Some(s.clone()),
        lsp_types::Documentation::MarkupContent(m) => Some(m.value.clone()),
    }
}

fn parameter_label_range(label: &str, param: &lsp_types::ParameterLabel) -> Option<(usize, usize)> {
    match param {
        lsp_types::ParameterLabel::Simple(s) => {
            let start = label.find(s)?;
            Some((start, start.saturating_add(s.len())))
        }
        lsp_types::ParameterLabel::LabelOffsets([start, end]) => {
            let start = utf16_offset_to_byte(label, *start);
            let end = utf16_offset_to_byte(label, *end);
            Some((start, end.max(start)))
        }
    }
}

fn utf16_offset_to_byte(s: &str, offset: u32) -> usize {
    let mut units = 0u32;
    for (byte, ch) in s.char_indices() {
        let next = units.saturating_add(ch.len_utf16() as u32);
        if next > offset {
            return byte;
        }
        units = next;
    }
    s.len()
}

fn push_marked_string(s: &lsp_types::MarkedString, out: &mut Vec<String>) {
    match s {
        lsp_types::MarkedString::String(s) => out.push(s.clone()),
        lsp_types::MarkedString::LanguageString(ls) => out.push(ls.value.clone()),
    }
}

fn definition_location(resp: lsp_types::GotoDefinitionResponse) -> Option<(PathBuf, u32, u32)> {
    match resp {
        lsp_types::GotoDefinitionResponse::Scalar(loc) => location_from_location(&loc),
        lsp_types::GotoDefinitionResponse::Array(locs) => {
            locs.first().and_then(location_from_location)
        }
        lsp_types::GotoDefinitionResponse::Link(links) => {
            links.first().and_then(location_from_link)
        }
    }
}

fn location_from_location(loc: &lsp_types::Location) -> Option<(PathBuf, u32, u32)> {
    let path = loc.uri.to_file_path().ok()?;
    Some((path, loc.range.start.line, loc.range.start.character))
}

fn location_from_link(link: &lsp_types::LocationLink) -> Option<(PathBuf, u32, u32)> {
    let path = link.target_uri.to_file_path().ok()?;
    let range = &link.target_selection_range;
    Some((path, range.start.line, range.start.character))
}

fn completion_item_kind_u32(kind: lsp_types::CompletionItemKind) -> u32 {
    use lsp_types::CompletionItemKind as Kind;

    if kind == Kind::TEXT {
        1
    } else if kind == Kind::METHOD {
        2
    } else if kind == Kind::FUNCTION {
        3
    } else if kind == Kind::CONSTRUCTOR {
        4
    } else if kind == Kind::FIELD {
        5
    } else if kind == Kind::VARIABLE {
        6
    } else if kind == Kind::CLASS {
        7
    } else if kind == Kind::INTERFACE {
        8
    } else if kind == Kind::MODULE {
        9
    } else if kind == Kind::PROPERTY {
        10
    } else if kind == Kind::UNIT {
        11
    } else if kind == Kind::VALUE {
        12
    } else if kind == Kind::ENUM {
        13
    } else if kind == Kind::KEYWORD {
        14
    } else if kind == Kind::SNIPPET {
        15
    } else if kind == Kind::COLOR {
        16
    } else if kind == Kind::FILE {
        17
    } else if kind == Kind::REFERENCE {
        18
    } else if kind == Kind::FOLDER {
        19
    } else if kind == Kind::ENUM_MEMBER {
        20
    } else if kind == Kind::CONSTANT {
        21
    } else if kind == Kind::STRUCT {
        22
    } else if kind == Kind::EVENT {
        23
    } else if kind == Kind::OPERATOR {
        24
    } else if kind == Kind::TYPE_PARAMETER {
        25
    } else {
        0
    }
}

fn insert_text_format(fmt: Option<lsp_types::InsertTextFormat>) -> LspInsertTextFormat {
    match fmt {
        Some(fmt) if fmt == lsp_types::InsertTextFormat::SNIPPET => LspInsertTextFormat::Snippet,
        _ => LspInsertTextFormat::PlainText,
    }
}

fn completion_items(resp: lsp_types::CompletionResponse) -> (Vec<LspCompletionItem>, bool) {
    let (items, mut is_incomplete) = match resp {
        lsp_types::CompletionResponse::Array(items) => (items, false),
        lsp_types::CompletionResponse::List(list) => (list.items, list.is_incomplete),
    };

    let mut out = Vec::with_capacity(items.len().min(64));
    let mut next_id = 1u64;
    let truncated = items.len() > 200;

    for item in items.into_iter().take(200) {
        let label = item.label.trim().to_string();
        if label.is_empty() {
            continue;
        }

        let detail = item.detail;
        let kind = item.kind.map(completion_item_kind_u32).filter(|k| *k != 0);
        let documentation = item
            .documentation
            .as_ref()
            .and_then(documentation_text)
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());
        let sort_text = item.sort_text;
        let filter_text = item.filter_text;
        let insert_text_format = insert_text_format(item.insert_text_format);
        let additional_text_edits = item
            .additional_text_edits
            .unwrap_or_default()
            .into_iter()
            .map(|edit| LspTextEdit {
                range: range_from_lsp(edit.range),
                new_text: edit.new_text,
            })
            .collect::<Vec<_>>();
        let command = item.command.map(command_from_lsp);
        let data = item.data;

        let mut insert_range = None;
        let mut replace_range = None;
        let mut insert_text = None;

        if let Some(text_edit) = item.text_edit {
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
            insert_text = item.insert_text;
        }

        let insert_text = insert_text.unwrap_or_else(|| label.clone());

        let id = next_id;
        next_id = next_id.saturating_add(1);

        out.push(LspCompletionItem {
            id,
            label,
            detail,
            kind,
            documentation,
            insert_text,
            insert_text_format,
            insert_range,
            replace_range,
            sort_text,
            filter_text,
            additional_text_edits,
            command,
            data,
        });
    }

    if truncated {
        is_incomplete = true;
    }

    (out, is_incomplete)
}

fn completion_item_kind_from_u32(kind: u32) -> Option<lsp_types::CompletionItemKind> {
    serde_json::from_value(Value::from(kind as i64)).ok()
}

fn completion_item_to_lsp(item: &LspCompletionItem) -> lsp_types::CompletionItem {
    let mut out = lsp_types::CompletionItem::default();
    out.label = item.label.clone();
    out.detail = item.detail.clone();
    out.kind = item.kind.and_then(completion_item_kind_from_u32);
    out.sort_text = item.sort_text.clone();
    out.filter_text = item.filter_text.clone();
    out.insert_text = Some(item.insert_text.clone());
    out.insert_text_format = Some(match item.insert_text_format {
        LspInsertTextFormat::PlainText => lsp_types::InsertTextFormat::PLAIN_TEXT,
        LspInsertTextFormat::Snippet => lsp_types::InsertTextFormat::SNIPPET,
    });
    out.data = item.data.clone();

    if let Some(range) = item.replace_range {
        out.text_edit = Some(lsp_types::CompletionTextEdit::Edit(lsp_types::TextEdit {
            range: lsp_types::Range {
                start: lsp_types::Position {
                    line: range.start.line,
                    character: range.start.character,
                },
                end: lsp_types::Position {
                    line: range.end.line,
                    character: range.end.character,
                },
            },
            new_text: item.insert_text.clone(),
        }));
    }

    out
}

fn code_actions_from_lsp(items: Vec<lsp_types::CodeActionOrCommand>) -> Vec<LspCodeAction> {
    let mut out = Vec::with_capacity(items.len().min(64));

    for item in items.into_iter().take(200) {
        match item {
            lsp_types::CodeActionOrCommand::Command(cmd) => {
                let title = cmd.title.clone();
                out.push(LspCodeAction {
                    title,
                    kind: None,
                    is_preferred: false,
                    edit: None,
                    command: Some(command_from_lsp(cmd)),
                });
            }
            lsp_types::CodeActionOrCommand::CodeAction(action) => {
                if action.disabled.is_some() {
                    continue;
                }

                let edit = action.edit.map(workspace_edit_from_lsp);
                let command = action.command.map(command_from_lsp);

                out.push(LspCodeAction {
                    title: action.title,
                    kind: action.kind.map(|k| k.as_str().to_string()),
                    is_preferred: action.is_preferred.unwrap_or(false),
                    edit,
                    command,
                });
            }
        }
    }

    out
}

fn command_from_lsp(command: lsp_types::Command) -> LspCommand {
    LspCommand {
        command: command.command,
        arguments: command.arguments.unwrap_or_default(),
    }
}

fn workspace_edit_from_lsp(edit: lsp_types::WorkspaceEdit) -> LspWorkspaceEdit {
    let mut by_path: FxHashMap<PathBuf, Vec<LspTextEdit>> = FxHashMap::default();
    let mut resource_ops: Vec<LspResourceOp> = Vec::new();

    if let Some(changes) = edit.changes {
        for (uri, edits) in changes {
            let Ok(path) = uri.to_file_path() else {
                continue;
            };
            let out = by_path.entry(path).or_default();
            for edit in edits {
                out.push(LspTextEdit {
                    range: range_from_lsp(edit.range),
                    new_text: edit.new_text,
                });
            }
        }
    }

    if let Some(doc_changes) = edit.document_changes {
        match doc_changes {
            lsp_types::DocumentChanges::Edits(edits) => {
                for doc in edits {
                    merge_text_document_edits(&mut by_path, doc);
                }
            }
            lsp_types::DocumentChanges::Operations(ops) => {
                for op in ops {
                    match op {
                        lsp_types::DocumentChangeOperation::Edit(doc) => {
                            merge_text_document_edits(&mut by_path, doc);
                        }
                        lsp_types::DocumentChangeOperation::Op(op) => match op {
                            lsp_types::ResourceOp::Create(create) => {
                                if let Ok(path) = create.uri.to_file_path() {
                                    let overwrite = create
                                        .options
                                        .as_ref()
                                        .and_then(|o| o.overwrite)
                                        .unwrap_or(false);
                                    let ignore_if_exists = create
                                        .options
                                        .as_ref()
                                        .and_then(|o| o.ignore_if_exists)
                                        .unwrap_or(false);
                                    resource_ops.push(LspResourceOp::CreateFile {
                                        path,
                                        overwrite,
                                        ignore_if_exists,
                                    });
                                }
                            }
                            lsp_types::ResourceOp::Rename(rename) => {
                                let Ok(old_path) = rename.old_uri.to_file_path() else {
                                    continue;
                                };
                                let Ok(new_path) = rename.new_uri.to_file_path() else {
                                    continue;
                                };
                                let overwrite = rename
                                    .options
                                    .as_ref()
                                    .and_then(|o| o.overwrite)
                                    .unwrap_or(false);
                                let ignore_if_exists = rename
                                    .options
                                    .as_ref()
                                    .and_then(|o| o.ignore_if_exists)
                                    .unwrap_or(false);
                                resource_ops.push(LspResourceOp::RenameFile {
                                    old_path,
                                    new_path,
                                    overwrite,
                                    ignore_if_exists,
                                });
                            }
                            lsp_types::ResourceOp::Delete(delete) => {
                                if let Ok(path) = delete.uri.to_file_path() {
                                    let recursive = delete
                                        .options
                                        .as_ref()
                                        .and_then(|o| o.recursive)
                                        .unwrap_or(false);
                                    let ignore_if_not_exists = delete
                                        .options
                                        .as_ref()
                                        .and_then(|o| o.ignore_if_not_exists)
                                        .unwrap_or(false);
                                    resource_ops.push(LspResourceOp::DeleteFile {
                                        path,
                                        recursive,
                                        ignore_if_not_exists,
                                    });
                                }
                            }
                        },
                    }
                }
            }
        }
    }

    let changes = by_path
        .into_iter()
        .filter_map(|(path, edits)| {
            if edits.is_empty() {
                None
            } else {
                Some(LspWorkspaceFileEdit { path, edits })
            }
        })
        .collect();

    LspWorkspaceEdit {
        changes,
        resource_ops,
    }
}

fn merge_text_document_edits(
    by_path: &mut FxHashMap<PathBuf, Vec<LspTextEdit>>,
    doc: lsp_types::TextDocumentEdit,
) {
    let Ok(path) = doc.text_document.uri.to_file_path() else {
        return;
    };

    let out = by_path.entry(path).or_default();
    for edit in doc.edits {
        let edit = match edit {
            lsp_types::OneOf::Left(edit) => edit,
            lsp_types::OneOf::Right(edit) => edit.text_edit,
        };
        out.push(LspTextEdit {
            range: range_from_lsp(edit.range),
            new_text: edit.new_text,
        });
    }
}

fn diagnostics_from_params(
    params: lsp_types::PublishDiagnosticsParams,
) -> Option<(PathBuf, Vec<ProblemItem>)> {
    let path = params.uri.to_file_path().ok()?;

    let mut items = Vec::with_capacity(params.diagnostics.len());
    for diag in params.diagnostics {
        let severity = match diag.severity {
            Some(lsp_types::DiagnosticSeverity::ERROR) => ProblemSeverity::Error,
            Some(lsp_types::DiagnosticSeverity::WARNING) => ProblemSeverity::Warning,
            Some(lsp_types::DiagnosticSeverity::INFORMATION) => ProblemSeverity::Information,
            Some(lsp_types::DiagnosticSeverity::HINT) => ProblemSeverity::Hint,
            _ => ProblemSeverity::Information,
        };

        items.push(ProblemItem {
            path: path.clone(),
            range: ProblemRange {
                start_line: diag.range.start.line,
                start_col: diag.range.start.character,
                end_line: diag.range.end.line,
                end_col: diag.range.end.character,
            },
            severity,
            message: diag.message,
            source: diag.source,
        });
    }

    Some((path, items))
}

fn text_change_event(change: LspTextChange) -> lsp_types::TextDocumentContentChangeEvent {
    match change.range {
        Some(range) => lsp_types::TextDocumentContentChangeEvent {
            range: Some(lsp_types::Range {
                start: lsp_types::Position {
                    line: range.start.line,
                    character: range.start.character,
                },
                end: lsp_types::Position {
                    line: range.end.line,
                    character: range.end.character,
                },
            }),
            range_length: None,
            text: change.text,
        },
        None => lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: change.text,
        },
    }
}

fn range_from_lsp(range: lsp_types::Range) -> LspRange {
    LspRange {
        start: LspPosition {
            line: range.start.line,
            character: range.start.character,
        },
        end: LspPosition {
            line: range.end.line,
            character: range.end.character,
        },
    }
}

fn lsp_version(version: u64) -> i32 {
    i32::try_from(version).unwrap_or(i32::MAX)
}

fn path_to_url(path: &Path) -> Option<lsp_types::Url> {
    lsp_types::Url::from_file_path(path).ok()
}

fn workspace_folders_for_root(root: &Path) -> Option<Vec<lsp_types::WorkspaceFolder>> {
    let uri = path_to_url(root)?;
    let name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workspace")
        .to_string();
    Some(vec![lsp_types::WorkspaceFolder { uri, name }])
}

fn client_capabilities() -> lsp_types::ClientCapabilities {
    let completion = lsp_types::CompletionClientCapabilities {
        completion_item: Some(lsp_types::CompletionItemCapability {
            snippet_support: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };

    let signature_help = lsp_types::SignatureHelpClientCapabilities {
        context_support: Some(true),
        ..Default::default()
    };

    let document_symbol = lsp_types::DocumentSymbolClientCapabilities {
        hierarchical_document_symbol_support: Some(true),
        ..Default::default()
    };

    let workspace_symbol = lsp_types::WorkspaceSymbolClientCapabilities::default();
    let semantic_tokens = lsp_types::SemanticTokensClientCapabilities {
        dynamic_registration: Some(false),
        requests: lsp_types::SemanticTokensClientCapabilitiesRequests {
            range: Some(true),
            full: Some(lsp_types::SemanticTokensFullOptions::Bool(true)),
        },
        token_types: vec![
            lsp_types::SemanticTokenType::NAMESPACE,
            lsp_types::SemanticTokenType::TYPE,
            lsp_types::SemanticTokenType::CLASS,
            lsp_types::SemanticTokenType::ENUM,
            lsp_types::SemanticTokenType::INTERFACE,
            lsp_types::SemanticTokenType::STRUCT,
            lsp_types::SemanticTokenType::TYPE_PARAMETER,
            lsp_types::SemanticTokenType::PARAMETER,
            lsp_types::SemanticTokenType::VARIABLE,
            lsp_types::SemanticTokenType::PROPERTY,
            lsp_types::SemanticTokenType::ENUM_MEMBER,
            lsp_types::SemanticTokenType::EVENT,
            lsp_types::SemanticTokenType::FUNCTION,
            lsp_types::SemanticTokenType::METHOD,
            lsp_types::SemanticTokenType::MACRO,
            lsp_types::SemanticTokenType::KEYWORD,
            lsp_types::SemanticTokenType::MODIFIER,
            lsp_types::SemanticTokenType::COMMENT,
            lsp_types::SemanticTokenType::STRING,
            lsp_types::SemanticTokenType::NUMBER,
            lsp_types::SemanticTokenType::REGEXP,
            lsp_types::SemanticTokenType::OPERATOR,
            lsp_types::SemanticTokenType::DECORATOR,
        ],
        token_modifiers: vec![
            lsp_types::SemanticTokenModifier::DECLARATION,
            lsp_types::SemanticTokenModifier::DEFINITION,
            lsp_types::SemanticTokenModifier::READONLY,
            lsp_types::SemanticTokenModifier::STATIC,
            lsp_types::SemanticTokenModifier::DEPRECATED,
            lsp_types::SemanticTokenModifier::ABSTRACT,
            lsp_types::SemanticTokenModifier::ASYNC,
            lsp_types::SemanticTokenModifier::MODIFICATION,
            lsp_types::SemanticTokenModifier::DOCUMENTATION,
            lsp_types::SemanticTokenModifier::DEFAULT_LIBRARY,
        ],
        formats: vec![lsp_types::TokenFormat::RELATIVE],
        overlapping_token_support: Some(false),
        multiline_token_support: Some(false),
        server_cancel_support: Some(true),
        augments_syntax_tokens: Some(true),
    };
    let inlay_hint = lsp_types::InlayHintClientCapabilities {
        dynamic_registration: Some(false),
        resolve_support: None,
    };
    let general = lsp_types::GeneralClientCapabilities {
        position_encodings: Some(vec![
            lsp_types::PositionEncodingKind::UTF16,
            lsp_types::PositionEncodingKind::UTF8,
            lsp_types::PositionEncodingKind::UTF32,
        ]),
        ..Default::default()
    };

    lsp_types::ClientCapabilities {
        workspace: Some(lsp_types::WorkspaceClientCapabilities {
            apply_edit: Some(true),
            workspace_folders: Some(true),
            configuration: Some(true),
            symbol: Some(workspace_symbol),
            ..Default::default()
        }),
        text_document: Some(lsp_types::TextDocumentClientCapabilities {
            completion: Some(completion),
            signature_help: Some(signature_help),
            document_symbol: Some(document_symbol),
            semantic_tokens: Some(semantic_tokens),
            inlay_hint: Some(inlay_hint),
            ..Default::default()
        }),
        general: Some(general),
        ..Default::default()
    }
}

fn language_id_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|s| s.to_str()) {
        Some("rs") => "rust",
        _ => "plaintext",
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/kernel/services/adapters/lsp.rs"]
mod tests;
