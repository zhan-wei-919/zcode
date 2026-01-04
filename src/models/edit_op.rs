use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpId {
    pub timestamp: u64,
    pub counter: u16,
}

impl OpId {
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self { timestamp, counter }
    }

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OpKind {
    Insert {
        char_offset: usize,
        text: String,
    },
    Delete {
        start: usize,
        end: usize,
        deleted: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditOp {
    pub id: OpId,
    pub parent: OpId,
    pub kind: OpKind,
    pub cursor_before: (usize, usize),
    pub cursor_after: (usize, usize),
}

impl EditOp {
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

    pub fn cursor_after(&self) -> (usize, usize) {
        self.cursor_after
    }

    pub fn cursor_before(&self) -> (usize, usize) {
        self.cursor_before
    }

    pub fn apply(&self, rope: &mut ropey::Rope) {
        self.kind.apply(rope);
    }

    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json_line(line: &str) -> Option<Self> {
        serde_json::from_str(line).ok()
    }
}

impl OpKind {
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
