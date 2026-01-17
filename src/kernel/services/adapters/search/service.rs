//! 单文件搜索服务
//!
//! 用于编辑器内搜索（Rope 已存在于内存中）

use super::searcher::{search_regex_in_slice, RopeReader, SearchConfig, StreamSearcher};
use crate::core::Service;
use crate::kernel::services::ports::search::{Match, SearchMessage};
use ropey::Rope;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
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
        tx: Sender<SearchMessage>,
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
                Err(_) => {
                    let _ = tx_for_complete.send(SearchMessage::Cancelled { search_id });
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
    ) -> Result<Vec<Match>, String> {
        if pattern.is_empty() {
            return Ok(Vec::new());
        }

        let config = if is_regex {
            SearchConfig::regex(pattern, case_sensitive)
                .map_err(|e| format!("Invalid regex: {}", e))?
        } else {
            SearchConfig::literal(pattern, case_sensitive)
        };

        match &config {
            SearchConfig::Literal { .. } => {
                let reader = RopeReader::new(rope);
                StreamSearcher::new(reader, &config)
                    .search()
                    .map_err(|e| e.to_string())
            }
            SearchConfig::Regex { .. } => {
                // Regex 需要全量数据
                let text = rope.to_string();
                Ok(search_regex_in_slice(text.as_bytes(), &config))
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
    ) -> Result<Option<Match>, String> {
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
    ) -> Result<Option<Match>, String> {
        let matches = Self::search_sync(rope, pattern, case_sensitive, is_regex)?;
        Ok(matches.into_iter().filter(|m| m.start < from_byte).last())
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
    tx: &Sender<SearchMessage>,
) -> usize {
    let all_matches = match config {
        SearchConfig::Literal { .. } => {
            let reader = RopeReader::new(rope);
            match StreamSearcher::new(reader, config).search() {
                Ok(m) => m,
                Err(_) => return 0,
            }
        }
        SearchConfig::Regex { .. } => {
            let text = rope.to_string();
            search_regex_in_slice(text.as_bytes(), config)
        }
    };

    let total = all_matches.len();

    // 分批发送结果
    for (i, chunk) in all_matches.chunks(BATCH_SIZE).enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            let _ = tx.send(SearchMessage::Cancelled { search_id });
            return 0;
        }

        let is_final = (i + 1) * BATCH_SIZE >= total;
        let _ = tx.send(SearchMessage::Matches {
            search_id,
            matches: chunk.to_vec(),
            is_final,
        });
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn create_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn test_search_sync_literal() {
        let rope = Rope::from_str("hello world hello");
        let matches = SearchService::search_sync(&rope, "hello", true, false).unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_search_sync_regex() {
        let rope = Rope::from_str("hello123 world456");
        let matches = SearchService::search_sync(&rope, r"\w+\d+", true, true).unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_find_next() {
        let rope = Rope::from_str("hello world hello");
        let m = SearchService::find_next(&rope, "hello", 0, true, false).unwrap();
        assert!(m.is_some());
        assert_eq!(m.unwrap().start, 0);

        let m = SearchService::find_next(&rope, "hello", 1, true, false).unwrap();
        assert!(m.is_some());
        assert_eq!(m.unwrap().start, 12);
    }

    #[test]
    fn test_find_prev() {
        let rope = Rope::from_str("hello world hello");
        let m = SearchService::find_prev(&rope, "hello", 17, true, false).unwrap();
        assert!(m.is_some());
        assert_eq!(m.unwrap().start, 12);

        let m = SearchService::find_prev(&rope, "hello", 12, true, false).unwrap();
        assert!(m.is_some());
        assert_eq!(m.unwrap().start, 0);
    }

    #[test]
    fn test_async_search() {
        let rt = create_runtime();
        let service = SearchService::new(rt.handle().clone());
        let (tx, rx) = mpsc::channel();

        let rope = Rope::from_str("hello world hello");
        let _task = service.search_in_rope(rope, "hello".to_string(), true, false, tx);

        let mut total_matches = 0;
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(SearchMessage::Matches { matches, .. }) => {
                    total_matches += matches.len();
                }
                Ok(SearchMessage::Complete { total, .. }) => {
                    assert_eq!(total, 2);
                    assert_eq!(total_matches, 2);
                    break;
                }
                Ok(SearchMessage::Cancelled { .. }) => {
                    panic!("Search was cancelled unexpectedly");
                }
                Ok(SearchMessage::Error { message, .. }) => {
                    panic!("Error: {}", message);
                }
                Err(_) => {
                    panic!("Timeout waiting for search results");
                }
            }
        }
    }

    #[test]
    fn test_cancel_search() {
        let rt = create_runtime();
        let service = SearchService::new(rt.handle().clone());
        let (tx, rx) = mpsc::channel();

        let text = "hello ".repeat(10000);
        let rope = Rope::from_str(&text);

        let task = service.search_in_rope(rope, "hello".to_string(), true, false, tx);
        task.cancel();

        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(SearchMessage::Cancelled { .. }) => break,
                Ok(SearchMessage::Complete { .. }) => break,
                Ok(SearchMessage::Matches { .. }) => continue,
                Ok(SearchMessage::Error { .. }) => break,
                Err(_) => break,
            }
        }
    }

    #[test]
    fn test_invalid_regex() {
        let rope = Rope::from_str("hello world");
        let result = SearchService::search_sync(&rope, "[invalid", true, true);
        assert!(result.is_err());
    }
}
