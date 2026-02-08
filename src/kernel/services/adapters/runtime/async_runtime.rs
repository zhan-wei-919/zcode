use super::message::AppMessage;
use crate::kernel::editor::ReloadRequest;
use crate::kernel::services::adapters::git as git_helpers;
use crate::kernel::services::ports::DirEntryInfo;
use crate::kernel::services::ports::{
    LspPositionEncoding, LspResourceOp, LspTextEdit, LspWorkspaceFileEdit,
};
use crate::kernel::TerminalId;
use crate::models::should_ignore;
#[cfg(feature = "terminal")]
use portable_pty::{CommandBuilder, PtySize};
use ropey::Rope;
#[cfg(feature = "terminal")]
use std::collections::HashMap;
#[cfg(feature = "terminal")]
use std::io::Read;
use std::io::{self, Write};
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
#[cfg(feature = "terminal")]
use std::sync::{Arc, Mutex};

pub struct AsyncRuntime {
    runtime: tokio::runtime::Runtime,
    tx: Sender<AppMessage>,
    #[cfg(feature = "terminal")]
    terminals: Arc<Mutex<HashMap<TerminalId, TerminalHandle>>>,
}

#[cfg(feature = "terminal")]
struct TerminalHandle {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn std::io::Write + Send>,
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
}

impl AsyncRuntime {
    pub fn new(tx: Sender<AppMessage>) -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .or_else(|e| {
                tracing::error!(
                    error = %e,
                    "Failed to create multi-thread tokio runtime, falling back to current-thread"
                );
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
            })?;
        Ok(Self {
            runtime,
            tx,
            #[cfg(feature = "terminal")]
            terminals: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn tokio_handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    pub fn git_detect_repo(&self, workspace_root: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let output = match git_output(workspace_root.as_path(), |cmd| {
                cmd.arg("rev-parse").arg("--show-toplevel");
            })
            .await
            {
                Ok(out) => out,
                Err(e) => {
                    tracing::debug!(error = %e, "git rev-parse failed");
                    let _ = tx.send(AppMessage::GitRepoCleared);
                    return;
                }
            };

            if !output.status.success() {
                let _ = tx.send(AppMessage::GitRepoCleared);
                return;
            }

            let repo_root_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if repo_root_str.is_empty() {
                let _ = tx.send(AppMessage::GitRepoCleared);
                return;
            }

            let repo_root = PathBuf::from(repo_root_str);

            let branch_output = git_output(repo_root.as_path(), |cmd| {
                cmd.arg("symbolic-ref").arg("--short").arg("HEAD");
            })
            .await;

            let (branch, detached) = match branch_output {
                Ok(out) if out.status.success() => {
                    let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if b.is_empty() {
                        (None, true)
                    } else {
                        (Some(b), false)
                    }
                }
                _ => (None, true),
            };

            let commit = git_output(repo_root.as_path(), |cmd| {
                cmd.arg("rev-parse").arg("--short").arg("HEAD");
            })
            .await
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

            let worktrees_output = git_output(repo_root.as_path(), |cmd| {
                cmd.arg("worktree").arg("list").arg("--porcelain");
            })
            .await;

            let mut worktrees = worktrees_output
                .ok()
                .filter(|o| o.status.success())
                .map(|o| git_helpers::parse_worktree_list(&String::from_utf8_lossy(&o.stdout)))
                .unwrap_or_default();

            for item in &mut worktrees {
                if item.head.short_commit.len() > 12 {
                    item.head.short_commit = item.head.short_commit[..12].to_string();
                }
            }

            let head = crate::kernel::GitHead {
                branch,
                short_commit: commit,
                detached,
            };

            let _ = tx.send(AppMessage::GitRepoDetected {
                repo_root,
                head,
                worktrees,
            });
        });
    }

    pub fn git_refresh_status(&self, repo_root: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let output = match git_output(repo_root.as_path(), |cmd| {
                cmd.arg("status")
                    .arg("--porcelain")
                    .arg("-z")
                    .arg("--untracked-files=all");
            })
            .await
            {
                Ok(out) => out,
                Err(e) => {
                    tracing::debug!(error = %e, "git status failed");
                    return;
                }
            };

            if !output.status.success() {
                let _ = tx.send(AppMessage::GitRepoCleared);
                return;
            }

            let statuses =
                git_helpers::parse_status_porcelain_z(&output.stdout, repo_root.as_path());
            let _ = tx.send(AppMessage::GitStatusUpdated { statuses });
        });
    }

    pub fn git_refresh_diff(&self, repo_root: PathBuf, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let rel = match path.strip_prefix(repo_root.as_path()) {
                Ok(p) => p.to_path_buf(),
                Err(_) => return,
            };

            let output = match git_output(repo_root.as_path(), |cmd| {
                cmd.arg("diff")
                    .arg("--no-color")
                    .arg("-U0")
                    .arg("HEAD")
                    .arg("--")
                    .arg(rel);
            })
            .await
            {
                Ok(out) => out,
                Err(e) => {
                    tracing::debug!(error = %e, "git diff failed");
                    return;
                }
            };

            if !output.status.success() {
                return;
            }

            let text = String::from_utf8_lossy(&output.stdout);
            let marks = git_helpers::parse_diff_hunks_to_gutter_marks(&text);
            let _ = tx.send(AppMessage::GitDiffUpdated { path, marks });
        });
    }

    pub fn git_list_worktrees(&self, repo_root: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let output = match git_output(repo_root.as_path(), |cmd| {
                cmd.arg("worktree").arg("list").arg("--porcelain");
            })
            .await
            {
                Ok(out) => out,
                Err(e) => {
                    tracing::debug!(error = %e, "git worktree list failed");
                    return;
                }
            };

            if !output.status.success() {
                return;
            }

            let worktrees =
                git_helpers::parse_worktree_list(&String::from_utf8_lossy(&output.stdout));
            let _ = tx.send(AppMessage::GitWorktreesUpdated { worktrees });
        });
    }

    pub fn git_list_branches(&self, repo_root: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let output = match git_output(repo_root.as_path(), |cmd| {
                cmd.arg("for-each-ref")
                    .arg("--format=%(refname:short)")
                    .arg("refs/heads");
            })
            .await
            {
                Ok(out) => out,
                Err(e) => {
                    tracing::debug!(error = %e, "git for-each-ref failed");
                    return;
                }
            };

            if !output.status.success() {
                return;
            }

            let text = String::from_utf8_lossy(&output.stdout);
            let mut branches = git_helpers::parse_branch_list(&text);
            branches.sort();
            branches.dedup();
            let _ = tx.send(AppMessage::GitBranchesUpdated { branches });
        });
    }

    pub fn git_worktree_add(&self, repo_root: PathBuf, branch: String) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let branch = branch.trim().strip_prefix("refs/heads/").unwrap_or(&branch);
            let branch = branch.trim();
            if branch.is_empty() {
                return;
            }

            if let Ok(out) = git_output(repo_root.as_path(), |cmd| {
                cmd.arg("worktree").arg("list").arg("--porcelain");
            })
            .await
            {
                if out.status.success() {
                    let worktrees =
                        git_helpers::parse_worktree_list(&String::from_utf8_lossy(&out.stdout));
                    if let Some(item) = worktrees
                        .into_iter()
                        .find(|w| w.head.branch.as_deref() == Some(branch))
                    {
                        let _ = tx.send(AppMessage::GitWorktreeResolved { path: item.path });
                        return;
                    }
                }
            }

            let path = repo_root.join(".worktrees").join(branch);
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }

            let try_existing = git_output(repo_root.as_path(), |cmd| {
                cmd.arg("worktree").arg("add").arg(&path).arg(branch);
            })
            .await;

            let ok = matches!(try_existing.as_ref(), Ok(out) if out.status.success());

            let result = if ok {
                try_existing
            } else {
                git_output(repo_root.as_path(), |cmd| {
                    cmd.arg("worktree")
                        .arg("add")
                        .arg("-b")
                        .arg(branch)
                        .arg(&path);
                })
                .await
            };

            if matches!(result.as_ref(), Ok(out) if out.status.success()) {
                let _ = tx.send(AppMessage::GitWorktreeResolved { path });
            }
        });
    }

    pub fn git_worktree_resolve(&self, repo_root: PathBuf, branch: String) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let output = match git_output(repo_root.as_path(), |cmd| {
                cmd.arg("worktree").arg("list").arg("--porcelain");
            })
            .await
            {
                Ok(out) => out,
                Err(_) => return,
            };

            if !output.status.success() {
                return;
            }

            let worktrees =
                git_helpers::parse_worktree_list(&String::from_utf8_lossy(&output.stdout));
            let wanted = branch.trim();
            let found = worktrees.into_iter().find(|w| {
                w.head.branch.as_deref() == Some(wanted)
                    || w.head
                        .branch
                        .as_deref()
                        .is_some_and(|b| format!("refs/heads/{b}") == wanted)
            });

            if let Some(item) = found {
                let _ = tx.send(AppMessage::GitWorktreeResolved { path: item.path });
            }
        });
    }

    pub fn load_dir(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::read_dir(&path).await {
                Ok(mut entries) => {
                    let mut result = Vec::new();
                    loop {
                        let entry = match entries.next_entry().await {
                            Ok(Some(entry)) => entry,
                            Ok(None) => break,
                            Err(e) => {
                                let _ = tx.send(AppMessage::DirLoadError {
                                    path,
                                    error: e.to_string(),
                                });
                                return;
                            }
                        };

                        let name = entry.file_name().to_string_lossy().to_string();
                        if should_ignore(&name) {
                            continue;
                        }

                        if let Ok(file_type) = entry.file_type().await {
                            result.push(DirEntryInfo {
                                name,
                                is_dir: file_type.is_dir(),
                            });
                        }
                    }
                    let _ = tx.send(AppMessage::DirLoaded {
                        path,
                        entries: result,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::DirLoadError {
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn load_file(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => {
                    let _ = tx.send(AppMessage::FileLoaded { path, content });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FileError {
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn reload_file(&self, request: ReloadRequest) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            if let Ok(content) = tokio::fs::read_to_string(&request.path).await {
                let _ = tx.send(AppMessage::FileReloaded { request, content });
            }
        });
    }

    pub fn write_file(&self, pane: usize, path: PathBuf, version: u64, rope: Rope) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let tx_for_error = tx.clone();
            let path_for_error = path.clone();
            let path_for_write = path.clone();
            let result =
                tokio::task::spawn_blocking(move || write_rope_to_path(&path_for_write, &rope))
                    .await;

            let success = match result {
                Ok(Ok(())) => true,
                Ok(Err(e)) => {
                    let _ = tx_for_error.send(AppMessage::FsOpError {
                        op: "write_file",
                        path: path_for_error,
                        to: None,
                        error: e.to_string(),
                    });
                    false
                }
                Err(e) => {
                    let _ = tx_for_error.send(AppMessage::FsOpError {
                        op: "write_file",
                        path: path_for_error,
                        to: None,
                        error: e.to_string(),
                    });
                    false
                }
            };

            let _ = tx.send(AppMessage::FileSaved {
                pane,
                path,
                success,
                version,
            });
        });
    }

    pub fn create_file(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .await
            {
                Ok(_) => {
                    let _ = tx.send(AppMessage::PathCreated {
                        path,
                        is_dir: false,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FsOpError {
                        op: "create_file",
                        path,
                        to: None,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn create_dir(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::create_dir(&path).await {
                Ok(_) => {
                    let _ = tx.send(AppMessage::PathCreated { path, is_dir: true });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FsOpError {
                        op: "create_dir",
                        path,
                        to: None,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn rename_path(&self, from: PathBuf, to: PathBuf, overwrite: bool) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let tx_for_error = tx.clone();
            let from_for_error = from.clone();
            let to_for_error = to.clone();
            let from_for_work = from.clone();
            let to_for_work = to.clone();

            let result = tokio::task::spawn_blocking(move || {
                move_path(from_for_work.as_path(), to_for_work.as_path(), overwrite)
            })
            .await;

            match result {
                Ok(Ok(())) => {
                    let _ = tx.send(AppMessage::PathRenamed { from, to });
                }
                Ok(Err(e)) => {
                    let _ = tx_for_error.send(AppMessage::FsOpError {
                        op: "rename_path",
                        path: from_for_error,
                        to: Some(to_for_error),
                        error: e.to_string(),
                    });
                }
                Err(e) => {
                    let _ = tx_for_error.send(AppMessage::FsOpError {
                        op: "rename_path",
                        path: from_for_error,
                        to: Some(to_for_error),
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn delete_path(&self, path: PathBuf, is_dir: bool) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let result = if is_dir {
                tokio::fs::remove_dir_all(&path).await
            } else {
                tokio::fs::remove_file(&path).await
            };
            match result {
                Ok(_) => {
                    let _ = tx.send(AppMessage::PathDeleted { path });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FsOpError {
                        op: "delete_path",
                        path,
                        to: None,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn apply_file_edits(
        &self,
        position_encoding: LspPositionEncoding,
        resource_ops: Vec<LspResourceOp>,
        edits: Vec<LspWorkspaceFileEdit>,
    ) {
        if resource_ops.is_empty() && edits.is_empty() {
            return;
        }

        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            for op in resource_ops {
                let tx_for_error = tx.clone();
                match op {
                    LspResourceOp::CreateFile {
                        path,
                        overwrite,
                        ignore_if_exists,
                    } => {
                        let path_for_work = path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            apply_create_file(path_for_work.as_path(), overwrite, ignore_if_exists)
                        })
                        .await;

                        match result {
                            Ok(Ok(())) => {
                                let _ = tx.send(AppMessage::PathCreated {
                                    path,
                                    is_dir: false,
                                });
                            }
                            Ok(Err(e)) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_create_file",
                                    path,
                                    to: None,
                                    error: e,
                                });
                            }
                            Err(e) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_create_file",
                                    path,
                                    to: None,
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                    LspResourceOp::RenameFile {
                        old_path,
                        new_path,
                        overwrite,
                        ignore_if_exists,
                    } => {
                        let from = old_path.clone();
                        let to = new_path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            apply_rename_file(
                                from.as_path(),
                                to.as_path(),
                                overwrite,
                                ignore_if_exists,
                            )
                        })
                        .await;

                        match result {
                            Ok(Ok(())) => {
                                let _ = tx.send(AppMessage::PathRenamed {
                                    from: old_path,
                                    to: new_path,
                                });
                            }
                            Ok(Err(e)) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_rename_file",
                                    path: old_path,
                                    to: Some(new_path),
                                    error: e,
                                });
                            }
                            Err(e) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_rename_file",
                                    path: old_path,
                                    to: Some(new_path),
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                    LspResourceOp::DeleteFile {
                        path,
                        recursive,
                        ignore_if_not_exists,
                    } => {
                        let path_for_work = path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            apply_delete_file(
                                path_for_work.as_path(),
                                recursive,
                                ignore_if_not_exists,
                            )
                        })
                        .await;

                        match result {
                            Ok(Ok(())) => {
                                let _ = tx.send(AppMessage::PathDeleted { path });
                            }
                            Ok(Err(e)) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_delete_file",
                                    path,
                                    to: None,
                                    error: e,
                                });
                            }
                            Err(e) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_delete_file",
                                    path,
                                    to: None,
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                }
            }

            for file_edit in edits {
                if file_edit.edits.is_empty() {
                    continue;
                }

                let tx_for_error = tx.clone();
                let path = file_edit.path;
                let edits = file_edit.edits;
                let path_for_work = path.clone();

                let result = tokio::task::spawn_blocking(move || {
                    apply_text_edits_to_path(path_for_work.as_path(), &edits, position_encoding)
                })
                .await;

                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        let _ = tx_for_error.send(AppMessage::FsOpError {
                            op: "apply_file_edits",
                            path,
                            to: None,
                            error: e,
                        });
                    }
                    Err(e) => {
                        let _ = tx_for_error.send(AppMessage::FsOpError {
                            op: "apply_file_edits",
                            path,
                            to: None,
                            error: e.to_string(),
                        });
                    }
                }
            }
        });
    }

    #[cfg(feature = "terminal")]
    pub fn terminal_spawn(
        &self,
        id: TerminalId,
        cwd: PathBuf,
        shell: Option<String>,
        args: Vec<String>,
        cols: u16,
        rows: u16,
    ) {
        let tx = self.tx.clone();
        let terminals = self.terminals.clone();
        self.runtime.spawn(async move {
            let tx_for_spawn = tx.clone();
            let result = tokio::task::spawn_blocking(move || {
                spawn_terminal_session(id, cwd, shell, args, cols, rows, tx_for_spawn)
            })
            .await;

            match result {
                Ok(Ok((handle, title))) => {
                    let mut guard = terminals.lock().unwrap();
                    guard.insert(id, handle);
                    let _ = tx.send(AppMessage::TerminalSpawned { id, title });
                }
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "terminal spawn failed");
                    let _ = tx.send(AppMessage::TerminalExited { id, code: None });
                }
                Err(e) => {
                    tracing::error!(error = %e, "terminal spawn join failed");
                    let _ = tx.send(AppMessage::TerminalExited { id, code: None });
                }
            }
        });
    }

    #[cfg(not(feature = "terminal"))]
    pub fn terminal_spawn(
        &self,
        _id: TerminalId,
        _cwd: PathBuf,
        _shell: Option<String>,
        _args: Vec<String>,
        _cols: u16,
        _rows: u16,
    ) {
    }

    #[cfg(feature = "terminal")]
    pub fn terminal_write(&self, id: TerminalId, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }

        let mut guard = self.terminals.lock().unwrap();
        let Some(handle) = guard.get_mut(&id) else {
            return;
        };

        let _ = handle.writer.write_all(&bytes);
        let _ = handle.writer.flush();
    }

    #[cfg(not(feature = "terminal"))]
    pub fn terminal_write(&self, _id: TerminalId, _bytes: Vec<u8>) {}

    #[cfg(feature = "terminal")]
    pub fn terminal_resize(&self, id: TerminalId, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }

        let guard = self.terminals.lock().unwrap();
        let Some(handle) = guard.get(&id) else {
            return;
        };

        let _ = handle.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    #[cfg(not(feature = "terminal"))]
    pub fn terminal_resize(&self, _id: TerminalId, _cols: u16, _rows: u16) {}

    #[cfg(feature = "terminal")]
    pub fn terminal_kill(&self, id: TerminalId) {
        let mut guard = self.terminals.lock().unwrap();
        if let Some(mut handle) = guard.remove(&id) {
            let _ = handle.killer.kill();
        }
    }

    #[cfg(not(feature = "terminal"))]
    pub fn terminal_kill(&self, _id: TerminalId) {}
}

#[cfg(feature = "terminal")]
fn spawn_terminal_session(
    id: TerminalId,
    cwd: PathBuf,
    shell: Option<String>,
    args: Vec<String>,
    cols: u16,
    rows: u16,
    tx: Sender<AppMessage>,
) -> Result<(TerminalHandle, String), String> {
    let shell = shell.unwrap_or_else(default_shell);
    let title = terminal_title(&shell);
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    let mut cmd = CommandBuilder::new(&shell);
    if !args.is_empty() {
        cmd.args(args);
    }
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");

    let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    let killer = child.clone_killer();

    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let tx_output = tx.clone();
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = tx_output.send(AppMessage::TerminalOutput {
                        id,
                        bytes: buf[..n].to_vec(),
                    });
                }
                Err(_) => break,
            }
        }
    });

    let tx_exit = tx.clone();
    std::thread::spawn(move || {
        let code = child.wait().ok().map(|status| status.exit_code() as i32);
        let _ = tx_exit.send(AppMessage::TerminalExited { id, code });
    });

    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    Ok((
        TerminalHandle {
            master: pair.master,
            writer,
            killer,
        },
        title,
    ))
}

#[cfg(feature = "terminal")]
fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    }

    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

#[cfg(feature = "terminal")]
fn terminal_title(shell: &str) -> String {
    std::path::Path::new(shell)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| shell.to_string())
}

async fn git_output(
    cwd: &Path,
    configure: impl FnOnce(&mut tokio::process::Command),
) -> io::Result<std::process::Output> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("-C").arg(cwd);
    cmd.env("GIT_OPTIONAL_LOCKS", "0");
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    configure(&mut cmd);
    cmd.output().await
}

fn move_path(from: &std::path::Path, to: &std::path::Path, overwrite: bool) -> io::Result<()> {
    move_path_impl(from, to, overwrite, |from, to| std::fs::rename(from, to))
}

fn move_path_impl(
    from: &std::path::Path,
    to: &std::path::Path,
    overwrite: bool,
    rename_fn: impl FnOnce(&std::path::Path, &std::path::Path) -> io::Result<()>,
) -> io::Result<()> {
    if from == to {
        return Ok(());
    }

    let from_meta = std::fs::symlink_metadata(from)?;
    if from_meta.is_dir() && to.starts_with(from) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot move a directory into itself",
        ));
    }

    let Some(parent) = to.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination has no parent directory",
        ));
    };
    if !std::fs::metadata(parent).is_ok_and(|m| m.is_dir()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination parent is not a directory",
        ));
    }

    if !overwrite && std::fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination exists",
        ));
    }

    if overwrite {
        remove_existing_path(to)?;
    }

    match rename_fn(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => move_across_devices(from, to),
        Err(e) => Err(e),
    }
}

fn remove_existing_path(path: &std::path::Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn move_across_devices(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    let from_meta = std::fs::symlink_metadata(from)?;
    if from_meta.file_type().is_symlink() {
        copy_symlink(from, to)?;
        std::fs::remove_file(from)?;
        return Ok(());
    }

    if from_meta.is_dir() {
        copy_dir_recursive(from, to)?;
        std::fs::remove_dir_all(from)?;
        return Ok(());
    }

    std::fs::copy(from, to)?;
    std::fs::remove_file(from)?;
    Ok(())
}

fn copy_dir_recursive(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    std::fs::create_dir(to)?;

    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let from_child = entry.path();
        let to_child = to.join(entry.file_name());
        let meta = std::fs::symlink_metadata(&from_child)?;

        if meta.file_type().is_symlink() {
            copy_symlink(&from_child, &to_child)?;
        } else if meta.is_dir() {
            copy_dir_recursive(&from_child, &to_child)?;
        } else {
            std::fs::copy(&from_child, &to_child)?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn copy_symlink(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    let target = std::fs::read_link(from)?;
    std::os::unix::fs::symlink(target, to)
}

#[cfg(windows)]
fn copy_symlink(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    let target = std::fs::read_link(from)?;
    match std::fs::metadata(from) {
        Ok(meta) if meta.is_dir() => std::os::windows::fs::symlink_dir(target, to),
        _ => std::os::windows::fs::symlink_file(target, to),
    }
}

#[cfg(not(any(unix, windows)))]
fn copy_symlink(_from: &std::path::Path, _to: &std::path::Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symlinks are not supported on this platform",
    ))
}

fn apply_create_file(
    path: &std::path::Path,
    overwrite: bool,
    ignore_if_exists: bool,
) -> Result<(), String> {
    match std::fs::metadata(path) {
        Ok(meta) => {
            if ignore_if_exists {
                return Ok(());
            }
            if meta.is_dir() {
                return Err("path exists and is a directory".to_string());
            }
            if overwrite {
                std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(path)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            } else {
                Err("path exists".to_string())
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn apply_rename_file(
    old_path: &std::path::Path,
    new_path: &std::path::Path,
    overwrite: bool,
    ignore_if_exists: bool,
) -> Result<(), String> {
    if ignore_if_exists && std::fs::metadata(new_path).is_ok() {
        return Ok(());
    }

    if overwrite {
        if let Ok(meta) = std::fs::metadata(new_path) {
            if meta.is_dir() {
                std::fs::remove_dir_all(new_path).map_err(|e| e.to_string())?;
            } else {
                std::fs::remove_file(new_path).map_err(|e| e.to_string())?;
            }
        }
    }

    std::fs::rename(old_path, new_path).map_err(|e| e.to_string())
}

fn apply_delete_file(
    path: &std::path::Path,
    recursive: bool,
    ignore_if_not_exists: bool,
) -> Result<(), String> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if ignore_if_not_exists {
                return Ok(());
            }
            return Err(e.to_string());
        }
        Err(e) => return Err(e.to_string()),
    };

    if meta.is_dir() {
        if recursive {
            std::fs::remove_dir_all(path).map_err(|e| e.to_string())
        } else {
            std::fs::remove_dir(path).map_err(|e| e.to_string())
        }
    } else {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

fn write_rope_to_path(path: &std::path::Path, rope: &Rope) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    for chunk in rope.chunks() {
        writer.write_all(chunk.as_bytes())?;
    }
    writer.flush()
}

fn apply_text_edits_to_path(
    path: &std::path::Path,
    edits: &[LspTextEdit],
    encoding: LspPositionEncoding,
) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut rope = Rope::from_str(&content);
    apply_text_edits_to_rope(&mut rope, edits, encoding);
    write_rope_to_path(path, &rope).map_err(|e| e.to_string())
}

fn apply_text_edits_to_rope(rope: &mut Rope, edits: &[LspTextEdit], encoding: LspPositionEncoding) {
    let mut ordered: Vec<&LspTextEdit> = edits.iter().collect();
    ordered.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then_with(|| b.range.start.character.cmp(&a.range.start.character))
            .then_with(|| b.range.end.line.cmp(&a.range.end.line))
            .then_with(|| b.range.end.character.cmp(&a.range.end.character))
    });

    for edit in ordered {
        let start_byte = lsp_position_to_byte_offset(
            rope,
            edit.range.start.line,
            edit.range.start.character,
            encoding,
        );
        let end_byte = lsp_position_to_byte_offset(
            rope,
            edit.range.end.line,
            edit.range.end.character,
            encoding,
        );

        if start_byte == end_byte && edit.new_text.is_empty() {
            continue;
        }

        let len_bytes = rope.len_bytes();
        let start_byte = start_byte.min(len_bytes);
        let end_byte = end_byte.min(len_bytes);

        let mut start_char = rope.byte_to_char(start_byte);
        let mut end_char = rope.byte_to_char(end_byte);
        if start_char > end_char {
            std::mem::swap(&mut start_char, &mut end_char);
        }

        rope.remove(start_char..end_char);
        if !edit.new_text.is_empty() {
            rope.insert(start_char, &edit.new_text);
        }
    }
}

fn lsp_position_to_byte_offset(
    rope: &Rope,
    line: u32,
    column: u32,
    encoding: LspPositionEncoding,
) -> usize {
    if rope.len_chars() == 0 {
        return 0;
    }

    let line_index = (line as usize).min(rope.len_lines().saturating_sub(1));
    let line_slice = rope.line(line_index);
    let col_chars = lsp_col_to_char_offset_in_line(line_slice, column, encoding);
    let line_start = rope.line_to_char(line_index);
    let line_len = line_len_chars(line_slice);
    let char_offset = (line_start + col_chars.min(line_len)).min(rope.len_chars());
    rope.char_to_byte(char_offset)
}

fn lsp_col_to_char_offset_in_line(
    line: ropey::RopeSlice<'_>,
    col: u32,
    encoding: LspPositionEncoding,
) -> usize {
    let mut units = 0u32;
    let mut chars = 0usize;
    let mut it = line.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\n' {
            break;
        }
        if ch == '\r' && matches!(it.peek(), Some('\n')) {
            break;
        }
        let next = units
            + match encoding {
                LspPositionEncoding::Utf8 => ch.len_utf8() as u32,
                LspPositionEncoding::Utf16 => ch.len_utf16() as u32,
                LspPositionEncoding::Utf32 => 1,
            };
        if next > col {
            break;
        }
        units = next;
        chars += 1;
    }
    chars
}

fn line_len_chars(line: ropey::RopeSlice<'_>) -> usize {
    let mut len = 0usize;
    let mut it = line.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\n' {
            break;
        }
        if ch == '\r' && matches!(it.peek(), Some('\n')) {
            break;
        }
        len += 1;
    }
    len
}

#[cfg(test)]
#[path = "../../../../../tests/unit/kernel/services/adapters/runtime/async_runtime.rs"]
mod tests;
