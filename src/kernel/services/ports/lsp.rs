//! LSP data contracts used across kernel + adapters.

use ropey::RopeSlice;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LspServerKind {
    RustAnalyzer,
    Gopls,
    Pyright,
    TypeScriptLanguageServer,
    Clangd,
    Jdtls,
}

impl LspServerKind {
    pub fn from_settings_key(key: &str) -> Option<Self> {
        let key = key.trim().to_ascii_lowercase();
        match key.as_str() {
            // Rust
            "rust-analyzer" | "rust_analyzer" | "ra" | "rust" => Some(Self::RustAnalyzer),
            // Go
            "gopls" | "go" => Some(Self::Gopls),
            // Python
            "pyright" | "pyright-langserver" | "python" => Some(Self::Pyright),
            // JS/TS
            "typescript-language-server"
            | "typescript_language_server"
            | "tsls"
            | "typescript"
            | "javascript"
            | "js"
            | "ts" => Some(Self::TypeScriptLanguageServer),
            // C/C++
            "clangd" | "c" | "cpp" | "c++" => Some(Self::Clangd),
            // Java
            "jdtls" | "java" => Some(Self::Jdtls),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LspClientKey {
    pub server: LspServerKind,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LspPositionEncoding {
    Utf8,
    #[default]
    Utf16,
    Utf32,
}

// 行内位置 ↔ LSP 列的纯换算。只依赖 ropey + `LspPositionEncoding`，无任何策略，
// 故安置在 ports 层供 adapter / store / app 三侧共消费，杜绝逐字节复制各自漂移。

/// 一行（不含行尾 `\n` / `\r\n`）的字符数。
pub fn line_len_chars(line: RopeSlice<'_>) -> usize {
    let mut len = 0usize;
    let mut it = line.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\n' {
            break;
        }
        if ch == '\r' && matches!(it.peek(), Some('\n')) {
            break;
        }
        len += 1;
    }
    len
}

/// 把 LSP 列（按 `encoding` 计的编码单元数）换算成行内字符偏移。
pub fn lsp_col_to_char_offset_in_line(
    line: RopeSlice<'_>,
    col: u32,
    encoding: LspPositionEncoding,
) -> usize {
    let mut units = 0u32;
    let mut chars = 0usize;
    let mut it = line.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\n' {
            break;
        }
        if ch == '\r' && matches!(it.peek(), Some('\n')) {
            break;
        }
        let next = units
            + match encoding {
                LspPositionEncoding::Utf8 => ch.len_utf8() as u32,
                LspPositionEncoding::Utf16 => ch.len_utf16() as u32,
                LspPositionEncoding::Utf32 => 1,
            };
        if next > col {
            break;
        }
        units = next;
        chars += 1;
    }
    chars
}

/// 把行内字符偏移换算成 LSP 列（按 `encoding` 计的编码单元数），即上者的逆。
pub fn column_for_chars(
    line: RopeSlice<'_>,
    col_chars: usize,
    encoding: LspPositionEncoding,
) -> u32 {
    match encoding {
        LspPositionEncoding::Utf8 => line
            .chars()
            .take(col_chars)
            .map(|ch| ch.len_utf8() as u32)
            .sum(),
        LspPositionEncoding::Utf16 => line
            .chars()
            .take(col_chars)
            .map(|ch| ch.len_utf16() as u32)
            .sum(),
        LspPositionEncoding::Utf32 => col_chars as u32,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspMarkup {
    Markdown(String),
    PlainText(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspSignatureParameterLabel {
    Simple(String),
    Offsets { start: u32, end: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspSignatureParameter {
    pub label: LspSignatureParameterLabel,
    pub documentation: Option<LspMarkup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspSignatureInfo {
    pub label: String,
    pub documentation: Option<LspMarkup>,
    pub parameters: Vec<LspSignatureParameter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LspSignatureHelpPayload {
    pub signatures: Vec<LspSignatureInfo>,
    pub active_signature: Option<u32>,
    pub active_parameter: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspHoverBlock {
    Markdown(String),
    Code {
        language: Option<String>,
        code: String,
    },
    PlainText(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LspHoverPayload {
    pub blocks: Vec<LspHoverBlock>,
    pub range: Option<LspRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LspHoverPreviewPayload {
    pub title: String,
    pub blocks: Vec<LspHoverBlock>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LspCompletionTriggerKind {
    #[default]
    Invoked,
    TriggerCharacter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LspCompletionTriggerContext {
    pub kind: LspCompletionTriggerKind,
    pub character: Option<char>,
}

impl LspCompletionTriggerContext {
    pub fn invoked() -> Self {
        Self {
            kind: LspCompletionTriggerKind::Invoked,
            character: None,
        }
    }

    pub fn trigger_character(character: char) -> Self {
        Self {
            kind: LspCompletionTriggerKind::TriggerCharacter,
            character: Some(character),
        }
    }
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

#[cfg(test)]
#[path = "../../../../tests/unit/kernel/services/ports/lsp.rs"]
mod tests;
