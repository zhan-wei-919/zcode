//! 编辑器视图
//!
//! 实现 View trait，负责：
//! - 渲染文本内容
//! - 处理键盘输入
//! - 处理鼠标交互
//! - 管理选区
//! - Undo/Redo 历史管理
//! - 崩溃恢复（持久化）

use super::viewport::Viewport;
use crate::core::event::InputEvent;
use crate::core::view::{EventResult, View};
use crate::core::Command;
use crate::models::{slice_to_cow, EditHistory, Granularity, Selection, TextBuffer};
use crate::services::{ensure_backup_dir, get_ops_file_path, ClipboardService, EditorConfig};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// 判断字符是否是词边界字符（标点符号等）
fn is_word_boundary_char(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(
            c,
            '（' | '）'
                | '【' | '】'
                | '「' | '」'
                | '，' | '。' | '：' | '；'
        )
}

pub struct EditorView {
    buffer: TextBuffer,
    viewport: Viewport,
    config: EditorConfig,
    file_path: Option<PathBuf>,
    dirty: bool,
    mouse_state: MouseState,
    history: EditHistory,
    clipboard: ClipboardService,
}

struct MouseState {
    last_click: Option<(u16, u16, Instant)>,
    click_count: u8,
    dragging: bool,
}

impl MouseState {
    fn new() -> Self {
        Self {
            last_click: None,
            click_count: 0,
            dragging: false,
        }
    }

    fn on_click(&mut self, x: u16, y: u16, config: &EditorConfig) -> Granularity {
        let now = Instant::now();

        if let Some((lx, ly, lt)) = self.last_click {
            let dx = (x as i32 - lx as i32).abs();
            let dy = (y as i32 - ly as i32).abs();
            let dt = now.duration_since(lt).as_millis() as u64;

            if dx <= config.click_slop as i32
                && dy <= config.click_slop as i32
                && dt < config.triple_click_ms
            {
                self.click_count = (self.click_count % 3) + 1;
            } else {
                self.click_count = 1;
            }
        } else {
            self.click_count = 1;
        }

        self.last_click = Some((x, y, now));
        self.dragging = true;

        match self.click_count {
            1 => Granularity::Char,
            2 => Granularity::Word,
            _ => Granularity::Line,
        }
    }

    fn on_release(&mut self) {
        self.dragging = false;
    }
}

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

        // 尝试启用持久化
        let history = if let Some(ops_path) = ops_file_path {
            // 检查是否有未恢复的备份
            if EditHistory::has_backup(&ops_path) {
                // 尝试恢复
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
                        };
                        view.buffer.set_rope(recovered_rope);
                        view.buffer.set_cursor(cursor.0, cursor.1);
                        return view;
                    }
                    Err(_) => {
                        // 恢复失败，清除损坏的备份文件，使用新的历史
                        let _ = EditHistory::clear_backup(&ops_path);
                        Self::create_history_with_backup(buffer.rope().clone(), ops_path)
                    }
                }
            } else {
                // 没有备份，创建新的带持久化的历史
                Self::create_history_with_backup(buffer.rope().clone(), ops_path)
            }
        } else {
            // 无法获取备份路径，使用内存历史
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
        }
    }

    fn create_history_with_backup(base_snapshot: ropey::Rope, ops_path: PathBuf) -> EditHistory {
        // 确保备份目录存在
        if ensure_backup_dir().is_ok() {
            EditHistory::with_backup(base_snapshot.clone(), ops_path)
                .unwrap_or_else(|_| EditHistory::new(base_snapshot))
        } else {
            EditHistory::new(base_snapshot)
        }
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

        // 清除备份文件
        if let Some(path) = &self.file_path {
            if let Some(ops_path) = get_ops_file_path(path) {
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
            .map(|a| x >= a.x && x < a.x + a.width && y >= a.y && y < a.y + a.height)
            .unwrap_or(false)
    }

    fn handle_key(&mut self, event: &crossterm::event::KeyEvent) -> EventResult {
        // 键盘操作时重新启用光标跟随
        self.viewport.enable_follow_cursor();

        match (event.code, event.modifiers) {
            // 普通光标移动
            (KeyCode::Left, KeyModifiers::NONE) => self.execute(Command::CursorLeft),
            (KeyCode::Right, KeyModifiers::NONE) => self.execute(Command::CursorRight),
            (KeyCode::Up, KeyModifiers::NONE) => self.execute(Command::CursorUp),
            (KeyCode::Down, KeyModifiers::NONE) => self.execute(Command::CursorDown),
            (KeyCode::Home, KeyModifiers::NONE) => self.execute(Command::CursorLineStart),
            (KeyCode::End, KeyModifiers::NONE) => self.execute(Command::CursorLineEnd),
            (KeyCode::Home, KeyModifiers::CONTROL) => self.execute(Command::CursorFileStart),
            (KeyCode::End, KeyModifiers::CONTROL) => self.execute(Command::CursorFileEnd),
            (KeyCode::Left, KeyModifiers::CONTROL) => self.execute(Command::CursorWordLeft),
            (KeyCode::Right, KeyModifiers::CONTROL) => self.execute(Command::CursorWordRight),

            // Shift+方向键：扩展选区
            (KeyCode::Left, KeyModifiers::SHIFT) => self.extend_selection_left(),
            (KeyCode::Right, KeyModifiers::SHIFT) => self.extend_selection_right(),
            (KeyCode::Up, KeyModifiers::SHIFT) => self.extend_selection_up(),
            (KeyCode::Down, KeyModifiers::SHIFT) => self.extend_selection_down(),
            (KeyCode::Home, KeyModifiers::SHIFT) => self.extend_selection_to_line_start(),
            (KeyCode::End, KeyModifiers::SHIFT) => self.extend_selection_to_line_end(),

            // Ctrl+Shift+方向键：按词扩展选区
            (KeyCode::Left, mods) if mods == KeyModifiers::CONTROL | KeyModifiers::SHIFT => {
                self.extend_selection_word_left()
            }
            (KeyCode::Right, mods) if mods == KeyModifiers::CONTROL | KeyModifiers::SHIFT => {
                self.extend_selection_word_right()
            }

            (KeyCode::PageUp, KeyModifiers::NONE) => self.execute(Command::PageUp),
            (KeyCode::PageDown, KeyModifiers::NONE) => self.execute(Command::PageDown),
            (KeyCode::Enter, KeyModifiers::NONE) => self.execute(Command::InsertNewline),
            (KeyCode::Tab, KeyModifiers::NONE) => self.execute(Command::InsertTab),
            (KeyCode::Backspace, KeyModifiers::NONE) => self.execute(Command::DeleteBackward),
            (KeyCode::Delete, KeyModifiers::NONE) => self.execute(Command::DeleteForward),
            (KeyCode::Esc, KeyModifiers::NONE) => self.execute(Command::ClearSelection),
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => return EventResult::Quit,
            // Undo: Ctrl+Z
            (KeyCode::Char('z'), KeyModifiers::CONTROL) => self.undo(),
            // Redo: Shift+Ctrl+Z
            (KeyCode::Char('z'), mods) if mods == KeyModifiers::CONTROL | KeyModifiers::SHIFT => {
                self.redo()
            }
            // Redo: Ctrl+Y (alternative)
            (KeyCode::Char('y'), KeyModifiers::CONTROL) => self.redo(),
            // Copy: Ctrl+C
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => self.copy(),
            // Cut: Ctrl+X
            (KeyCode::Char('x'), KeyModifiers::CONTROL) => self.cut(),
            // Paste: Ctrl+V
            (KeyCode::Char('v'), KeyModifiers::CONTROL) => self.paste(),
            // Select All: Ctrl+A
            (KeyCode::Char('a'), KeyModifiers::CONTROL) => self.execute(Command::SelectAll),
            (KeyCode::Char(c), mods) if mods.is_empty() || mods == KeyModifiers::SHIFT => {
                self.execute(Command::InsertChar(c))
            }
            _ => return EventResult::Ignored,
        }

        EventResult::Consumed
    }

    fn handle_mouse(&mut self, event: &crossterm::event::MouseEvent) -> EventResult {
        let area = match self.viewport.area() {
            Some(a) => a,
            None => return EventResult::Ignored,
        };

        let x = event.column.saturating_sub(area.x);
        let y = event.row.saturating_sub(area.y);

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // 鼠标点击时重新启用光标跟随
                self.viewport.enable_follow_cursor();

                let granularity = self.mouse_state.on_click(x, y, &self.config);

                if let Some(pos) = self.viewport.screen_to_pos(x, y, &self.buffer) {
                    self.buffer.set_cursor(pos.0, pos.1);

                    let mut selection = Selection::new(pos, granularity);
                    if granularity != Granularity::Char {
                        selection.update_cursor(pos, self.buffer.rope());
                    }
                    self.buffer.set_selection(Some(selection));
                }
                EventResult::Consumed
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.mouse_state.dragging {
                    if let Some(pos) = self.viewport.screen_to_pos(x, y, &self.buffer) {
                        self.buffer.update_selection_cursor(pos);
                        self.buffer.set_cursor(pos.0, pos.1);
                    }
                }
                EventResult::Consumed
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.mouse_state.on_release();
                EventResult::Consumed
            }
            MouseEventKind::ScrollUp => {
                let step = self.config.scroll_step();
                self.viewport
                    .scroll_vertical(-(step as isize), self.buffer.len_lines());
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                let step = self.config.scroll_step();
                self.viewport
                    .scroll_vertical(step as isize, self.buffer.len_lines());
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    fn execute(&mut self, command: Command) {
        match command {
            Command::CursorLeft => self.cursor_left(),
            Command::CursorRight => self.cursor_right(),
            Command::CursorUp => self.cursor_up(),
            Command::CursorDown => self.cursor_down(),
            Command::CursorLineStart => {
                let (row, _) = self.buffer.cursor();
                self.buffer.set_cursor(row, 0);
            }
            Command::CursorLineEnd => {
                let (row, _) = self.buffer.cursor();
                let len = self.buffer.line_grapheme_len(row);
                self.buffer.set_cursor(row, len);
            }
            Command::CursorFileStart => {
                self.buffer.set_cursor(0, 0);
            }
            Command::CursorFileEnd => {
                let last = self.buffer.len_lines().saturating_sub(1);
                let len = self.buffer.line_grapheme_len(last);
                self.buffer.set_cursor(last, len);
            }
            Command::PageUp => {
                let height = self.viewport.viewport_height();
                self.viewport
                    .scroll_vertical(-(height as isize), self.buffer.len_lines());
                let (row, col) = self.buffer.cursor();
                let new_row = row.saturating_sub(height);
                self.buffer.set_cursor(new_row, col);
            }
            Command::PageDown => {
                let height = self.viewport.viewport_height();
                self.viewport
                    .scroll_vertical(height as isize, self.buffer.len_lines());
                let (row, col) = self.buffer.cursor();
                let new_row = (row + height).min(self.buffer.len_lines().saturating_sub(1));
                self.buffer.set_cursor(new_row, col);
            }
            Command::InsertChar(c) => {
                self.delete_selection();
                self.insert_char(c);
            }
            Command::InsertNewline => {
                self.delete_selection();
                self.insert_char('\n');
            }
            Command::InsertTab => {
                self.delete_selection();
                self.insert_char('\t');
            }
            Command::DeleteBackward => {
                if !self.delete_selection() {
                    self.delete_backward();
                }
            }
            Command::DeleteForward => {
                if !self.delete_selection() {
                    self.delete_forward();
                }
            }
            Command::ClearSelection => {
                self.buffer.clear_selection();
            }
            Command::CursorWordLeft => self.cursor_word_left(),
            Command::CursorWordRight => self.cursor_word_right(),
            Command::SelectAll => self.select_all(),
            _ => {}
        }
    }

    fn cursor_left(&mut self) {
        let (row, col) = self.buffer.cursor();
        if col > 0 {
            self.buffer.set_cursor(row, col - 1);
        } else if row > 0 {
            let prev_len = self.buffer.line_grapheme_len(row - 1);
            self.buffer.set_cursor(row - 1, prev_len);
        }
    }

    fn cursor_right(&mut self) {
        let (row, col) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);
        if col < line_len {
            self.buffer.set_cursor(row, col + 1);
        } else if row + 1 < self.buffer.len_lines() {
            self.buffer.set_cursor(row + 1, 0);
        }
    }

    fn cursor_up(&mut self) {
        let (row, col) = self.buffer.cursor();
        if row > 0 {
            let new_len = self.buffer.line_grapheme_len(row - 1);
            self.buffer.set_cursor(row - 1, col.min(new_len));
        }
    }

    fn cursor_down(&mut self) {
        let (row, col) = self.buffer.cursor();
        if row + 1 < self.buffer.len_lines() {
            let new_len = self.buffer.line_grapheme_len(row + 1);
            self.buffer.set_cursor(row + 1, col.min(new_len));
        }
    }

    fn cursor_word_left(&mut self) {
        let (row, col) = self.buffer.cursor();

        if col == 0 {
            // 行首，跳到上一行末尾
            if row > 0 {
                let prev_len = self.buffer.line_grapheme_len(row - 1);
                self.buffer.set_cursor(row - 1, prev_len);
            }
            return;
        }

        // 获取当前行文本
        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();

        // 从当前位置向左找词边界
        let mut pos = col.min(graphemes.len());

        // 跳过当前位置左边的空白
        while pos > 0 && graphemes[pos - 1].chars().all(|c| c.is_whitespace()) {
            pos -= 1;
        }

        // 跳过词字符
        while pos > 0 && !graphemes[pos - 1].chars().all(|c| c.is_whitespace() || is_word_boundary_char(c)) {
            pos -= 1;
        }

        self.buffer.set_cursor(row, pos);
    }

    fn cursor_word_right(&mut self) {
        let (row, col) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);

        if col >= line_len {
            // 行尾，跳到下一行开头
            if row + 1 < self.buffer.len_lines() {
                self.buffer.set_cursor(row + 1, 0);
            }
            return;
        }

        // 获取当前行文本
        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();
        let len = graphemes.len();

        let mut pos = col;

        // 跳过当前词字符
        while pos < len && !graphemes[pos].chars().all(|c| c.is_whitespace() || is_word_boundary_char(c)) {
            pos += 1;
        }

        // 跳过空白和标点
        while pos < len && graphemes[pos].chars().all(|c| c.is_whitespace() || is_word_boundary_char(c)) {
            pos += 1;
        }

        self.buffer.set_cursor(row, pos.min(line_len));
    }

    fn select_all(&mut self) {
        let last_line = self.buffer.len_lines().saturating_sub(1);
        let last_col = self.buffer.line_grapheme_len(last_line);

        let mut selection = Selection::new((0, 0), Granularity::Char);
        selection.update_cursor((last_line, last_col), self.buffer.rope());
        self.buffer.set_selection(Some(selection));
        self.buffer.set_cursor(last_line, last_col);
    }

    /// 开始或继续扩展选区
    /// 如果没有选区，以当前光标位置为 anchor 创建选区
    fn ensure_selection(&mut self) {
        if self.buffer.selection().is_none() {
            let pos = self.buffer.cursor();
            self.buffer.set_selection(Some(Selection::new(pos, Granularity::Char)));
        }
    }

    /// 扩展选区：向左一个字符
    fn extend_selection_left(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let new_pos = if col > 0 {
            (row, col - 1)
        } else if row > 0 {
            let prev_len = self.buffer.line_grapheme_len(row - 1);
            (row - 1, prev_len)
        } else {
            (row, col)
        };
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    /// 扩展选区：向右一个字符
    fn extend_selection_right(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);
        let new_pos = if col < line_len {
            (row, col + 1)
        } else if row + 1 < self.buffer.len_lines() {
            (row + 1, 0)
        } else {
            (row, col)
        };
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    /// 扩展选区：向上一行
    fn extend_selection_up(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        if row > 0 {
            let new_len = self.buffer.line_grapheme_len(row - 1);
            let new_pos = (row - 1, col.min(new_len));
            self.buffer.update_selection_cursor(new_pos);
            self.buffer.set_cursor(new_pos.0, new_pos.1);
        }
    }

    /// 扩展选区：向下一行
    fn extend_selection_down(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        if row + 1 < self.buffer.len_lines() {
            let new_len = self.buffer.line_grapheme_len(row + 1);
            let new_pos = (row + 1, col.min(new_len));
            self.buffer.update_selection_cursor(new_pos);
            self.buffer.set_cursor(new_pos.0, new_pos.1);
        }
    }

    /// 扩展选区：到行首
    fn extend_selection_to_line_start(&mut self) {
        self.ensure_selection();
        let (row, _) = self.buffer.cursor();
        let new_pos = (row, 0);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    /// 扩展选区：到行尾
    fn extend_selection_to_line_end(&mut self) {
        self.ensure_selection();
        let (row, _) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);
        let new_pos = (row, line_len);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    /// 扩展选区：向左一个词
    fn extend_selection_word_left(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();

        if col == 0 {
            if row > 0 {
                let prev_len = self.buffer.line_grapheme_len(row - 1);
                let new_pos = (row - 1, prev_len);
                self.buffer.update_selection_cursor(new_pos);
                self.buffer.set_cursor(new_pos.0, new_pos.1);
            }
            return;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();

        let mut pos = col.min(graphemes.len());

        while pos > 0 && graphemes[pos - 1].chars().all(|c| c.is_whitespace()) {
            pos -= 1;
        }

        while pos > 0 && !graphemes[pos - 1].chars().all(|c| c.is_whitespace() || is_word_boundary_char(c)) {
            pos -= 1;
        }

        let new_pos = (row, pos);
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    /// 扩展选区：向右一个词
    fn extend_selection_word_right(&mut self) {
        self.ensure_selection();
        let (row, col) = self.buffer.cursor();
        let line_len = self.buffer.line_grapheme_len(row);

        if col >= line_len {
            if row + 1 < self.buffer.len_lines() {
                let new_pos = (row + 1, 0);
                self.buffer.update_selection_cursor(new_pos);
                self.buffer.set_cursor(new_pos.0, new_pos.1);
            }
            return;
        }

        let line_slice = match self.buffer.line_slice(row) {
            Some(s) => s,
            None => return,
        };
        let line = slice_to_cow(line_slice);
        let graphemes: Vec<&str> = line.graphemes(true).collect();
        let len = graphemes.len();

        let mut pos = col;

        while pos < len && !graphemes[pos].chars().all(|c| c.is_whitespace() || is_word_boundary_char(c)) {
            pos += 1;
        }

        while pos < len && graphemes[pos].chars().all(|c| c.is_whitespace() || is_word_boundary_char(c)) {
            pos += 1;
        }

        let new_pos = (row, pos.min(line_len));
        self.buffer.update_selection_cursor(new_pos);
        self.buffer.set_cursor(new_pos.0, new_pos.1);
    }

    fn insert_char(&mut self, c: char) {
        let parent = self.history.head();
        let op = self.buffer.insert_char_op(c, parent);
        self.history.push(op, self.buffer.rope());
        self.dirty = true;
    }

    fn delete_backward(&mut self) {
        let parent = self.history.head();
        if let Some(op) = self.buffer.delete_backward_op(parent) {
            self.history.push(op, self.buffer.rope());
            self.dirty = true;
        }
    }

    fn delete_forward(&mut self) {
        let parent = self.history.head();
        if let Some(op) = self.buffer.delete_forward_op(parent) {
            self.history.push(op, self.buffer.rope());
            self.dirty = true;
        }
    }

    fn delete_selection(&mut self) -> bool {
        let parent = self.history.head();
        if let Some(op) = self.buffer.delete_selection_op(parent) {
            self.history.push(op, self.buffer.rope());
            self.dirty = true;
            true
        } else {
            false
        }
    }

    fn undo(&mut self) {
        if let Some((rope, cursor)) = self.history.undo() {
            self.buffer.set_rope(rope);
            self.buffer.set_cursor(cursor.0, cursor.1);
            self.dirty = self.history.is_dirty();
        }
    }

    fn redo(&mut self) {
        if let Some((rope, cursor)) = self.history.redo() {
            self.buffer.set_rope(rope);
            self.buffer.set_cursor(cursor.0, cursor.1);
            self.dirty = self.history.is_dirty();
        }
    }

    fn copy(&mut self) {
        if let Some(text) = self.buffer.get_selection_text() {
            let _ = self.clipboard.set_text(&text);
        }
    }

    fn cut(&mut self) {
        if let Some(text) = self.buffer.get_selection_text() {
            if self.clipboard.set_text(&text).is_ok() {
                self.delete_selection();
            }
        }
    }

    fn paste(&mut self) {
        if let Ok(text) = self.clipboard.get_text() {
            if !text.is_empty() {
                self.delete_selection();
                self.insert_str(&text);
            }
        }
    }

    fn insert_str(&mut self, s: &str) {
        let parent = self.history.head();
        let op = self.buffer.insert_str_op(s, parent);
        self.history.push(op, self.buffer.rope());
        self.dirty = true;
    }

    /// 处理 bracketed paste 事件
    /// TODO: 大文本粘贴优化（>10MB 时考虑分块处理或警告）
    fn handle_paste(&mut self, text: &str) {
        const PASTE_MAX_SIZE: usize = 10 * 1024 * 1024; // 10MB
        if text.len() > PASTE_MAX_SIZE || text.is_empty() {
            return;
        }
        self.delete_selection();
        self.insert_str(text);
    }

    fn render_line(&self, line_str: &str, row: usize) -> Line<'static> {
        let expanded = self.viewport.expand_tabs_cow(line_str);
        let graphemes: Vec<&str> = expanded.graphemes(true).collect();

        let selection = self.buffer.selection();
        let selection_range = selection.map(|s| s.range());

        if selection_range.is_none() {
            return self.render_line_plain(&graphemes);
        }

        let ((start_row, start_col), (end_row, end_col)) = selection_range.unwrap();

        if row < start_row || row > end_row {
            return self.render_line_plain(&graphemes);
        }

        let (sel_start, sel_end) = if row == start_row && row == end_row {
            (start_col, end_col)
        } else if row == start_row {
            (start_col, graphemes.len())
        } else if row == end_row {
            (0, end_col)
        } else {
            (0, graphemes.len())
        };

        self.render_line_with_selection(&graphemes, sel_start, sel_end)
    }

    fn render_line_plain(&self, graphemes: &[&str]) -> Line<'static> {
        let horiz = self.viewport.horiz_offset() as usize;
        let mut skip = 0;
        let mut acc = 0usize;

        for g in graphemes.iter() {
            if acc >= horiz {
                break;
            }
            acc += g.width();
            skip += 1;
        }

        let visible: String = graphemes.iter().skip(skip).copied().collect();
        Line::from(visible)
    }

    fn render_line_with_selection(
        &self,
        graphemes: &[&str],
        sel_start: usize,
        sel_end: usize,
    ) -> Line<'static> {
        let horiz = self.viewport.horiz_offset() as usize;
        let mut skip = 0;
        let mut acc = 0usize;

        for g in graphemes.iter() {
            if acc >= horiz {
                break;
            }
            acc += g.width();
            skip += 1;
        }

        let mut spans = Vec::new();
        let mut current = String::new();
        let mut in_sel = false;

        for (idx, g) in graphemes.iter().enumerate().skip(skip) {
            let should_highlight = idx >= sel_start && idx < sel_end;

            if should_highlight != in_sel {
                if !current.is_empty() {
                    if in_sel {
                        spans.push(Span::styled(
                            current.clone(),
                            Style::default().bg(Color::Blue).fg(Color::White),
                        ));
                    } else {
                        spans.push(Span::raw(current.clone()));
                    }
                    current.clear();
                }
                in_sel = should_highlight;
            }
            current.push_str(g);
        }

        if !current.is_empty() {
            if in_sel {
                spans.push(Span::styled(
                    current,
                    Style::default().bg(Color::Blue).fg(Color::White),
                ));
            } else {
                spans.push(Span::raw(current));
            }
        }

        Line::from(spans)
    }
}

impl Default for EditorView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for EditorView {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        match event {
            InputEvent::Key(key_event) => self.handle_key(key_event),
            InputEvent::Mouse(mouse_event) => self.handle_mouse(mouse_event),
            InputEvent::Paste(text) => {
                self.handle_paste(text);
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let total_lines = self.buffer.len_lines();
        let max_line_width = total_lines.to_string().len();
        let gutter_width = (max_line_width + 2) as u16;

        let content_width = area.width.saturating_sub(gutter_width);
        let content_area = Rect::new(
            area.x + gutter_width,
            area.y,
            content_width,
            area.height,
        );

        self.viewport.set_area(content_area);
        self.viewport
            .update(&self.buffer, area.height as usize, content_width as usize);

        let (visible_start, visible_end) = self.viewport.visible_range(total_lines);

        let gutter_lines: Vec<Line> = (visible_start..visible_end)
            .map(|i| {
                Line::from(Span::styled(
                    format!("{:>width$} ", i + 1, width = max_line_width),
                    Style::default().fg(Color::DarkGray),
                ))
            })
            .collect();

        let gutter_area = Rect::new(area.x, area.y, gutter_width, area.height);
        let gutter_widget = Paragraph::new(gutter_lines);
        frame.render_widget(gutter_widget, gutter_area);

        let content_lines: Vec<Line> = (visible_start..visible_end)
            .map(|i| {
                if let Some(slice) = self.buffer.line_slice(i) {
                    let line_str = slice_to_cow(slice);
                    self.render_line(&line_str, i)
                } else {
                    Line::default()
                }
            })
            .collect();

        let content_widget = Paragraph::new(content_lines).block(Block::default());
        frame.render_widget(content_widget, content_area);
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        let area = self.viewport.area()?;
        let (row, _) = self.buffer.cursor();
        let offset = self.viewport.viewport_offset();

        if row < offset || row >= offset + self.viewport.viewport_height() {
            return None;
        }

        let x = area.x + self.viewport.get_cursor_display_x(&self.buffer);
        let y = area.y + (row - offset) as u16;

        Some((x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_view_new() {
        let view = EditorView::new();
        assert_eq!(view.cursor(), (0, 0));
        assert!(!view.is_dirty());
    }

    #[test]
    fn test_editor_view_from_text() {
        let view = EditorView::from_text("hello\nworld");
        assert_eq!(view.buffer().len_lines(), 2);
    }

    #[test]
    fn test_cursor_movement() {
        let mut view = EditorView::from_text("hello\nworld");

        view.execute(Command::CursorRight);
        assert_eq!(view.cursor(), (0, 1));

        view.execute(Command::CursorDown);
        assert_eq!(view.cursor(), (1, 1));

        view.execute(Command::CursorLineEnd);
        assert_eq!(view.cursor(), (1, 5));
    }

    #[test]
    fn test_insert_char() {
        let mut view = EditorView::new();
        view.execute(Command::InsertChar('a'));
        assert_eq!(view.buffer().text(), "a");
        assert!(view.is_dirty());
    }

    #[test]
    fn test_undo_redo() {
        let mut view = EditorView::new();

        // 插入 "abc"
        view.execute(Command::InsertChar('a'));
        view.execute(Command::InsertChar('b'));
        view.execute(Command::InsertChar('c'));
        assert_eq!(view.buffer().text(), "abc");
        assert_eq!(view.cursor(), (0, 3));

        // Undo 一次
        view.undo();
        assert_eq!(view.buffer().text(), "ab");
        assert_eq!(view.cursor(), (0, 2));

        // Undo 再一次
        view.undo();
        assert_eq!(view.buffer().text(), "a");
        assert_eq!(view.cursor(), (0, 1));

        // Redo
        view.redo();
        assert_eq!(view.buffer().text(), "ab");
        assert_eq!(view.cursor(), (0, 2));

        // Redo 再一次
        view.redo();
        assert_eq!(view.buffer().text(), "abc");
        assert_eq!(view.cursor(), (0, 3));
    }

    #[test]
    fn test_undo_redo_with_delete() {
        let mut view = EditorView::from_text("hello");
        view.execute(Command::CursorLineEnd);
        assert_eq!(view.cursor(), (0, 5));

        // 删除 'o'
        view.execute(Command::DeleteBackward);
        assert_eq!(view.buffer().text(), "hell");
        assert_eq!(view.cursor(), (0, 4));

        // Undo
        view.undo();
        assert_eq!(view.buffer().text(), "hello");
        assert_eq!(view.cursor(), (0, 5));

        // Redo
        view.redo();
        assert_eq!(view.buffer().text(), "hell");
        assert_eq!(view.cursor(), (0, 4));
    }

    #[test]
    fn test_undo_branch() {
        let mut view = EditorView::new();

        // 插入 "ab"
        view.execute(Command::InsertChar('a'));
        view.execute(Command::InsertChar('b'));
        assert_eq!(view.buffer().text(), "ab");

        // Undo 一次（回到 "a"）
        view.undo();
        assert_eq!(view.buffer().text(), "a");

        // 插入 "c"（创建分支）
        view.execute(Command::InsertChar('c'));
        assert_eq!(view.buffer().text(), "ac");

        // Undo（回到 "a"）
        view.undo();
        assert_eq!(view.buffer().text(), "a");

        // Redo（应该走最新的分支，即 "c"）
        view.redo();
        assert_eq!(view.buffer().text(), "ac");
    }
}
