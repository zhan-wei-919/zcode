use super::convert::{client_capabilities, path_to_url, workspace_folders_for_root};
use super::wire::{
    child_watch_loop, reader_loop, stderr_loop, writer_loop, InitState, LspPending, LspProcess,
    ReaderLoopArgs,
};
use super::LspService;
use lsp_server::{Message, Request, RequestId};
use lsp_types::request::Request as _;
use std::collections::VecDeque;
use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

impl LspService {
    pub(super) fn schedule_restart_backoff(&mut self) {
        let attempt = self.restart_attempts.saturating_add(1);
        self.restart_attempts = attempt;

        let shift = attempt.saturating_sub(1).min(6);
        let delay_ms = 200u64.saturating_mul(1u64 << shift);
        let delay = Duration::from_millis(delay_ms.min(5_000));
        self.restart_backoff_until = Some(Instant::now() + delay);
    }

    pub(super) fn ensure_started(&mut self) -> bool {
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
}
