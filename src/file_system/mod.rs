//zcode/src/file_system/mod.rs
use slotmap::{SlotMap, new_key_type};
use std::{
    ffi::OsString,
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    io,
    fmt,
};
use rustc_hash::FxHashSet;
use walkdir::WalkDir;

// 为节点生成稳定的、带代数的 ID（防止悬垂引用）
new_key_type! { pub struct NodeId; }

/// 节点类型
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
}

/// 文件树操作错误
#[derive(Debug)]
pub enum FsTreeError {
    ParentNotDirectory,
    NameExists,
    MoveIntoDescendant,
    InvalidNodeId,
}

impl fmt::Display for FsTreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsTreeError::ParentNotDirectory => write!(f, "parent is not a directory"),
            FsTreeError::NameExists => write!(f, "name already exists in parent"),
            FsTreeError::MoveIntoDescendant => write!(f, "cannot move node into its own subtree"),
            FsTreeError::InvalidNodeId => write!(f, "invalid node id"),
        }
    }
}

impl std::error::Error for FsTreeError {}

/// 树节点：仅存储 basename 和父指针，完整路径按需计算
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    pub name: OsString,                         // 仅文件名，不含路径
    pub parent: Option<NodeId>,                 // 父节点 ID
    pub children: Option<BTreeMap<OsString, NodeId>>, // 仅目录有值，保持有序
}

impl Node {
    fn new_file(name: OsString, parent: Option<NodeId>) -> Self {
        Self {
            kind: Kind::File,
            name,
            parent,
            children: None,
        }
    }

    fn new_dir(name: OsString, parent: Option<NodeId>) -> Self {
        Self {
            kind: Kind::Dir,
            name,
            parent,
            children: Some(BTreeMap::new()),
        }
    }
}

/// 文件树：平坦存储 + 视图状态分离
pub struct FileTree {
    pub arena: SlotMap<NodeId, Node>,           // 节点池
    pub root: NodeId,                           // 根节点 ID
    pub expanded: FxHashSet<NodeId>,            // 展开的目录集合
    pub selected: Option<NodeId>,               // 当前选中的节点
    absolute_root: PathBuf,                     // 真实根路径（规范化后）
    path_cache: HashMap<NodeId, PathBuf>,       // 真正的路径缓存
}

impl FileTree {
    /// 创建一个只有根节点的空树
    fn new_with_root(root_name: OsString, absolute_root: PathBuf) -> Self {
        let mut arena = SlotMap::with_key();
        let root = arena.insert(Node::new_dir(root_name, None));
        
        let mut expanded = FxHashSet::default();
        expanded.insert(root); // 根目录默认展开
        
        Self {
            arena,
            root,
            expanded,
            selected: Some(root),
            absolute_root,
            path_cache: HashMap::new(),
        }
    }

    /// 向指定父节点插入子节点（带重名检查）
    pub fn insert_child(&mut self, parent: NodeId, name: OsString, kind: Kind) -> Result<NodeId, FsTreeError> {
        // 只读预检（不会占用 &mut self.arena）
        {
            let parent_ro = self.arena.get(parent).ok_or(FsTreeError::InvalidNodeId)?;
            let children_ro = parent_ro.children.as_ref().ok_or(FsTreeError::ParentNotDirectory)?;
            if children_ro.contains_key(&name) {
                return Err(FsTreeError::NameExists);
            }
        }

        // 创建节点并放入 arena
        let node = match kind {
            Kind::File => Node::new_file(name.clone(), Some(parent)),
            Kind::Dir => Node::new_dir(name.clone(), Some(parent)),
        };
        let id = self.arena.insert(node);

        // 再次借用父节点（可变），把子节点挂上去
        let parent_node = self.arena.get_mut(parent).ok_or(FsTreeError::InvalidNodeId)?;
        let children = parent_node.children.as_mut().ok_or(FsTreeError::ParentNotDirectory)?;
        children.insert(name, id);

        Ok(id)
    }

    /// 计算节点的完整路径（带缓存）
    pub fn full_path(&self, id: NodeId) -> PathBuf {
        if id == self.root {
            return self.absolute_root.clone();
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

        path
    }

    /// 使子树的路径缓存失效
    fn invalidate_path_cache_subtree(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(node_id) = stack.pop() {
            self.path_cache.remove(&node_id);
            if let Some(node) = self.arena.get(node_id) {
                if let Some(children) = &node.children {
                    for &child_id in children.values() {
                        stack.push(child_id);
                    }
                }
            }
        }
    }
    
    /// 检查 ancestor 是否是 descendant 的祖先
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

    /// 重命名节点（带重名检查）
    pub fn rename(&mut self, id: NodeId, new_name: OsString) -> Result<(), FsTreeError> {
        // 读取阶段：提取节点信息
        let (parent, old_name) = {
            let node = self.arena.get(id).ok_or(FsTreeError::InvalidNodeId)?;
            (node.parent, node.name.clone())
        };
        
        if old_name == new_name {
            return Ok(());
        }
        
        // 写入阶段 1：更新父节点的 children map
        if let Some(parent_id) = parent {
            let parent_node = self.arena.get_mut(parent_id).ok_or(FsTreeError::InvalidNodeId)?;
            let children = parent_node.children.as_mut().ok_or(FsTreeError::ParentNotDirectory)?;
            
            if children.contains_key(&new_name) {
                return Err(FsTreeError::NameExists);
            }
            children.remove(&old_name);
            children.insert(new_name.clone(), id);
        }
        
        // 写入阶段 2：更新节点自身
        self.arena.get_mut(id)
            .ok_or(FsTreeError::InvalidNodeId)?
            .name = new_name;
        
        self.invalidate_path_cache_subtree(id);
        Ok(())
    }
    
    /// 移动节点到新父节点（带循环检查和重名检查）
    pub fn move_to(&mut self, id: NodeId, new_parent: NodeId) -> Result<(), FsTreeError> {
        if self.is_ancestor(id, new_parent) {
            return Err(FsTreeError::MoveIntoDescendant);
        }
        // 读取阶段：提取节点信息
        let (name, old_parent) = {
            let node = self.arena.get(id).ok_or(FsTreeError::InvalidNodeId)?;
            (node.name.clone(), node.parent)
        };
        if old_parent == Some(new_parent) {
            return Ok(());
        }
        // 写入阶段 1：从旧父节点移除
        if let Some(old_parent_id) = old_parent {
            if let Some(children) = self.arena.get_mut(old_parent_id)
                .and_then(|n| n.children.as_mut()) {
                children.remove(&name);
            }
        }
        // 写入阶段 2：添加到新父节点
        let new_parent_node = self.arena.get_mut(new_parent).ok_or(FsTreeError::InvalidNodeId)?;
        let children = new_parent_node.children.as_mut().ok_or(FsTreeError::ParentNotDirectory)?;
        if children.contains_key(&name) {
            return Err(FsTreeError::NameExists);
        }
        children.insert(name, id);
        // 写入阶段 3：更新节点的父指针
        self.arena.get_mut(id)
            .ok_or(FsTreeError::InvalidNodeId)?
            .parent = Some(new_parent);
        self.invalidate_path_cache_subtree(id);
        Ok(())
    }
    
    /// 删除节点及其整个子树
    pub fn delete(&mut self, id: NodeId) -> Result<(), FsTreeError> {
        // 不能删除根节点
        if id == self.root {
            return Err(FsTreeError::InvalidNodeId);
        }
        // 从父节点断开
        let (parent, name) = {
            let node = self.arena.get(id).ok_or(FsTreeError::InvalidNodeId)?;
            (node.parent, node.name.clone())
        };
        if let Some(parent_id) = parent {
            if let Some(children) = self.arena.get_mut(parent_id)
                .and_then(|n| n.children.as_mut()) {
                children.remove(&name);
            }
        }
        // 递归删除整个子树
        self.recursive_remove(id);
        Ok(())
    }
    
    /// 递归删除节点及其子树（内部方法）
    fn recursive_remove(&mut self, id: NodeId) {
        if let Some(node) = self.arena.get(id).cloned() {
            // 先递归删除所有子节点
            if let Some(children) = node.children {
                for (_, child_id) in children {
                    self.recursive_remove(child_id);
                }
            }
            // 清理状态
            self.expanded.remove(&id);
            self.path_cache.remove(&id);
            // 如果 selected 指向这个节点，清空选中
            if self.selected == Some(id) {
                self.selected = node.parent;
            }
            // 从 arena 移除
            self.arena.remove(id);
        }
    }
    
    /// 切换目录的展开/折叠状态
    pub fn toggle_expand(&mut self, id: NodeId) {
        if self.arena.get(id).map_or(false, |n| n.kind == Kind::Dir){
            if self.expanded.contains(&id) {
                self.expanded.remove(&id);
            } else {
                self.expanded.insert(id);
            }
        }
    }

    /// 获取节点名称
    pub fn get_name(&self, id: NodeId) -> Option<&OsString> {
        self.arena.get(id).map(|n| &n.name)
    }

    /// 检查节点是否为目录
    pub fn is_dir(&self, id: NodeId) -> bool {
        self.arena.get(id).map(|n| n.kind == Kind::Dir).unwrap_or(false)
    }

    /// 检查目录是否已展开
    pub fn is_expanded(&self, id: NodeId) -> bool {
        self.expanded.contains(&id)
    }
}

/// 渲染用的扁平化行结构
#[derive(Debug, Clone)]
pub struct Row {
    pub id: NodeId,
    pub depth: u16,
    pub name: OsString,
    pub is_dir: bool,
    pub is_expanded: bool,
}

impl FileTree {
    /// 将树按展开状态拍扁成列表（用于渲染）
    /// 目录优先排序：先显示文件夹，再显示文件
    pub fn flatten_for_view(&self) -> Vec<Row> {
        let mut result = Vec::new();
        let mut stack: Vec<(NodeId, u16)> = vec![(self.root, 0)];
        while let Some((id, depth)) = stack.pop() {
            // 跳过根节点本身（不显示项目根目录）
            if id != self.root {
                if let Some(node) = self.arena.get(id) {
                    result.push(Row {
                        id,
                        depth,
                        name: node.name.clone(),
                        is_dir: node.kind == Kind::Dir,
                        is_expanded: self.expanded.contains(&id),
                    });
                }
            }
            // 如果展开，将子节点按目录优先原则入栈
            if self.expanded.contains(&id) {
                if let Some(node) = self.arena.get(id) {
                    if let Some(children) = &node.children {
                        // 分离目录和文件
                        let mut dirs = Vec::new();
                        let mut files = Vec::new();
                        for (&ref name, &child_id) in children.iter() {
                            if let Some(child) = self.arena.get(child_id) {
                                if child.kind == Kind::Dir {
                                    dirs.push((name.clone(), child_id));
                                } else {
                                    files.push((name.clone(), child_id));
                                }
                            }
                        }
                        // 逆序入栈：先文件，后目录（保证显示时目录在前）
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
}

/// 从磁盘路径构建文件树（O(n) 复杂度，带健壮性保护）
pub fn build_from_path(root_path: &Path) -> io::Result<FileTree> {
    use std::collections::HashMap;
    
    // 规范化根路径（处理符号链接、相对路径等）
    let absolute_root = root_path.canonicalize()
        .unwrap_or_else(|_| root_path.to_path_buf());
    
    // 显示名：处理根路径 file_name() 可能为 None 的情况
    let root_name = root_path
        .file_name()
        .or_else(|| root_path.iter().last())
        .unwrap_or(root_path.as_os_str())
        .to_os_string();
    
    let mut tree = FileTree::new_with_root(root_name, absolute_root.clone());
    
    // 路径字符串 -> NodeId 的映射（用于 O(1) 查找父节点）
    let mut path_to_id: HashMap<PathBuf, NodeId> = HashMap::new();
    path_to_id.insert(absolute_root.clone(), tree.root);
    
    // 先序遍历，按深度构建
    for entry in WalkDir::new(&absolute_root)
        .follow_links(false)  // 不跟随符号链接，防止递归循环
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| {
            // 过滤大目录（提高性能）
            let name = e.file_name().to_string_lossy();
            !(name == "target" || name == "node_modules" || name == ".git")
        }) 
    {
        // 错误处理：权限问题等跳过
        let entry = match entry {
            Ok(e) => e,
            Err(_e) => {
                // TODO: 生产环境可加 tracing::warn!("skip entry: {}", e);
                continue;
            }
        };
        
        let path = entry.path();
        
        // 找到父节点
        let parent_path = path.parent().unwrap_or(&absolute_root);
        let parent_id = path_to_id
            .get(parent_path)
            .copied()
            .unwrap_or(tree.root);
        
        // 提取文件名
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_os_string();
        
        // 确定类型
        let kind = if entry.file_type().is_dir() {
            Kind::Dir
        } else {
            Kind::File
        };
        
        // 插入节点（忽略重名错误，在构建阶段不应出现）
        if let Ok(node_id) = tree.insert_child(parent_id, name, kind) {
            path_to_id.insert(path.to_path_buf(), node_id);
        }
    }
    
    Ok(tree)
}
