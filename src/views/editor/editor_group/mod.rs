//! 编辑器组：管理多个 Tab
//!
//! 负责：
//! - 多 Tab 管理
//! - Tab 切换
//! - 打开/关闭文件
//! - 搜索/替换功能

mod commands;
mod search;
mod view;

#[cfg(test)]
mod tests;

use super::editor_view::EditorView;
use super::search_bar::SearchBar;
use crate::app::theme::UiTheme;
use crate::services::EditorConfig;
use ratatui::layout::Rect;
use std::path::PathBuf;

const TAB_BAR_HEIGHT: u16 = 1;

pub struct EditorTab {
    pub editor: EditorView,
    pub title: String,
}

impl EditorTab {
    pub fn new(title: String) -> Self {
        Self {
            editor: EditorView::new(),
            title,
        }
    }

    pub fn with_config(title: String, config: EditorConfig) -> Self {
        Self {
            editor: EditorView::with_config(config),
            title,
        }
    }

    pub fn from_file(path: PathBuf, content: &str) -> Self {
        let title = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let editor = EditorView::from_file(path, content);

        Self { editor, title }
    }

    pub fn from_file_with_config(path: PathBuf, content: &str, config: EditorConfig) -> Self {
        let title = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let editor = EditorView::from_file_with_config(path, content, config);

        Self { editor, title }
    }

    pub fn display_title(&self) -> String {
        if self.editor.is_dirty() {
            format!("● {}", self.title)
        } else {
            self.title.clone()
        }
    }
}

pub struct EditorGroup {
    tabs: Vec<EditorTab>,
    active_index: usize,
    area: Option<Rect>,
    config: EditorConfig,
    search_bar: SearchBar,
    theme: UiTheme,
}

impl EditorGroup {
    pub fn new() -> Self {
        let mut group = Self {
            tabs: Vec::new(),
            active_index: 0,
            area: None,
            config: EditorConfig::default(),
            search_bar: SearchBar::new(),
            theme: UiTheme::default(),
        };
        group.tabs.push(EditorTab::with_config(
            "Untitled".to_string(),
            group.config.clone(),
        ));
        group
    }

    pub fn with_config(config: EditorConfig) -> Self {
        let mut group = Self {
            tabs: Vec::new(),
            active_index: 0,
            area: None,
            config,
            search_bar: SearchBar::new(),
            theme: UiTheme::default(),
        };
        group.tabs.push(EditorTab::with_config(
            "Untitled".to_string(),
            group.config.clone(),
        ));
        group
    }

    pub fn set_theme(&mut self, theme: UiTheme) {
        self.theme = theme.clone();
        self.search_bar.set_theme(theme.clone());
        for tab in &mut self.tabs {
            tab.editor.set_theme(theme.clone());
        }
    }

    pub fn open_file(&mut self, path: PathBuf, content: &str) {
        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.editor.file_path() == Some(&path) {
                self.active_index = i;
                return;
            }
        }

        let mut tab = EditorTab::from_file_with_config(path, content, self.config.clone());
        tab.editor.set_theme(self.theme.clone());
        self.tabs.push(tab);
        self.active_index = self.tabs.len() - 1;
    }

    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }

        if index < self.tabs.len() {
            self.tabs.remove(index);
            if self.active_index >= self.tabs.len() {
                self.active_index = self.tabs.len() - 1;
            } else if self.active_index > index {
                self.active_index -= 1;
            }
            true
        } else {
            false
        }
    }

    pub fn close_active_tab(&mut self) -> bool {
        self.close_tab(self.active_index)
    }

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_index = (self.active_index + 1) % self.tabs.len();
        }
    }

    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_index = if self.active_index == 0 {
                self.tabs.len() - 1
            } else {
                self.active_index - 1
            };
        }
    }

    pub fn active_tab(&self) -> Option<&EditorTab> {
        self.tabs.get(self.active_index)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut EditorTab> {
        self.tabs.get_mut(self.active_index)
    }

    pub fn active_editor(&self) -> Option<&EditorView> {
        self.active_tab().map(|t| &t.editor)
    }

    pub fn active_editor_mut(&mut self) -> Option<&mut EditorView> {
        self.active_tab_mut().map(|t| &mut t.editor)
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_index(&self) -> usize {
        self.active_index
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        self.area
            .is_some_and(|a| x >= a.x && x < a.x + a.width && y >= a.y && y < a.y + a.height)
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.tabs.iter().any(|t| t.editor.is_dirty())
    }

    pub fn search_bar(&self) -> &SearchBar {
        &self.search_bar
    }

    pub fn search_bar_mut(&mut self) -> &mut SearchBar {
        &mut self.search_bar
    }
}

impl Default for EditorGroup {
    fn default() -> Self {
        Self::new()
    }
}
