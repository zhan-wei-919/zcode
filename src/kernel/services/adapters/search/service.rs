//! 单文件搜索服务
//!
//! 用于编辑器内搜索（Rope 已存在于内存中）

use super::searcher::{search_regex_in_slice, RopeReader, SearchConfig, StreamSearcher};
use crate::core::Service;
use crate::kernel::services::ports::search::{Match, Result as SearchResult, SearchMessage};
use ropey::Rope;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;

static SEARCH_ID: AtomicU64 = AtomicU64::new(0);

fn next_search_id() -> u64 {
    SEARCH_ID.fetch_add(1, Ordering::Relaxed)
}

pub struct SearchTask {
    id: u64,
    cancelled: Arc<AtomicBool>,
}

impl SearchTask {
    pub fn new() -> Self {
        Self {
            id: next_search_id(),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn cancelled_flag(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }
}

impl Default for SearchTask {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SearchService {
    runtime: tokio::runtime::Handle,
}

impl SearchService {
    pub fn new(runtime: tokio::runtime::Handle) -> Self {
        Self { runtime }
    }

    /// 在 Rope 中异步搜索
    pub fn search_in_rope(
        &self,
        rope: Rope,
        pattern: String,
        case_sensitive: bool,
        is_regex: bool,
        tx: SyncSender<SearchMessage>,
    ) -> SearchTask {
        let task = SearchTask::new();
        let search_id = task.id();
        let cancelled = task.cancelled_flag();
        let cancelled_for_check = cancelled.clone();
        let tx_for_complete = tx.clone();

        self.runtime.spawn(async move {
            if pattern.is_empty() {
                let _ = tx_for_complete.send(SearchMessage::Complete {
                    search_id,
                    total: 0,
                });
                return;
            }

            // 编译搜索配置
            let config = if is_regex {
                match SearchConfig::regex(&pattern, case_sensitive) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx_for_complete.send(SearchMessage::Error {
                            search_id,
                            message: format!("Invalid regex: {}", e),
                        });
                        return;
                    }
                }
            } else {
                SearchConfig::literal(&pattern, case_sensitive)
            };

            let result = tokio::task::spawn_blocking(move || {
                search_rope_sync(&rope, &config, search_id, &cancelled, &tx)
            })
            .await;

            match result {
                Ok(total) => {
                    if !cancelled_for_check.load(Ordering::Relaxed) {
                        let _ = tx_for_complete.send(SearchMessage::Complete { search_id, total });
                    }
                }
                Err(e) => {
                    if cancelled_for_check.load(Ordering::Relaxed) {
                        let _ = tx_for_complete.send(SearchMessage::Cancelled { search_id });
                    } else {
                        let _ = tx_for_complete.send(SearchMessage::Error {
                            search_id,
                            message: format!("Search task failed: {}", e),
                        });
                    }
                }
            }
        });

        task
    }

    /// 同步搜索（用于小文件或需要立即结果的场景）
    pub fn search_sync(
        rope: &Rope,
        pattern: &str,
        case_sensitive: bool,
        is_regex: bool,
    ) -> SearchResult<Vec<Match>> {
        if pattern.is_empty() {
            return Ok(Vec::new());
        }

        let config = if is_regex {
            SearchConfig::regex(pattern, case_sensitive)?
        } else {
            SearchConfig::literal(pattern, case_sensitive)
        };

        match &config {
            SearchConfig::Literal { .. } => {
                let reader = RopeReader::new(rope);
                StreamSearcher::new(reader, &config)
                    .search()
                    .map_err(Into::into)
            }
            SearchConfig::Regex { regex } => {
                // Regex 需要全量数据
                let text = rope.to_string();
                Ok(search_regex_in_slice(text.as_bytes(), regex))
            }
        }
    }

    /// 查找下一个匹配
    pub fn find_next(
        rope: &Rope,
        pattern: &str,
        from_byte: usize,
        case_sensitive: bool,
        is_regex: bool,
    ) -> SearchResult<Option<Match>> {
        let matches = Self::search_sync(rope, pattern, case_sensitive, is_regex)?;
        Ok(matches.into_iter().find(|m| m.start >= from_byte))
    }

    /// 查找上一个匹配
    pub fn find_prev(
        rope: &Rope,
        pattern: &str,
        from_byte: usize,
        case_sensitive: bool,
        is_regex: bool,
    ) -> SearchResult<Option<Match>> {
        let matches = Self::search_sync(rope, pattern, case_sensitive, is_regex)?;
        Ok(matches.into_iter().rfind(|m| m.start < from_byte))
    }
}

impl Service for SearchService {
    fn name(&self) -> &'static str {
        "SearchService"
    }
}

const BATCH_SIZE: usize = 100;

fn search_rope_sync(
    rope: &Rope,
    config: &SearchConfig,
    search_id: u64,
    cancelled: &AtomicBool,
    tx: &SyncSender<SearchMessage>,
) -> usize {
    let mut total = 0usize;
    let mut batch = Vec::with_capacity(BATCH_SIZE);

    let flush = |batch: &mut Vec<Match>, is_final: bool| {
        if batch.is_empty() && !is_final {
            return;
        }
        let matches = std::mem::take(batch);
        let _ = tx.send(SearchMessage::Matches {
            search_id,
            matches,
            is_final,
        });
        *batch = Vec::with_capacity(BATCH_SIZE);
    };

    match config {
        SearchConfig::Literal { .. } => {
            let reader = RopeReader::new(rope);
            let _ = StreamSearcher::new(reader, config).search_with(
                |m| {
                    total += 1;
                    batch.push(m);
                    if batch.len() >= BATCH_SIZE {
                        flush(&mut batch, false);
                    }
                    Ok(())
                },
                || cancelled.load(Ordering::Relaxed),
            );
        }
        SearchConfig::Regex { regex } => {
            if cancelled.load(Ordering::Relaxed) {
                let _ = tx.send(SearchMessage::Cancelled { search_id });
                return 0;
            }

            let text = rope.to_string();
            let bytes = text.as_bytes();

            let mut current_line = 0usize;
            let mut line_start = 0usize;
            let mut last_pos = 0usize;

            for mat in regex.find_iter(&text) {
                if cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(SearchMessage::Cancelled { search_id });
                    return 0;
                }

                let start = mat.start();
                let end = mat.end();

                let newlines = super::count_byte(&bytes[last_pos..start], b'\n');
                if newlines > 0 {
                    current_line += newlines;
                    for i in (last_pos..start).rev() {
                        if bytes[i] == b'\n' {
                            line_start = i + 1;
                            break;
                        }
                    }
                }
                last_pos = start;

                let col = start - line_start;
                total += 1;
                batch.push(Match::new(start, end, current_line, col));
                if batch.len() >= BATCH_SIZE {
                    flush(&mut batch, false);
                }
            }
        }
    }

    if cancelled.load(Ordering::Relaxed) {
        let _ = tx.send(SearchMessage::Cancelled { search_id });
        return 0;
    }

    flush(&mut batch, true);
    total
}

#[cfg(test)]
#[path = "../../../../../tests/unit/kernel/services/adapters/search/service.rs"]
mod tests;
