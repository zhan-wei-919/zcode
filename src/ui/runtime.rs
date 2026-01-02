//! 异步运行时 - 处理文件系统等 IO 操作

use crate::kernel::{EditorCore, NodeId};
use rustc_hash::FxHashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::mpsc;
use tokio::runtime::Runtime;

/// 异步任务结果
#[derive(Debug)]
pub enum AsyncResult {
    FileRead {
        path: PathBuf,
        result: Result<String, String>,
    },
    FileWrite {
        path: PathBuf,
        result: Result<(), String>,
    },
    DirLoaded {
        parent: NodeId,
        entries: Vec<(OsString, bool)>,
    },
    DirLoadError {
        parent: NodeId,
        error: String,
    },
    FilePreloaded {
        path: PathBuf,
    },
}

/// 异步运行时
pub struct AsyncRuntime {
    runtime: Runtime,
    tx: mpsc::Sender<AsyncResult>,
    rx: mpsc::Receiver<AsyncResult>,
    /// 预加载的 EditorCore 缓存
    preload_cache: FxHashMap<PathBuf, EditorCore>,
    /// 正在预加载的文件
    preloading: FxHashMap<PathBuf, ()>,
    /// 预加载结果接收
    preload_rx: mpsc::Receiver<(PathBuf, EditorCore)>,
    preload_tx: mpsc::Sender<(PathBuf, EditorCore)>,
}

impl AsyncRuntime {
    pub fn new() -> Self {
        let runtime = Runtime::new().expect("Failed to create tokio runtime");
        let (tx, rx) = mpsc::channel();
        let (preload_tx, preload_rx) = mpsc::channel();
        Self {
            runtime,
            tx,
            rx,
            preload_cache: FxHashMap::default(),
            preloading: FxHashMap::default(),
            preload_rx,
            preload_tx,
        }
    }

    /// 异步读取文件
    pub fn read_file(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let result = tokio::fs::read_to_string(&path).await;
            let _ = tx.send(AsyncResult::FileRead {
                path,
                result: result.map_err(|e| e.to_string()),
            });
        });
    }

    /// 预加载文件并构建 EditorCore（在后台线程）
    pub fn preload_file(&mut self, path: PathBuf) {
        if self.preload_cache.contains_key(&path) || self.preloading.contains_key(&path) {
            return;
        }
        self.preloading.insert(path.clone(), ());
        let preload_tx = self.preload_tx.clone();

        // 使用 spawn_blocking 在独立线程构建 EditorCore
        self.runtime.spawn(async move {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                // 在阻塞线程中构建 EditorCore
                let path_clone = path.clone();
                let editor = tokio::task::spawn_blocking(move || {
                    EditorCore::from_file(path_clone, &content)
                })
                .await;

                if let Ok(editor) = editor {
                    let _ = preload_tx.send((path, editor));
                }
            }
        });
    }

    /// 尝试从缓存获取 EditorCore
    pub fn get_cached_editor(&mut self, path: &PathBuf) -> Option<EditorCore> {
        self.preload_cache.remove(path)
    }

    /// 异步写入文件
    pub fn write_file(&self, path: PathBuf, content: String) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let result = tokio::fs::write(&path, &content).await;
            let _ = tx.send(AsyncResult::FileWrite {
                path,
                result: result.map_err(|e| e.to_string()),
            });
        });
    }

    /// 异步加载目录
    pub fn load_dir(&self, parent: NodeId, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::read_dir(&path).await {
                Ok(mut entries) => {
                    let mut result = Vec::new();
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let name = entry.file_name();
                        if should_ignore(&name.to_string_lossy()) {
                            continue;
                        }
                        let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                        result.push((name, is_dir));
                    }
                    let _ = tx.send(AsyncResult::DirLoaded {
                        parent,
                        entries: result,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AsyncResult::DirLoadError {
                        parent,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    /// 非阻塞地获取所有已完成的异步结果
    pub fn poll_results(&mut self) -> Vec<AsyncResult> {
        // 先处理预加载完成的 EditorCore
        while let Ok((path, editor)) = self.preload_rx.try_recv() {
            self.preloading.remove(&path);
            self.preload_cache.insert(path, editor);
        }

        let mut results = Vec::new();
        while let Ok(result) = self.rx.try_recv() {
            results.push(result);
        }
        results
    }
}

fn should_ignore(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__"
}
