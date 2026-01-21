//! Headless application core (state/action/effect).

pub mod action;
pub mod editor;
pub mod effect;
pub mod palette;
pub mod problems;
pub mod search;
pub mod services;
pub mod state;
pub mod store;

pub use action::Action;
pub use editor::{EditorAction, EditorState};
pub use effect::Effect;
pub use problems::{ProblemItem, ProblemRange, ProblemSeverity, ProblemsState};
pub use search::{SearchResultItem, SearchResultsSnapshot, SearchState, SearchViewport};
pub use state::{
    AppState, BottomPanelTab, ConfirmDialogState, EditorLayoutState, ExplorerState, FocusTarget,
    InputDialogKind, InputDialogState, PendingAction, SidebarTab, SplitDirection, UiState,
};
pub use store::{DispatchResult, Store};
