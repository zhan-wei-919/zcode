//! Headless application core (state/action/effect).

pub mod action;
pub mod code_actions;
pub mod editor;
pub mod effect;
pub mod locations;
pub mod palette;
pub mod problems;
pub mod search;
pub mod services;
pub mod state;
pub mod store;
pub mod symbols;

pub use action::Action;
pub use code_actions::CodeActionsState;
pub use editor::{EditorAction, EditorState};
pub use effect::Effect;
pub use locations::{LocationItem, LocationsState};
pub use problems::{ProblemItem, ProblemRange, ProblemSeverity, ProblemsState};
pub use search::{SearchResultItem, SearchResultsSnapshot, SearchState, SearchViewport};
pub use state::{
    AppState, BottomPanelTab, ConfirmDialogState, EditorLayoutState, ExplorerState, FocusTarget,
    InputDialogKind, InputDialogState, LspState, PendingAction, SidebarTab, SplitDirection, UiState,
};
pub use store::{DispatchResult, Store};
pub use symbols::{SymbolItem, SymbolsState};
