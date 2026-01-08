//! Service ports: traits + data contracts.

pub mod config;
pub mod file;
pub mod search;
pub mod settings;

pub use config::EditorConfig;
pub use file::{
    DirEntry, DirEntryInfo, FileError, FileMetadata, FileProvider, Result as FileResult,
};
pub use search::{FileMatches, GlobalSearchMessage, Match, SearchMessage};
pub use settings::{KeybindingRule, Settings, ThemeSettings};
