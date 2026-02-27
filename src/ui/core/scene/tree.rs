use super::geom::{Pos, Rect};
use super::id::Id;
use std::ops::{BitOr, BitOrAssign};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Sense(u16);

impl Sense {
    pub const NONE: Self = Self(0);
    pub const HOVER: Self = Self(1 << 0);
    pub const CLICK: Self = Self(1 << 1);
    pub const CONTEXT_MENU: Self = Self(1 << 2);
    pub const DRAG_SOURCE: Self = Self(1 << 3);
    pub const DROP_TARGET: Self = Self(1 << 4);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl BitOr for Sense {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Sense {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDrop {
    Right,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Unknown,
    Splitter { axis: Axis },
    Tab { pane: usize, tab_id: u64 },
    TabBar { pane: usize },
    ExplorerRow { node_id: u64 },
    ExplorerFolderDrop { node_id: u64 },
    EditorArea { pane: usize },
    EditorSplitDrop { pane: usize, drop: SplitDrop },
    MenuItem { menu_id: u32, index: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: Id,
    pub rect: Rect,
    pub layer: u8,
    pub z: u32,
    pub sense: Sense,
    pub kind: NodeKind,
}

impl Node {
    pub fn contains(&self, p: Pos) -> bool {
        self.rect.contains(p)
    }
}

#[derive(Clone, Debug, Default)]
pub struct UiTree {
    nodes: Vec<Node>,
}

impl UiTree {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn node(&self, id: Id) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn push(&mut self, mut node: Node) {
        // Default z-order: insertion order within the same layer.
        if node.z == 0 {
            node.z = self.nodes.len() as u32;
        }
        self.nodes.push(node);
    }

    pub fn hit_test(&self, p: Pos) -> Option<&Node> {
        // Highest layer wins; within a layer, higher z wins.
        self.nodes
            .iter()
            .filter(|n| n.contains(p))
            .max_by(|a, b| (a.layer, a.z).cmp(&(b.layer, b.z)))
    }

    pub fn hit_test_with_sense(&self, p: Pos, required: Sense) -> Option<&Node> {
        self.nodes
            .iter()
            .filter(|n| n.sense.contains(required) && n.contains(p))
            .max_by(|a, b| (a.layer, a.z).cmp(&(b.layer, b.z)))
    }

    pub fn hit_test_with_sense_where<F>(
        &self,
        p: Pos,
        required: Sense,
        mut pred: F,
    ) -> Option<&Node>
    where
        F: FnMut(&Node) -> bool,
    {
        self.nodes
            .iter()
            .filter(|n| n.sense.contains(required) && n.contains(p) && pred(n))
            .max_by(|a, b| (a.layer, a.z).cmp(&(b.layer, b.z)))
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/ui/core/tree.rs"]
mod tests;
