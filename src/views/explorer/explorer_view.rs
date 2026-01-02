//! 文件浏览器视图
//!
//! 实现 View trait，负责：
//! - 渲染文件树
//! - 处理键盘导航
//! - 处理鼠标点击

use crate::core::event::InputEvent;
use crate::core::view::{EventResult, View};
use crate::models::{FileTree, FileTreeRow, LoadState, NodeId};
use crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// 双击判定时间阈值
const DOUBLE_CLICK_MS: u64 = 300;

pub struct ExplorerView {
    file_tree: FileTree,
    area: Option<Rect>,
    scroll_offset: usize,
    cached_rows: Vec<FileTreeRow>,
    /// 上次点击的时间和节点 ID，用于双击检测
    last_click: Option<(Instant, NodeId)>,
}

impl ExplorerView {
    pub fn new(file_tree: FileTree) -> Self {
        let cached_rows = file_tree.flatten_for_view();
        Self {
            file_tree,
            area: None,
            scroll_offset: 0,
            cached_rows,
            last_click: None,
        }
    }

    pub fn file_tree(&self) -> &FileTree {
        &self.file_tree
    }

    pub fn file_tree_mut(&mut self) -> &mut FileTree {
        &mut self.file_tree
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.file_tree.selected()
    }

    pub fn selected_path(&mut self) -> Option<PathBuf> {
        self.file_tree.selected().map(|id| self.file_tree.full_path(id))
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        self.area
            .map(|a| x >= a.x && x < a.x + a.width && y >= a.y && y < a.y + a.height)
            .unwrap_or(false)
    }

    pub fn refresh_cache(&mut self) {
        self.cached_rows = self.file_tree.flatten_for_view();
    }

    /// 尝试展开目录，根据 LoadState 决定行为
    fn try_expand_dir(&mut self, id: NodeId) -> EventResult {
        if self.file_tree.is_expanded(id) {
            self.file_tree.collapse(id);
            self.refresh_cache();
            return EventResult::Consumed;
        }

        match self.file_tree.load_state(id) {
            Some(LoadState::NotLoaded) => {
                self.file_tree.set_load_state(id, LoadState::Loading);
                self.file_tree.expand(id);
                self.refresh_cache();
                let path = self.file_tree.full_path(id);
                EventResult::LoadDir(path)
            }
            Some(LoadState::Loading) => {
                EventResult::Consumed
            }
            Some(LoadState::Loaded) | None => {
                self.file_tree.expand(id);
                self.refresh_cache();
                EventResult::Consumed
            }
        }
    }

    fn handle_key(&mut self, event: &crossterm::event::KeyEvent) -> EventResult {
        match event.code {
            KeyCode::Up => {
                self.move_selection(-1);
                EventResult::Consumed
            }
            KeyCode::Down => {
                self.move_selection(1);
                EventResult::Consumed
            }
            KeyCode::Enter | KeyCode::Right => {
                if let Some(id) = self.file_tree.selected() {
                    if self.file_tree.is_dir(id) {
                        return self.try_expand_dir(id);
                    }
                }
                EventResult::Consumed
            }
            KeyCode::Left => {
                if let Some(id) = self.file_tree.selected() {
                    if self.file_tree.is_dir(id) && self.file_tree.is_expanded(id) {
                        self.file_tree.collapse(id);
                        self.refresh_cache();
                    }
                }
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    fn handle_mouse(&mut self, event: &crossterm::event::MouseEvent) -> EventResult {
        let area = match self.area {
            Some(a) => a,
            None => return EventResult::Ignored,
        };

        if event.column < area.x || event.column >= area.x + area.width {
            return EventResult::Ignored;
        }
        if event.row < area.y || event.row >= area.y + area.height {
            return EventResult::Ignored;
        }

        // Block 有标题行，需要减去 1
        let content_start_y = area.y + 1;

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // 点击在标题行上，忽略
                if event.row < content_start_y {
                    return EventResult::Consumed;
                }

                let row_index = (event.row - content_start_y) as usize + self.scroll_offset;
                if row_index >= self.cached_rows.len() {
                    return EventResult::Consumed;
                }

                let node_id = self.cached_rows[row_index].id;
                let now = Instant::now();

                // 检测双击：同一节点，时间间隔小于阈值
                let is_double_click = self
                    .last_click
                    .map(|(last_time, last_id)| {
                        last_id == node_id
                            && now.duration_since(last_time)
                                < Duration::from_millis(DOUBLE_CLICK_MS)
                    })
                    .unwrap_or(false);

                if is_double_click {
                    // 双击：目录则展开/折叠，文件则打开
                    self.last_click = None;
                    if self.file_tree.is_dir(node_id) {
                        self.try_expand_dir(node_id)
                    } else {
                        // 返回 OpenFile 让 Workbench 处理打开文件
                        EventResult::OpenFile
                    }
                } else {
                    // 单击：选中节点，记录点击信息
                    self.file_tree.set_selected(Some(node_id));
                    self.last_click = Some((now, node_id));
                    EventResult::Consumed
                }
            }
            MouseEventKind::ScrollUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
                EventResult::Consumed
            }
            MouseEventKind::ScrollDown => {
                let max_scroll = self.cached_rows.len().saturating_sub(
                    self.area.map(|a| a.height as usize).unwrap_or(10),
                );
                self.scroll_offset = (self.scroll_offset + 3).min(max_scroll);
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.cached_rows.is_empty() {
            return;
        }

        let current_index = self
            .file_tree
            .selected()
            .and_then(|id| self.cached_rows.iter().position(|r| r.id == id))
            .unwrap_or(0);

        let new_index = if delta < 0 {
            current_index.saturating_sub((-delta) as usize)
        } else {
            (current_index + delta as usize).min(self.cached_rows.len() - 1)
        };

        let new_id = self.cached_rows[new_index].id;
        self.file_tree.set_selected(Some(new_id));

        if let Some(area) = self.area {
            let visible_height = area.height as usize;
            if new_index < self.scroll_offset {
                self.scroll_offset = new_index;
            } else if new_index >= self.scroll_offset + visible_height {
                self.scroll_offset = new_index - visible_height + 1;
            }
        }
    }

    fn render_row(&self, row: &FileTreeRow, is_selected: bool) -> Line<'static> {
        let indent = "  ".repeat(row.depth as usize);
        let icon = if row.is_dir {
            if row.is_expanded {
                "▼ "
            } else {
                "▶ "
            }
        } else {
            "  "
        };

        let name = row.name.to_string_lossy().to_string();
        let text = format!("{}{}{}", indent, icon, name);

        let style = if is_selected {
            Style::default().bg(Color::Blue).fg(Color::White)
        } else if row.is_dir {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        Line::from(Span::styled(text, style))
    }
}

impl View for ExplorerView {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        match event {
            InputEvent::Key(key_event) => self.handle_key(key_event),
            InputEvent::Mouse(mouse_event) => self.handle_mouse(mouse_event),
            _ => EventResult::Ignored,
        }
    }

    fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.area = Some(area);

        let selected_id = self.file_tree.selected();
        let visible_height = area.height as usize;
        let visible_end = (self.scroll_offset + visible_height).min(self.cached_rows.len());

        let lines: Vec<Line> = self.cached_rows[self.scroll_offset..visible_end]
            .iter()
            .map(|row| {
                let is_selected = selected_id == Some(row.id);
                self.render_row(row, is_selected)
            })
            .collect();

        let paragraph =
            Paragraph::new(lines).block(Block::default().borders(Borders::RIGHT).title("Explorer"));

        frame.render_widget(paragraph, area);
    }

    fn cursor_position(&self) -> Option<(u16, u16)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explorer_view_new() {
        use crate::models::FileTree;

        let tree = FileTree::new_with_root_for_test("test".into(), PathBuf::from("/test"));
        let view = ExplorerView::new(tree);

        assert!(view.selected().is_some());
    }
}
