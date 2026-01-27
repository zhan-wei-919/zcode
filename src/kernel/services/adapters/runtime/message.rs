use crate::kernel::services::ports::DirEntryInfo;
use crate::kernel::{GitFileStatusKind, GitGutterMarks, GitHead, GitWorktreeItem};
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
        error: String,
    },
    GitRepoDetected {
        repo_root: PathBuf,
        head: GitHead,
        worktrees: Vec<GitWorktreeItem>,
    },
    GitRepoCleared,
    GitStatusUpdated {
        statuses: Vec<(PathBuf, GitFileStatusKind)>,
    },
    GitDiffUpdated {
        path: PathBuf,
        marks: GitGutterMarks,
    },
    GitWorktreesUpdated {
        worktrees: Vec<GitWorktreeItem>,
    },
    GitWorktreeResolved {
        path: PathBuf,
    },
}
