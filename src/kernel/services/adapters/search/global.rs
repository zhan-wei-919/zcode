//! 全局搜索服务
//!
//! - Literal 模式：流式搜索，8KB 栈 buffer
//! - Regex 模式：逐行流式搜索
//! - 使用 ignore crate 的并行遍历，自动利用多核

use super::searcher::{SearchConfig, StreamSearcher};
use crate::core::Service;
use crate::kernel::services::ports::search::{FileMatches, GlobalSearchMessage, Match};
use ignore::{WalkBuilder, WalkState};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::SyncSender;
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
        tx: SyncSender<GlobalSearchMessage>,
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

            let cancelled_for_blocking = cancelled.clone();
            let tx_for_blocking = tx.clone();
            let result = tokio::task::spawn_blocking(move || {
                search_dir_parallel(
                    &root,
                    &config,
                    search_id,
                    &cancelled_for_blocking,
                    &tx_for_blocking,
                )
            })
            .await;

            if let Err(e) = result {
                if cancelled.load(Ordering::Relaxed) {
                    let _ = tx.send(GlobalSearchMessage::Cancelled { search_id });
                } else {
                    let _ = tx.send(GlobalSearchMessage::Error {
                        search_id,
                        message: format!("Global search task failed: {}", e),
                    });
                }
            }
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
    tx: &SyncSender<GlobalSearchMessage>,
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

            let matches = match search_file(path, &config, cancelled) {
                Ok(m) => m,
                Err(_) => return WalkState::Continue,
            };

            if cancelled.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

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
            if searched.is_multiple_of(100) {
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

fn search_file(
    path: &Path,
    config: &SearchConfig,
    cancelled: &AtomicBool,
) -> std::io::Result<Vec<Match>> {
    if cancelled.load(Ordering::Relaxed) {
        return Ok(Vec::new());
    }

    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len() as usize;

    if file_size == 0 {
        return Ok(Vec::new());
    }

    let mut preview = [0u8; 8192];
    let preview_len = std::io::Read::read(&mut &file, &mut preview)?;
    if is_likely_binary(&preview[..preview_len]) {
        return Ok(Vec::new());
    }

    match config {
        SearchConfig::Literal { .. } => {
            // Literal 模式：流式搜索
            let file = File::open(path)?;
            let mut matches = Vec::new();
            let _ = StreamSearcher::new(file, config).search_with(
                |m| {
                    matches.push(m);
                    Ok(())
                },
                || cancelled.load(Ordering::Relaxed),
            )?;
            if cancelled.load(Ordering::Relaxed) {
                return Ok(Vec::new());
            }
            Ok(matches)
        }
        SearchConfig::Regex { .. } => {
            // Regex 模式：逐行搜索，避免 mmap 在文件被截断时触发 SIGBUS
            let file = File::open(path)?;
            search_regex_by_line(file, config, cancelled)
        }
    }
}

fn search_regex_by_line(
    file: File,
    config: &SearchConfig,
    cancelled: &AtomicBool,
) -> std::io::Result<Vec<Match>> {
    let regex = match config {
        SearchConfig::Regex { regex } => regex,
        SearchConfig::Literal { .. } => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "search_regex_by_line only supports Regex mode",
            ));
        }
    };

    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut matches = Vec::new();
    let mut line_no = 0usize;
    let mut offset = 0usize;

    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(Vec::new());
        }

        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }

        let trimmed = buf
            .strip_suffix(b"\n")
            .unwrap_or(&buf)
            .strip_suffix(b"\r")
            .unwrap_or(buf.as_slice());

        let line = match std::str::from_utf8(trimmed) {
            Ok(s) => s,
            Err(_) => return Ok(Vec::new()),
        };

        for mat in regex.find_iter(line) {
            let start = offset + mat.start();
            let end = offset + mat.end();
            matches.push(Match::new(start, end, line_no, mat.start()));
        }

        offset = offset.saturating_add(n);
        line_no = line_no.saturating_add(1);
    }

    Ok(matches)
}

#[cfg(test)]
#[path = "../../../../../tests/unit/kernel/services/adapters/search/global.rs"]
mod tests;
