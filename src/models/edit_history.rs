use super::edit_op::{EditOp, OpId};
use ropey::Rope;
use rustc_hash::FxHashMap;

#[derive(Clone, Debug)]
pub struct UndoResult {
    pub rope: Rope,
    pub cursor: (usize, usize),
    pub secondary_cursors: Option<Vec<(usize, usize)>>,
}

/// 纯内存撤销/重做 DAG。
///
/// 历史只在进程内维护，不落盘——磁盘持久化 / 恢复子系统（备份文件、检查点快照、
/// reflog/checkout）已整体移除，因为生产从未启用它（`ops_file_path` 恒为 None）。
/// undo/redo 全程增量：对调用方传入的当前 Rope 应用 `op` 的逆 / 正变换，无需基线快照。
pub struct EditHistory {
    ops: FxHashMap<OpId, EditOp>,
    head: OpId,
    saved_head: OpId,
    children: FxHashMap<OpId, Vec<OpId>>,
    preferred_child: FxHashMap<OpId, OpId>,
}

impl Default for EditHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl EditHistory {
    /// 创建空历史。
    pub fn new() -> Self {
        Self {
            ops: FxHashMap::default(),
            head: OpId::root(),
            saved_head: OpId::root(),
            children: FxHashMap::default(),
            preferred_child: FxHashMap::default(),
        }
    }

    /// 记录新操作。
    pub fn push(&mut self, op: EditOp) {
        let op_id = op.id;
        let parent_id = op.parent;

        self.ops.insert(op_id, op);
        self.children.entry(parent_id).or_default().push(op_id);
        self.preferred_child.insert(parent_id, op_id);
        self.head = op_id;
    }

    /// Undo：返回恢复后的 Rope 和光标位置。
    pub fn undo(&mut self, current_rope: &Rope) -> Option<UndoResult> {
        let current_id = self.head;
        if current_id.is_root() {
            return None;
        }

        let current_op = self.ops.get(&current_id)?;
        let cursor_pos = current_op.cursor_before();
        let secondary_cursors = current_op.extra_cursors_before.clone();
        let parent_id = current_op.parent;

        self.preferred_child.insert(parent_id, current_id);

        let mut rope = current_rope.clone();
        current_op.inverse().apply(&mut rope);

        // 移动 HEAD 到父节点
        self.head = parent_id;

        Some(UndoResult {
            rope,
            cursor: cursor_pos,
            secondary_cursors,
        })
    }

    /// Redo：沿着最近的分支前进。
    pub fn redo(&mut self, current_rope: &Rope) -> Option<UndoResult> {
        let head_id = self.head;
        let children = self.children.get(&head_id)?;
        if children.is_empty() {
            return None;
        }

        let next_id = self
            .preferred_child
            .get(&head_id)
            .copied()
            .filter(|id| children.contains(id))
            .unwrap_or_else(|| *children.last().unwrap());
        let next_op = self.ops.get(&next_id)?;
        let cursor_pos = next_op.cursor_after();
        let secondary_cursors = next_op.extra_cursors_after.clone();

        let mut rope = current_rope.clone();
        next_op.apply(&mut rope);

        // 移动 HEAD
        self.head = next_id;
        self.preferred_child.insert(head_id, next_id);

        Some(UndoResult {
            rope,
            cursor: cursor_pos,
            secondary_cursors,
        })
    }

    /// 获取当前 HEAD
    pub fn head(&self) -> OpId {
        self.head
    }

    /// 是否有未保存的修改
    pub fn is_dirty(&self) -> bool {
        self.head != self.saved_head
    }

    /// 是否可以 Undo
    pub fn can_undo(&self) -> bool {
        !self.head.is_root()
    }

    /// 是否可以 Redo
    pub fn can_redo(&self) -> bool {
        self.children
            .get(&self.head)
            .map(|c| !c.is_empty())
            .unwrap_or(false)
    }

    pub fn get_op(&self, id: &OpId) -> Option<&EditOp> {
        self.ops.get(id)
    }

    /// 保存后把当前 HEAD 记为已保存基线（驱动脏标记）。
    pub fn on_save(&mut self) {
        self.saved_head = self.head;
    }

    /// 重置历史，丢弃全部已记录操作。
    pub fn clear(&mut self) {
        self.saved_head = OpId::root();
        self.ops.clear();
        self.head = OpId::root();
        self.children.clear();
        self.preferred_child.clear();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/models/edit_history.rs"]
mod tests;
