//! Editor domain: headless state + actions.

mod action;
mod edit;
mod mouse;
mod reducer;
mod search;
mod state;
mod syntax;
mod viewport;

pub use crate::kernel::language::LanguageId;
pub use action::EditorAction;
pub use state::{
    EditorMouseState, EditorPaneState, EditorState, EditorTabState, EditorViewportState,
    SearchBarField, SearchBarMode, SearchBarState, TabId,
};
pub use syntax::{highlight_snippet, HighlightKind, HighlightSpan};
pub(crate) use viewport::clamp_and_follow;
pub use viewport::cursor_display_x_abs;
pub(crate) use viewport::screen_to_col;
