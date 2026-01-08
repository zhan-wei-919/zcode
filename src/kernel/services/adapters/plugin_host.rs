//! 外部插件宿主：进程管理 + stdio(JSON-RPC/LSP framing)
//!
//! MVP：
//! - 只支持 stdio transport
//! - 启动时按 plugins.json 启动进程
//! - 接收 `zcode/register`、`zcode/ui/patch`、`zcode/log`
//! - 发送 `zcode/initialize`、`zcode/command/invoked`

use super::plugins::{PluginConfigEntry, PluginsConfig};
use crate::kernel::plugins::{PluginAction, PluginRegisterParams, PluginUiPatchParams, PluginPriority};
use crate::kernel::Action as KernelAction;
use rustc_hash::FxHashMap;
use serde_json::json;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc::{Receiver, SyncSender};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const PROTOCOL_VERSION: u32 = 1;
const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const EVENT_QUEUE_CAP: usize = 4096;

#[derive(Debug)]
pub enum PluginHostEvent {
    Action(KernelAction),
    Log(String),
}

#[derive(Clone)]
pub struct PluginHostHandle {
    tx: tokio::sync::mpsc::UnboundedSender<PluginHostCommand>,
}

#[derive(Debug)]
enum PluginHostCommand {
    Notify {
        plugin_id: String,
        method: String,
        params: serde_json::Value,
    },
}

impl PluginHostHandle {
    pub fn notify_command_invoked(&self, plugin_id: String, command_id: String) {
        let _ = self.tx.send(PluginHostCommand::Notify {
            plugin_id,
            method: "zcode/command/invoked".to_string(),
            params: json!({ "id": command_id }),
        });
    }
}

pub struct PluginHost {
    pub handle: PluginHostHandle,
    pub high_rx: Receiver<PluginHostEvent>,
    pub low_rx: Receiver<PluginHostEvent>,
}

impl PluginHost {
    pub fn start(
        runtime: tokio::runtime::Handle,
        workspace_root: PathBuf,
        config: PluginsConfig,
    ) -> Self {
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let (high_tx, high_rx) = std::sync::mpsc::sync_channel(EVENT_QUEUE_CAP);
        let (low_tx, low_rx) = std::sync::mpsc::sync_channel(EVENT_QUEUE_CAP);

        runtime.spawn(async move {
            run_manager(workspace_root, config, cmd_rx, high_tx, low_tx).await;
        });

        Self {
            handle: PluginHostHandle { tx: cmd_tx },
            high_rx,
            low_rx,
        }
    }
}

async fn run_manager(
    workspace_root: PathBuf,
    config: PluginsConfig,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<PluginHostCommand>,
    high_tx: SyncSender<PluginHostEvent>,
    low_tx: SyncSender<PluginHostEvent>,
) {
    let mut senders: FxHashMap<String, tokio::sync::mpsc::UnboundedSender<Vec<u8>>> =
        FxHashMap::default();

    for plugin in config.plugins {
        if !plugin.enabled {
            continue;
        }
        if plugin.transport.kind != "stdio" {
            let _ = try_send(
                &low_tx,
                PluginHostEvent::Log(format!(
                    "[plugin:{}] unsupported transport: {}",
                    plugin.id, plugin.transport.kind
                )),
            );
            continue;
        }

        let tx = if plugin.priority == PluginPriority::High {
            &high_tx
        } else {
            &low_tx
        };

        let _ = try_send(
            tx,
            PluginHostEvent::Action(KernelAction::Plugin(PluginAction::Discovered {
                id: plugin.id.clone(),
                priority: plugin.priority,
            })),
        );

        match spawn_plugin(workspace_root.clone(), plugin, high_tx.clone(), low_tx.clone()).await {
            Ok((plugin_id, out_tx)) => {
                senders.insert(plugin_id, out_tx);
            }
            Err(e) => {
                let _ = try_send(&low_tx, PluginHostEvent::Log(format!("{e}")));
            }
        }
    }

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            PluginHostCommand::Notify {
                plugin_id,
                method,
                params,
            } => {
                let Some(tx) = senders.get(&plugin_id) else {
                    continue;
                };
                let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
                let bytes = encode_lsp_frame(&msg);
                if tx.send(bytes).is_err() {
                    senders.remove(&plugin_id);
                }
            }
        }
    }
}

async fn spawn_plugin(
    workspace_root: PathBuf,
    plugin: PluginConfigEntry,
    high_tx: SyncSender<PluginHostEvent>,
    low_tx: SyncSender<PluginHostEvent>,
) -> Result<(String, tokio::sync::mpsc::UnboundedSender<Vec<u8>>), String> {
    let mut cmd = Command::new(&plugin.command);
    cmd.args(&plugin.args)
        .current_dir(&workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (k, v) in &plugin.env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "[plugin:{}] spawn failed: {} ({})",
            plugin.id, plugin.command, e
        )
    })?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("[plugin:{}] stdin unavailable", plugin.id))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("[plugin:{}] stdout unavailable", plugin.id))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("[plugin:{}] stderr unavailable", plugin.id))?;

    let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

    let event_tx = if plugin.priority == PluginPriority::High {
        high_tx
    } else {
        low_tx
    };

    let plugin_id = plugin.id.clone();
    tokio::spawn(writer_loop(plugin_id.clone(), stdin, out_rx));
    tokio::spawn(reader_loop(
        plugin_id.clone(),
        plugin.priority,
        child,
        stdout,
        out_tx.clone(),
        event_tx.clone(),
    ));
    tokio::spawn(stderr_loop(plugin_id.clone(), stderr, event_tx.clone()));

    let init = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "zcode/initialize",
        "params": {
            "protocol_version": PROTOCOL_VERSION,
            "workspace_root": workspace_root,
        }
    });
    let _ = out_tx.send(encode_lsp_frame(&init));

    Ok((plugin_id, out_tx))
}

async fn writer_loop(
    plugin_id: String,
    mut stdin: tokio::process::ChildStdin,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
) {
    while let Some(msg) = rx.recv().await {
        if stdin.write_all(&msg).await.is_err() {
            break;
        }
        let _ = stdin.flush().await;
    }

    tracing::debug!(plugin_id = %plugin_id, "plugin writer loop ended");
}

async fn reader_loop(
    plugin_id: String,
    priority: PluginPriority,
    mut child: tokio::process::Child,
    stdout: tokio::process::ChildStdout,
    out_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    event_tx: SyncSender<PluginHostEvent>,
) {
    let mut reader = BufReader::new(stdout);
    let mut header_line = String::new();

    loop {
        match read_lsp_frame(&mut reader, &mut header_line).await {
            Ok(Some(bytes)) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(msg) => {
                    if let Some(method) = msg.get("method").and_then(|v| v.as_str()) {
                        handle_method(
                            &plugin_id,
                            priority,
                            method,
                            msg.get("id"),
                            msg.get("params"),
                            &out_tx,
                            &event_tx,
                        )
                        .await;
                    }
                }
                Err(e) => {
                    let _ = try_send(
                        &event_tx,
                        PluginHostEvent::Log(format!("[plugin:{plugin_id}] invalid json: {e}")),
                    );
                }
            },
            Ok(None) => break,
            Err(e) => {
                let _ = try_send(
                    &event_tx,
                    PluginHostEvent::Log(format!("[plugin:{plugin_id}] read error: {e}")),
                );
                break;
            }
        }
    }

    let _ = child.kill().await;
    let status = child.wait().await.ok();
    let reason = status
        .map(|s| format!("process exited: {s}"))
        .unwrap_or_else(|| "process ended".to_string());

    let _ = try_send(
        &event_tx,
        PluginHostEvent::Action(KernelAction::Plugin(PluginAction::Offline {
            id: plugin_id.clone(),
            reason: Some(reason),
        })),
    );
}

async fn stderr_loop(
    plugin_id: String,
    stderr: tokio::process::ChildStderr,
    event_tx: SyncSender<PluginHostEvent>,
) {
    let mut reader = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        let _ = try_send(
            &event_tx,
            PluginHostEvent::Log(format!("[plugin:{plugin_id}] {line}")),
        );
    }
}

async fn handle_method(
    plugin_id: &str,
    priority: PluginPriority,
    method: &str,
    id: Option<&serde_json::Value>,
    params: Option<&serde_json::Value>,
    out_tx: &tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    event_tx: &SyncSender<PluginHostEvent>,
) {
    match method {
        "zcode/register" => {
            let Some(params) = params else {
                return;
            };
            let parsed = serde_json::from_value::<PluginRegisterParams>(params.clone());
            match parsed {
                Ok(reg) => {
                    if reg.id != plugin_id {
                        let _ = try_send(
                            event_tx,
                            PluginHostEvent::Log(format!(
                                "[plugin:{plugin_id}] register id mismatch: {}",
                                reg.id
                            )),
                        );
                        return;
                    }

                    let _ = try_send(
                        event_tx,
                        PluginHostEvent::Action(KernelAction::Plugin(PluginAction::Registered {
                            id: reg.id,
                            name: reg.name,
                            priority,
                            commands: reg.commands,
                            status_items: reg.status_items,
                        })),
                    );

                    if let Some(id) = id.cloned() {
                        let resp = json!({ "jsonrpc": "2.0", "id": id, "result": { "ok": true } });
                        let _ = out_tx.send(encode_lsp_frame(&resp));
                    }
                }
                Err(e) => {
                    let _ = try_send(
                        event_tx,
                        PluginHostEvent::Log(format!("[plugin:{plugin_id}] register decode: {e}")),
                    );
                }
            }
        }
        "zcode/ui/patch" => {
            let Some(params) = params else {
                return;
            };
            match serde_json::from_value::<PluginUiPatchParams>(params.clone()) {
                Ok(patch) => {
                    let _ = try_send(
                        event_tx,
                        PluginHostEvent::Action(KernelAction::Plugin(PluginAction::UiPatch {
                            id: plugin_id.to_string(),
                            patch,
                        })),
                    );
                }
                Err(e) => {
                    let _ = try_send(
                        event_tx,
                        PluginHostEvent::Log(format!("[plugin:{plugin_id}] ui/patch decode: {e}")),
                    );
                }
            }
        }
        "zcode/log" => {
            if let Some(params) = params {
                if let Some(msg) = params.get("message").and_then(|v| v.as_str()) {
                    let _ = try_send(
                        event_tx,
                        PluginHostEvent::Log(format!("[plugin:{plugin_id}] {msg}")),
                    );
                }
            }
        }
        _ => {}
    }
}

fn encode_lsp_frame(msg: &serde_json::Value) -> Vec<u8> {
    let body = serde_json::to_vec(msg).unwrap_or_else(|_| b"{}".to_vec());
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut out = Vec::with_capacity(header.len() + body.len());
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(&body);
    out
}

async fn read_lsp_frame<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    header_line: &mut String,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut content_len: Option<usize> = None;

    loop {
        header_line.clear();
        let n = reader.read_line(header_line).await?;
        if n == 0 {
            return Ok(None);
        }

        let line = header_line.trim_end_matches(|c| c == '\r' || c == '\n');
        if line.is_empty() {
            break;
        }

        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        if k.eq_ignore_ascii_case("content-length") {
            content_len = v.trim().parse::<usize>().ok();
        }
    }

    let len = content_len.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "missing Content-Length")
    })?;
    if len > MAX_MESSAGE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "message too large",
        ));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    Ok(Some(buf))
}

fn try_send(tx: &SyncSender<PluginHostEvent>, event: PluginHostEvent) -> Result<(), ()> {
    tx.try_send(event).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_framing_roundtrip() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let (mut tx, rx) = tokio::io::duplex(1024);
            let msg = json!({
                "jsonrpc": "2.0",
                "method": "zcode/register",
                "params": { "id": "x" }
            });
            let frame = encode_lsp_frame(&msg);
            tx.write_all(&frame).await.unwrap();
            tx.shutdown().await.unwrap();

            let mut reader = BufReader::new(rx);
            let mut header = String::new();
            let bytes = read_lsp_frame(&mut reader, &mut header)
                .await
                .unwrap()
                .unwrap();
            let decoded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(decoded, msg);
        });
    }
}
