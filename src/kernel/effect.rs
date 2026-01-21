use ropey::Rope;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Effect {
    LoadFile(PathBuf),
    LoadDir(PathBuf),
    CreateFile(PathBuf),
    CreateDir(PathBuf),
    DeletePath { path: PathBuf, is_dir: bool },
    ReloadSettings,
    OpenSettings,
    StartGlobalSearch {
        root: PathBuf,
        pattern: String,
        case_sensitive: bool,
        use_regex: bool,
    },
    StartEditorSearch {
        pane: usize,
        rope: Rope,
        pattern: String,
        case_sensitive: bool,
        use_regex: bool,
    },
    CancelEditorSearch {
        pane: usize,
    },
    WriteFile {
        pane: usize,
        path: PathBuf,
    },
    SetClipboardText(String),
    RequestClipboardText {
        pane: usize,
    },
    LspHoverRequest {
        path: PathBuf,
        line: u32,
        column: u32,
    },
    LspDefinitionRequest {
        path: PathBuf,
        line: u32,
        column: u32,
    },
}
