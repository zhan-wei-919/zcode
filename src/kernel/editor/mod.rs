//! Editor domain: headless state + actions.

mod action;
mod edit;
mod mouse;
mod reducer;
mod search;
mod state;
mod syntax;
mod syntax_highlight_cache;
mod viewport;

pub use crate::kernel::language::LanguageId;
pub use action::EditorAction;
pub(crate) use state::SnippetTabstop;
pub use state::{
    DiskSnapshot, DiskState, EditorPaneState, EditorState, EditorTabState, EditorViewportState,
    ReloadCause, ReloadRequest, SearchBarField, SearchBarMode, SearchBarState, TabId,
};
pub(crate) use syntax::compute_highlight_patches;
pub use syntax::{
    highlight_snippet, HighlightKind, HighlightSpan, SyntaxColorGroup, SyntaxHighlightPatch,
    DEFAULT_CONFIGURABLE_SYNTAX_RGB_HEX,
};
pub(crate) use viewport::clamp_and_follow;
pub use viewport::cursor_display_x_abs;
