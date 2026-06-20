//! LSP / 文件监听的同步台账：去抖计时、已保存版本、已打开路径集合。

use rustc_hash::{FxHashMap, FxHashSet};
use std::path::PathBuf;
use std::time::Instant;

/// 三类按需 LSP 请求的去抖截止时间。到点后由 tick 触发一次请求。
#[derive(Debug, Default)]
pub(in crate::app::workbench) struct LspDebounceState {
    pub(in crate::app::workbench) inlay_hints: Option<Instant>,
    pub(in crate::app::workbench) folding_range: Option<Instant>,
}

/// LSP 与文件监听共享的同步状态。记录已打开路径，避免对同一路径重复 open/close。
#[derive(Debug, Default)]
pub(in crate::app::workbench) struct LspSyncState {
    pub(in crate::app::workbench) debounce: LspDebounceState,
    pub(in crate::app::workbench) file_save_versions: FxHashMap<(usize, PathBuf), u64>,
    pub(in crate::app::workbench) open_paths: FxHashSet<PathBuf>,
    pub(in crate::app::workbench) open_paths_version: u64,
    pub(in crate::app::workbench) file_watcher_open_paths_version: u64,
}
