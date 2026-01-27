use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpId {
    pub(crate) timestamp: u64,
    pub(crate) counter: u16,
}

impl OpId {
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
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
    Replace {
        start: usize,
        end: usize,
        deleted: String,
        inserted: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditOp {
    pub(crate) id: OpId,
    pub(crate) parent: OpId,
    pub(crate) kind: OpKind,
    pub(crate) cursor_before: (usize, usize),
    pub(crate) cursor_after: (usize, usize),
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

    pub fn replace(
        parent: OpId,
        start: usize,
        end: usize,
        deleted: String,
        inserted: String,
        cursor_before: (usize, usize),
        cursor_after: (usize, usize),
    ) -> Self {
        Self {
            id: OpId::new(),
            parent,
            kind: OpKind::Replace {
                start,
                end,
                deleted,
                inserted,
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
            OpKind::Replace {
                start,
                deleted,
                inserted,
                ..
            } => OpKind::Replace {
                start: *start,
                end: start + inserted.chars().count(),
                deleted: inserted.clone(),
                inserted: deleted.clone(),
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

    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
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
            OpKind::Replace {
                start,
                end,
                inserted,
                ..
            } => {
                rope.remove(*start..*end);
                rope.insert(*start, inserted);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/models/edit_op.rs"]
mod tests;
