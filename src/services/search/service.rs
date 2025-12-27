//! 异步搜索服务
//!
//! 提供增量搜索功能，支持取消和批量返回结果

use super::searcher::{Match, StreamSearcher};
use ropey::Rope;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

static SEARCH_ID: AtomicU64 = AtomicU64::new(0);

fn next_search_id() -> u64 {
    SEARCH_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone)]
pub enum SearchMessage {
    Matches {
        search_id: u64,
        matches: Vec<Match>,
        is_final: bool,
    },
    Complete {
        search_id: u64,
        total: usize,
    },
    Cancelled {
        search_id: u64,
    },
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

    /// 在单个 Rope 中搜索
    /// 返回 SearchTask 用于取消搜索
    pub fn search_in_rope(
        &self,
        rope: Rope,
        pattern: String,
        case_sensitive: bool,
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

            // 在阻塞任务中执行搜索
            let result = tokio::task::spawn_blocking(move || {
                search_rope_sync(&rope, &pattern, case_sensitive, search_id, &cancelled, &tx)
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
    ) -> Vec<Match> {
        if pattern.is_empty() {
            return Vec::new();
        }
        let searcher = StreamSearcher::new(rope, pattern, case_sensitive);
        searcher.find_all()
    }

    /// 查找下一个匹配
    pub fn find_next(
        rope: &Rope,
        pattern: &str,
        from_byte: usize,
        case_sensitive: bool,
    ) -> Option<Match> {
        if pattern.is_empty() {
            return None;
        }
        let searcher = StreamSearcher::new(rope, pattern, case_sensitive);
        searcher.find_next(from_byte)
    }

    /// 查找上一个匹配
    pub fn find_prev(
        rope: &Rope,
        pattern: &str,
        from_byte: usize,
        case_sensitive: bool,
    ) -> Option<Match> {
        if pattern.is_empty() {
            return None;
        }
        let searcher = StreamSearcher::new(rope, pattern, case_sensitive);
        searcher.find_prev(from_byte)
    }
}

const BATCH_SIZE: usize = 100;

fn search_rope_sync(
    rope: &Rope,
    pattern: &str,
    case_sensitive: bool,
    search_id: u64,
    cancelled: &AtomicBool,
    tx: &Sender<SearchMessage>,
) -> usize {
    let searcher = StreamSearcher::new(rope, pattern, case_sensitive);
    let all_matches = searcher.find_all();

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
    fn test_search_sync() {
        let rope = Rope::from_str("hello world hello");
        let matches = SearchService::search_sync(&rope, "hello", true);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_find_next() {
        let rope = Rope::from_str("hello world hello");
        let m = SearchService::find_next(&rope, "hello", 0, true);
        assert!(m.is_some());
        assert_eq!(m.unwrap().start_byte, 0);

        let m = SearchService::find_next(&rope, "hello", 1, true);
        assert!(m.is_some());
        assert_eq!(m.unwrap().start_byte, 12);
    }

    #[test]
    fn test_async_search() {
        let rt = create_runtime();
        let service = SearchService::new(rt.handle().clone());
        let (tx, rx) = mpsc::channel();

        let rope = Rope::from_str("hello world hello");
        let _task = service.search_in_rope(rope, "hello".to_string(), true, tx);

        // 等待结果
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

        // 创建一个大文本
        let text = "hello ".repeat(10000);
        let rope = Rope::from_str(&text);

        let task = service.search_in_rope(rope, "hello".to_string(), true, tx);
        task.cancel();

        // 等待取消消息或完成
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(1)) {
                Ok(SearchMessage::Cancelled { .. }) => {
                    break;
                }
                Ok(SearchMessage::Complete { .. }) => {
                    // 搜索可能在取消前完成
                    break;
                }
                Ok(SearchMessage::Matches { .. }) => {
                    continue;
                }
                Err(_) => {
                    break;
                }
            }
        }
    }
}
