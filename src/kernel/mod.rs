//! Headless application core (state/action/effect).

pub mod action;
pub mod editor;
pub mod effect;
pub mod git;
pub mod language;
pub mod lsp_registry;
pub mod panel;
pub mod palette;
pub mod search;
pub mod services;
pub mod state;
pub mod store;
pub mod terminal;

pub use action::Action;
pub use editor::{EditorAction, EditorState};
pub use effect::Effect;
pub use git::{
    GitFileStatus, GitFileStatusKind, GitGutterMarkKind, GitGutterMarkRange, GitGutterMarks,
    GitHead, GitState, GitWorktreeItem,
};
pub use panel::code_actions::CodeActionsState;
pub use panel::locations::{LocationItem, LocationsState};
pub use panel::problems::{ProblemItem, ProblemRange, ProblemSeverity, ProblemsState};
pub use panel::symbols::{SymbolItem, SymbolsState};
pub use search::{SearchResultItem, SearchResultsSnapshot, SearchState, SearchViewport};
pub use state::{
    AppState, BottomPanelTab, ConfirmDialogState, EditorLayoutState, ExplorerState, FocusTarget,
    InputDialogKind, InputDialogState, LspState, PendingAction, SidebarTab, SplitDirection,
    UiState,
};
pub use store::{CompletionRanker, DispatchResult, Store};
pub use terminal::{TerminalId, TerminalSession, TerminalState};
