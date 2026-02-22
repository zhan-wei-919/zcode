//! 数据模型层

pub mod cursor_set;
pub mod edit_history;
pub mod edit_op;
pub mod file_tree;
pub mod selection;
pub mod text_buffer;

pub use cursor_set::SecondaryCursor;
pub use edit_history::{EditHistory, EditHistoryConfig, UndoResult};
pub use edit_op::{EditOp, OpId, OpKind};
pub use file_tree::{
    build_file_tree, should_ignore, FileTree, FileTreeError, FileTreeRow, LoadState, NodeId,
    NodeKind,
};
pub use selection::{Granularity, Selection};
pub use text_buffer::{slice_to_cow, TextBuffer};
