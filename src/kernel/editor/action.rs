use crate::kernel::services::ports::SearchMessage;
use std::path::PathBuf;
use std::time::Instant;

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
    MouseDown {
        pane: usize,
        x: u16,
        y: u16,
        now: Instant,
    },
    MouseDrag {
        pane: usize,
        x: u16,
        y: u16,
    },
    MouseUp {
        pane: usize,
    },
    Scroll {
        pane: usize,
        delta_lines: isize,
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
    },
}
