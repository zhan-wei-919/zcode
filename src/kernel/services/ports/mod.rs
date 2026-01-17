//! Service ports: traits + data contracts.

pub mod config;
pub mod file;
pub mod runtime;
pub mod search;
pub mod settings;

pub use config::EditorConfig;
pub use file::{
    DirEntry, DirEntryInfo, FileError, FileMetadata, FileProvider, Result as FileResult,
};
pub use runtime::{AsyncExecutor, BoxFuture};
pub use search::{FileMatches, GlobalSearchMessage, Match, SearchMessage};
pub use settings::{KeybindingRule, Settings, ThemeSettings};
