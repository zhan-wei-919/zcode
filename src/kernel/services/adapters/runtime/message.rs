use crate::kernel::editor::ReloadRequest;
use crate::kernel::editor::{SyntaxHighlightPatch, TabId};
use crate::kernel::services::ports::DirEntryInfo;
use crate::models::OpId;
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
        // version：单调计数器，用于回调去重/排序；head：落盘内容对应的 HEAD。
        version: u64,
        head: OpId,
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
    FileReloaded {
        request: ReloadRequest,
        content: String,
    },
    SyntaxHighlightsComputed {
        tab_id: TabId,
        version: u64,
        patches: Vec<SyntaxHighlightPatch>,
    },
}
