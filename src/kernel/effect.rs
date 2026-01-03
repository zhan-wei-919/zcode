use std::path::PathBuf;
use ropey::Rope;

#[derive(Debug, Clone)]
pub enum Effect {
    LoadFile(PathBuf),
    LoadDir(PathBuf),
    ReloadSettings,
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
}
