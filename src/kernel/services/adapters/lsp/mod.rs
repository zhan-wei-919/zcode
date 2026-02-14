use crate::core::Service;
use crate::kernel::services::ports::{
    LspCompletionItem, LspCompletionTriggerContext, LspPosition, LspRange, LspServerKind,
    LspTextChange,
};
use crate::kernel::services::KernelServiceContext;
use lsp_server::RequestId;
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicI32;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::kernel::language::LanguageId;
use crate::kernel::lsp_registry::language_root_for_file;

mod convert;
mod discovery;
mod process;
mod requests;
mod sync;
mod wire;

use wire::{LspProcess, LspRequestKind};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LspServerCommandOverride {
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub initialization_options: Option<Value>,
}

#[cfg(test)]
use lsp_server::Response;
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use wire::handle_response;

impl LspServerKind {
    fn install_hint(self) -> &'static str {
        match self {
            Self::RustAnalyzer => "install rust-analyzer (e.g. `rustup component add rust-analyzer`)",
            Self::Gopls => "install gopls (e.g. `go install golang.org/x/tools/gopls@latest`)",
            Self::Pyright => "install pyright-langserver (e.g. `npm i -g pyright` or `pip install pyright`)",
            Self::TypeScriptLanguageServer => {
                "install typescript-language-server (e.g. `npm i -g typescript-language-server typescript`)"
            }
            Self::Clangd => "install clangd (usually from llvm/clang toolchain packages)",
            Self::Jdtls => "install jdtls (Eclipse JDT Language Server) and ensure `jdtls` is in PATH",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ClientKey {
    server: LspServerKind,
    root: PathBuf,
}

/// LSP hub service: manages per-(language,root) LSP clients.
pub struct LspService {
    workspace_root: PathBuf,
    ctx: KernelServiceContext,
    command_override: Option<(String, Vec<String>, Option<Value>)>,
    server_command_overrides: FxHashMap<LspServerKind, LspServerCommandOverride>,
    clients: FxHashMap<ClientKey, LspClient>,
    warned_missing: FxHashSet<LspServerKind>,
    debug_command: String,
    debug_args: Vec<String>,
}

impl LspService {
    pub fn new(workspace_root: PathBuf, ctx: KernelServiceContext) -> Self {
        Self {
            workspace_root,
            ctx,
            command_override: None,
            server_command_overrides: FxHashMap::default(),
            clients: FxHashMap::default(),
            warned_missing: FxHashSet::default(),
            debug_command: "rust-analyzer".to_string(),
            debug_args: Vec::new(),
        }
    }

    /// Backward-compatible debug view (used by tests): returns the configured override if present,
    /// otherwise the Rust default command name.
    pub fn command_config(&self) -> (&str, &[String]) {
        (&self.debug_command, &self.debug_args)
    }

    /// Global override (primarily for tests / debugging).
    pub fn with_command(mut self, command: String, args: Vec<String>) -> Self {
        self.command_override = Some((command.clone(), args.clone(), None));
        self.debug_command = command;
        self.debug_args = args;
        self
    }

    pub fn with_command_and_init_options(
        mut self,
        command: String,
        args: Vec<String>,
        initialization_options: Option<Value>,
    ) -> Self {
        self.command_override = Some((command.clone(), args.clone(), initialization_options));
        self.debug_command = command;
        self.debug_args = args;
        self
    }

    pub(crate) fn with_server_command_overrides(
        mut self,
        overrides: FxHashMap<LspServerKind, LspServerCommandOverride>,
    ) -> Self {
        self.server_command_overrides = overrides;
        self
    }

    pub(crate) fn reconfigure(
        &mut self,
        command_override: Option<(String, Vec<String>, Option<Value>)>,
        server_command_overrides: FxHashMap<LspServerKind, LspServerCommandOverride>,
    ) -> bool {
        let changed = self.command_override != command_override
            || self.server_command_overrides != server_command_overrides;
        if !changed {
            return false;
        }

        self.command_override = command_override;
        self.server_command_overrides = server_command_overrides;

        if let Some((command, args, _)) = &self.command_override {
            self.debug_command = command.clone();
            self.debug_args = args.clone();
        } else {
            self.debug_command = "rust-analyzer".to_string();
            self.debug_args.clear();
        }

        for client in self.clients.values_mut() {
            client.shutdown();
        }
        self.clients.clear();
        self.warned_missing.clear();
        true
    }

    fn client_key_for_path(&self, path: &Path) -> Option<(LanguageId, ClientKey)> {
        let language = LanguageId::from_path(path)?;
        let root = language_root_for_file(&self.workspace_root, language, path);
        let server = language.server_kind()?;
        Some((language, ClientKey { server, root }))
    }

    fn default_args_for_language(language: LanguageId) -> Vec<String> {
        match language {
            LanguageId::Python
            | LanguageId::JavaScript
            | LanguageId::TypeScript
            | LanguageId::Jsx
            | LanguageId::Tsx => vec!["--stdio".to_string()],
            LanguageId::Java | LanguageId::C | LanguageId::Cpp => Vec::new(),
            _ => Vec::new(),
        }
    }

    fn default_initialization_options_for_server(server: LspServerKind) -> Option<Value> {
        match server {
            LspServerKind::Gopls => Some(json!({ "semanticTokens": true })),
            _ => None,
        }
    }

    fn resolve_server_command(
        &mut self,
        language: LanguageId,
        key: &ClientKey,
    ) -> Option<(String, Vec<String>, Option<Value>)> {
        if let Some(global) = self.command_override.clone() {
            return Some(global);
        }

        let per_server = self.server_command_overrides.get(&key.server);
        let default =
            discovery::resolve_default_server_command(&self.workspace_root, &key.root, language);

        let command = per_server
            .and_then(|cfg| cfg.command.clone())
            .or_else(|| default.as_ref().map(|(cmd, _)| cmd.clone()))?;

        let args = per_server
            .and_then(|cfg| cfg.args.clone())
            .or_else(|| default.as_ref().map(|(_, args)| args.clone()))
            .unwrap_or_else(|| Self::default_args_for_language(language));

        let initialization_options = per_server
            .and_then(|cfg| cfg.initialization_options.clone())
            .or_else(|| Self::default_initialization_options_for_server(key.server));

        Some((command, args, initialization_options))
    }

    fn client_for_path_mut(&mut self, path: &Path) -> Option<&mut LspClient> {
        let (language, key) = self.client_key_for_path(path)?;
        if !self.clients.contains_key(&key) {
            let Some((command, args, initialization_options)) =
                self.resolve_server_command(language, &key)
            else {
                if self.warned_missing.insert(key.server) {
                    tracing::warn!(
                        language = language.display_name(),
                        hint = key.server.install_hint(),
                        "lsp server not found"
                    );
                }
                return None;
            };

            let client = LspClient::new(key.root.clone(), key.server, self.ctx.clone())
                .with_command(command, args)
                .with_initialization_options(initialization_options);
            self.clients.insert(key.clone(), client);
        }

        self.clients.get_mut(&key)
    }

    pub fn needs_sync(&mut self, path: &Path, version: u64) -> bool {
        self.client_for_path_mut(path)
            .is_some_and(|client| client.needs_sync(path, version))
    }

    pub fn sync_document(
        &mut self,
        path: &Path,
        version: u64,
        change: Option<LspTextChange>,
        text: impl FnOnce() -> String,
    ) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.sync_document(path, version, change, text);
    }

    pub fn close_document(&mut self, path: &Path) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.close_document(path);
    }

    pub fn save_document(&mut self, path: &Path) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.save_document(path);
    }

    pub fn cancel_hover(&mut self) {
        for client in self.clients.values_mut() {
            client.cancel_hover();
        }
    }

    pub fn request_hover(&mut self, path: &Path, position: LspPosition) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_hover(path, position);
    }

    pub fn request_definition(&mut self, path: &Path, position: LspPosition) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_definition(path, position);
    }

    pub fn request_references(&mut self, path: &Path, position: LspPosition) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_references(path, position);
    }

    pub fn request_document_symbols(&mut self, path: &Path) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_document_symbols(path);
    }

    pub fn request_workspace_symbols(&mut self, query: String) {
        for client in self.clients.values_mut() {
            client.request_workspace_symbols(query.clone());
        }
    }

    pub fn request_code_action(&mut self, path: &Path, position: LspPosition) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_code_action(path, position);
    }

    pub fn request_completion(
        &mut self,
        path: &Path,
        position: LspPosition,
        trigger: LspCompletionTriggerContext,
    ) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_completion(path, position, trigger);
    }

    pub fn request_completion_resolve(&mut self, item: LspCompletionItem) {
        for client in self.clients.values_mut() {
            client.request_completion_resolve(item.clone());
        }
    }

    pub fn request_semantic_tokens(&mut self, path: &Path, version: u64) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_semantic_tokens(path, version);
    }

    pub fn request_semantic_tokens_range(&mut self, path: &Path, range: LspRange, version: u64) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_semantic_tokens_range(path, range, version);
    }

    pub fn request_inlay_hints(&mut self, path: &Path, range: LspRange, version: u64) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_inlay_hints(path, range, version);
    }

    pub fn request_folding_range(&mut self, path: &Path, version: u64) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_folding_range(path, version);
    }

    pub fn request_signature_help(&mut self, path: &Path, position: LspPosition) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_signature_help(path, position);
    }

    pub fn request_rename(&mut self, path: &Path, position: LspPosition, new_name: String) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_rename(path, position, new_name);
    }

    pub fn request_format(&mut self, path: &Path) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_format(path);
    }

    pub fn request_range_format(&mut self, path: &Path, range: LspRange) {
        let Some(client) = self.client_for_path_mut(path) else {
            return;
        };
        client.request_range_format(path, range);
    }

    pub fn execute_command(&mut self, command: String, arguments: Vec<serde_json::Value>) {
        for client in self.clients.values_mut() {
            client.execute_command(command.clone(), arguments.clone());
        }
    }

    pub fn shutdown(&mut self) {
        for client in self.clients.values_mut() {
            client.shutdown();
        }
    }
}

impl Service for LspService {
    fn name(&self) -> &'static str {
        "LspService"
    }
}

struct LspClient {
    server: LspServerKind,
    root: PathBuf,
    command: String,
    args: Vec<String>,
    initialization_options: Option<Value>,
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
    latest_semantic_tokens_by_path: Arc<Mutex<FxHashMap<PathBuf, i32>>>,
    latest_inlay_hints: Arc<AtomicI32>,
    latest_folding_range: Arc<AtomicI32>,
    latest_signature_help: Arc<AtomicI32>,
    latest_format: Arc<AtomicI32>,
    latest_rename: Arc<AtomicI32>,
    latest_shutdown: Arc<AtomicI32>,
}

impl LspClient {
    fn new(root: PathBuf, server: LspServerKind, ctx: KernelServiceContext) -> Self {
        Self {
            server,
            root,
            command: "rust-analyzer".to_string(),
            args: Vec::new(),
            initialization_options: None,
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
            latest_semantic_tokens_by_path: Arc::new(Mutex::new(FxHashMap::default())),
            latest_inlay_hints: Arc::new(AtomicI32::new(0)),
            latest_folding_range: Arc::new(AtomicI32::new(0)),
            latest_signature_help: Arc::new(AtomicI32::new(0)),
            latest_format: Arc::new(AtomicI32::new(0)),
            latest_rename: Arc::new(AtomicI32::new(0)),
            latest_shutdown: Arc::new(AtomicI32::new(0)),
        }
    }

    fn with_command(mut self, command: String, args: Vec<String>) -> Self {
        self.command = command;
        self.args = args;
        self
    }

    fn with_initialization_options(mut self, initialization_options: Option<Value>) -> Self {
        self.initialization_options = initialization_options;
        self
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/kernel/services/adapters/lsp.rs"]
mod tests;
