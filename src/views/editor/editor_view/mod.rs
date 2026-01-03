//! 编辑器视图
//!
//! 实现 View trait，负责：
//! - 渲染文本内容
//! - 处理键盘输入
//! - 处理鼠标交互
//! - 管理选区
//! - Undo/Redo 历史管理
//! - 崩溃恢复（持久化）

mod edit;
mod mouse;
mod persistence;
mod render;

#[cfg(test)]
mod tests;

use super::viewport::Viewport;
use crate::app::theme::UiTheme;
use crate::models::{EditHistory, TextBuffer};
use crate::services::{ClipboardService, EditorConfig};
use std::path::PathBuf;

use mouse::MouseState;

pub struct EditorView {
    buffer: TextBuffer,
    viewport: Viewport,
    config: EditorConfig,
    file_path: Option<PathBuf>,
    dirty: bool,
    mouse_state: MouseState,
    history: EditHistory,
    clipboard: ClipboardService,
    theme: UiTheme,
}

impl EditorView {
    pub fn set_theme(&mut self, theme: UiTheme) {
        self.theme = theme;
    }

    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut TextBuffer {
        &mut self.buffer
    }

    pub fn file_path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }

    pub fn set_file_path(&mut self, path: PathBuf) {
        self.file_path = Some(path);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    /// 定时检查是否需要刷盘（由主循环调用）
    pub fn tick(&mut self) {
        self.history.tick();
    }

    /// 保存后调用，更新基准快照并清除备份
    pub fn on_save(&mut self) {
        self.history.on_save(self.buffer.rope());

        if let Some(path) = &self.file_path {
            if let Some(ops_path) = crate::services::get_ops_file_path(path) {
                let _ = EditHistory::clear_backup(&ops_path);
            }
        }
    }

    pub fn set_content(&mut self, text: &str) {
        self.buffer = TextBuffer::from_text(text);
        self.history = EditHistory::new(self.buffer.rope().clone());
        self.dirty = false;
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.buffer.cursor()
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        self.viewport
            .area()
            .is_some_and(|a| x >= a.x && x < a.x + a.width && y >= a.y && y < a.y + a.height)
    }
}

impl Default for EditorView {
    fn default() -> Self {
        Self::new()
    }
}
