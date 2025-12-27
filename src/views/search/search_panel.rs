//! 全局搜索面板
//!
//! VS Code 风格的全局搜索面板，显示在侧边栏

use crate::core::event::InputEvent;
use crate::core::view::{EventResult, View};
use crate::services::search::{FileMatches, Match};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;

/// 搜索结果项
#[derive(Debug, Clone)]
pub enum SearchResultItem {
    /// 文件头（显示文件路径和匹配数）
    FileHeader {
        path: PathBuf,
        match_count: usize,
        expanded: bool,
    },
    /// 单个匹配（显示行号和内容预览）
    MatchLine {
        file_path: PathBuf,
        match_info: Match,
        line_preview: String,
    },
}

pub struct GlobalSearchPanel {
    visible: bool,
    search_text: String,
    cursor_pos: usize,
    case_sensitive: bool,
    use_regex: bool,
    results: Vec<FileMatches>,
    flat_items: Vec<SearchResultItem>,
    selected_index: usize,
    list_state: ListState,
    searching: bool,
    total_matches: usize,
    files_searched: usize,
    files_with_matches: usize,
    area: Option<Rect>,
}

impl GlobalSearchPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            search_text: String::new(),
            cursor_pos: 0,
            case_sensitive: false,
            use_regex: false,
            results: Vec::new(),
            flat_items: Vec::new(),
            selected_index: 0,
            list_state: ListState::default(),
            searching: false,
            total_matches: 0,
            files_searched: 0,
            files_with_matches: 0,
            area: None,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn search_text(&self) -> &str {
        &self.search_text
    }

    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub fn use_regex(&self) -> bool {
        self.use_regex
    }

    pub fn is_searching(&self) -> bool {
        self.searching
    }

    pub fn set_searching(&mut self, searching: bool) {
        self.searching = searching;
    }

    pub fn clear_results(&mut self) {
        self.results.clear();
        self.flat_items.clear();
        self.selected_index = 0;
        self.total_matches = 0;
        self.files_searched = 0;
        self.files_with_matches = 0;
        self.list_state.select(None);
    }

    pub fn add_file_matches(&mut self, file_matches: FileMatches) {
        self.total_matches += file_matches.matches.len();
        self.results.push(file_matches);
        self.rebuild_flat_items();
    }

    pub fn set_progress(&mut self, files_searched: usize, files_with_matches: usize) {
        self.files_searched = files_searched;
        self.files_with_matches = files_with_matches;
    }

    pub fn selected_item(&self) -> Option<&SearchResultItem> {
        self.flat_items.get(self.selected_index)
    }

    fn rebuild_flat_items(&mut self) {
        self.flat_items.clear();

        for file_matches in &self.results {
            // 添加文件头
            self.flat_items.push(SearchResultItem::FileHeader {
                path: file_matches.path.clone(),
                match_count: file_matches.matches.len(),
                expanded: true,
            });

            // 添加匹配行
            for m in &file_matches.matches {
                self.flat_items.push(SearchResultItem::MatchLine {
                    file_path: file_matches.path.clone(),
                    match_info: m.clone(),
                    line_preview: String::new(), // TODO: 从文件读取行内容
                });
            }
        }

        if !self.flat_items.is_empty() && self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
    }

    fn move_up(&mut self) {
        if self.flat_items.is_empty() {
            return;
        }
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else {
            self.selected_index = self.flat_items.len() - 1;
        }
        self.list_state.select(Some(self.selected_index));
    }

    fn move_down(&mut self) {
        if self.flat_items.is_empty() {
            return;
        }
        if self.selected_index < self.flat_items.len() - 1 {
            self.selected_index += 1;
        } else {
            self.selected_index = 0;
        }
        self.list_state.select(Some(self.selected_index));
    }

    fn insert_char(&mut self, c: char) {
        if self.cursor_pos >= self.search_text.len() {
            self.search_text.push(c);
        } else {
            self.search_text.insert(self.cursor_pos, c);
        }
        self.cursor_pos += c.len_utf8();
    }

    fn delete_backward(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let mut char_indices = self.search_text.char_indices();
        let mut prev_pos = 0;
        while let Some((pos, _)) = char_indices.next() {
            if pos >= self.cursor_pos {
                break;
            }
            prev_pos = pos;
        }
        self.search_text.remove(prev_pos);
        self.cursor_pos = prev_pos;
    }

    fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            let mut new_pos = self.cursor_pos - 1;
            while new_pos > 0 && !self.search_text.is_char_boundary(new_pos) {
                new_pos -= 1;
            }
            self.cursor_pos = new_pos;
        }
    }

    fn cursor_right(&mut self) {
        if self.cursor_pos < self.search_text.len() {
            let mut new_pos = self.cursor_pos + 1;
            while new_pos < self.search_text.len() && !self.search_text.is_char_boundary(new_pos) {
                new_pos += 1;
            }
            self.cursor_pos = new_pos;
        }
    }
}

impl Default for GlobalSearchPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl View for GlobalSearchPanel {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        if !self.visible {
            return EventResult::Ignored;
        }

        match event {
            InputEvent::Key(key_event) => {
                match (key_event.code, key_event.modifiers) {
                    // 导航
                    (KeyCode::Up, KeyModifiers::NONE) => {
                        self.move_up();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Down, KeyModifiers::NONE) => {
                        self.move_down();
                        return EventResult::Consumed;
                    }
                    // 输入
                    (KeyCode::Left, KeyModifiers::NONE) => {
                        self.cursor_left();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Right, KeyModifiers::NONE) => {
                        self.cursor_right();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Backspace, KeyModifiers::NONE) => {
                        self.delete_backward();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Char('c'), KeyModifiers::ALT) => {
                        // Alt+C: 切换大小写敏感
                        self.case_sensitive = !self.case_sensitive;
                        return EventResult::Consumed;
                    }
                    (KeyCode::Char('x'), KeyModifiers::ALT) => {
                        // Alt+X: 切换正则模式
                        self.use_regex = !self.use_regex;
                        return EventResult::Consumed;
                    }
                    (KeyCode::Char(c), mods)
                        if mods.is_empty() || mods == KeyModifiers::SHIFT =>
                    {
                        self.insert_char(c);
                        return EventResult::Consumed;
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        EventResult::Ignored
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        self.area = Some(area);
        frame.render_widget(Clear, area);

        // 布局：搜索框 (2行) + 结果列表
        let search_box_height = 2u16;
        let results_height = area.height.saturating_sub(search_box_height);

        let search_area = Rect::new(area.x, area.y, area.width, search_box_height);
        let results_area = Rect::new(
            area.x,
            area.y + search_box_height,
            area.width,
            results_height,
        );

        // 渲染搜索框
        let status = if self.searching {
            format!("Searching... {} files ({} with matches)", self.files_searched, self.files_with_matches)
        } else if self.total_matches > 0 {
            format!("{} results in {} files", self.total_matches, self.results.len())
        } else if !self.search_text.is_empty() {
            "No results".to_string()
        } else {
            "Enter search term".to_string()
        };

        let case_indicator = if self.case_sensitive { "[Aa]" } else { "[aa]" };
        let regex_indicator = if self.use_regex { "[.*]" } else { "[  ]" };

        let search_line = Line::from(vec![
            Span::styled("Search: ", Style::default().fg(Color::Cyan)),
            Span::styled(&self.search_text, Style::default().fg(Color::White)),
            Span::raw(" "),
            Span::styled(case_indicator, Style::default().fg(Color::DarkGray)),
            Span::styled(regex_indicator, Style::default().fg(Color::DarkGray)),
        ]);

        let status_line = Line::from(Span::styled(
            status,
            Style::default().fg(Color::DarkGray),
        ));

        let search_widget = Paragraph::new(vec![search_line, status_line])
            .block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(search_widget, search_area);

        // 渲染结果列表
        let items: Vec<ListItem> = self
            .flat_items
            .iter()
            .map(|item| match item {
                SearchResultItem::FileHeader { path, match_count, .. } => {
                    let file_name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.to_string_lossy().to_string());

                    ListItem::new(Line::from(vec![
                        Span::styled("📄 ", Style::default()),
                        Span::styled(
                            file_name,
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" ({})", match_count),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                }
                SearchResultItem::MatchLine { match_info, .. } => {
                    ListItem::new(Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(
                            format!("L{}: ", match_info.line + 1),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            format!("col {}", match_info.col + 1),
                            Style::default().fg(Color::Cyan),
                        ),
                    ]))
                }
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().title("Results").borders(Borders::NONE))
            .highlight_style(Style::default().bg(Color::DarkGray));

        frame.render_stateful_widget(list, results_area, &mut self.list_state);
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        if !self.visible {
            return None;
        }
        let area = self.area?;
        let x = area.x + "Search: ".len() as u16 + self.cursor_pos as u16;
        let y = area.y;
        Some((x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panel_toggle() {
        let mut panel = GlobalSearchPanel::new();
        assert!(!panel.is_visible());

        panel.toggle();
        assert!(panel.is_visible());

        panel.toggle();
        assert!(!panel.is_visible());
    }

    #[test]
    fn test_input() {
        let mut panel = GlobalSearchPanel::new();
        panel.show();

        panel.insert_char('h');
        panel.insert_char('e');
        panel.insert_char('l');
        panel.insert_char('l');
        panel.insert_char('o');

        assert_eq!(panel.search_text(), "hello");
    }

    #[test]
    fn test_add_results() {
        let mut panel = GlobalSearchPanel::new();

        let file_matches = FileMatches {
            path: PathBuf::from("/test/file.rs"),
            matches: vec![
                Match {
                    start: 0,
                    end: 5,
                    line: 0,
                    col: 0,
                },
                Match {
                    start: 10,
                    end: 15,
                    line: 1,
                    col: 0,
                },
            ],
        };

        panel.add_file_matches(file_matches);

        assert_eq!(panel.total_matches, 2);
        assert_eq!(panel.flat_items.len(), 3); // 1 header + 2 matches
    }
}
