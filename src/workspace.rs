//zcode/src/workspace.rs
use crate::file_system::FileTree;
use crate::editor::state::Editor;

/// 工作区：顶层状态容器，聚合所有功能模块
/// 
/// 架构理念：
/// - 单一职责：每个模块专注于自己的领域
/// - 高内聚低耦合：模块间通过 Workspace 协调，但互不依赖
/// - 可扩展：未来添加 Terminal、Git、Debug 等功能只需扩展此结构
pub struct Workspace {
    pub file_tree: FileTree,
    pub editor: Editor,
}

impl Workspace {
    /// 创建新的工作区
    pub fn new(file_tree: FileTree) -> Self {
        Self {
            file_tree,
            editor: Editor::new(),
        }
    }
}
