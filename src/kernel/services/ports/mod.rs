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
    LspClientKey, LspCodeAction, LspCommand, LspCompletionItem, LspCompletionTriggerContext,
    LspCompletionTriggerKind, LspFoldingRange, LspHoverBlock, LspHoverPayload,
    LspHoverPreviewPayload, LspInlayHint, LspInsertTextFormat, LspMarkup, LspPosition,
    LspPositionEncoding, LspRange, LspResourceOp, LspSemanticToken, LspSemanticTokensLegend,
    LspServerCapabilities, LspServerKind, LspSignatureHelpPayload, LspSignatureInfo,
    LspSignatureParameter, LspSignatureParameterLabel, LspTextChange, LspTextEdit,
    LspWorkspaceEdit, LspWorkspaceFileEdit,
};
pub use runtime::{AsyncExecutor, BoxFuture};
pub use search::{
    FileMatches, GlobalSearchMessage, Match, Result as SearchResult, SearchError, SearchMessage,
};
pub use settings::{KeybindingRule, Settings, ThemeSettings};
