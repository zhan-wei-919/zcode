//! 编辑历史管理（Git 模型）
//!
//! 采用类似 Git 的 DAG 结构存储历史：
//! - 每个操作有唯一 ID 和父指针
//! - HEAD 指向当前状态
//! - 历史永不丢失，Undo 后新编辑会创建分支
//! - .ops 文件 append-only，崩溃恢复简单

use super::edit_op::{EditOp, OpId};
use ropey::Rope;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// 刷新到磁盘的间隔（毫秒）
pub const DEFAULT_FLUSH_INTERVAL_MS: u64 = 1000;

/// 累积多少操作后强制刷新
pub const DEFAULT_FLUSH_THRESHOLD: usize = 50;

/// 每隔多少操作创建一个检查点
pub const DEFAULT_CHECKPOINT_INTERVAL: usize = 100;

/// 编辑历史配置
#[derive(Clone)]
pub struct EditHistoryConfig {
    pub flush_interval_ms: u64,
    pub flush_threshold: usize,
    pub checkpoint_interval: usize,
}

impl Default for EditHistoryConfig {
    fn default() -> Self {
        Self {
            flush_interval_ms: DEFAULT_FLUSH_INTERVAL_MS,
            flush_threshold: DEFAULT_FLUSH_THRESHOLD,
            checkpoint_interval: DEFAULT_CHECKPOINT_INTERVAL,
        }
    }
}

/// 检查点：某个操作后的 Rope 快照
struct Checkpoint {
    snapshot: Rope,
}

/// 编辑历史（Git 模型）
pub struct EditHistory {
    /// 基准快照（文件打开时或保存后的状态，对应 root）
    base_snapshot: Rope,
    /// 所有操作（DAG 结构）
    ops: HashMap<OpId, EditOp>,
    /// 当前 HEAD 指向的操作 ID
    head: OpId,
    /// 子节点索引（parent -> children）
    children: HashMap<OpId, Vec<OpId>>,
    /// 检查点缓存（op_id -> snapshot）
    checkpoints: HashMap<OpId, Checkpoint>,
    /// 待写入磁盘的操作
    pending_ops: Vec<EditOp>,
    /// 上次刷新时间
    last_flush: Instant,
    /// 备份文件路径
    ops_file_path: Option<PathBuf>,
    /// 备份文件句柄
    ops_file: Option<File>,
    /// 配置
    config: EditHistoryConfig,
    /// 操作计数（用于决定何时创建检查点）
    op_count: usize,
}

impl EditHistory {
    /// 创建新的编辑历史
    pub fn new(base_snapshot: Rope) -> Self {
        Self {
            base_snapshot,
            ops: HashMap::new(),
            head: OpId::root(),
            children: HashMap::new(),
            checkpoints: HashMap::new(),
            pending_ops: Vec::new(),
            last_flush: Instant::now(),
            ops_file_path: None,
            ops_file: None,
            config: EditHistoryConfig::default(),
            op_count: 0,
        }
    }

    /// 创建带备份文件的编辑历史
    pub fn with_backup(base_snapshot: Rope, ops_file_path: PathBuf) -> std::io::Result<Self> {
        let ops_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ops_file_path)?;

        Ok(Self {
            base_snapshot,
            ops: HashMap::new(),
            head: OpId::root(),
            children: HashMap::new(),
            checkpoints: HashMap::new(),
            pending_ops: Vec::new(),
            last_flush: Instant::now(),
            ops_file_path: Some(ops_file_path),
            ops_file: Some(ops_file),
            config: EditHistoryConfig::default(),
            op_count: 0,
        })
    }

    /// 设置配置
    pub fn with_config(mut self, config: EditHistoryConfig) -> Self {
        self.config = config;
        self
    }

    // ==================== 基础 API（EditorView 使用）====================

    /// 记录新操作
    pub fn push(&mut self, op: EditOp, current_rope: &Rope) {
        let op_id = op.id;

        // 添加到 DAG
        self.ops.insert(op_id, op.clone());
        self.children
            .entry(op.parent)
            .or_insert_with(Vec::new)
            .push(op_id);

        // 更新 HEAD
        self.head = op_id;
        self.op_count += 1;

        // 创建检查点
        if self.op_count % self.config.checkpoint_interval == 0 {
            self.checkpoints.insert(
                op_id,
                Checkpoint {
                    snapshot: current_rope.clone(),
                },
            );
        }

        // 延迟写入磁盘
        self.pending_ops.push(op);
        self.maybe_flush();
    }

    /// Undo：返回恢复后的 Rope 和光标位置
    pub fn undo(&mut self) -> Option<(Rope, (usize, usize))> {
        if self.head.is_root() {
            return None;
        }

        let current_op = self.ops.get(&self.head)?;
        let cursor_pos = current_op.cursor_before();
        let parent_id = current_op.parent;

        // 移动 HEAD 到父节点
        self.head = parent_id;

        // 重建 Rope
        let rope = self.rebuild_rope_at(parent_id);
        Some((rope, cursor_pos))
    }

    /// Redo：沿着最近的分支前进
    pub fn redo(&mut self) -> Option<(Rope, (usize, usize))> {
        // 获取当前节点的子节点
        let children = self.children.get(&self.head)?;
        if children.is_empty() {
            return None;
        }

        // 选择最后一个子节点（最近的操作）
        let next_id = *children.last()?;
        let next_op = self.ops.get(&next_id)?;
        let cursor_pos = next_op.cursor_after();

        // 移动 HEAD
        self.head = next_id;

        // 重建 Rope
        let rope = self.rebuild_rope_at(next_id);
        Some((rope, cursor_pos))
    }

    /// 获取当前 HEAD
    pub fn head(&self) -> OpId {
        self.head
    }

    /// 是否有未保存的修改
    pub fn is_dirty(&self) -> bool {
        !self.head.is_root()
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

    // ==================== Git 风格 API（预留）====================

    /// 获取操作
    pub fn get_op(&self, id: &OpId) -> Option<&EditOp> {
        self.ops.get(id)
    }

    /// 获取父操作 ID
    pub fn parent(&self, id: &OpId) -> Option<OpId> {
        self.ops.get(id).map(|op| op.parent)
    }

    /// 获取子操作 ID 列表
    pub fn children_of(&self, id: &OpId) -> Vec<OpId> {
        self.children.get(id).cloned().unwrap_or_default()
    }

    /// 跳转到任意历史点
    pub fn checkout(&mut self, id: OpId) -> Option<(Rope, (usize, usize))> {
        if id.is_root() {
            self.head = id;
            return Some((self.base_snapshot.clone(), (0, 0)));
        }

        if !self.ops.contains_key(&id) {
            return None;
        }

        self.head = id;
        let rope = self.rebuild_rope_at(id);
        let cursor = self
            .ops
            .get(&id)
            .map(|op| op.cursor_after())
            .unwrap_or((0, 0));
        Some((rope, cursor))
    }

    /// 从 HEAD 回溯历史（类似 git log）
    pub fn log(&self) -> Vec<&EditOp> {
        let mut result = Vec::new();
        let mut current = self.head;

        while !current.is_root() {
            if let Some(op) = self.ops.get(&current) {
                result.push(op);
                current = op.parent;
            } else {
                break;
            }
        }

        result
    }

    /// 获取所有操作（类似 git reflog）
    pub fn reflog(&self) -> impl Iterator<Item = &EditOp> {
        self.ops.values()
    }

    /// 获取分叉点（有多个子节点的操作）
    pub fn branch_points(&self) -> Vec<OpId> {
        self.children
            .iter()
            .filter(|(_, children)| children.len() > 1)
            .map(|(id, _)| *id)
            .collect()
    }

    // ==================== 内部方法 ====================

    /// 从最近的检查点重建指定位置的 Rope
    fn rebuild_rope_at(&self, target: OpId) -> Rope {
        if target.is_root() {
            return self.base_snapshot.clone();
        }

        // 收集从 root 到 target 的路径
        let path = self.path_to(target);

        // 找最近的检查点
        let (start_idx, mut rope) = self.find_nearest_checkpoint(&path);

        // 从检查点重放到目标
        for &op_id in &path[start_idx..] {
            if let Some(op) = self.ops.get(&op_id) {
                op.apply(&mut rope);
            }
        }

        rope
    }

    /// 获取从 root 到指定节点的路径
    fn path_to(&self, target: OpId) -> Vec<OpId> {
        let mut path = Vec::new();
        let mut current = target;

        while !current.is_root() {
            path.push(current);
            if let Some(op) = self.ops.get(&current) {
                current = op.parent;
            } else {
                break;
            }
        }

        path.reverse();
        path
    }

    /// 找最近的检查点，返回 (路径中的起始索引, Rope)
    fn find_nearest_checkpoint(&self, path: &[OpId]) -> (usize, Rope) {
        for (i, &op_id) in path.iter().enumerate().rev() {
            if let Some(cp) = self.checkpoints.get(&op_id) {
                return (i + 1, cp.snapshot.clone());
            }
        }
        (0, self.base_snapshot.clone())
    }

    // ==================== 持久化 ====================

    /// 保存时调用：更新基准快照，但保留历史
    pub fn on_save(&mut self, current_rope: &Rope) {
        self.force_flush();

        // 更新基准快照为当前状态
        // 注意：Git 模型下我们不清空历史，只是标记一个"保存点"
        // 如果需要清空历史，可以调用 clear()
        self.base_snapshot = current_rope.clone();

        // 可选：在 HEAD 处创建检查点
        if !self.head.is_root() {
            self.checkpoints.insert(
                self.head,
                Checkpoint {
                    snapshot: current_rope.clone(),
                },
            );
        }
    }

    /// 清空历史（保存后可选调用）
    pub fn clear(&mut self, current_rope: &Rope) {
        self.base_snapshot = current_rope.clone();
        self.ops.clear();
        self.head = OpId::root();
        self.children.clear();
        self.checkpoints.clear();
        self.op_count = 0;

        // 清空备份文件
        if let Some(path) = &self.ops_file_path {
            if let Ok(file) = File::create(path) {
                self.ops_file = Some(file);
            }
        }
    }

    /// 检查是否需要刷新
    fn maybe_flush(&mut self) {
        let should_flush = self.pending_ops.len() >= self.config.flush_threshold
            || self.last_flush.elapsed() > Duration::from_millis(self.config.flush_interval_ms);

        if should_flush {
            self.flush();
        }
    }

    /// 刷新待写入的操作到磁盘
    fn flush(&mut self) {
        if self.pending_ops.is_empty() {
            return;
        }

        if let Some(f) = &mut self.ops_file {
            for op in &self.pending_ops {
                let _ = writeln!(f, "{}", op.to_json_line());
            }
            // 写入当前 HEAD
            let _ = writeln!(f, "HEAD={}", self.head);
            let _ = f.sync_data();
        }

        self.pending_ops.clear();
        self.last_flush = Instant::now();
    }

    /// 强制刷新
    pub fn force_flush(&mut self) {
        self.flush();
    }

    /// 定时检查是否需要刷新
    pub fn tick(&mut self) {
        if !self.pending_ops.is_empty()
            && self.last_flush.elapsed() > Duration::from_millis(self.config.flush_interval_ms)
        {
            self.flush();
        }
    }

    /// 从备份文件恢复
    pub fn recover(
        base_snapshot: Rope,
        ops_file_path: PathBuf,
    ) -> std::io::Result<(Self, Rope, (usize, usize))> {
        let file = File::open(&ops_file_path)?;
        let reader = BufReader::new(file);

        let mut ops = HashMap::new();
        let mut children: HashMap<OpId, Vec<OpId>> = HashMap::new();
        let mut head = OpId::root();
        let mut last_cursor = (0, 0);

        for line in reader.lines() {
            if let Ok(line) = line {
                if line.starts_with("HEAD=") {
                    // 解析 HEAD 行
                    if let Some(head_str) = line.strip_prefix("HEAD=") {
                        if let Some((ts, cnt)) = head_str.split_once(':') {
                            if let (Ok(timestamp), Ok(counter)) =
                                (u64::from_str_radix(ts, 16), u16::from_str_radix(cnt, 16))
                            {
                                head = OpId { timestamp, counter };
                            }
                        }
                    }
                } else if let Some(op) = EditOp::from_json_line(&line) {
                    last_cursor = op.cursor_after();
                    children
                        .entry(op.parent)
                        .or_insert_with(Vec::new)
                        .push(op.id);
                    ops.insert(op.id, op);
                }
            }
        }

        // 如果没有找到 HEAD 行，使用最后一个操作
        if head.is_root() && !ops.is_empty() {
            // 找到没有子节点的操作作为 HEAD
            for op_id in ops.keys() {
                if !children.values().any(|c| c.contains(op_id)) {
                    head = *op_id;
                    break;
                }
            }
        }

        // 重建 Rope
        let mut history = Self {
            base_snapshot: base_snapshot.clone(),
            ops,
            head,
            children,
            checkpoints: HashMap::new(),
            pending_ops: Vec::new(),
            last_flush: Instant::now(),
            ops_file_path: Some(ops_file_path.clone()),
            ops_file: None,
            config: EditHistoryConfig::default(),
            op_count: 0,
        };

        let rope = history.rebuild_rope_at(head);

        // 重新打开文件用于追加
        let ops_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ops_file_path)?;
        history.ops_file = Some(ops_file);

        Ok((history, rope, last_cursor))
    }

    /// 检查备份文件是否存在且非空
    pub fn has_backup(ops_file_path: &PathBuf) -> bool {
        if let Ok(metadata) = std::fs::metadata(ops_file_path) {
            metadata.len() > 0
        } else {
            false
        }
    }

    /// 清除备份文件
    pub fn clear_backup(ops_file_path: &PathBuf) -> std::io::Result<()> {
        if ops_file_path.exists() {
            std::fs::remove_file(ops_file_path)?;
        }
        Ok(())
    }
}

impl Drop for EditHistory {
    fn drop(&mut self) {
        self.force_flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undo_redo() {
        let base = Rope::from_str("hello");
        let mut history = EditHistory::new(base.clone());

        // 插入 " world"
        let mut rope = base.clone();
        let op = EditOp::insert(history.head(), 5, " world".to_string(), (0, 5), (0, 11));
        op.apply(&mut rope);
        history.push(op, &rope);

        assert!(history.can_undo());
        assert!(!history.can_redo());

        // Undo
        let (undo_rope, cursor) = history.undo().unwrap();
        assert_eq!(undo_rope.to_string(), "hello");
        assert_eq!(cursor, (0, 5));

        assert!(!history.can_undo());
        assert!(history.can_redo());

        // Redo
        let (redo_rope, cursor) = history.redo().unwrap();
        assert_eq!(redo_rope.to_string(), "hello world");
        assert_eq!(cursor, (0, 11));
    }

    #[test]
    fn test_branch_on_edit_after_undo() {
        let base = Rope::from_str("");
        let mut history = EditHistory::new(base.clone());

        // 插入 "a"
        let mut rope = Rope::from_str("a");
        let op_a = EditOp::insert(history.head(), 0, "a".to_string(), (0, 0), (0, 1));
        history.push(op_a.clone(), &rope);

        // 插入 "b"
        rope = Rope::from_str("ab");
        let op_b = EditOp::insert(history.head(), 1, "b".to_string(), (0, 1), (0, 2));
        history.push(op_b.clone(), &rope);

        // Undo 一次（回到 "a"）
        let (_, _) = history.undo().unwrap();

        // 插入 "c"（创建分支）
        rope = Rope::from_str("ac");
        let op_c = EditOp::insert(history.head(), 1, "c".to_string(), (0, 1), (0, 2));
        history.push(op_c.clone(), &rope);

        // 验证分支存在
        let children = history.children_of(&op_a.id);
        assert_eq!(children.len(), 2); // b 和 c 都是 a 的子节点

        // 可以 Redo（会选择最后一个分支，即 c）
        history.undo().unwrap();
        assert!(history.can_redo());
    }

    #[test]
    fn test_log() {
        let base = Rope::from_str("");
        let mut history = EditHistory::new(base.clone());

        // 插入 "a", "b", "c"
        let mut rope = Rope::from_str("a");
        let op_a = EditOp::insert(history.head(), 0, "a".to_string(), (0, 0), (0, 1));
        history.push(op_a, &rope);

        rope = Rope::from_str("ab");
        let op_b = EditOp::insert(history.head(), 1, "b".to_string(), (0, 1), (0, 2));
        history.push(op_b, &rope);

        rope = Rope::from_str("abc");
        let op_c = EditOp::insert(history.head(), 2, "c".to_string(), (0, 2), (0, 3));
        history.push(op_c, &rope);

        // log 应该返回 c, b, a
        let log = history.log();
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn test_checkout() {
        let base = Rope::from_str("");
        let mut history = EditHistory::new(base.clone());

        // 插入 "a"
        let mut rope = Rope::from_str("a");
        let op_a = EditOp::insert(history.head(), 0, "a".to_string(), (0, 0), (0, 1));
        let op_a_id = op_a.id;
        history.push(op_a, &rope);

        // 插入 "b"
        rope = Rope::from_str("ab");
        let op_b = EditOp::insert(history.head(), 1, "b".to_string(), (0, 1), (0, 2));
        history.push(op_b, &rope);

        // checkout 到 op_a
        let (checkout_rope, _) = history.checkout(op_a_id).unwrap();
        assert_eq!(checkout_rope.to_string(), "a");
        assert_eq!(history.head(), op_a_id);
    }

    #[test]
    fn test_is_dirty() {
        let base = Rope::from_str("hello");
        let mut history = EditHistory::new(base.clone());

        assert!(!history.is_dirty());

        let rope = Rope::from_str("hello world");
        let op = EditOp::insert(history.head(), 5, " world".to_string(), (0, 5), (0, 11));
        history.push(op, &rope);

        assert!(history.is_dirty());
    }
}
