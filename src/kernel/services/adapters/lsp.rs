use crate::core::Service;
use crate::kernel::problems::{ProblemItem, ProblemRange, ProblemSeverity};
use crate::kernel::services::KernelServiceContext;
use crate::kernel::Action;
use rustc_hash::FxHashMap;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Debug, Clone, Copy)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

#[derive(Debug, Clone)]
pub struct LspTextChange {
    pub range: Option<LspRange>,
    pub text: String,
}

impl LspTextChange {
    fn to_json(&self) -> Value {
        match self.range {
            Some(range) => json!({
                "range": {
                    "start": { "line": range.start.line, "character": range.start.character },
                    "end": { "line": range.end.line, "character": range.end.character },
                },
                "text": self.text,
            }),
            None => json!({
                "text": self.text,
            }),
        }
    }
}

pub struct LspService {
    root: PathBuf,
    command: String,
    args: Vec<String>,
    ctx: KernelServiceContext,
    handle: tokio::runtime::Handle,
    process: Option<LspProcess>,
    next_id: u64,
    doc_versions: FxHashMap<PathBuf, u64>,
    pending_requests: Arc<Mutex<FxHashMap<u64, LspRequestKind>>>,
}

struct LspProcess {
    tx: UnboundedSender<String>,
    init_done: Arc<AtomicBool>,
    pending: Arc<Mutex<VecDeque<String>>>,
}

#[derive(Debug, Clone, Copy)]
enum LspRequestKind {
    Hover,
    Definition,
}

impl LspService {
    pub fn new(root: PathBuf, ctx: KernelServiceContext, handle: tokio::runtime::Handle) -> Self {
        Self {
            root,
            command: "rust-analyzer".to_string(),
            args: Vec::new(),
            ctx,
            handle,
            process: None,
            next_id: 1,
            doc_versions: FxHashMap::default(),
            pending_requests: Arc::new(Mutex::new(FxHashMap::default())),
        }
    }

    pub fn with_command(mut self, command: String, args: Vec<String>) -> Self {
        self.command = command;
        self.args = args;
        self
    }

    pub fn needs_sync(&self, path: &Path, version: u64) -> bool {
        self.doc_versions
            .get(path)
            .map_or(true, |v| *v != version)
    }

    pub fn sync_document(
        &mut self,
        path: &Path,
        text: &str,
        version: u64,
        change: Option<LspTextChange>,
    ) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            self.did_open(path, text, version);
            return;
        }

        if !self.needs_sync(path, version) {
            return;
        }

        self.did_change(path, text, version, change);
    }

    pub fn close_document(&mut self, path: &Path) {
        if !self.ensure_started() {
            return;
        }

        if self.doc_versions.remove(path).is_none() {
            return;
        }

        let uri = path_to_uri(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": {
                "textDocument": {
                    "uri": uri,
                }
            }
        });
        self.send_json(msg, true);
    }

    pub fn save_document(&mut self, path: &Path) {
        if !self.ensure_started() {
            return;
        }

        if !self.doc_versions.contains_key(path) {
            return;
        }

        let uri = path_to_uri(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didSave",
            "params": {
                "textDocument": {
                    "uri": uri,
                }
            }
        });
        self.send_json(msg, true);
    }

    pub fn request_hover(&mut self, path: &Path, position: LspPosition) {
        if !self.ensure_started() {
            return;
        }

        let uri = path_to_uri(path);
        let id = self.next_id();
        self.track_request(id, LspRequestKind::Hover);

        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": position.line, "character": position.character },
            }
        });
        self.send_json(msg, true);
    }

    pub fn request_definition(&mut self, path: &Path, position: LspPosition) {
        if !self.ensure_started() {
            return;
        }

        let uri = path_to_uri(path);
        let id = self.next_id();
        self.track_request(id, LspRequestKind::Definition);

        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": position.line, "character": position.character },
            }
        });
        self.send_json(msg, true);
    }

    fn did_open(&mut self, path: &Path, text: &str, version: u64) {
        let uri = path_to_uri(path);
        let language_id = language_id_for_path(path);
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": version,
                    "text": text,
                }
            }
        });
        self.doc_versions.insert(path.to_path_buf(), version);
        self.send_json(msg, true);
    }

    fn did_change(
        &mut self,
        path: &Path,
        text: &str,
        version: u64,
        change: Option<LspTextChange>,
    ) {
        let uri = path_to_uri(path);
        let changes = match change {
            Some(change) => vec![change.to_json()],
            None => vec![json!({ "text": text })],
        };
        let msg = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "version": version,
                },
                "contentChanges": changes,
            }
        });
        self.doc_versions.insert(path.to_path_buf(), version);
        self.send_json(msg, true);
    }

    fn ensure_started(&mut self) -> bool {
        if self.process.is_some() {
            return true;
        }

        let _guard = self.handle.enter();
        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                tracing::error!(error = %e, "spawn rust-analyzer failed");
                return false;
            }
        };

        let stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                tracing::error!("rust-analyzer stdin unavailable");
                return false;
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                tracing::error!("rust-analyzer stdout unavailable");
                return false;
            }
        };
        let stderr = child.stderr.take();

        let (tx, rx) = unbounded_channel::<String>();
        let init_done = Arc::new(AtomicBool::new(false));
        let init_id = self.next_id();
        let pending = Arc::new(Mutex::new(VecDeque::new()));

        self.handle.spawn(writer_loop(stdin, rx));
        self.handle.spawn(reader_loop(
            stdout,
            self.ctx.clone(),
            init_id,
            init_done.clone(),
            pending.clone(),
            self.pending_requests.clone(),
            tx.clone(),
        ));
        if let Some(stderr) = stderr {
            self.handle.spawn(stderr_loop(stderr));
        }

        self.process = Some(LspProcess {
            tx,
            init_done,
            pending,
        });

        self.send_initialize(init_id);
        true
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn send_initialize(&mut self, init_id: u64) {
        let root_uri = path_to_uri(&self.root);
        let params = json!({
            "processId": null,
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "synchronization": {
                        "didSave": true,
                        "change": 2,
                    }
                }
            },
            "trace": "off",
        });
        let msg = json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": params,
        });
        self.send_json(msg, false);
    }

    fn send_json(&mut self, msg: Value, requires_init: bool) {
        let Some(process) = self.process.as_mut() else {
            return;
        };

        let payload = match serde_json::to_string(&msg) {
            Ok(payload) => payload,
            Err(e) => {
                tracing::error!(error = %e, "serialize lsp message failed");
                return;
            }
        };

        if requires_init && !process.init_done.load(Ordering::Relaxed) {
            if let Ok(mut pending) = process.pending.lock() {
                pending.push_back(payload);
            }
            return;
        }

        if process.tx.send(payload).is_err() {
            tracing::warn!("lsp writer channel closed");
        }

        if requires_init && process.init_done.load(Ordering::Relaxed) {
            self.flush_pending();
        }
    }

    fn flush_pending(&mut self) {
        let Some(process) = self.process.as_mut() else {
            return;
        };
        if !process.init_done.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut pending) = process.pending.lock() {
            while let Some(payload) = pending.pop_front() {
                if process.tx.send(payload).is_err() {
                    break;
                }
            }
        }
    }

    fn track_request(&mut self, id: u64, kind: LspRequestKind) {
        if let Ok(mut map) = self.pending_requests.lock() {
            map.insert(id, kind);
        }
    }
}

impl Service for LspService {
    fn name(&self) -> &'static str {
        "LspService"
    }
}

async fn writer_loop(
    mut stdin: tokio::process::ChildStdin,
    mut rx: UnboundedReceiver<String>,
) {
    while let Some(payload) = rx.recv().await {
        let header = format!("Content-Length: {}\r\n\r\n", payload.as_bytes().len());
        if stdin.write_all(header.as_bytes()).await.is_err() {
            break;
        }
        if stdin.write_all(payload.as_bytes()).await.is_err() {
            break;
        }
    }
}

async fn reader_loop(
    stdout: tokio::process::ChildStdout,
    ctx: KernelServiceContext,
    init_id: u64,
    init_done: Arc<AtomicBool>,
    pending: Arc<Mutex<VecDeque<String>>>,
    pending_requests: Arc<Mutex<FxHashMap<u64, LspRequestKind>>>,
    tx: UnboundedSender<String>,
) {
    let mut reader = BufReader::new(stdout);

    loop {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            let read = match reader.read_line(&mut line).await {
                Ok(read) => read,
                Err(_) => return,
            };
            if read == 0 {
                return;
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = trimmed.strip_prefix("Content-Length:") {
                let len = value.trim().parse::<usize>().ok();
                content_length = len;
            }
        }

        let Some(len) = content_length else {
            continue;
        };
        let mut buf = vec![0u8; len];
        if reader.read_exact(&mut buf).await.is_err() {
            return;
        }

        let Ok(msg) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };

        if let Some(id) = msg.get("id") {
            if let Some(method) = msg.get("method").and_then(Value::as_str) {
                if let Some(response) = handle_server_request(method, id, &msg) {
                    let _ = tx.send(response);
                }
                continue;
            }

            let Some(id_value) = id.as_u64() else {
                continue;
            };

            if id_value == init_id {
                if let Some(err) = msg.get("error") {
                    tracing::error!(error = %err, "lsp initialize failed");
                    if let Ok(mut queue) = pending.lock() {
                        queue.clear();
                    }
                    continue;
                }
                init_done.store(true, Ordering::Relaxed);
                let initialized = json!({
                    "jsonrpc": "2.0",
                    "method": "initialized",
                    "params": {},
                });
                let _ = tx.send(initialized.to_string());
                if let Ok(mut queue) = pending.lock() {
                    while let Some(payload) = queue.pop_front() {
                        let _ = tx.send(payload);
                    }
                }
                continue;
            }

            if let Some(kind) = pending_requests
                .lock()
                .ok()
                .and_then(|mut map| map.remove(&id_value))
            {
                handle_response(kind, &msg, &ctx);
            }
            continue;
        }

        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            continue;
        };

        if method == "textDocument/publishDiagnostics" {
            if let Some(params) = msg.get("params") {
                if let Some((path, items)) = diagnostics_from_params(params) {
                    ctx.dispatch(Action::LspDiagnostics { path, items });
                }
            }
        }
    }
}

async fn stderr_loop(mut stderr: tokio::process::ChildStderr) {
    let mut buf = Vec::new();
    loop {
        let mut chunk = [0u8; 1024];
        let n = match stderr.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        buf.extend_from_slice(&chunk[..n]);
        while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
            let line = String::from_utf8_lossy(&buf[..pos]).to_string();
            buf.drain(..=pos);
            if !line.trim().is_empty() {
                tracing::info!("lsp: {}", line.trim_end());
            }
        }
    }
}

fn handle_response(kind: LspRequestKind, msg: &Value, ctx: &KernelServiceContext) {
    if let Some(err) = msg.get("error") {
        tracing::warn!(error = %err, "lsp request failed");
        if matches!(kind, LspRequestKind::Hover) {
            ctx.dispatch(Action::LspHover {
                text: String::new(),
            });
        }
        return;
    }

    let Some(result) = msg.get("result") else {
        if matches!(kind, LspRequestKind::Hover) {
            ctx.dispatch(Action::LspHover {
                text: String::new(),
            });
        }
        return;
    };

    match kind {
        LspRequestKind::Hover => {
            let text = hover_text_from_result(result).unwrap_or_default();
            ctx.dispatch(Action::LspHover { text });
        }
        LspRequestKind::Definition => {
            if let Some((path, line, column)) = definition_from_result(result) {
                ctx.dispatch(Action::LspDefinition { path, line, column });
            }
        }
    }
}

fn handle_server_request(method: &str, id: &Value, msg: &Value) -> Option<String> {
    let result = match method {
        "workspace/configuration" => {
            let items_len = msg
                .get("params")
                .and_then(|params| params.get("items"))
                .and_then(Value::as_array)
                .map(|items| items.len())
                .unwrap_or(0);
            let items: Vec<Value> = std::iter::repeat(Value::Null)
                .take(items_len)
                .collect();
            Value::Array(items)
        }
        "client/registerCapability" | "client/unregisterCapability" => Value::Null,
        _ => {
            let error = json!({
                "code": -32601,
                "message": "Method not found",
            });
            let resp = json!({
                "jsonrpc": "2.0",
                "id": id.clone(),
                "error": error,
            });
            return serde_json::to_string(&resp).ok();
        }
    };

    let resp = json!({
        "jsonrpc": "2.0",
        "id": id.clone(),
        "result": result,
    });
    serde_json::to_string(&resp).ok()
}

fn hover_text_from_result(result: &Value) -> Option<String> {
    let contents = result.get("contents")?;
    let mut parts = Vec::new();
    collect_hover_contents(contents, &mut parts);
    let text = parts.join("\n").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn collect_hover_contents(value: &Value, parts: &mut Vec<String>) {
    match value {
        Value::String(s) => parts.push(s.clone()),
        Value::Array(items) => {
            for item in items {
                collect_hover_contents(item, parts);
            }
        }
        Value::Object(map) => {
            if let Some(val) = map.get("value").and_then(Value::as_str) {
                parts.push(val.to_string());
            }
        }
        _ => {}
    }
}

fn definition_from_result(result: &Value) -> Option<(PathBuf, u32, u32)> {
    match result {
        Value::Array(items) => items.iter().find_map(location_from_value),
        _ => location_from_value(result),
    }
}

fn location_from_value(value: &Value) -> Option<(PathBuf, u32, u32)> {
    if let Some(uri) = value.get("uri").and_then(Value::as_str) {
        let range = value.get("range")?;
        return location_from_uri_range(uri, range);
    }
    if let Some(uri) = value.get("targetUri").and_then(Value::as_str) {
        let range = value
            .get("targetSelectionRange")
            .or_else(|| value.get("targetRange"))?;
        return location_from_uri_range(uri, range);
    }
    None
}

fn location_from_uri_range(uri: &str, range: &Value) -> Option<(PathBuf, u32, u32)> {
    let path = uri_to_path(uri)?;
    let start = range.get("start")?;
    let line = start.get("line")?.as_u64()? as u32;
    let column = start.get("character")?.as_u64()? as u32;
    Some((path, line, column))
}

fn diagnostics_from_params(params: &Value) -> Option<(PathBuf, Vec<ProblemItem>)> {
    let uri = params.get("uri")?.as_str()?;
    let path = uri_to_path(uri)?;
    let diags = params.get("diagnostics")?.as_array()?;

    let mut items = Vec::with_capacity(diags.len());
    for diag in diags {
        let range = diag.get("range")?;
        let start = range.get("start")?;
        let end = range.get("end")?;
        let start_line = start.get("line")?.as_u64()? as u32;
        let start_col = start.get("character")?.as_u64()? as u32;
        let end_line = end.get("line")?.as_u64()? as u32;
        let end_col = end.get("character")?.as_u64()? as u32;
        let message = diag.get("message")?.as_str()?.to_string();
        let source = diag
            .get("source")
            .and_then(Value::as_str)
            .map(|s| s.to_string());
        let severity = match diag.get("severity").and_then(Value::as_u64) {
            Some(1) => ProblemSeverity::Error,
            Some(2) => ProblemSeverity::Warning,
            Some(3) => ProblemSeverity::Information,
            Some(4) => ProblemSeverity::Hint,
            _ => ProblemSeverity::Information,
        };

        items.push(ProblemItem {
            path: path.clone(),
            range: ProblemRange {
                start_line,
                start_col,
                end_line,
                end_col,
            },
            severity,
            message,
            source,
        });
    }

    Some((path, items))
}

fn path_to_uri(path: &Path) -> String {
    let mut raw = path.to_string_lossy().replace('\\', "/");
    if !raw.starts_with('/') {
        raw = format!("/{raw}");
    }
    format!("file://{}", percent_encode(&raw))
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let uri = uri.strip_prefix("file://")?;
    let decoded = percent_decode(uri);
    if decoded.starts_with('/') && decoded.get(2..3) == Some(":") {
        Some(PathBuf::from(decoded.trim_start_matches('/')))
    } else {
        Some(PathBuf::from(decoded))
    }
}

fn percent_encode(input: &str) -> String {
    let mut out = String::new();
    for b in input.as_bytes() {
        let keep = matches!(b,
            b'a'..=b'z'
                | b'A'..=b'Z'
                | b'0'..=b'9'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b'/'
        );
        if keep {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = bytes[i + 1];
            let h2 = bytes[i + 2];
            if let (Some(d1), Some(d2)) = (hex_val(h1), hex_val(h2)) {
                out.push((d1 << 4) + d2);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + b - b'a'),
        b'A'..=b'F' => Some(10 + b - b'A'),
        _ => None,
    }
}

fn language_id_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|s| s.to_str()) {
        Some("rs") => "rust",
        _ => "plaintext",
    }
}
