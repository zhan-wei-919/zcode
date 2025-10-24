# 文件系统架构设计方案

## 1. 设计目标

### 1.1 核心原则
- **抽象解耦**：文件树逻辑与具体文件系统实现分离
- **懒写入**：操作立即在内存生效，批量提交到磁盘
- **事务性**：支持 commit/rollback，保证数据一致性
- **可扩展**：支持本地、远程、内存等多种文件系统后端

### 1.2 对比 VSCode 架构
| 特性 | VSCode | 我们的设计 |
|------|--------|-----------|
| **抽象层** | FileSystemProvider | ✅ 同样的 Provider 模式 |
| **URI 寻址** | `file://`, `ssh://` | ✅ Path + Provider |
| **懒加载** | 展开时才读取 | ⚠️ 暂未实现（Phase 2） |
| **文件监控** | watch API | ✅ Provider 接口支持 |
| **事务性** | 每次操作立即写入 | ✅ 批量提交（更优） |

---

## 2. 架构分层

```
┌─────────────────────────────────────────────────────┐
│                   用户交互层                         │
│         (FileTree API: rename, move, delete)        │
├─────────────────────────────────────────────────────┤
│                  操作日志层                          │
│   PendingOperation Queue + Dirty Tracking          │
│   - rename/move/delete 先入队，不立即执行            │
│   - commit() 时批量写入                              │
│   - rollback() 时丢弃队列                            │
├─────────────────────────────────────────────────────┤
│              FileSystemProvider 抽象层               │
│   Trait: stat, read_dir, rename, delete, watch...  │
├──────────────┬──────────────┬───────────────────────┤
│  LocalFS     │  MemoryFS    │  RemoteFS (未来)      │
│  (std::fs)   │  (测试用)     │  (SSH/HTTP)          │
└──────────────┴──────────────┴───────────────────────┘
```

---

## 3. 核心组件设计

### 3.1 FileSystemProvider Trait

```rust
pub trait FileSystemProvider: Send + Sync {
    // 查询操作（只读）
    fn stat(&self, uri: &Path) -> io::Result<FileMetadata>;
    fn read_dir(&self, uri: &Path) -> io::Result<Vec<DirEntry>>;
    fn read_file(&self, uri: &Path) -> io::Result<Vec<u8>>;
    
    // 修改操作（写入）
    fn write_file(&self, uri: &Path, data: &[u8]) -> io::Result<()>;
    fn create_dir(&self, uri: &Path) -> io::Result<()>;
    fn rename(&self, old: &Path, new: &Path) -> io::Result<()>;
    fn delete_file(&self, uri: &Path) -> io::Result<()>;
    fn delete_dir(&self, uri: &Path) -> io::Result<()>;
    
    // 监控（可选）
    fn watch(&self, uri: &Path) -> io::Result<Option<Box<dyn FileWatcher>>>;
}
```

**设计要点**：
- 类似 Linux VFS，所有操作通过统一接口
- `Send + Sync`：支持多线程环境
- 返回 `io::Result`：统一错误处理

---

### 3.2 懒写入机制

#### 3.2.1 操作日志

```rust
pub enum PendingOperation {
    Rename { 
        id: NodeId, 
        old_path: PathBuf,  // 用于回滚/提交
        new_name: OsString 
    },
    Move { 
        id: NodeId, 
        old_parent: NodeId, 
        new_parent: NodeId,
        old_path: PathBuf,
    },
    Delete { 
        id: NodeId, 
        path: PathBuf,
    },
    CreateFile { parent: NodeId, name: OsString },
    CreateDir { parent: NodeId, name: OsString },
}
```

#### 3.2.2 执行流程

```
用户调用 rename()
    ↓
1. 检查合法性（重名、权限等）
    ↓
2. 立即修改内存树（用户立刻看到效果）
    ↓
3. 将操作加入 pending_ops 队列
    ↓
4. 标记节点为 dirty
    ↓
... 用户继续其他操作 ...
    ↓
用户调用 commit()
    ↓
5. 遍历 pending_ops，调用 fs_provider
    ↓
6. 磁盘操作成功 → 清空队列
   磁盘操作失败 → 返回错误（可选回滚）
```

**关键优势**：
- **性能**：减少 I/O 次数（批量操作）
- **体验**：UI 立即响应，后台提交
- **事务性**：支持 undo/redo

---

### 3.3 FileTree 结构

```rust
pub struct FileTree {
    // 核心数据（内存树）
    pub arena: SlotMap<NodeId, Node>,
    pub root: NodeId,
    pub expanded: FxHashSet<NodeId>,
    pub selected: Option<NodeId>,
    
    // 路径管理
    absolute_root: PathBuf,
    path_cache: HashMap<NodeId, PathBuf>,
    
    // 懒写入
    pending_ops: VecDeque<PendingOperation>,
    dirty_nodes: FxHashSet<NodeId>,
    
    // ✅ Provider 抽象
    fs_provider: Box<dyn FileSystemProvider>,
}
```

---

## 4. 实现计划

### Phase 1: Provider 抽象层 ✅
- [x] 定义 `FileSystemProvider` trait
- [ ] 实现 `LocalFileSystemProvider`
- [ ] 实现 `MemoryFileSystemProvider`（测试用）
- [ ] 重构 `FileTree` 使用 Provider

**工作量**：约 500 行代码，1-2 天

---

### Phase 2: 懒写入机制 ✅
- [ ] 定义 `PendingOperation` 枚举
- [ ] 重构 `rename/move/delete` 为懒操作
- [ ] 实现 `commit()` 批量提交
- [ ] 实现 `rollback()` 回滚机制
- [ ] 添加 `is_dirty()` / `dirty_count()` 查询

**工作量**：约 300 行代码，1 天

---

### Phase 3: 文件监控（可选）
- [ ] 集成 `notify` crate
- [ ] 实现 `FileWatcher` trait
- [ ] 在 `LocalFileSystemProvider` 中实现 `watch()`
- [ ] 自动同步外部文件变化

**工作量**：约 400 行代码，2 天

---

### Phase 4: 高级特性（未来）
- [ ] 懒加载：展开时才读取子目录
- [ ] 虚拟滚动：只渲染可见区域
- [ ] 远程文件系统：SSH/HTTP Provider
- [ ] 加密文件系统：透明加解密

---

## 5. 关键设计决策

### 5.1 为什么用 Provider 模式？

**问题**：直接调用 `std::fs` 的缺陷
```rust
// ❌ 耦合具体实现
pub fn rename(&mut self, id: NodeId, new_name: OsString) -> Result<()> {
    std::fs::rename(old_path, new_path)?;  // 无法测试、无法扩展
}
```

**解决**：Provider 抽象
```rust
// ✅ 解耦
pub fn rename(&mut self, id: NodeId, new_name: OsString) -> Result<()> {
    self.fs_provider.rename(old_path, new_path)?;  // 可替换实现
}
```

**收益**：
- 单元测试无需真实磁盘（用 MemoryFS）
- 支持远程文件系统（SSH、HTTP）
- 支持虚拟文件系统（加密、压缩）

---

### 5.2 为什么要懒写入？

**场景**：用户批量重命名 100 个文件
```rust
// ❌ 传统方式：100 次磁盘 I/O
for file in files {
    tree.rename(file, new_name)?;  // 每次立即写磁盘
}

// ✅ 懒写入：1 次批量 I/O
for file in files {
    tree.rename(file, new_name)?;  // 仅修改内存
}
tree.commit()?;  // 批量提交
```

**性能对比**：
- 传统方式：100 次 `rename()` 系统调用 ≈ 100-500ms
- 懒写入：1 次批量提交 ≈ 10-50ms

**额外收益**：
- 支持事务（commit/rollback）
- 支持 undo/redo（保存操作日志）
- UI 响应更快（立即显示结果）

---

### 5.3 为什么不用路径缓存？

**你删掉的代码是对的**：
```rust
// ❌ 伪优化
path_cache: HashMap<NodeId, PathBuf>,

pub fn full_path(&mut self, id: NodeId) -> PathBuf {
    if let Some(cached) = self.path_cache.get(&id) {
        return cached.clone();
    }
    // 计算路径...
    self.path_cache.insert(id, path.clone());
    path
}
```

**问题**：
1. **缓存命中率低**：大部分节点只访问一次
2. **失效逻辑复杂**：rename/move 后需递归失效子树
3. **内存浪费**：PathBuf 占用 ≈ 100 字节/节点

**正确做法**：
```rust
// ✅ 直接计算（O(深度) ≈ O(log n)）
pub fn full_path(&self, id: NodeId) -> PathBuf {
    let mut components = vec![];
    let mut current = id;
    while let Some(node) = self.arena.get(current) {
        if let Some(parent) = node.parent {
            components.push(&node.name);
            current = parent;
        } else { break; }
    }
    components.reverse();
    self.absolute_root.join(components)
}
```

**理由**：
- 深度通常 < 10，计算开销 < 1μs
- 不需要失效逻辑，永远正确
- 节省 30% 内存

---

## 6. 错误处理策略

### 6.1 错误分类

```rust
#[derive(Debug)]
pub enum FsTreeError {
    // 树结构错误
    InvalidNodeId,
    ParentNotDirectory,
    NameExists,
    MoveIntoDescendant,
    
    // 文件系统错误
    IoError(io::Error),
    PermissionDenied,
    PathNotFound,
    DiskFull,
}

impl From<io::Error> for FsTreeError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            io::ErrorKind::NotFound => Self::PathNotFound,
            io::ErrorKind::AlreadyExists => Self::NameExists,
            _ => Self::IoError(err),
        }
    }
}
```

### 6.2 提交失败处理

```rust
pub fn commit(&mut self) -> Result<(), FsTreeError> {
    for op in &self.pending_ops {
        // 方案 A：遇错即停（推荐）
        self.execute_op(op)?;  // 失败时队列保留，可重试
        
        // 方案 B：记录错误继续
        // if let Err(e) = self.execute_op(op) {
        //     errors.push((op.clone(), e));
        // }
    }
    
    self.pending_ops.clear();
    self.dirty_nodes.clear();
    Ok(())
}
```

**选择方案 A**：
- 简单可靠：失败时状态清晰
- 用户可重试：`commit()` 失败后再次调用
- 一致性强：要么全成功，要么保持原样

---

## 7. 测试策略

### 7.1 单元测试（MemoryFS）

```rust
#[test]
fn test_lazy_rename() {
    let fs = Box::new(MemoryFileSystemProvider::new());
    let mut tree = FileTree::new_with_provider(
        "test".into(),
        PathBuf::from("/test"),
        fs,
    );
    
    let file = tree.create_file(tree.root, "old.txt".into()).unwrap();
    
    // 重命名（仅内存）
    tree.rename(file, "new.txt".into()).unwrap();
    assert_eq!(tree.get_name(file), Some(&"new.txt".into()));
    
    // 提交前，磁盘未变
    assert!(tree.fs_provider.stat("/test/new.txt").is_err());
    
    // 提交后，磁盘已变
    tree.commit().unwrap();
    assert!(tree.fs_provider.stat("/test/new.txt").is_ok());
}
```

### 7.2 集成测试（LocalFS）

```rust
#[test]
fn test_real_filesystem() {
    let tmpdir = tempfile::tempdir().unwrap();
    let mut tree = FileTree::new_with_root(
        "tmp".into(),
        tmpdir.path().to_path_buf(),
    );
    
    // 创建文件
    let file = tree.create_file(tree.root, "test.txt".into()).unwrap();
    tree.commit().unwrap();
    
    // 验证真实文件存在
    assert!(tmpdir.path().join("test.txt").exists());
}
```

---

## 8. 性能优化方向

### 8.1 当前性能特性
| 操作 | 复杂度 | 说明 |
|------|--------|------|
| **rename** | O(1) | 仅修改 BTreeMap + 入队 |
| **move** | O(深度) | is_ancestor 检查 |
| **delete** | O(1) | 仅入队，实际删除在 commit |
| **commit** | O(n) | n = 待执行操作数 |
| **full_path** | O(深度) | 沿父指针向上 |

### 8.2 未来优化（Phase 3+）

#### 懒加载
```rust
pub struct FileTree {
    loaded_dirs: FxHashSet<NodeId>,  // 已加载的目录
}

pub fn expand(&mut self, dir_id: NodeId) -> Result<()> {
    if self.loaded_dirs.contains(&dir_id) {
        self.expanded.insert(dir_id);
        return Ok(());
    }
    
    // 首次展开才读取子目录
    let path = self.full_path(dir_id);
    let entries = self.fs_provider.read_dir(&path)?;
    
    for entry in entries {
        self.insert_child(dir_id, entry.name, ...)?;
    }
    
    self.loaded_dirs.insert(dir_id);
    self.expanded.insert(dir_id);
    Ok(())
}
```

#### 虚拟滚动
```rust
pub fn visible_rows(&self, scroll: usize, height: usize) -> &[Row] {
    let all_rows = self.flatten_for_view();
    &all_rows[scroll..scroll+height]  // 仅返回可见行
}
```

---

## 9. 开放问题

### 9.1 自动提交策略？
- **选项 A**：手动提交（完全控制）
- **选项 B**：达到阈值自动提交（透明，但可能在不合适时机）
- **选项 C**：空闲时延迟提交（类似 VSCode 的 auto-save）

**建议**：Phase 1 用选项 A，Phase 2 可选支持 C

---

### 9.2 回滚粒度？
- **选项 A**：全部回滚（简单）
- **选项 B**：撤销单个操作（复杂，需要反向操作）

**建议**：Phase 1 用选项 A，Phase 3+ 支持 B（实现 undo/redo）

---

### 9.3 文件监控冲突？
**场景**：
1. 用户在编辑器中重命名 `a.txt` → `b.txt`（pending）
2. 外部进程也修改了 `a.txt`
3. 用户 `commit()` 时，`a.txt` 已不存在

**解决**：
```rust
pub fn commit(&mut self) -> Result<(), FsTreeError> {
    for op in &self.pending_ops {
        // 提交前重新检查状态
        if let PendingOperation::Rename { old_path, .. } = op {
            if !old_path.exists() {
                return Err(FsTreeError::PathNotFound);
            }
        }
        self.execute_op(op)?;
    }
}
```

---

## 10. 总结

### 优势
✅ **工程级架构**：Provider 抽象 + 懒写入  
✅ **高性能**：批量 I/O，减少系统调用  
✅ **可测试**：MemoryFS mock，无需真实磁盘  
✅ **可扩展**：支持远程、虚拟文件系统  
✅ **事务性**：commit/rollback 保证一致性  

### 风险
⚠️ **复杂度增加**：需要维护操作队列  
⚠️ **内存占用**：pending_ops 可能积累大量操作  
⚠️ **并发冲突**：需处理外部修改与 pending 操作的冲突  

### 下一步
1. **Review 这份设计**：确认架构方向
2. **实现 Phase 1**：Provider 抽象层
3. **实现 Phase 2**：懒写入机制
4. **集成到编辑器**：UI 层调用 commit()

---

## 附录：VSCode 源码参考

### FileSystemProvider 接口
```typescript
// vscode.d.ts
export interface FileSystemProvider {
  stat(uri: Uri): FileStat | Thenable<FileStat>;
  readDirectory(uri: Uri): [string, FileType][] | Thenable<[string, FileType][]>;
  createDirectory(uri: Uri): void | Thenable<void>;
  readFile(uri: Uri): Uint8Array | Thenable<Uint8Array>;
  writeFile(uri: Uri, content: Uint8Array, options: { create: boolean, overwrite: boolean }): void | Thenable<void>;
  delete(uri: Uri, options: { recursive: boolean }): void | Thenable<void>;
  rename(oldUri: Uri, newUri: Uri, options: { overwrite: boolean }): void | Thenable<void>;
  watch(uri: Uri, options: { recursive: boolean, excludes: string[] }): Disposable;
}
```

### 关键差异
| 特性 | VSCode | 我们的设计 |
|------|--------|-----------|
| **异步** | Promise/Thenable | io::Result（同步，Phase 3 可改 async） |
| **URI** | `Uri` 对象 | `Path`（更简单） |
| **选项** | `{ create, overwrite }` | 隐式处理（更简洁） |
| **Watch** | 返回 `Disposable` | 返回 `FileWatcher` trait |

我们的设计更简洁，适合当前阶段；未来可逐步向 VSCode 靠拢。
