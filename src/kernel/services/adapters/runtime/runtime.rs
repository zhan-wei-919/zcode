use super::message::AppMessage;
use crate::kernel::services::ports::DirEntryInfo;
use crate::models::should_ignore;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

pub struct AsyncRuntime {
    runtime: tokio::runtime::Runtime,
    tx: Sender<AppMessage>,
}

impl AsyncRuntime {
    pub fn new(tx: Sender<AppMessage>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");
        Self { runtime, tx }
    }

    pub fn tokio_handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    pub fn load_dir(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::read_dir(&path).await {
                Ok(mut entries) => {
                    let mut result = Vec::new();
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        if let Some(name) = entry.file_name().to_str() {
                            if should_ignore(name) {
                                continue;
                            }
                            if let Ok(file_type) = entry.file_type().await {
                                result.push(DirEntryInfo {
                                    name: name.to_string(),
                                    is_dir: file_type.is_dir(),
                                });
                            }
                        }
                    }
                    let _ = tx.send(AppMessage::DirLoaded {
                        path,
                        entries: result,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::DirLoadError {
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn load_file(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => {
                    let _ = tx.send(AppMessage::FileLoaded { path, content });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FileError {
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn create_file(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::write(&path, "").await {
                Ok(_) => {
                    let _ = tx.send(AppMessage::PathCreated { path, is_dir: false });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FsOpError {
                        op: "create_file",
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn create_dir(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::create_dir(&path).await {
                Ok(_) => {
                    let _ = tx.send(AppMessage::PathCreated { path, is_dir: true });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FsOpError {
                        op: "create_dir",
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn delete_path(&self, path: PathBuf, is_dir: bool) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let result = if is_dir {
                tokio::fs::remove_dir_all(&path).await
            } else {
                tokio::fs::remove_file(&path).await
            };
            match result {
                Ok(_) => {
                    let _ = tx.send(AppMessage::PathDeleted { path });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FsOpError {
                        op: "delete_path",
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }
}
