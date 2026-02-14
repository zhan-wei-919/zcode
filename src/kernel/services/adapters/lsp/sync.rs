use super::convert::{language_id_for_path, lsp_version, path_to_url, text_change_event};
use super::LspClient;
use crate::kernel::services::ports::LspTextChange;
use lsp_server::{Message, Notification};
use lsp_types::notification::Notification as _;
use std::path::Path;

impl LspClient {
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

        let Some(previous_version) = self.doc_versions.get(path).copied() else {
            let text = text();
            self.did_open(path, &text, version);
            return;
        };

        let can_apply_incremental = previous_version.saturating_add(1) == version;

        match (change, can_apply_incremental) {
            (Some(change), true) => self.did_change(path, "", version, Some(change)),
            _ => {
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
        if let Ok(mut map) = self.latest_semantic_tokens_by_path.lock() {
            map.remove(path);
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

    fn did_open(&mut self, path: &Path, text: &str, version: u64) {
        let Some(uri) = path_to_url(path) else {
            return;
        };

        let language_id = language_id_for_path(path);
        let item = lsp_types::TextDocumentItem::new(
            uri,
            language_id.to_string(),
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
        tracing::debug!(
            path = %path.display(),
            language_id,
            version,
            "lsp didOpen"
        );
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
}
