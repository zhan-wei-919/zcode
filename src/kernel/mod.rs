//! Headless application core (state/action/effect).

pub mod action;
pub mod editor;
pub mod effect;
pub mod language;
pub mod lsp_registry;
pub mod palette;
pub mod panel;
pub mod search;
pub mod services;
pub mod state;
pub mod store;

pub use action::Action;
pub use editor::{EditorAction, EditorState};
pub use effect::Effect;
pub use panel::code_actions::CodeActionsState;
pub use panel::locations::{LocationItem, LocationsState};
pub use panel::problems::{ProblemItem, ProblemRange, ProblemSeverity, ProblemsState};
pub use panel::symbols::{SymbolItem, SymbolsState};
pub use search::{SearchResultItem, SearchResultsSnapshot, SearchState};
pub use state::{
    AppState, CommandLineState, ConfirmDialogState, EditorLayoutState, ExplorerState, FocusTarget,
    InputDialogKind, InputDialogState, LspState, OverlayKind, OverlayState, PendingAction, UiState,
};
pub use store::{CompletionRanker, DispatchResult, Store};
