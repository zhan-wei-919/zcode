//! Service adapters: OS/runtime specific implementations (IO/async).

pub mod backup;
pub mod clipboard;
pub mod config;
pub mod file;
pub mod keybinding;
pub mod lsp;
pub mod perf;
pub mod runtime;
pub mod search;
pub mod settings;

pub use backup::{
    ensure_backup_dir, ensure_log_dir, get_backup_dir, get_log_dir, get_ops_file_path,
};
pub use clipboard::{ClipboardError, ClipboardService};
pub use config::ConfigService;
pub use file::{FileService, LocalFileProvider};
pub use keybinding::{KeybindingContext, KeybindingService};
pub use lsp::{LspPosition, LspRange, LspService, LspTextChange};
pub use runtime::{AppMessage, AsyncRuntime};
pub use search::{
    search_regex_in_slice, GlobalSearchService, GlobalSearchTask, RopeReader, SearchConfig,
    SearchService, SearchTask, StreamSearcher,
};
pub use settings::{ensure_settings_file, get_settings_path, load_settings, parse_keybinding};
