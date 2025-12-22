//! 数据模型层
//!
//! 纯数据结构，无 UI 逻辑：
//! - FileTree: 文件树结构
//! - TextBuffer: 文本缓冲区
//! - Selection: 选区模型

pub mod file_tree;
pub mod selection;
pub mod text_buffer;

pub use file_tree::{build_file_tree, FileTree, FileTreeError, FileTreeRow, Node, NodeId, NodeKind};
pub use selection::{Granularity, Selection};
pub use text_buffer::TextBuffer;
