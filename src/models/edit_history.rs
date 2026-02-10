//! 编辑历史管理
//!
//! 采用 DAG 结构存储历史：
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
use tracing::{error, warn};

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

/// 编辑历史
pub struct EditHistory {
    base_snapshot: Rope,
    ops: HashMap<OpId, EditOp>,
    head: OpId,
    saved_head: OpId,
    children: HashMap<OpId, Vec<OpId>>,
    preferred_child: HashMap<OpId, OpId>,
    checkpoints: HashMap<OpId, Checkpoint>,
    pending_ops: Vec<EditOp>,
    last_flush: Instant,
    last_persisted_head: OpId,
    ops_file_path: Option<PathBuf>,
    ops_file: Option<File>,
    config: EditHistoryConfig,
    op_count: usize,
}

impl EditHistory {
    /// 创建新的编辑历史
    pub fn new(base_snapshot: Rope) -> Self {
        Self {
            base_snapshot,
            ops: HashMap::new(),
            head: OpId::root(),
            saved_head: OpId::root(),
            children: HashMap::new(),
            preferred_child: HashMap::new(),
            checkpoints: HashMap::new(),
            pending_ops: Vec::new(),
            last_flush: Instant::now(),
            last_persisted_head: OpId::root(),
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
            saved_head: OpId::root(),
            children: HashMap::new(),
            preferred_child: HashMap::new(),
            checkpoints: HashMap::new(),
            pending_ops: Vec::new(),
            last_flush: Instant::now(),
            last_persisted_head: OpId::root(),
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
        let parent_id = op.parent;

        // 添加到 DAG
        self.ops.insert(op_id, op.clone());
        self.children.entry(parent_id).or_default().push(op_id);
        self.preferred_child.insert(parent_id, op_id);

        // 更新 HEAD
        self.head = op_id;
        self.op_count += 1;

        // 创建检查点
        if self.config.checkpoint_interval != 0
            && self
                .op_count
                .is_multiple_of(self.config.checkpoint_interval)
        {
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
    pub fn undo(&mut self, current_rope: &Rope) -> Option<(Rope, (usize, usize))> {
        let current_id = self.head;
        if current_id.is_root() {
            return None;
        }

        let current_op = self.ops.get(&current_id)?;
        let cursor_pos = current_op.cursor_before();
        let parent_id = current_op.parent;

        self.preferred_child.insert(parent_id, current_id);

        let mut rope = current_rope.clone();
        current_op.inverse().apply(&mut rope);

        // 移动 HEAD 到父节点
        self.head = parent_id;
        self.maybe_flush();

        Some((rope, cursor_pos))
    }

    /// Redo：沿着最近的分支前进
    pub fn redo(&mut self, current_rope: &Rope) -> Option<(Rope, (usize, usize))> {
        // 获取当前节点的子节点
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

        let mut rope = current_rope.clone();
        next_op.apply(&mut rope);

        // 移动 HEAD
        self.head = next_id;
        self.preferred_child.insert(head_id, next_id);
        self.maybe_flush();

        Some((rope, cursor_pos))
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

    // ==================== 预留 ====================

    pub fn get_op(&self, id: &OpId) -> Option<&EditOp> {
        self.ops.get(id)
    }

    pub fn parent(&self, id: &OpId) -> Option<OpId> {
        self.ops.get(id).map(|op| op.parent)
    }

    pub fn children_of(&self, id: &OpId) -> Vec<OpId> {
        self.children.get(id).cloned().unwrap_or_default()
    }

    pub fn checkout(&mut self, id: OpId) -> Option<(Rope, (usize, usize))> {
        if id.is_root() {
            self.head = id;
            self.maybe_flush();
            return Some((self.base_snapshot.clone(), (0, 0)));
        }

        if !self.ops.contains_key(&id) {
            return None;
        }

        self.head = id;
        self.maybe_flush();
        let rope = self.rebuild_rope_at(id);
        let cursor = self
            .ops
            .get(&id)
            .map(|op| op.cursor_after())
            .unwrap_or((0, 0));
        Some((rope, cursor))
    }

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

    pub fn reflog(&self) -> impl Iterator<Item = &EditOp> {
        self.ops.values()
    }

    pub fn branch_points(&self) -> Vec<OpId> {
        self.children
            .iter()
            .filter(|(_, children)| children.len() > 1)
            .map(|(id, _)| *id)
            .collect()
    }

    // ==================== 内部方法 ====================

    fn rebuild_rope_at(&self, target: OpId) -> Rope {
        if target.is_root() {
            return self.base_snapshot.clone();
        }
        let path = self.path_to(target);
        let (start_idx, mut rope) = self.find_nearest_checkpoint(&path);
        for &op_id in &path[start_idx..] {
            if let Some(op) = self.ops.get(&op_id) {
                op.apply(&mut rope);
            }
        }

        rope
    }

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

    fn find_nearest_checkpoint(&self, path: &[OpId]) -> (usize, Rope) {
        for (i, &op_id) in path.iter().enumerate().rev() {
            if let Some(cp) = self.checkpoints.get(&op_id) {
                return (i + 1, cp.snapshot.clone());
            }
        }
        (0, self.base_snapshot.clone())
    }

    // ==================== 持久化 ====================

    pub fn on_save(&mut self, current_rope: &Rope) {
        self.force_flush();
        self.saved_head = self.head;
        if self.head.is_root() {
            return;
        }
        if !self.checkpoints.contains_key(&self.head) {
            self.checkpoints.insert(
                self.head,
                Checkpoint {
                    snapshot: current_rope.clone(),
                },
            );
        }
    }

    pub fn clear(&mut self, current_rope: &Rope) {
        self.base_snapshot = current_rope.clone();
        self.saved_head = OpId::root();
        self.ops.clear();
        self.head = OpId::root();
        self.children.clear();
        self.preferred_child.clear();
        self.checkpoints.clear();
        self.pending_ops.clear();
        self.last_persisted_head = OpId::root();
        self.op_count = 0;

        if let Some(path) = &self.ops_file_path {
            match File::create(path) {
                Ok(file) => {
                    self.ops_file = Some(file);
                }
                Err(e) => {
                    self.ops_file = None;
                    error!(error = %e, path = %path.display(), "failed to recreate ops file");
                }
            }
        }
    }

    fn maybe_flush(&mut self) {
        let interval = Duration::from_millis(self.config.flush_interval_ms);
        let elapsed = self.last_flush.elapsed() > interval;
        let should_flush = self.pending_ops.len() >= self.config.flush_threshold
            || (elapsed && (!self.pending_ops.is_empty() || self.head != self.last_persisted_head));

        if should_flush {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if self.pending_ops.is_empty() && self.head == self.last_persisted_head {
            return;
        }

        if self.ops_file.is_none() {
            self.pending_ops.clear();
            self.last_persisted_head = self.head;
            self.last_flush = Instant::now();
            return;
        }

        let mut written_ops = 0usize;
        let mut had_error = false;
        let mut disable_persistence = false;

        {
            let Some(f) = self.ops_file.as_mut() else {
                self.pending_ops.clear();
                self.last_persisted_head = self.head;
                self.last_flush = Instant::now();
                return;
            };

            while written_ops < self.pending_ops.len() {
                let line = match self.pending_ops[written_ops].to_json_line() {
                    Ok(line) => line,
                    Err(e) => {
                        error!(error = %e, "serialize edit op failed; disabling history persistence");
                        disable_persistence = true;
                        had_error = true;
                        break;
                    }
                };

                if let Err(e) = writeln!(f, "{line}") {
                    error!(error = %e, "write edit op failed");
                    had_error = true;
                    break;
                }

                written_ops += 1;
            }

            if !had_error && self.head != self.last_persisted_head {
                if let Err(e) = writeln!(f, "HEAD={}", self.head) {
                    error!(error = %e, "write edit history head failed");
                    had_error = true;
                }
            }

            if !had_error {
                if let Err(e) = f.sync_data() {
                    warn!(error = %e, "sync edit history failed");
                }
            }
        }

        if written_ops > 0 {
            self.pending_ops.drain(..written_ops);
        }

        if had_error {
            self.last_flush = Instant::now();
            if disable_persistence {
                self.ops_file = None;
                self.pending_ops.clear();
                self.last_persisted_head = self.head;
            }
            return;
        }

        self.pending_ops.clear();
        self.last_persisted_head = self.head;
        self.last_flush = Instant::now();
    }

    pub fn force_flush(&mut self) {
        self.flush();
    }

    pub fn tick(&mut self) {
        if (self.head != self.last_persisted_head || !self.pending_ops.is_empty())
            && self.last_flush.elapsed() > Duration::from_millis(self.config.flush_interval_ms)
        {
            self.flush();
        }
    }

    pub fn recover(
        base_snapshot: Rope,
        ops_file_path: PathBuf,
    ) -> std::io::Result<(Self, Rope, (usize, usize))> {
        let file = File::open(&ops_file_path)?;
        let reader = BufReader::new(file);
        let mut ops = HashMap::new();
        let mut children: HashMap<OpId, Vec<OpId>> = HashMap::new();
        let mut preferred_child: HashMap<OpId, OpId> = HashMap::new();
        let mut head = OpId::root();
        let mut last_cursor = (0, 0);
        for line in reader.lines() {
            let line = line?;
            if line.starts_with("HEAD=") {
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
                let op_id = op.id;
                let parent = op.parent;
                let is_new = ops.insert(op_id, op).is_none();
                if is_new {
                    children.entry(parent).or_default().push(op_id);
                }
                preferred_child.insert(parent, op_id);
            }
        }
        let op_count = ops.len();
        if head.is_root() && !ops.is_empty() {
            let mut best: Option<OpId> = None;
            for &op_id in ops.keys() {
                if children.contains_key(&op_id) {
                    continue;
                }
                best = match best {
                    None => Some(op_id),
                    Some(prev) => Some(max_op_id(prev, op_id)),
                };
            }
            if let Some(best) = best {
                head = best;
            }
        }
        let mut history = Self {
            base_snapshot,
            ops,
            head,
            saved_head: OpId::root(),
            children,
            checkpoints: HashMap::new(),
            pending_ops: Vec::new(),
            last_flush: Instant::now(),
            last_persisted_head: head,
            ops_file_path: Some(ops_file_path.clone()),
            ops_file: None,
            config: EditHistoryConfig::default(),
            op_count,
            preferred_child,
        };
        let rope = history.rebuild_rope_at(head);
        let cursor = if head.is_root() {
            (0, 0)
        } else {
            history
                .ops
                .get(&head)
                .map(|op| op.cursor_after())
                .unwrap_or(last_cursor)
        };
        let ops_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ops_file_path)?;
        history.ops_file = Some(ops_file);

        Ok((history, rope, cursor))
    }

    pub fn has_backup(ops_file_path: &PathBuf) -> bool {
        if let Ok(metadata) = std::fs::metadata(ops_file_path) {
            metadata.len() > 0
        } else {
            false
        }
    }

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

fn max_op_id(a: OpId, b: OpId) -> OpId {
    if (a.timestamp, a.counter) >= (b.timestamp, b.counter) {
        a
    } else {
        b
    }
}

#[cfg(test)]
#[path = "../../tests/unit/models/edit_history.rs"]
mod tests;
