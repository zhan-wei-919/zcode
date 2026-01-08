//! Editor domain: headless state + actions.

mod action;
mod edit;
mod mouse;
mod reducer;
mod search;
mod state;
mod syntax;
mod viewport;

pub use action::EditorAction;
pub use state::{
    EditorMouseState, EditorPaneState, EditorState, EditorTabState, EditorViewportState,
    SearchBarField, SearchBarMode, SearchBarState,
};
pub use syntax::{HighlightKind, HighlightSpan, LanguageId};
