//! 全局搜索服务
//!
//! 提供跨文件搜索功能：
//! - 使用 ignore crate 遵守 .gitignore 规则
//! - 自动跳过二进制文件
//! - 异步增量返回结果

use super::searcher::{Match, StreamSearcher};
use ignore::WalkBuilder;
use ropey::Rope;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

static GLOBAL_SEARCH_ID: AtomicU64 = AtomicU64::new(0);

fn next_global_search_id() -> u64 {
    GLOBAL_SEARCH_ID.fetch_add(1, Ordering::Relaxed)
}

/// 单个文件的搜索结果
#[derive(Debug, Clone)]
pub struct FileMatches {
    pub path: PathBuf,
    pub matches: Vec<Match>,
}

/// 全局搜索消息
#[derive(Debug, Clone)]
pub enum GlobalSearchMessage {
    /// 找到一个文件的匹配结果
    FileMatches {
        search_id: u64,
        file_matches: FileMatches,
    },
    /// 搜索进度更新
    Progress {
        search_id: u64,
        files_searched: usize,
        files_with_matches: usize,
    },
    /// 搜索完成
    Complete {
        search_id: u64,
        total_files: usize,
        total_matches: usize,
    },
    /// 搜索被取消
    Cancelled {
        search_id: u64,
    },
}

/// 全局搜索任务句柄
pub struct GlobalSearchTask {
    id: u64,
    cancelled: Arc<AtomicBool>,
}

impl GlobalSearchTask {
    pub fn new() -> Self {
        Self {
            id: next_global_search_id(),
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

impl Default for GlobalSearchTask {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局搜索服务
pub struct GlobalSearchService {
    runtime: tokio::runtime::Handle,
}

impl GlobalSearchService {
    pub fn new(runtime: tokio::runtime::Handle) -> Self {
        Self { runtime }
    }

    /// 在目录中搜索
    pub fn search_in_dir(
        &self,
        root: PathBuf,
        pattern: String,
        case_sensitive: bool,
        tx: Sender<GlobalSearchMessage>,
    ) -> GlobalSearchTask {
        let task = GlobalSearchTask::new();
        let search_id = task.id();
        let cancelled = task.cancelled_flag();

        self.runtime.spawn(async move {
            if pattern.is_empty() {
                let _ = tx.send(GlobalSearchMessage::Complete {
                    search_id,
                    total_files: 0,
                    total_matches: 0,
                });
                return;
            }

            // 在阻塞任务中执行搜索
            let _ = tokio::task::spawn_blocking(move || {
                search_dir_sync(&root, &pattern, case_sensitive, search_id, &cancelled, &tx)
            })
            .await;
        });

        task
    }
}

/// 检查文件是否可能是二进制文件
fn is_likely_binary(content: &[u8]) -> bool {
    // 检查前 8KB 是否有 NUL 字节
    content.iter().take(8192).any(|&b| b == 0)
}

/// 同步搜索目录
fn search_dir_sync(
    root: &Path,
    pattern: &str,
    case_sensitive: bool,
    search_id: u64,
    cancelled: &AtomicBool,
    tx: &Sender<GlobalSearchMessage>,
) {
    let walker = WalkBuilder::new(root)
        .hidden(true)           // 跳过隐藏文件
        .git_ignore(true)       // 遵守 .gitignore
        .git_global(true)       // 遵守全局 gitignore
        .git_exclude(true)      // 遵守 .git/info/exclude
        .build();

    let mut files_searched = 0usize;
    let mut files_with_matches = 0usize;
    let mut total_matches = 0usize;

    for entry in walker.flatten() {
        // 检查是否被取消
        if cancelled.load(Ordering::Relaxed) {
            let _ = tx.send(GlobalSearchMessage::Cancelled { search_id });
            return;
        }

        let path = entry.path();

        // 跳过目录
        if !path.is_file() {
            continue;
        }

        // 读取文件内容
        let content = match std::fs::read(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // 跳过二进制文件
        if is_likely_binary(&content) {
            continue;
        }

        // 尝试转换为 UTF-8
        let text = match std::str::from_utf8(&content) {
            Ok(t) => t,
            Err(_) => continue,
        };

        files_searched += 1;

        // 创建 Rope 并搜索
        let rope = Rope::from_str(text);
        let searcher = StreamSearcher::new(&rope, pattern, case_sensitive);
        let matches = searcher.find_all();

        if !matches.is_empty() {
            files_with_matches += 1;
            total_matches += matches.len();

            let _ = tx.send(GlobalSearchMessage::FileMatches {
                search_id,
                file_matches: FileMatches {
                    path: path.to_path_buf(),
                    matches,
                },
            });
        }

        // 每搜索 100 个文件发送一次进度
        if files_searched % 100 == 0 {
            let _ = tx.send(GlobalSearchMessage::Progress {
                search_id,
                files_searched,
                files_with_matches,
            });
        }
    }

    let _ = tx.send(GlobalSearchMessage::Complete {
        search_id,
        total_files: files_searched,
        total_matches,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use tempfile::tempdir;
    use std::fs;

    fn create_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn test_global_search_basic() {
        let rt = create_runtime();
        let service = GlobalSearchService::new(rt.handle().clone());
        let (tx, rx) = mpsc::channel();

        // 创建临时目录和文件
        let dir = tempdir().unwrap();
        let file1 = dir.path().join("test1.txt");
        let file2 = dir.path().join("test2.txt");
        let file3 = dir.path().join("other.txt");

        fs::write(&file1, "hello world").unwrap();
        fs::write(&file2, "hello rust").unwrap();
        fs::write(&file3, "goodbye world").unwrap();

        let _task = service.search_in_dir(
            dir.path().to_path_buf(),
            "hello".to_string(),
            true,
            tx,
        );

        // 收集结果
        let mut file_matches = Vec::new();
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(GlobalSearchMessage::FileMatches { file_matches: fm, .. }) => {
                    file_matches.push(fm);
                }
                Ok(GlobalSearchMessage::Complete { total_matches, .. }) => {
                    assert_eq!(total_matches, 2);
                    break;
                }
                Ok(GlobalSearchMessage::Progress { .. }) => continue,
                Ok(GlobalSearchMessage::Cancelled { .. }) => panic!("Unexpected cancel"),
                Err(_) => panic!("Timeout"),
            }
        }

        assert_eq!(file_matches.len(), 2);
    }

    #[test]
    fn test_skip_binary_files() {
        let rt = create_runtime();
        let service = GlobalSearchService::new(rt.handle().clone());
        let (tx, rx) = mpsc::channel();

        let dir = tempdir().unwrap();
        let text_file = dir.path().join("text.txt");
        let binary_file = dir.path().join("binary.bin");

        fs::write(&text_file, "hello world").unwrap();
        // 写入包含 NUL 字节的二进制内容
        fs::write(&binary_file, b"hello\x00world").unwrap();

        let _task = service.search_in_dir(
            dir.path().to_path_buf(),
            "hello".to_string(),
            true,
            tx,
        );

        let mut found_files = Vec::new();
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(GlobalSearchMessage::FileMatches { file_matches, .. }) => {
                    found_files.push(file_matches.path);
                }
                Ok(GlobalSearchMessage::Complete { .. }) => break,
                Ok(_) => continue,
                Err(_) => panic!("Timeout"),
            }
        }

        // 只应该找到文本文件
        assert_eq!(found_files.len(), 1);
        assert!(found_files[0].ends_with("text.txt"));
    }

    #[test]
    fn test_cancel_search() {
        let rt = create_runtime();
        let service = GlobalSearchService::new(rt.handle().clone());
        let (tx, rx) = mpsc::channel();

        let dir = tempdir().unwrap();
        for i in 0..100 {
            let file = dir.path().join(format!("file{}.txt", i));
            fs::write(&file, "hello world").unwrap();
        }

        let task = service.search_in_dir(
            dir.path().to_path_buf(),
            "hello".to_string(),
            true,
            tx,
        );

        task.cancel();

        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(GlobalSearchMessage::Cancelled { .. }) => break,
                Ok(GlobalSearchMessage::Complete { .. }) => break,
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    }
}
