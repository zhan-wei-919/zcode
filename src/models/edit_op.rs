//! 编辑操作定义
//!
//! EditOp 是原子编辑操作，同时描述文本变更和光标变更。
//! 采用 Git 模型：每个操作有唯一 ID 和父指针，历史形成 DAG。
//! 支持序列化到磁盘用于崩溃恢复。

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// 操作唯一标识符（时间戳 + 计数器，避免 UUID 依赖）
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpId {
    /// 毫秒时间戳
    pub timestamp: u64,
    /// 同一毫秒内的计数器
    pub counter: u16,
}

impl OpId {
    /// 生成新的 OpId
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self { timestamp, counter }
    }

    /// 根节点 ID（表示文件初始状态）
    pub fn root() -> Self {
        Self {
            timestamp: 0,
            counter: 0,
        }
    }

    pub fn is_root(&self) -> bool {
        self.timestamp == 0 && self.counter == 0
    }
}

impl Default for OpId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for OpId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OpId({:x}:{:04x})", self.timestamp, self.counter)
    }
}

impl fmt::Display for OpId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x}:{:04x}", self.timestamp, self.counter)
    }
}

/// 操作内容
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OpKind {
    /// 插入文本
    Insert {
        /// 插入位置（char offset）
        char_offset: usize,
        /// 插入的文本
        text: String,
    },
    /// 删除文本
    Delete {
        /// 删除起始位置（char offset）
        start: usize,
        /// 删除结束位置（char offset）
        end: usize,
        /// 被删除的文本（用于 Undo）
        deleted: String,
    },
}

/// 原子编辑操作（Git 模型）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditOp {
    /// 唯一标识
    pub id: OpId,
    /// 父操作 ID
    pub parent: OpId,
    /// 操作内容
    pub kind: OpKind,
    /// 操作前的光标位置
    pub cursor_before: (usize, usize),
    /// 操作后的光标位置
    pub cursor_after: (usize, usize),
}

impl EditOp {
    /// 创建插入操作
    pub fn insert(
        parent: OpId,
        char_offset: usize,
        text: String,
        cursor_before: (usize, usize),
        cursor_after: (usize, usize),
    ) -> Self {
        Self {
            id: OpId::new(),
            parent,
            kind: OpKind::Insert { char_offset, text },
            cursor_before,
            cursor_after,
        }
    }

    /// 创建删除操作
    pub fn delete(
        parent: OpId,
        start: usize,
        end: usize,
        deleted: String,
        cursor_before: (usize, usize),
        cursor_after: (usize, usize),
    ) -> Self {
        Self {
            id: OpId::new(),
            parent,
            kind: OpKind::Delete {
                start,
                end,
                deleted,
            },
            cursor_before,
            cursor_after,
        }
    }

    /// 生成反向操作（用于 Undo，新操作的 parent 指向当前操作）
    pub fn inverse(&self) -> OpKind {
        match &self.kind {
            OpKind::Insert { char_offset, text } => OpKind::Delete {
                start: *char_offset,
                end: char_offset + text.chars().count(),
                deleted: text.clone(),
            },
            OpKind::Delete { start, deleted, .. } => OpKind::Insert {
                char_offset: *start,
                text: deleted.clone(),
            },
        }
    }

    /// 获取操作后的光标位置
    pub fn cursor_after(&self) -> (usize, usize) {
        self.cursor_after
    }

    /// 获取操作前的光标位置
    pub fn cursor_before(&self) -> (usize, usize) {
        self.cursor_before
    }

    /// 应用操作到 Rope
    pub fn apply(&self, rope: &mut ropey::Rope) {
        self.kind.apply(rope);
    }

    /// 序列化为 JSON 行
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// 从 JSON 行反序列化
    pub fn from_json_line(line: &str) -> Option<Self> {
        serde_json::from_str(line).ok()
    }
}

impl OpKind {
    /// 应用操作到 Rope
    pub fn apply(&self, rope: &mut ropey::Rope) {
        match self {
            OpKind::Insert { char_offset, text } => {
                rope.insert(*char_offset, text);
            }
            OpKind::Delete { start, end, .. } => {
                rope.remove(*start..*end);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    #[test]
    fn test_insert_apply() {
        let mut rope = Rope::from_str("hello");
        let op = EditOp::insert(OpId::root(), 5, " world".to_string(), (0, 5), (0, 11));
        op.apply(&mut rope);
        assert_eq!(rope.to_string(), "hello world");
    }

    #[test]
    fn test_delete_apply() {
        let mut rope = Rope::from_str("hello world");
        let op = EditOp::delete(OpId::root(), 5, 11, " world".to_string(), (0, 11), (0, 5));
        op.apply(&mut rope);
        assert_eq!(rope.to_string(), "hello");
    }

    #[test]
    fn test_inverse() {
        let insert_op = EditOp::insert(OpId::root(), 0, "hello".to_string(), (0, 0), (0, 5));
        let delete_kind = insert_op.inverse();

        let mut rope = Rope::new();
        insert_op.apply(&mut rope);
        assert_eq!(rope.to_string(), "hello");

        delete_kind.apply(&mut rope);
        assert_eq!(rope.to_string(), "");
    }

    #[test]
    fn test_serialization() {
        let op = EditOp::insert(OpId::root(), 0, "hello".to_string(), (0, 0), (0, 5));
        let json = op.to_json_line();
        let restored = EditOp::from_json_line(&json).unwrap();

        assert_eq!(restored.cursor_after(), (0, 5));
        assert_eq!(restored.parent, OpId::root());
    }

    #[test]
    fn test_opid_uniqueness() {
        let id1 = OpId::new();
        let id2 = OpId::new();
        assert_ne!(id1, id2);
    }
}
