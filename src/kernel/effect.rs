use ropey::Rope;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Effect {
    LoadFile(PathBuf),
    LoadDir(PathBuf),
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
    PluginCommandInvoked {
        plugin_id: String,
        command_id: String,
    },
}
