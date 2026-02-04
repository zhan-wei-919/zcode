//! LSP data contracts used across kernel + adapters.

use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LspPositionEncoding {
    Utf8,
    #[default]
    Utf16,
    Utf32,
}

#[derive(Debug, Clone, Copy)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

#[derive(Debug, Clone)]
pub struct LspTextChange {
    pub range: Option<LspRange>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct LspTextEdit {
    pub range: LspRange,
    pub new_text: String,
}

#[derive(Debug, Clone)]
pub struct LspWorkspaceFileEdit {
    pub path: PathBuf,
    pub edits: Vec<LspTextEdit>,
}

#[derive(Debug, Clone)]
pub enum LspResourceOp {
    CreateFile {
        path: PathBuf,
        overwrite: bool,
        ignore_if_exists: bool,
    },
    RenameFile {
        old_path: PathBuf,
        new_path: PathBuf,
        overwrite: bool,
        ignore_if_exists: bool,
    },
    DeleteFile {
        path: PathBuf,
        recursive: bool,
        ignore_if_not_exists: bool,
    },
}

#[derive(Debug, Clone, Default)]
pub struct LspWorkspaceEdit {
    pub changes: Vec<LspWorkspaceFileEdit>,
    pub resource_ops: Vec<LspResourceOp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LspSemanticTokensLegend {
    pub token_types: Vec<String>,
    pub token_modifiers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LspSemanticToken {
    pub line: u32,
    pub start: u32,
    pub length: u32,
    pub token_type: u32,
    pub modifiers: u32,
}

#[derive(Debug, Clone)]
pub struct LspInlayHint {
    pub position: LspPosition,
    pub label: String,
    pub padding_left: bool,
    pub padding_right: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LspFoldingRange {
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LspInsertTextFormat {
    #[default]
    PlainText,
    Snippet,
}

#[derive(Debug, Clone)]
pub struct LspCompletionItem {
    pub id: u64,
    pub label: String,
    pub detail: Option<String>,
    pub kind: Option<u32>,
    pub documentation: Option<String>,
    pub insert_text: String,
    pub insert_text_format: LspInsertTextFormat,
    pub insert_range: Option<LspRange>,
    pub replace_range: Option<LspRange>,
    pub sort_text: Option<String>,
    pub filter_text: Option<String>,
    pub additional_text_edits: Vec<LspTextEdit>,
    pub command: Option<LspCommand>,
    pub data: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct LspCommand {
    pub command: String,
    pub arguments: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct LspCodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub is_preferred: bool,
    pub edit: Option<LspWorkspaceEdit>,
    pub command: Option<LspCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LspServerCapabilities {
    pub position_encoding: LspPositionEncoding,
    pub hover: bool,
    pub definition: bool,
    pub references: bool,
    pub document_symbols: bool,
    pub workspace_symbols: bool,
    pub code_action: bool,
    pub completion: bool,
    pub signature_help: bool,
    pub rename: bool,
    pub format: bool,
    pub range_format: bool,
    pub semantic_tokens: bool,
    pub semantic_tokens_range: bool,
    pub semantic_tokens_full: bool,
    pub semantic_tokens_legend: Option<LspSemanticTokensLegend>,
    pub inlay_hints: bool,
    pub folding_range: bool,
    pub completion_resolve: bool,
    pub completion_triggers: Vec<char>,
    pub signature_help_triggers: Vec<char>,
}
