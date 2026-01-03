use super::super::viewport::Viewport;
use super::mouse::MouseState;
use super::EditorView;
use crate::app::theme::UiTheme;
use crate::models::{EditHistory, TextBuffer};
use crate::services::{ensure_backup_dir, get_ops_file_path, ClipboardService, EditorConfig};
use std::path::PathBuf;

impl EditorView {
    pub fn new() -> Self {
        let buffer = TextBuffer::new();
        let history = EditHistory::new(buffer.rope().clone());
        Self {
            buffer,
            viewport: Viewport::new(4),
            config: EditorConfig::default(),
            file_path: None,
            dirty: false,
            mouse_state: MouseState::new(),
            history,
            clipboard: ClipboardService::new(),
            theme: UiTheme::default(),
        }
    }

    pub fn with_config(config: EditorConfig) -> Self {
        let buffer = TextBuffer::new();
        let history = EditHistory::new(buffer.rope().clone());
        Self {
            buffer,
            viewport: Viewport::new(config.tab_size),
            config,
            file_path: None,
            dirty: false,
            mouse_state: MouseState::new(),
            history,
            clipboard: ClipboardService::new(),
            theme: UiTheme::default(),
        }
    }

    pub fn from_text(text: &str) -> Self {
        let buffer = TextBuffer::from_text(text);
        let history = EditHistory::new(buffer.rope().clone());
        Self {
            buffer,
            viewport: Viewport::new(4),
            config: EditorConfig::default(),
            file_path: None,
            dirty: false,
            mouse_state: MouseState::new(),
            history,
            clipboard: ClipboardService::new(),
            theme: UiTheme::default(),
        }
    }

    /// 从文件创建编辑器，支持持久化和崩溃恢复
    pub fn from_file(path: PathBuf, content: &str) -> Self {
        Self::from_file_with_config(path, content, EditorConfig::default())
    }

    /// 从文件创建编辑器（可注入配置），支持持久化和崩溃恢复
    pub fn from_file_with_config(path: PathBuf, content: &str, config: EditorConfig) -> Self {
        let buffer = TextBuffer::from_text(content);
        let ops_file_path = get_ops_file_path(&path);
        let tab_size = config.tab_size;

        let history = if let Some(ops_path) = ops_file_path {
            if EditHistory::has_backup(&ops_path) {
                match EditHistory::recover(buffer.rope().clone(), ops_path.clone()) {
                    Ok((history, recovered_rope, cursor)) => {
                        let mut view = Self {
                            buffer: TextBuffer::from_text(content),
                            viewport: Viewport::new(tab_size),
                            config,
                            file_path: Some(path),
                            dirty: history.is_dirty(),
                            mouse_state: MouseState::new(),
                            history,
                            clipboard: ClipboardService::new(),
                            theme: UiTheme::default(),
                        };
                        view.buffer.set_rope(recovered_rope);
                        view.buffer.set_cursor(cursor.0, cursor.1);
                        return view;
                    }
                    Err(_) => {
                        let _ = EditHistory::clear_backup(&ops_path);
                        Self::create_history_with_backup(buffer.rope().clone(), ops_path)
                    }
                }
            } else {
                Self::create_history_with_backup(buffer.rope().clone(), ops_path)
            }
        } else {
            EditHistory::new(buffer.rope().clone())
        };

        Self {
            buffer,
            viewport: Viewport::new(tab_size),
            config,
            file_path: Some(path),
            dirty: false,
            mouse_state: MouseState::new(),
            history,
            clipboard: ClipboardService::new(),
            theme: UiTheme::default(),
        }
    }

    fn create_history_with_backup(base_snapshot: ropey::Rope, ops_path: PathBuf) -> EditHistory {
        if ensure_backup_dir().is_ok() {
            EditHistory::with_backup(base_snapshot.clone(), ops_path)
                .unwrap_or_else(|_| EditHistory::new(base_snapshot))
        } else {
            EditHistory::new(base_snapshot)
        }
    }
}
