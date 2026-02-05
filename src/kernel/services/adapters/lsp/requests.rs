use super::convert::{completion_item_to_lsp, path_to_url};
use super::{LspClient, LspRequestKind};
use crate::kernel::services::ports::{LspCompletionItem, LspPosition, LspRange};
use lsp_server::{Message, Notification, Request, RequestId};
use lsp_types::notification::Notification as _;
use lsp_types::request::Request as _;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::Ordering;

impl LspClient {
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
            tracing::debug!(
                path = %path.display(),
                version,
                "lsp inlay hints skipped (server not ready)"
            );
            return;
        }

        if !self.doc_versions.contains_key(path) {
            tracing::debug!(
                path = %path.display(),
                version,
                "lsp inlay hints skipped (document not synced)"
            );
            return;
        }

        let Some(uri) = path_to_url(path) else {
            return;
        };

        let id = self.next_id();
        let prev = self.latest_inlay_hints.swap(id, Ordering::Relaxed);

        tracing::debug!(
            id,
            path = %path.display(),
            version,
            start_line = range.start.line,
            start_char = range.start.character,
            end_line = range.end.line,
            end_char = range.end.character,
            "lsp inlay hints request"
        );

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
