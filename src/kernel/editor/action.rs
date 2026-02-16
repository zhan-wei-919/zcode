use crate::kernel::services::ports::SearchMessage;
use crate::models::Granularity;
use std::path::PathBuf;

use super::ReloadRequest;
use super::TabId;

#[derive(Debug, Clone)]
pub enum EditorAction {
    OpenFile {
        pane: usize,
        path: PathBuf,
        content: String,
    },
    GotoByteOffset {
        pane: usize,
        byte_offset: usize,
    },
    SetActiveTab {
        pane: usize,
        index: usize,
    },
    SetViewportSize {
        pane: usize,
        width: usize,
        height: usize,
    },
    InsertText {
        pane: usize,
        text: String,
    },
    ApplyTextEdit {
        pane: usize,
        start_byte: usize,
        end_byte: usize,
        text: String,
    },
    ApplyTextEditToTab {
        pane: usize,
        tab_index: usize,
        start_byte: usize,
        end_byte: usize,
        text: String,
    },
    ReplaceRangeChars {
        pane: usize,
        start_char: usize,
        end_char: usize,
        text: String,
    },
    PlaceCursor {
        pane: usize,
        row: usize,
        col: usize,
        granularity: Granularity,
    },
    ExtendSelection {
        pane: usize,
        row: usize,
        col: usize,
    },
    EndSelectionGesture {
        pane: usize,
    },
    Scroll {
        pane: usize,
        delta_lines: isize,
    },
    ScrollHorizontal {
        pane: usize,
        delta_columns: isize,
    },
    SearchBarAppend {
        pane: usize,
        ch: char,
    },
    SearchBarBackspace {
        pane: usize,
    },
    SearchBarDeleteForward {
        pane: usize,
    },
    SearchBarCursorLeft {
        pane: usize,
    },
    SearchBarCursorRight {
        pane: usize,
    },
    SearchBarCursorHome {
        pane: usize,
    },
    SearchBarCursorEnd {
        pane: usize,
    },
    SearchBarSwitchField {
        pane: usize,
    },
    SearchBarToggleCaseSensitive {
        pane: usize,
    },
    SearchBarToggleRegex {
        pane: usize,
    },
    SearchBarToggleReplaceMode {
        pane: usize,
    },
    ReplaceCurrent {
        pane: usize,
    },
    ReplaceAll {
        pane: usize,
    },
    SearchStarted {
        pane: usize,
        search_id: u64,
    },
    SearchMessage {
        pane: usize,
        message: SearchMessage,
    },
    Saved {
        pane: usize,
        path: PathBuf,
        success: bool,
        version: u64,
    },
    CloseTabAt {
        pane: usize,
        index: usize,
    },
    CloseTabsById {
        pane: usize,
        tab_ids: Vec<u64>,
    },
    MoveTab {
        tab_id: TabId,
        from_pane: usize,
        to_pane: usize,
        to_index: usize,
    },
    FileReloaded {
        content: String,
        request: ReloadRequest,
    },
    FileExternallyModified {
        path: PathBuf,
    },
    FileExternallyDeleted {
        path: PathBuf,
    },
    AcceptDiskVersion {
        pane: usize,
        path: PathBuf,
        content: String,
    },
    KeepMemoryVersion {
        pane: usize,
    },
}
