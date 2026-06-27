use ropey::Rope;
use serde_json::Value;
use std::path::PathBuf;
use tree_sitter::Tree;

use crate::kernel::editor::{ReloadRequest, TabId};
use crate::kernel::language::LanguageId;
use crate::kernel::services::ports::{
    LspCompletionItem, LspCompletionTriggerContext, LspPositionEncoding, LspRange, LspResourceOp,
    LspWorkspaceFileEdit,
};
use crate::models::OpId;

#[derive(Debug, Clone)]
pub enum Effect {
    LoadFile(PathBuf),
    LoadDir(PathBuf),
    CreateFile(PathBuf),
    CreateDir(PathBuf),
    RenamePath {
        from: PathBuf,
        to: PathBuf,
        overwrite: bool,
    },
    CopyPath {
        from: PathBuf,
        to: PathBuf,
        overwrite: bool,
    },
    DeletePath {
        path: PathBuf,
        is_dir: bool,
    },
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
    ComputeSyntaxHighlights {
        tab_id: TabId,
        version: u64,
        language: LanguageId,
        rope: Rope,
        tree: Tree,
        segments: Vec<(usize, usize)>,
    },
    WriteFile {
        pane: usize,
        path: PathBuf,
        // version：单调计数器，仅用于回调的去重/排序（LSP 同步）。
        version: u64,
        // head：发起写盘那一刻的编辑历史 HEAD，标识被写入磁盘的内容。
        // 保存成功后据此判断当前缓冲区是否仍等于磁盘内容，避免用 version
        // 计数器误判（undo/redo 会前进 version 却不改变内容）。
        head: OpId,
    },
    SetClipboardText(String),
    RequestClipboardText {
        pane: usize,
    },
    LspHoverRequest {
        path: PathBuf,
        line: u32,
        column: u32,
    },
    LspDefinitionRequest {
        path: PathBuf,
        line: u32,
        column: u32,
    },
    LspReferencesRequest {
        path: PathBuf,
        line: u32,
        column: u32,
    },
    LspDocumentSymbolsRequest {
        path: PathBuf,
    },
    LspWorkspaceSymbolsRequest {
        query: String,
    },
    LspCodeActionRequest {
        path: PathBuf,
        line: u32,
        column: u32,
    },
    LspCompletionRequest {
        path: PathBuf,
        line: u32,
        column: u32,
        trigger: LspCompletionTriggerContext,
    },
    LspCompletionResolveRequest {
        item: Box<LspCompletionItem>,
    },
    LspSignatureHelpRequest {
        path: PathBuf,
        line: u32,
        column: u32,
    },
    LspRenameRequest {
        path: PathBuf,
        line: u32,
        column: u32,
        new_name: String,
    },
    LspFormatRequest {
        path: PathBuf,
    },
    LspRangeFormatRequest {
        path: PathBuf,
        range: LspRange,
    },
    LspInlayHintsRequest {
        path: PathBuf,
        version: u64,
        range: LspRange,
    },
    LspFoldingRangeRequest {
        path: PathBuf,
        version: u64,
    },
    LspExecuteCommand {
        command: String,
        arguments: Vec<Value>,
    },
    LspShutdown,
    ApplyFileEdits {
        position_encoding: LspPositionEncoding,
        resource_ops: Vec<LspResourceOp>,
        edits: Vec<LspWorkspaceFileEdit>,
    },
    Restart {
        path: PathBuf,
        hard: bool,
    },
    ReloadFile(ReloadRequest),
}
