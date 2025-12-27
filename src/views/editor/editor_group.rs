//! 编辑器组：管理多个 Tab
//!
//! 负责：
//! - 多 Tab 管理
//! - Tab 切换
//! - 打开/关闭文件
//! - 搜索/替换功能

use super::editor_view::EditorView;
use super::search_bar::{SearchBar, SearchBarMode};
use crate::core::event::InputEvent;
use crate::core::view::{EventResult, View};
use crate::services::EditorConfig;
use crossterm::event::{KeyCode, KeyModifiers};
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
    search_bar: SearchBar,
}

impl EditorGroup {
    pub fn new() -> Self {
        let mut group = Self {
            tabs: Vec::new(),
            active_index: 0,
            area: None,
            config: EditorConfig::default(),
            search_bar: SearchBar::new(),
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
            search_bar: SearchBar::new(),
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

    pub fn search_bar(&self) -> &SearchBar {
        &self.search_bar
    }

    pub fn search_bar_mut(&mut self) -> &mut SearchBar {
        &mut self.search_bar
    }

    pub fn toggle_search(&mut self) {
        self.search_bar.toggle();
        if self.search_bar.is_visible() {
            self.trigger_search();
        }
    }

    pub fn show_search(&mut self) {
        self.search_bar.show(SearchBarMode::Search);
    }

    pub fn show_replace(&mut self) {
        self.search_bar.show(SearchBarMode::Replace);
    }

    pub fn hide_search(&mut self) {
        self.search_bar.hide();
    }

    fn trigger_search(&mut self) {
        if let Some(editor) = self.active_editor() {
            let rope = editor.buffer().rope().clone();
            self.search_bar.search(&rope);
        }
    }

    fn goto_current_match(&mut self) {
        if let Some(m) = self.search_bar.current_match() {
            let line = m.line;
            let col = m.col;
            if let Some(editor) = self.active_editor_mut() {
                editor.buffer_mut().set_cursor(line, col);
            }
        }
    }

    fn find_next(&mut self) {
        self.search_bar.next_match();
        self.goto_current_match();
    }

    fn find_prev(&mut self) {
        self.search_bar.prev_match();
        self.goto_current_match();
    }

    fn replace_current(&mut self) {
        let replace_text = self.search_bar.replace_text().to_string();
        if replace_text.is_empty() {
            return;
        }

        if let Some(m) = self.search_bar.current_match() {
            let start_char = m.start_char;
            let end_char = m.end_char;

            if let Some(editor) = self.active_editor_mut() {
                editor.buffer_mut().replace_range(start_char, end_char, &replace_text);
                editor.set_dirty(true);
            }

            // 重新搜索
            self.trigger_search();
        }
    }

    fn replace_all(&mut self) {
        let search_text = self.search_bar.search_text().to_string();
        let replace_text = self.search_bar.replace_text().to_string();
        let case_sensitive = self.search_bar.case_sensitive();

        if search_text.is_empty() {
            return;
        }

        if let Some(editor) = self.active_editor_mut() {
            let text = editor.buffer().text();
            let new_text = if case_sensitive {
                text.replace(&search_text, &replace_text)
            } else {
                // 大小写不敏感替换
                let mut result = text.clone();
                let lower_search = search_text.to_lowercase();
                let mut offset = 0;

                while let Some(pos) = result[offset..].to_lowercase().find(&lower_search) {
                    let actual_pos = offset + pos;
                    result.replace_range(actual_pos..actual_pos + search_text.len(), &replace_text);
                    offset = actual_pos + replace_text.len();
                }
                result
            };

            editor.set_content(&new_text);
            editor.set_dirty(true);
        }

        self.trigger_search();
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
        // 处理搜索相关的全局快捷键
        if let InputEvent::Key(key_event) = event {
            match (key_event.code, key_event.modifiers) {
                // Ctrl+F: 切换搜索栏
                (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                    if self.search_bar.is_visible() {
                        self.hide_search();
                    } else {
                        self.show_search();
                    }
                    return EventResult::Consumed;
                }
                // Ctrl+H: 打开替换
                (KeyCode::Char('h'), KeyModifiers::CONTROL) => {
                    self.show_replace();
                    return EventResult::Consumed;
                }
                // F3 / Ctrl+G: 下一个匹配
                (KeyCode::F(3), KeyModifiers::NONE)
                | (KeyCode::Char('g'), KeyModifiers::CONTROL) => {
                    if self.search_bar.is_visible() {
                        self.find_next();
                        return EventResult::Consumed;
                    }
                }
                // Shift+F3: 上一个匹配
                (KeyCode::F(3), KeyModifiers::SHIFT) => {
                    if self.search_bar.is_visible() {
                        self.find_prev();
                        return EventResult::Consumed;
                    }
                }
                // Ctrl+Shift+G: 上一个匹配
                (KeyCode::Char('g'), mods)
                    if mods == KeyModifiers::CONTROL | KeyModifiers::SHIFT =>
                {
                    if self.search_bar.is_visible() {
                        self.find_prev();
                        return EventResult::Consumed;
                    }
                }
                _ => {}
            }
        }

        // 如果搜索栏可见，优先处理搜索栏输入
        if self.search_bar.is_visible() {
            if let InputEvent::Key(key_event) = event {
                match (key_event.code, key_event.modifiers) {
                    // Enter: 下一个匹配
                    (KeyCode::Enter, KeyModifiers::NONE) => {
                        self.find_next();
                        return EventResult::Consumed;
                    }
                    // Shift+Enter: 上一个匹配
                    (KeyCode::Enter, KeyModifiers::SHIFT) => {
                        self.find_prev();
                        return EventResult::Consumed;
                    }
                    // Ctrl+Enter: 替换当前
                    (KeyCode::Enter, KeyModifiers::CONTROL) => {
                        self.replace_current();
                        return EventResult::Consumed;
                    }
                    // Ctrl+Shift+Enter: 替换全部
                    (KeyCode::Enter, mods)
                        if mods == KeyModifiers::CONTROL | KeyModifiers::SHIFT =>
                    {
                        self.replace_all();
                        return EventResult::Consumed;
                    }
                    _ => {}
                }
            }

            let old_text = self.search_bar.search_text().to_string();
            let result = self.search_bar.handle_input(event);

            // 如果搜索文本变化，重新搜索
            if self.search_bar.search_text() != old_text {
                self.trigger_search();
            }

            if result.is_consumed() {
                return result;
            }
        }

        // 传递给编辑器
        if let Some(editor) = self.active_editor_mut() {
            editor.handle_input(event)
        } else {
            EventResult::Ignored
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.area = Some(area);

        const TAB_BAR_HEIGHT: u16 = 1;
        let search_bar_height = self.search_bar.height();

        let total_chrome_height = TAB_BAR_HEIGHT + search_bar_height;
        if area.height <= total_chrome_height {
            return;
        }

        let tab_area = Rect::new(area.x, area.y, area.width, TAB_BAR_HEIGHT);

        let search_area = if search_bar_height > 0 {
            Rect::new(
                area.x,
                area.y + TAB_BAR_HEIGHT,
                area.width,
                search_bar_height,
            )
        } else {
            Rect::default()
        };

        let editor_area = Rect::new(
            area.x,
            area.y + total_chrome_height,
            area.width,
            area.height - total_chrome_height,
        );

        self.render_tabs(frame, tab_area);

        if self.search_bar.is_visible() {
            self.search_bar.render(frame, search_area);
        }

        if let Some(editor) = self.active_editor_mut() {
            editor.render(frame, editor_area);
        }
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        // 如果搜索栏可见且有焦点，返回搜索栏的光标位置
        if self.search_bar.is_visible() {
            return self.search_bar.cursor_position();
        }
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
