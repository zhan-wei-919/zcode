//! Service ports: traits + data contracts.

pub mod config;
pub mod file;
pub mod lsp;
pub mod runtime;
pub mod search;
pub mod settings;

pub use config::EditorConfig;
pub use file::{
    DirEntry, DirEntryInfo, FileError, FileMetadata, FileProvider, Result as FileResult,
};
pub use lsp::{
    LspCodeAction, LspCommand, LspCompletionItem, LspFoldingRange, LspInlayHint,
    LspInsertTextFormat, LspPosition, LspPositionEncoding, LspRange, LspResourceOp,
    LspSemanticToken, LspSemanticTokensLegend, LspServerCapabilities, LspTextChange, LspTextEdit,
    LspWorkspaceEdit, LspWorkspaceFileEdit,
};
pub use runtime::{AsyncExecutor, BoxFuture};
pub use search::{
    FileMatches, GlobalSearchMessage, Match, Result as SearchResult, SearchError, SearchMessage,
};
pub use settings::{KeybindingRule, Settings, ThemeSettings};
