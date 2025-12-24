//! 工作台：统一管理视图和输入分发

use crate::core::event::InputEvent;
use crate::core::view::{ActiveArea, EventResult, View};
use crate::models::{build_file_tree, LoadState, NodeKind};
use crate::runtime::{AppMessage, AsyncRuntime};
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
    runtime: AsyncRuntime,
    show_sidebar: bool,
}

impl Workbench {
    pub fn new(root_path: &Path, runtime: AsyncRuntime) -> std::io::Result<Self> {
        let file_tree = build_file_tree(root_path)?;

        Ok(Self {
            explorer: ExplorerView::new(file_tree),
            editor_group: EditorGroup::new(),
            active_area: ActiveArea::Editor,
            file_service: FileService::new(),
            keybindings: KeybindingService::new(),
            runtime,
            show_sidebar: true,
        })
    }

    pub fn handle_message(&mut self, msg: AppMessage) {
        match msg {
            AppMessage::DirLoaded { path, entries } => {
                if let Some(node_id) = self.explorer.file_tree_mut().find_node_by_path(&path) {
                    for entry in entries {
                        let kind = if entry.is_dir {
                            NodeKind::Dir
                        } else {
                            NodeKind::File
                        };
                        let _ = self.explorer.file_tree_mut().insert_child(
                            node_id,
                            entry.name.into(),
                            kind,
                        );
                    }
                    self.explorer.file_tree_mut().set_load_state(node_id, LoadState::Loaded);
                    self.explorer.refresh_cache();
                }
            }
            AppMessage::DirLoadError { path, error: _ } => {
                if let Some(node_id) = self.explorer.file_tree_mut().find_node_by_path(&path) {
                    self.explorer.file_tree_mut().set_load_state(node_id, LoadState::NotLoaded);
                    self.explorer.file_tree_mut().collapse(node_id);
                    self.explorer.refresh_cache();
                }
            }
            AppMessage::FileLoaded { path, content } => {
                self.editor_group.open_file(path, &content);
                self.active_area = ActiveArea::Editor;
            }
            AppMessage::FileError { path: _, error: _ } => {
                // TODO: 显示错误
            }
        }
    }

    pub fn runtime(&self) -> &AsyncRuntime {
        &self.runtime
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

    fn handle_explorer_result(&mut self, result: EventResult) -> EventResult {
        match result {
            EventResult::OpenFile => {
                self.open_selected_file();
                EventResult::Consumed
            }
            EventResult::LoadDir(path) => {
                self.runtime.load_dir(path);
                EventResult::Consumed
            }
            other => other,
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
                        self.handle_explorer_result(result)
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

                if matches!(self.active_area, ActiveArea::Explorer) {
                    return self.handle_explorer_result(result);
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
    use std::sync::mpsc;
    use tempfile::tempdir;

    fn create_test_runtime() -> AsyncRuntime {
        let (tx, _rx) = mpsc::channel();
        AsyncRuntime::new(tx)
    }

    #[test]
    fn test_workbench_new() {
        let dir = tempdir().unwrap();
        let runtime = create_test_runtime();
        let workbench = Workbench::new(dir.path(), runtime).unwrap();

        assert_eq!(workbench.active_area(), ActiveArea::Editor);
        assert!(workbench.show_sidebar);
    }

    #[test]
    fn test_toggle_sidebar() {
        let dir = tempdir().unwrap();
        let runtime = create_test_runtime();
        let mut workbench = Workbench::new(dir.path(), runtime).unwrap();

        assert!(workbench.show_sidebar);
        workbench.toggle_sidebar();
        assert!(!workbench.show_sidebar);
    }
}
