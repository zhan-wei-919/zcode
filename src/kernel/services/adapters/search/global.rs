//! 全局搜索服务
//!
//! - Literal 模式：流式搜索，8KB 栈 buffer
//! - Regex 模式：mmap 全量搜索
//! - 使用 ignore crate 的并行遍历，自动利用多核

use super::searcher::{search_regex_in_slice, SearchConfig, StreamSearcher};
use crate::core::Service;
use crate::kernel::services::ports::search::{FileMatches, GlobalSearchMessage, Match};
use ignore::{WalkBuilder, WalkState};
use memmap2::Mmap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

static GLOBAL_SEARCH_ID: AtomicU64 = AtomicU64::new(0);

fn next_global_search_id() -> u64 {
    GLOBAL_SEARCH_ID.fetch_add(1, Ordering::Relaxed)
}

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

pub struct GlobalSearchService {
    runtime: tokio::runtime::Handle,
}

impl GlobalSearchService {
    pub fn new(runtime: tokio::runtime::Handle) -> Self {
        Self { runtime }
    }

    pub fn search_in_dir(
        &self,
        root: PathBuf,
        pattern: String,
        case_sensitive: bool,
        is_regex: bool,
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

            // 编译搜索配置（只编译一次）
            let config = if is_regex {
                match SearchConfig::regex(&pattern, case_sensitive) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(GlobalSearchMessage::Error {
                            search_id,
                            message: format!("Invalid regex: {}", e),
                        });
                        return;
                    }
                }
            } else {
                SearchConfig::literal(&pattern, case_sensitive)
            };

            let _ = tokio::task::spawn_blocking(move || {
                search_dir_parallel(&root, &config, search_id, &cancelled, &tx)
            })
            .await;
        });

        task
    }
}

impl Service for GlobalSearchService {
    fn name(&self) -> &'static str {
        "GlobalSearchService"
    }
}

fn is_likely_binary(content: &[u8]) -> bool {
    content.iter().take(8192).any(|&b| b == 0)
}

/// 并行搜索目录
fn search_dir_parallel(
    root: &Path,
    config: &SearchConfig,
    search_id: u64,
    cancelled: &AtomicBool,
    tx: &Sender<GlobalSearchMessage>,
) {
    let files_searched = Arc::new(AtomicUsize::new(0));
    let files_with_matches = Arc::new(AtomicUsize::new(0));
    let total_matches = Arc::new(AtomicUsize::new(0));

    let walker = WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build_parallel();

    walker.run(|| {
        // 每个线程的局部状态
        let config = config.clone();
        let cancelled = cancelled;
        let tx = tx.clone();
        let files_searched = files_searched.clone();
        let files_with_matches = files_with_matches.clone();
        let total_matches = total_matches.clone();

        Box::new(move |entry| {
            // 检查取消
            if cancelled.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let entry = match entry {
                Ok(e) => e,
                Err(_) => return WalkState::Continue,
            };

            let path = entry.path();
            if !path.is_file() {
                return WalkState::Continue;
            }

            let matches = match search_file(path, &config) {
                Ok(m) => m,
                Err(_) => return WalkState::Continue,
            };

            let searched = files_searched.fetch_add(1, Ordering::Relaxed) + 1;

            if !matches.is_empty() {
                files_with_matches.fetch_add(1, Ordering::Relaxed);
                total_matches.fetch_add(matches.len(), Ordering::Relaxed);

                let _ = tx.send(GlobalSearchMessage::FileMatches {
                    search_id,
                    file_matches: FileMatches {
                        path: path.to_path_buf(),
                        matches,
                    },
                });
            }

            // 每 100 个文件发送进度
            if searched % 100 == 0 {
                let _ = tx.send(GlobalSearchMessage::Progress {
                    search_id,
                    files_searched: searched,
                    files_with_matches: files_with_matches.load(Ordering::Relaxed),
                });
            }

            WalkState::Continue
        })
    });

    // 发送完成或取消消息
    if cancelled.load(Ordering::Relaxed) {
        let _ = tx.send(GlobalSearchMessage::Cancelled { search_id });
    } else {
        let _ = tx.send(GlobalSearchMessage::Complete {
            search_id,
            total_files: files_searched.load(Ordering::Relaxed),
            total_matches: total_matches.load(Ordering::Relaxed),
        });
    }
}

fn search_file(path: &Path, config: &SearchConfig) -> std::io::Result<Vec<Match>> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len() as usize;

    if file_size == 0 {
        return Ok(Vec::new());
    }

    match config {
        SearchConfig::Literal { .. } => {
            // Literal 模式：流式搜索
            let mut preview = [0u8; 8192];
            let preview_len = std::io::Read::read(&mut &file, &mut preview)?;
            if is_likely_binary(&preview[..preview_len]) {
                return Ok(Vec::new());
            }

            let file = File::open(path)?;
            StreamSearcher::new(file, config).search()
        }
        SearchConfig::Regex { .. } => {
            // Regex 模式：mmap 全量搜索
            // SAFETY: 文件在搜索期间不会被修改
            let mmap = unsafe { Mmap::map(&file)? };

            if is_likely_binary(&mmap) {
                return Ok(Vec::new());
            }

            Ok(search_regex_in_slice(&mmap, config))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc;
    use tempfile::tempdir;

    fn create_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn test_global_search_literal() {
        let rt = create_runtime();
        let service = GlobalSearchService::new(rt.handle().clone());
        let (tx, rx) = mpsc::channel();

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
            false,
            tx,
        );

        let mut file_matches = Vec::new();
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(GlobalSearchMessage::FileMatches {
                    file_matches: fm, ..
                }) => {
                    file_matches.push(fm);
                }
                Ok(GlobalSearchMessage::Complete { total_matches, .. }) => {
                    assert_eq!(total_matches, 2);
                    break;
                }
                Ok(GlobalSearchMessage::Progress { .. }) => continue,
                Ok(GlobalSearchMessage::Cancelled { .. }) => panic!("Unexpected cancel"),
                Ok(GlobalSearchMessage::Error { message, .. }) => panic!("Error: {}", message),
                Err(_) => panic!("Timeout"),
            }
        }

        assert_eq!(file_matches.len(), 2);
    }

    #[test]
    fn test_global_search_regex() {
        let rt = create_runtime();
        let service = GlobalSearchService::new(rt.handle().clone());
        let (tx, rx) = mpsc::channel();

        let dir = tempdir().unwrap();
        let file1 = dir.path().join("test1.txt");
        let file2 = dir.path().join("test2.txt");

        fs::write(&file1, "hello123 world").unwrap();
        fs::write(&file2, "hello456 rust").unwrap();

        let _task = service.search_in_dir(
            dir.path().to_path_buf(),
            r"hello\d+".to_string(),
            true,
            true,
            tx,
        );

        let mut total = 0;
        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(GlobalSearchMessage::FileMatches { file_matches, .. }) => {
                    total += file_matches.matches.len();
                }
                Ok(GlobalSearchMessage::Complete { total_matches, .. }) => {
                    assert_eq!(total_matches, 2);
                    assert_eq!(total, 2);
                    break;
                }
                Ok(GlobalSearchMessage::Progress { .. }) => continue,
                Ok(GlobalSearchMessage::Cancelled { .. }) => panic!("Unexpected cancel"),
                Ok(GlobalSearchMessage::Error { message, .. }) => panic!("Error: {}", message),
                Err(_) => panic!("Timeout"),
            }
        }
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
        fs::write(&binary_file, b"hello\x00world").unwrap();

        let _task = service.search_in_dir(
            dir.path().to_path_buf(),
            "hello".to_string(),
            true,
            false,
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
            false,
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
