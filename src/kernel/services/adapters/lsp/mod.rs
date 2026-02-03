use crate::core::Service;
use crate::kernel::services::KernelServiceContext;
use lsp_server::RequestId;
use rustc_hash::FxHashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicI32;
use std::sync::{Arc, Mutex};
use std::time::Instant;

mod convert;
mod process;
mod requests;
mod sync;
mod wire;

use wire::{LspProcess, LspRequestKind};

#[cfg(test)]
use lsp_server::Response;
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use wire::handle_response;

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
}

impl Service for LspService {
    fn name(&self) -> &'static str {
        "LspService"
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/kernel/services/adapters/lsp.rs"]
mod tests;
