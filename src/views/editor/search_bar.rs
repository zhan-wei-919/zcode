//! 搜索栏视图
//!
//! VS Code 风格的搜索/替换栏，显示在编辑器上方

use crate::core::event::InputEvent;
use crate::core::view::{EventResult, View};
use crate::services::search::{Match, SearchService};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use ropey::Rope;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBarMode {
    Search,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusedField {
    Search,
    Replace,
}

pub struct SearchBar {
    visible: bool,
    mode: SearchBarMode,
    focused_field: FocusedField,
    search_text: String,
    replace_text: String,
    cursor_pos: usize,
    case_sensitive: bool,
    matches: Vec<Match>,
    current_match_index: Option<usize>,
    area: Option<Rect>,
}

impl SearchBar {
    pub fn new() -> Self {
        Self {
            visible: false,
            mode: SearchBarMode::Search,
            focused_field: FocusedField::Search,
            search_text: String::new(),
            replace_text: String::new(),
            cursor_pos: 0,
            case_sensitive: false,
            matches: Vec::new(),
            current_match_index: None,
            area: None,
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn show(&mut self, mode: SearchBarMode) {
        self.visible = true;
        self.mode = mode;
        self.focused_field = FocusedField::Search;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.matches.clear();
        self.current_match_index = None;
    }

    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show(SearchBarMode::Search);
        }
    }

    pub fn toggle_replace_mode(&mut self) {
        if self.mode == SearchBarMode::Search {
            self.mode = SearchBarMode::Replace;
        } else {
            self.mode = SearchBarMode::Search;
        }
    }

    pub fn search_text(&self) -> &str {
        &self.search_text
    }

    pub fn replace_text(&self) -> &str {
        &self.replace_text
    }

    pub fn matches(&self) -> &[Match] {
        &self.matches
    }

    pub fn current_match(&self) -> Option<&Match> {
        self.current_match_index.and_then(|i| self.matches.get(i))
    }

    pub fn current_match_index(&self) -> Option<usize> {
        self.current_match_index
    }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    pub fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub fn height(&self) -> u16 {
        if !self.visible {
            0
        } else {
            match self.mode {
                // 1 行内容 + 1 行边框
                SearchBarMode::Search => 2,
                // 2 行内容 + 1 行边框
                SearchBarMode::Replace => 3,
            }
        }
    }

    /// 执行搜索（同步）
    pub fn search(&mut self, rope: &Rope) {
        if self.search_text.is_empty() {
            self.matches.clear();
            self.current_match_index = None;
            return;
        }

        self.matches = SearchService::search_sync(rope, &self.search_text, self.case_sensitive, false)
            .unwrap_or_default();

        if self.matches.is_empty() {
            self.current_match_index = None;
        } else {
            self.current_match_index = Some(0);
        }
    }

    /// 根据光标位置更新当前匹配索引
    pub fn update_current_match(&mut self, cursor_byte: usize) {
        if self.matches.is_empty() {
            self.current_match_index = None;
            return;
        }

        // 找到光标位置之后的第一个匹配
        for (i, m) in self.matches.iter().enumerate() {
            if m.start >= cursor_byte {
                self.current_match_index = Some(i);
                return;
            }
        }

        // 如果没有找到，回到第一个
        self.current_match_index = Some(0);
    }

    /// 跳转到下一个匹配
    pub fn next_match(&mut self) -> Option<&Match> {
        if self.matches.is_empty() {
            return None;
        }

        let next = match self.current_match_index {
            Some(i) => (i + 1) % self.matches.len(),
            None => 0,
        };
        self.current_match_index = Some(next);
        self.matches.get(next)
    }

    /// 跳转到上一个匹配
    pub fn prev_match(&mut self) -> Option<&Match> {
        if self.matches.is_empty() {
            return None;
        }

        let prev = match self.current_match_index {
            Some(i) => {
                if i == 0 {
                    self.matches.len() - 1
                } else {
                    i - 1
                }
            }
            None => self.matches.len() - 1,
        };
        self.current_match_index = Some(prev);
        self.matches.get(prev)
    }

    fn current_text(&self) -> &str {
        match self.focused_field {
            FocusedField::Search => &self.search_text,
            FocusedField::Replace => &self.replace_text,
        }
    }

    fn current_text_mut(&mut self) -> &mut String {
        match self.focused_field {
            FocusedField::Search => &mut self.search_text,
            FocusedField::Replace => &mut self.replace_text,
        }
    }

    fn insert_char(&mut self, c: char) {
        let cursor_pos = self.cursor_pos;
        let text = self.current_text_mut();
        if cursor_pos >= text.len() {
            text.push(c);
        } else {
            text.insert(cursor_pos, c);
        }
        self.cursor_pos += c.len_utf8();
    }

    fn delete_backward(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }

        let cursor_pos = self.cursor_pos;
        let text = self.current_text_mut();
        let mut char_indices = text.char_indices();
        let mut prev_pos = 0;

        while let Some((pos, _)) = char_indices.next() {
            if pos >= cursor_pos {
                break;
            }
            prev_pos = pos;
        }

        text.remove(prev_pos);
        self.cursor_pos = prev_pos;
    }

    fn delete_forward(&mut self) {
        let cursor_pos = self.cursor_pos;
        let text = self.current_text_mut();
        if cursor_pos < text.len() {
            text.remove(cursor_pos);
        }
    }

    fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            let text = self.current_text();
            let mut new_pos = self.cursor_pos - 1;
            while new_pos > 0 && !text.is_char_boundary(new_pos) {
                new_pos -= 1;
            }
            self.cursor_pos = new_pos;
        }
    }

    fn cursor_right(&mut self) {
        let text = self.current_text();
        if self.cursor_pos < text.len() {
            let mut new_pos = self.cursor_pos + 1;
            while new_pos < text.len() && !text.is_char_boundary(new_pos) {
                new_pos += 1;
            }
            self.cursor_pos = new_pos;
        }
    }

    fn cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    fn cursor_end(&mut self) {
        self.cursor_pos = self.current_text().len();
    }

    fn switch_field(&mut self) {
        if self.mode == SearchBarMode::Replace {
            self.focused_field = match self.focused_field {
                FocusedField::Search => FocusedField::Replace,
                FocusedField::Replace => FocusedField::Search,
            };
            self.cursor_pos = self.current_text().len();
        }
    }
}

impl Default for SearchBar {
    fn default() -> Self {
        Self::new()
    }
}

impl View for SearchBar {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        if !self.visible {
            return EventResult::Ignored;
        }

        match event {
            InputEvent::Key(key_event) => {
                match (key_event.code, key_event.modifiers) {
                    (KeyCode::Enter, KeyModifiers::NONE) => {
                        // Enter: 下一个匹配（由 EditorGroup 处理）
                        return EventResult::Consumed;
                    }
                    (KeyCode::Enter, KeyModifiers::SHIFT) => {
                        // Shift+Enter: 上一个匹配（由 EditorGroup 处理）
                        return EventResult::Consumed;
                    }
                    (KeyCode::Tab, KeyModifiers::NONE) => {
                        self.switch_field();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Char('c'), KeyModifiers::ALT) => {
                        // Alt+C: 切换大小写敏感
                        self.case_sensitive = !self.case_sensitive;
                        return EventResult::Consumed;
                    }
                    (KeyCode::Char('r'), KeyModifiers::ALT) => {
                        // Alt+R: 切换替换模式
                        self.toggle_replace_mode();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Left, KeyModifiers::NONE) => {
                        self.cursor_left();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Right, KeyModifiers::NONE) => {
                        self.cursor_right();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Home, KeyModifiers::NONE) => {
                        self.cursor_home();
                        return EventResult::Consumed;
                    }
                    (KeyCode::End, KeyModifiers::NONE) => {
                        self.cursor_end();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Backspace, KeyModifiers::NONE) => {
                        self.delete_backward();
                        return EventResult::Consumed;
                    }
                    (KeyCode::Delete, KeyModifiers::NONE) => {
                        self.delete_forward();
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

        // 清除背景
        frame.render_widget(Clear, area);

        let match_info = if self.matches.is_empty() {
            if self.search_text.is_empty() {
                String::new()
            } else {
                "No results".to_string()
            }
        } else {
            let current = self.current_match_index.map(|i| i + 1).unwrap_or(0);
            format!("{}/{}", current, self.matches.len())
        };

        let case_indicator = if self.case_sensitive { "[Aa]" } else { "[aa]" };

        // 搜索行
        let search_style = if self.focused_field == FocusedField::Search {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let search_line = Line::from(vec![
            Span::styled("Find: ", Style::default().fg(Color::Cyan)),
            Span::styled(&self.search_text, search_style),
            Span::raw(" "),
            Span::styled(case_indicator, Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(&match_info, Style::default().fg(Color::Yellow)),
        ]);

        let search_para = Paragraph::new(search_line)
            .block(Block::default().borders(Borders::BOTTOM));

        if self.mode == SearchBarMode::Search {
            frame.render_widget(search_para, area);
        } else {
            // 替换模式：两行
            let search_area = Rect::new(area.x, area.y, area.width, 1);
            let replace_area = Rect::new(area.x, area.y + 1, area.width, 1);

            // 重新创建 search_line 因为 search_para 已经消费了它
            let search_line_for_replace = Line::from(vec![
                Span::styled("Find: ", Style::default().fg(Color::Cyan)),
                Span::styled(self.search_text.clone(), search_style),
                Span::raw(" "),
                Span::styled(case_indicator, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(match_info.clone(), Style::default().fg(Color::Yellow)),
            ]);

            let search_para_no_border = Paragraph::new(search_line_for_replace);
            frame.render_widget(search_para_no_border, search_area);

            let replace_style = if self.focused_field == FocusedField::Replace {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let replace_line = Line::from(vec![
                Span::styled("Replace: ", Style::default().fg(Color::Cyan)),
                Span::styled(self.replace_text.clone(), replace_style),
            ]);

            let replace_para = Paragraph::new(replace_line)
                .block(Block::default().borders(Borders::BOTTOM));
            frame.render_widget(replace_para, replace_area);
        }
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        if !self.visible {
            return None;
        }

        let area = self.area?;
        let prefix_len = match self.focused_field {
            FocusedField::Search => "Find: ".len(),
            FocusedField::Replace => "Replace: ".len(),
        };

        let y = match self.focused_field {
            FocusedField::Search => area.y,
            FocusedField::Replace => area.y + 1,
        };

        let x = area.x + prefix_len as u16 + self.cursor_pos as u16;
        Some((x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_bar_toggle() {
        let mut bar = SearchBar::new();
        assert!(!bar.is_visible());

        bar.toggle();
        assert!(bar.is_visible());

        bar.toggle();
        assert!(!bar.is_visible());
    }

    #[test]
    fn test_search_bar_input() {
        let mut bar = SearchBar::new();
        bar.show(SearchBarMode::Search);

        bar.insert_char('h');
        bar.insert_char('e');
        bar.insert_char('l');
        bar.insert_char('l');
        bar.insert_char('o');

        assert_eq!(bar.search_text(), "hello");
    }

    #[test]
    fn test_search_bar_search() {
        let mut bar = SearchBar::new();
        bar.show(SearchBarMode::Search);
        bar.search_text = "hello".to_string();

        let rope = Rope::from_str("hello world hello");
        bar.search(&rope);

        assert_eq!(bar.match_count(), 2);
        assert_eq!(bar.current_match_index(), Some(0));
    }

    #[test]
    fn test_search_bar_navigation() {
        let mut bar = SearchBar::new();
        bar.show(SearchBarMode::Search);
        bar.search_text = "hello".to_string();

        let rope = Rope::from_str("hello world hello");
        bar.search(&rope);

        assert_eq!(bar.current_match_index(), Some(0));

        bar.next_match();
        assert_eq!(bar.current_match_index(), Some(1));

        bar.next_match();
        assert_eq!(bar.current_match_index(), Some(0)); // wrap around

        bar.prev_match();
        assert_eq!(bar.current_match_index(), Some(1));
    }
}
