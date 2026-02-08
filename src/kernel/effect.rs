use ropey::Rope;
use serde_json::Value;
use std::path::PathBuf;

use crate::kernel::services::ports::{
    LspCompletionItem, LspCompletionTriggerContext, LspPositionEncoding, LspRange, LspResourceOp,
    LspWorkspaceFileEdit, ThemeSettings,
};
use crate::kernel::TerminalId;
use crate::kernel::editor::ReloadRequest;

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
    WriteFile {
        pane: usize,
        path: PathBuf,
        version: u64,
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
    LspSemanticTokensRequest {
        path: PathBuf,
        version: u64,
    },
    LspSemanticTokensRangeRequest {
        path: PathBuf,
        version: u64,
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
    GitDetectRepo {
        workspace_root: PathBuf,
    },
    GitRefreshStatus {
        repo_root: PathBuf,
    },
    GitRefreshDiff {
        repo_root: PathBuf,
        path: PathBuf,
    },
    GitListWorktrees {
        repo_root: PathBuf,
    },
    GitListBranches {
        repo_root: PathBuf,
    },
    GitWorktreeAdd {
        repo_root: PathBuf,
        branch: String,
    },
    GitWorktreeResolve {
        repo_root: PathBuf,
        branch: String,
    },
    TerminalSpawn {
        id: TerminalId,
        cwd: PathBuf,
        shell: Option<String>,
        args: Vec<String>,
        cols: u16,
        rows: u16,
    },
    TerminalWrite {
        id: TerminalId,
        bytes: Vec<u8>,
    },
    TerminalResize {
        id: TerminalId,
        cols: u16,
        rows: u16,
    },
    TerminalKill {
        id: TerminalId,
    },
    Restart {
        path: PathBuf,
        hard: bool,
    },
    SaveThemeSettings {
        theme_settings: Box<ThemeSettings>,
    },
    ReloadFile(ReloadRequest),
}
