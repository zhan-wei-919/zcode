//! 工作台：统一管理视图和输入分发
//!
//! 职责：
//! - 管理 ExplorerView 和 EditorGroup
//! - 根据鼠标点击位置切换活跃区域
//! - 分发键盘事件给活跃视图
//! - 处理全局快捷键

use crate::core::event::InputEvent;
use crate::core::view::{ActiveArea, EventResult, View};
use crate::core::Command;
use crate::models::build_file_tree;
use crate::services::{FileService, KeybindingService};
use crate::views::{EditorGroup, ExplorerView};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::path::Path;

const HEADER_HEIGHT: u16 = 1;
const STATUS_HEIGHT: u16 = 1;
const EXPLORER_WIDTH_PERCENT: u16 = 20;

pub struct Workbench {
    explorer: ExplorerView,
    editor_group: EditorGroup,
    active_area: ActiveArea,
    file_service: FileService,
    keybindings: KeybindingService,
    show_sidebar: bool,
}

impl Workbench {
    pub fn new(root_path: &Path) -> std::io::Result<Self> {
        let file_tree = build_file_tree(root_path)?;

        Ok(Self {
            explorer: ExplorerView::new(file_tree),
            editor_group: EditorGroup::new(),
            active_area: ActiveArea::Editor,
            file_service: FileService::new(),
            keybindings: KeybindingService::new(),
            show_sidebar: true,
        })
    }

    pub fn active_area(&self) -> ActiveArea {
        self.active_area
    }

    pub fn explorer(&self) -> &ExplorerView {
        &self.explorer
    }

    pub fn editor_group(&self) -> &EditorGroup {
        &self.editor_group
    }

    pub fn toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
    }

    fn handle_global_key(&mut self, event: &crossterm::event::KeyEvent) -> Option<EventResult> {
        match (event.code, event.modifiers) {
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => Some(EventResult::Quit),
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => {
                self.toggle_sidebar();
                Some(EventResult::Consumed)
            }
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                self.save_active_file();
                Some(EventResult::Consumed)
            }
            (KeyCode::Tab, KeyModifiers::CONTROL) => {
                self.editor_group.next_tab();
                Some(EventResult::Consumed)
            }
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                self.editor_group.close_active_tab();
                Some(EventResult::Consumed)
            }
            _ => None,
        }
    }

    fn handle_mouse_area(&mut self, event: &crossterm::event::MouseEvent) {
        if let MouseEventKind::Down(MouseButton::Left) = event.kind {
            if self.explorer.contains(event.column, event.row) {
                self.active_area = ActiveArea::Explorer;
            } else if self.editor_group.contains(event.column, event.row) {
                self.active_area = ActiveArea::Editor;
            }
        }
    }

    fn open_selected_file(&mut self) {
        if let Some(path) = self.explorer.selected_path() {
            if path.is_file() {
                if let Ok(content) = self.file_service.read_file(&path) {
                    self.editor_group.open_file(path, &content);
                    self.active_area = ActiveArea::Editor;
                }
            }
        }
    }

    fn save_active_file(&mut self) {
        if let Some(editor) = self.editor_group.active_editor_mut() {
            if let Some(path) = editor.file_path().cloned() {
                let content = editor.buffer().text();
                if self.file_service.write_file(&path, &content).is_ok() {
                    editor.set_dirty(false);
                }
            }
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title = "zcode - TUI Editor";
        let header = Paragraph::new(Span::styled(
            title,
            Style::default().fg(Color::Cyan),
        ))
        .block(Block::default().borders(Borders::BOTTOM));
        frame.render_widget(header, area);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let (mode, cursor_info) = if let Some(editor) = self.editor_group.active_editor() {
            let (row, col) = editor.cursor();
            let dirty = if editor.is_dirty() { " [+]" } else { "" };
            let file_name = editor
                .file_path()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string());

            (
                format!("{}{}", file_name, dirty),
                format!("Ln {}, Col {}", row + 1, col + 1),
            )
        } else {
            ("No file".to_string(), String::new())
        };

        let active = match self.active_area {
            ActiveArea::Explorer => "Explorer",
            ActiveArea::Editor => "Editor",
        };

        let status_text = format!("{} | {} | {}", mode, cursor_info, active);
        let status = Paragraph::new(status_text);
        frame.render_widget(status, area);
    }
}

impl View for Workbench {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        match event {
            InputEvent::Key(key_event) => {
                if let Some(result) = self.handle_global_key(key_event) {
                    return result;
                }

                match self.active_area {
                    ActiveArea::Explorer => {
                        let result = self.explorer.handle_input(event);
                        if key_event.code == KeyCode::Enter {
                            self.open_selected_file();
                        }
                        result
                    }
                    ActiveArea::Editor => self.editor_group.handle_input(event),
                }
            }
            InputEvent::Mouse(mouse_event) => {
                self.handle_mouse_area(mouse_event);

                let result = match self.active_area {
                    ActiveArea::Explorer => self.explorer.handle_input(event),
                    ActiveArea::Editor => self.editor_group.handle_input(event),
                };

                // 处理 Explorer 返回的 OpenFile 请求
                if matches!(result, EventResult::OpenFile) {
                    self.open_selected_file();
                    return EventResult::Consumed;
                }

                result
            }
            InputEvent::Resize(_, _) => EventResult::Consumed,
            _ => EventResult::Ignored,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(HEADER_HEIGHT),
                Constraint::Min(0),
                Constraint::Length(STATUS_HEIGHT),
            ])
            .split(area);

        let header_area = chunks[0];
        let body_area = chunks[1];
        let status_area = chunks[2];

        self.render_header(frame, header_area);
        self.render_status(frame, status_area);

        if self.show_sidebar {
            let body_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(EXPLORER_WIDTH_PERCENT),
                    Constraint::Percentage(100 - EXPLORER_WIDTH_PERCENT),
                ])
                .split(body_area);

            self.explorer.render(frame, body_chunks[0]);
            self.editor_group.render(frame, body_chunks[1]);
        } else {
            self.editor_group.render(frame, body_area);
        }

        if let Some((x, y)) = self.cursor_position() {
            frame.set_cursor_position((x, y));
        }
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        match self.active_area {
            ActiveArea::Explorer => None,
            ActiveArea::Editor => self.editor_group.cursor_position(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_workbench_new() {
        let dir = tempdir().unwrap();
        let workbench = Workbench::new(dir.path()).unwrap();

        assert_eq!(workbench.active_area(), ActiveArea::Editor);
        assert!(workbench.show_sidebar);
    }

    #[test]
    fn test_toggle_sidebar() {
        let dir = tempdir().unwrap();
        let mut workbench = Workbench::new(dir.path()).unwrap();

        assert!(workbench.show_sidebar);
        workbench.toggle_sidebar();
        assert!(!workbench.show_sidebar);
    }
}
