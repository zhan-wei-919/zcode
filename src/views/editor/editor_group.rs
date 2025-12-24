//! 编辑器组：管理多个 Tab
//!
//! 负责：
//! - 多 Tab 管理
//! - Tab 切换
//! - 打开/关闭文件

use super::editor_view::EditorView;
use crate::core::event::InputEvent;
use crate::core::view::{EventResult, View};
use crate::services::EditorConfig;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::Frame;
use std::path::PathBuf;

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

    pub fn from_file(path: PathBuf, content: &str) -> Self {
        let title = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let editor = EditorView::from_file(path, content);

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
}

impl EditorGroup {
    pub fn new() -> Self {
        let mut group = Self {
            tabs: Vec::new(),
            active_index: 0,
            area: None,
            config: EditorConfig::default(),
        };
        group.tabs.push(EditorTab::new("Untitled".to_string()));
        group
    }

    pub fn with_config(config: EditorConfig) -> Self {
        let mut group = Self {
            tabs: Vec::new(),
            active_index: 0,
            area: None,
            config,
        };
        group.tabs.push(EditorTab::new("Untitled".to_string()));
        group
    }

    pub fn open_file(&mut self, path: PathBuf, content: &str) {
        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.editor.file_path() == Some(&path) {
                self.active_index = i;
                return;
            }
        }

        let tab = EditorTab::from_file(path, content);
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
            .map(|a| x >= a.x && x < a.x + a.width && y >= a.y && y < a.y + a.height)
            .unwrap_or(false)
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.tabs.iter().any(|t| t.editor.is_dirty())
    }

    /// 定时检查所有编辑器是否需要刷盘
    pub fn tick(&mut self) {
        for tab in &mut self.tabs {
            tab.editor.tick();
        }
    }

    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let titles: Vec<Line> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let title = tab.display_title();
                if i == self.active_index {
                    Line::from(Span::styled(
                        format!(" {} ", title),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ))
                } else {
                    Line::from(Span::styled(
                        format!(" {} ", title),
                        Style::default().fg(Color::DarkGray),
                    ))
                }
            })
            .collect();

        let tabs_widget = Tabs::new(titles)
            .select(self.active_index)
            .highlight_style(Style::default().bg(Color::DarkGray));

        frame.render_widget(tabs_widget, area);
    }
}

impl Default for EditorGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl View for EditorGroup {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        if let Some(editor) = self.active_editor_mut() {
            editor.handle_input(event)
        } else {
            EventResult::Ignored
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.area = Some(area);

        const TAB_BAR_HEIGHT: u16 = 1;

        if area.height <= TAB_BAR_HEIGHT {
            return;
        }

        let tab_area = Rect::new(area.x, area.y, area.width, TAB_BAR_HEIGHT);
        let editor_area = Rect::new(
            area.x,
            area.y + TAB_BAR_HEIGHT,
            area.width,
            area.height - TAB_BAR_HEIGHT,
        );

        self.render_tabs(frame, tab_area);

        if let Some(editor) = self.active_editor_mut() {
            editor.render(frame, editor_area);
        }
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        self.active_editor().and_then(|e| e.cursor_position())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_group_new() {
        let group = EditorGroup::new();
        assert_eq!(group.tab_count(), 1);
        assert_eq!(group.active_index(), 0);
    }

    #[test]
    fn test_open_file() {
        let mut group = EditorGroup::new();
        group.open_file(PathBuf::from("/test/file.txt"), "hello");

        assert_eq!(group.tab_count(), 2);
        assert_eq!(group.active_index(), 1);
    }

    #[test]
    fn test_close_tab() {
        let mut group = EditorGroup::new();
        group.open_file(PathBuf::from("/test/file.txt"), "hello");

        assert!(group.close_active_tab());
        assert_eq!(group.tab_count(), 1);
    }

    #[test]
    fn test_tab_navigation() {
        let mut group = EditorGroup::new();
        group.open_file(PathBuf::from("/test/a.txt"), "a");
        group.open_file(PathBuf::from("/test/b.txt"), "b");

        assert_eq!(group.active_index(), 2);

        group.prev_tab();
        assert_eq!(group.active_index(), 1);

        group.next_tab();
        assert_eq!(group.active_index(), 2);
    }

    #[test]
    fn test_cannot_close_last_tab() {
        let mut group = EditorGroup::new();
        assert!(!group.close_active_tab());
        assert_eq!(group.tab_count(), 1);
    }
}
