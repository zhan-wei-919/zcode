use crate::kernel::editor::ReloadRequest;
use crate::kernel::services::ports::DirEntryInfo;
use crate::kernel::{GitFileStatus, GitGutterMarks, GitHead, GitWorktreeItem, TerminalId};
use std::path::PathBuf;

pub enum AppMessage {
    DirLoaded {
        path: PathBuf,
        entries: Vec<DirEntryInfo>,
    },
    DirLoadError {
        path: PathBuf,
        error: String,
    },
    FileLoaded {
        path: PathBuf,
        content: String,
    },
    FileError {
        path: PathBuf,
        error: String,
    },
    FileSaved {
        pane: usize,
        path: PathBuf,
        success: bool,
        version: u64,
    },
    PathCreated {
        path: PathBuf,
        is_dir: bool,
    },
    PathDeleted {
        path: PathBuf,
    },
    PathRenamed {
        from: PathBuf,
        to: PathBuf,
    },
    FsOpError {
        op: &'static str,
        path: PathBuf,
        to: Option<PathBuf>,
        error: String,
    },
    GitRepoDetected {
        repo_root: PathBuf,
        head: GitHead,
        worktrees: Vec<GitWorktreeItem>,
    },
    GitRepoCleared,
    GitStatusUpdated {
        statuses: Vec<(PathBuf, GitFileStatus)>,
    },
    GitDiffUpdated {
        path: PathBuf,
        marks: GitGutterMarks,
    },
    GitWorktreesUpdated {
        worktrees: Vec<GitWorktreeItem>,
    },
    GitBranchesUpdated {
        branches: Vec<String>,
    },
    GitWorktreeResolved {
        path: PathBuf,
    },
    TerminalSpawned {
        id: TerminalId,
        title: String,
    },
    TerminalOutput {
        id: TerminalId,
        bytes: Vec<u8>,
    },
    TerminalExited {
        id: TerminalId,
        code: Option<i32>,
    },
    FileReloaded {
        request: ReloadRequest,
        content: String,
    },
}
