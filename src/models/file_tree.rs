//! 文件树数据模型

use rustc_hash::FxHashSet;
use slotmap::{new_key_type, SlotMap};
use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    fmt, io,
    path::{Path, PathBuf},
};

new_key_type! { pub struct NodeId; }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Dir,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LoadState {
    NotLoaded,
    Loading,
    Loaded,
}

#[derive(Debug)]
pub enum FileTreeError {
    ParentNotDirectory,
    NameExists,
    MoveIntoDescendant,
    InvalidNodeId,
}

impl fmt::Display for FileTreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileTreeError::ParentNotDirectory => write!(f, "parent is not a directory"),
            FileTreeError::NameExists => write!(f, "name already exists in parent"),
            FileTreeError::MoveIntoDescendant => {
                write!(f, "cannot move node into its own subtree")
            }
            FileTreeError::InvalidNodeId => write!(f, "invalid node id"),
        }
    }
}

impl std::error::Error for FileTreeError {}

#[derive(Debug, Clone)]
struct Node {
    kind: NodeKind,
    name: OsString,
    parent: Option<NodeId>,
    children: Option<BTreeMap<OsString, NodeId>>,
    load_state: LoadState,
}

impl Node {
    fn new_file(name: OsString, parent: Option<NodeId>) -> Self {
        Self {
            kind: NodeKind::File,
            name,
            parent,
            children: None,
            load_state: LoadState::Loaded,
        }
    }

    fn new_dir(name: OsString, parent: Option<NodeId>, load_state: LoadState) -> Self {
        Self {
            kind: NodeKind::Dir,
            name,
            parent,
            children: Some(BTreeMap::new()),
            load_state,
        }
    }
}

pub struct FileTree {
    arena: SlotMap<NodeId, Node>,
    root: NodeId,
    expanded: FxHashSet<NodeId>,
    selected: Option<NodeId>,
    absolute_root: PathBuf,
    path_cache: HashMap<NodeId, PathBuf>,
    id_by_path: HashMap<PathBuf, NodeId>,
}

impl FileTree {
    fn new_with_root(root_name: OsString, absolute_root: PathBuf) -> Self {
        let mut arena = SlotMap::with_key();
        let root = arena.insert(Node::new_dir(root_name, None, LoadState::Loaded));

        let mut expanded = FxHashSet::default();
        expanded.insert(root);

        Self {
            arena,
            root,
            expanded,
            selected: Some(root),
            absolute_root,
            path_cache: HashMap::new(),
            id_by_path: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub fn new_with_root_for_test(root_name: OsString, absolute_root: PathBuf) -> Self {
        Self::new_with_root(root_name, absolute_root)
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.selected
    }

    pub fn set_selected(&mut self, id: Option<NodeId>) {
        self.selected = id;
    }

    pub fn absolute_root(&self) -> &Path {
        &self.absolute_root
    }

    pub fn load_state(&self, id: NodeId) -> Option<LoadState> {
        self.arena.get(id).map(|n| n.load_state)
    }

    pub fn set_load_state(&mut self, id: NodeId, state: LoadState) {
        if let Some(node) = self.arena.get_mut(id) {
            node.load_state = state;
        }
    }

    pub fn insert_child(
        &mut self,
        parent: NodeId,
        name: OsString,
        kind: NodeKind,
    ) -> Result<NodeId, FileTreeError> {
        self.insert_child_with_state(parent, name, kind, LoadState::NotLoaded)
    }

    pub fn insert_child_with_state(
        &mut self,
        parent: NodeId,
        name: OsString,
        kind: NodeKind,
        load_state: LoadState,
    ) -> Result<NodeId, FileTreeError> {
        {
            let parent_ro = self.arena.get(parent).ok_or(FileTreeError::InvalidNodeId)?;
            let children_ro = parent_ro
                .children
                .as_ref()
                .ok_or(FileTreeError::ParentNotDirectory)?;
            if children_ro.contains_key(&name) {
                return Err(FileTreeError::NameExists);
            }
        }

        let node = match kind {
            NodeKind::File => Node::new_file(name.clone(), Some(parent)),
            NodeKind::Dir => Node::new_dir(name.clone(), Some(parent), load_state),
        };
        let id = self.arena.insert(node);

        let parent_node = self
            .arena
            .get_mut(parent)
            .ok_or(FileTreeError::InvalidNodeId)?;
        let children = parent_node
            .children
            .as_mut()
            .ok_or(FileTreeError::ParentNotDirectory)?;
        children.insert(name, id);

        Ok(id)
    }

    pub fn full_path(&mut self, id: NodeId) -> PathBuf {
        if id == self.root {
            self.id_by_path
                .insert(self.absolute_root.clone(), self.root);
            return self.absolute_root.clone();
        }

        if let Some(cached_path) = self.path_cache.get(&id) {
            return cached_path.clone();
        }

        let mut path = self.absolute_root.clone();
        let mut current = id;
        let mut components = vec![];

        while let Some(node) = self.arena.get(current) {
            if let Some(parent) = node.parent {
                components.push(node.name.as_os_str());
                current = parent;
            } else {
                break;
            }
        }

        for comp in components.iter().rev() {
            path.push(comp);
        }

        self.path_cache.insert(id, path.clone());
        self.id_by_path.insert(path.clone(), id);
        path
    }

    fn invalidate_path_cache_subtree(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(node_id) = stack.pop() {
            if let Some(path) = self.path_cache.remove(&node_id) {
                self.id_by_path.remove(&path);
            }
            if let Some(node) = self.arena.get(node_id) {
                if let Some(children) = &node.children {
                    for &child_id in children.values() {
                        stack.push(child_id);
                    }
                }
            }
        }
    }

    fn is_ancestor(&self, ancestor: NodeId, mut descendant: NodeId) -> bool {
        while let Some(node) = self.arena.get(descendant) {
            if let Some(parent) = node.parent {
                if parent == ancestor {
                    return true;
                }
                descendant = parent;
            } else {
                break;
            }
        }
        false
    }

    pub fn rename(&mut self, id: NodeId, new_name: OsString) -> Result<(), FileTreeError> {
        let (parent, old_name) = {
            let node = self.arena.get(id).ok_or(FileTreeError::InvalidNodeId)?;
            (node.parent, node.name.clone())
        };

        if old_name == new_name {
            return Ok(());
        }

        if let Some(parent_id) = parent {
            let parent_node = self
                .arena
                .get_mut(parent_id)
                .ok_or(FileTreeError::InvalidNodeId)?;
            let children = parent_node
                .children
                .as_mut()
                .ok_or(FileTreeError::ParentNotDirectory)?;

            if children.contains_key(&new_name) {
                return Err(FileTreeError::NameExists);
            }
            children.remove(&old_name);
            children.insert(new_name.clone(), id);
        }

        self.arena
            .get_mut(id)
            .ok_or(FileTreeError::InvalidNodeId)?
            .name = new_name;

        self.invalidate_path_cache_subtree(id);
        Ok(())
    }

    pub fn move_to(&mut self, id: NodeId, new_parent: NodeId) -> Result<(), FileTreeError> {
        if self.is_ancestor(id, new_parent) {
            return Err(FileTreeError::MoveIntoDescendant);
        }

        let (name, old_parent) = {
            let node = self.arena.get(id).ok_or(FileTreeError::InvalidNodeId)?;
            (node.name.clone(), node.parent)
        };

        if old_parent == Some(new_parent) {
            return Ok(());
        }

        if let Some(old_parent_id) = old_parent {
            if let Some(children) = self
                .arena
                .get_mut(old_parent_id)
                .and_then(|n| n.children.as_mut())
            {
                children.remove(&name);
            }
        }

        let new_parent_node = self
            .arena
            .get_mut(new_parent)
            .ok_or(FileTreeError::InvalidNodeId)?;
        let children = new_parent_node
            .children
            .as_mut()
            .ok_or(FileTreeError::ParentNotDirectory)?;

        if children.contains_key(&name) {
            return Err(FileTreeError::NameExists);
        }
        children.insert(name, id);

        self.arena
            .get_mut(id)
            .ok_or(FileTreeError::InvalidNodeId)?
            .parent = Some(new_parent);

        self.invalidate_path_cache_subtree(id);
        Ok(())
    }

    pub fn delete(&mut self, id: NodeId) -> Result<(), FileTreeError> {
        if id == self.root {
            return Err(FileTreeError::InvalidNodeId);
        }

        let (parent, name) = {
            let node = self.arena.get(id).ok_or(FileTreeError::InvalidNodeId)?;
            (node.parent, node.name.clone())
        };

        if let Some(parent_id) = parent {
            if let Some(children) = self
                .arena
                .get_mut(parent_id)
                .and_then(|n| n.children.as_mut())
            {
                children.remove(&name);
            }
        }

        self.recursive_remove(id);
        Ok(())
    }

    fn recursive_remove(&mut self, id: NodeId) {
        if let Some(node) = self.arena.get(id).cloned() {
            if let Some(children) = node.children {
                for (_, child_id) in children {
                    self.recursive_remove(child_id);
                }
            }

            self.expanded.remove(&id);
            if let Some(path) = self.path_cache.remove(&id) {
                self.id_by_path.remove(&path);
            }

            if self.selected == Some(id) {
                self.selected = node.parent;
            }

            self.arena.remove(id);
        }
    }

    pub fn toggle_expand(&mut self, id: NodeId) {
        if self.arena.get(id).is_some_and(|n| n.kind == NodeKind::Dir) {
            if self.expanded.contains(&id) {
                self.expanded.remove(&id);
            } else {
                self.expanded.insert(id);
            }
        }
    }

    pub fn expand(&mut self, id: NodeId) {
        if self.arena.get(id).is_some_and(|n| n.kind == NodeKind::Dir) {
            self.expanded.insert(id);
        }
    }

    pub fn collapse(&mut self, id: NodeId) {
        self.expanded.remove(&id);
    }

    pub fn get_name(&self, id: NodeId) -> Option<&OsString> {
        self.arena.get(id).map(|n| &n.name)
    }

    pub fn is_dir(&self, id: NodeId) -> bool {
        self.arena
            .get(id)
            .map(|n| n.kind == NodeKind::Dir)
            .unwrap_or(false)
    }

    pub fn is_expanded(&self, id: NodeId) -> bool {
        self.expanded.contains(&id)
    }

    pub fn children(&self, id: NodeId) -> Option<impl Iterator<Item = (&OsString, &NodeId)>> {
        self.arena
            .get(id)
            .and_then(|n| n.children.as_ref())
            .map(|c| c.iter())
    }
}

#[derive(Debug, Clone)]
pub struct FileTreeRow {
    pub id: NodeId,
    pub depth: u16,
    pub name: OsString,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub load_state: LoadState,
}

impl FileTree {
    pub fn flatten_for_view(&self) -> Vec<FileTreeRow> {
        let mut result = Vec::new();
        let mut stack: Vec<(NodeId, u16)> = vec![(self.root, 0)];

        while let Some((id, depth)) = stack.pop() {
            if id != self.root {
                if let Some(node) = self.arena.get(id) {
                    result.push(FileTreeRow {
                        id,
                        depth,
                        name: node.name.clone(),
                        is_dir: node.kind == NodeKind::Dir,
                        is_expanded: self.expanded.contains(&id),
                        load_state: node.load_state,
                    });
                }
            }

            if self.expanded.contains(&id) {
                if let Some(node) = self.arena.get(id) {
                    if let Some(children) = &node.children {
                        let mut dirs = Vec::new();
                        let mut files = Vec::new();

                        for (name, &child_id) in children.iter() {
                            if let Some(child) = self.arena.get(child_id) {
                                if child.kind == NodeKind::Dir {
                                    dirs.push((name.clone(), child_id));
                                } else {
                                    files.push((name.clone(), child_id));
                                }
                            }
                        }

                        for (_, file_id) in files.into_iter().rev() {
                            stack.push((file_id, depth + 1));
                        }
                        for (_, dir_id) in dirs.into_iter().rev() {
                            stack.push((dir_id, depth + 1));
                        }
                    }
                }
            }
        }

        result
    }

    pub fn find_node_by_path(&mut self, path: &Path) -> Option<NodeId> {
        if path == self.absolute_root {
            self.id_by_path
                .insert(self.absolute_root.clone(), self.root);
            return Some(self.root);
        }

        if let Some(id) = self.id_by_path.get(path).copied() {
            return Some(id);
        }

        let relative = path.strip_prefix(&self.absolute_root).ok()?;
        let mut current = self.root;

        for component in relative.components() {
            let name = component.as_os_str();
            let children = self.arena.get(current)?.children.as_ref()?;
            current = *children.get(name)?;
        }

        self.path_cache.insert(current, path.to_path_buf());
        self.id_by_path.insert(path.to_path_buf(), current);
        Some(current)
    }
}

pub fn should_ignore(name: &str) -> bool {
    matches!(
        name,
        ".DS_Store"
            | ".Spotlight-V100"
            | ".Trashes"
            | ".fseventsd"
            | ".TemporaryItems"
            | "Thumbs.db"
            | "desktop.ini"
            | ".git"
            | ".claude"
            | "node_modules"
    )
}

fn load_dir_entries(path: &Path) -> io::Result<Vec<(OsString, bool)>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        if should_ignore(&name.to_string_lossy()) {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        entries.push((name, is_dir));
    }
    Ok(entries)
}

pub fn build_file_tree(root_path: &Path) -> io::Result<FileTree> {
    let absolute_root = root_path
        .canonicalize()
        .unwrap_or_else(|_| root_path.to_path_buf());

    let root_name = root_path
        .file_name()
        .or_else(|| root_path.iter().next_back())
        .unwrap_or(root_path.as_os_str())
        .to_os_string();

    let mut tree = FileTree::new_with_root(root_name, absolute_root.clone());

    let entries = load_dir_entries(&absolute_root)?;
    for (name, is_dir) in entries {
        let kind = if is_dir {
            NodeKind::Dir
        } else {
            NodeKind::File
        };
        let _ = tree.insert_child(tree.root, name, kind);
    }

    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tree() {
        let tree = FileTree::new_with_root("test".into(), PathBuf::from("/test"));
        assert!(tree.is_dir(tree.root()));
        assert!(tree.is_expanded(tree.root()));
    }

    #[test]
    fn test_insert_child() {
        let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
        let root = tree.root();

        let file_id = tree
            .insert_child(root, "file.txt".into(), NodeKind::File)
            .unwrap();
        let dir_id = tree
            .insert_child(root, "subdir".into(), NodeKind::Dir)
            .unwrap();

        assert!(!tree.is_dir(file_id));
        assert!(tree.is_dir(dir_id));
    }

    #[test]
    fn test_rename() {
        let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
        let root = tree.root();

        let file_id = tree
            .insert_child(root, "old.txt".into(), NodeKind::File)
            .unwrap();
        tree.rename(file_id, "new.txt".into()).unwrap();

        assert_eq!(tree.get_name(file_id), Some(&OsString::from("new.txt")));
    }

    #[test]
    fn test_delete() {
        let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
        let root = tree.root();

        let file_id = tree
            .insert_child(root, "file.txt".into(), NodeKind::File)
            .unwrap();
        tree.delete(file_id).unwrap();

        assert!(tree.get_name(file_id).is_none());
    }

    #[test]
    fn test_toggle_expand() {
        let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
        let root = tree.root();

        let dir_id = tree
            .insert_child(root, "subdir".into(), NodeKind::Dir)
            .unwrap();

        assert!(!tree.is_expanded(dir_id));
        tree.toggle_expand(dir_id);
        assert!(tree.is_expanded(dir_id));
        tree.toggle_expand(dir_id);
        assert!(!tree.is_expanded(dir_id));
    }

    #[test]
    fn test_flatten_for_view() {
        let mut tree = FileTree::new_with_root("root".into(), PathBuf::from("/root"));
        let root = tree.root();

        tree.insert_child(root, "file1.txt".into(), NodeKind::File)
            .unwrap();
        let dir_id = tree
            .insert_child(root, "subdir".into(), NodeKind::Dir)
            .unwrap();
        tree.insert_child(dir_id, "file2.txt".into(), NodeKind::File)
            .unwrap();

        let rows = tree.flatten_for_view();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].is_dir);

        tree.expand(dir_id);
        let rows = tree.flatten_for_view();
        assert_eq!(rows.len(), 3);
    }
}
