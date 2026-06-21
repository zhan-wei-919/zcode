//! Service ports: traits + data contracts.

pub mod config;
pub mod dir_entry;
pub mod lsp;
pub mod search;
pub mod settings;

pub use config::EditorConfig;
pub use dir_entry::DirEntryInfo;
pub use lsp::{
    LspClientKey, LspCodeAction, LspCommand, LspCompletionItem, LspCompletionTriggerContext,
    LspCompletionTriggerKind, LspFoldingRange, LspHoverBlock, LspHoverPayload,
    LspHoverPreviewPayload, LspInlayHint, LspInsertTextFormat, LspMarkup, LspPosition,
    LspPositionEncoding, LspRange, LspResourceOp, LspServerCapabilities, LspServerKind,
    LspSignatureHelpPayload, LspSignatureInfo, LspSignatureParameter, LspSignatureParameterLabel,
    LspTextChange, LspTextEdit, LspWorkspaceEdit, LspWorkspaceFileEdit,
};
pub use search::{
    FileMatches, GlobalSearchMessage, Match, Result as SearchResult, SearchError, SearchMessage,
};
pub use settings::{KeybindingRule, Settings};
