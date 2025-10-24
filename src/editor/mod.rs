//zcode/src/editor/mod.rs
use ratatui::{prelude::*, widgets::{Block, Borders, Paragraph}};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub mod state;
pub mod input;

use self::state::Editor;
use crate::workspace::Workspace;

// UI Layout Constants
const HEADER_HEIGHT: u16 = 3;
const STATUS_HEIGHT: u16 = 1;
const FILE_LIST_WIDTH_PERCENT: u16 = 20;
const EDITOR_AREA_WIDTH_PERCENT: u16 = 80; 

fn get_cursor_display_x(editor: &Editor) -> u16 {
    let (row, col) = editor.cursor;
    editor.rope.line(row)
        .as_str()
        .unwrap_or("")
        .graphemes(true)
        .take(col)
        .map(|g| g.width())
        .sum::<usize>() as u16
}

pub fn render(workspace: &mut Workspace, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(HEADER_HEIGHT),
            Constraint::Fill(1),
            Constraint::Length(STATUS_HEIGHT),
        ])
        .split(frame.area());

    let header_area = chunks[0];
    let sub_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(FILE_LIST_WIDTH_PERCENT),
            Constraint::Percentage(EDITOR_AREA_WIDTH_PERCENT)
        ])
        .split(chunks[1]);
    
    // 渲染文件列表
    let rows = workspace.file_tree.flatten_for_view();
    let file_list_text: String = rows
        .iter()
        .map(|row| {
            let indent = "  ".repeat(row.depth as usize);
            let icon = if row.is_dir {
                if row.is_expanded { "▼ " } else { "▶ " }
            } else {
                "  "
            };
            format!("{}{}{}", indent, icon, row.name.to_string_lossy())
        })
        .collect::<Vec<_>>()
        .join("\n");
    
    let file_list = Paragraph::new(file_list_text)
        .block(Block::default().borders(Borders::RIGHT));
    frame.render_widget(file_list, sub_chunks[0]);
    
    let status_area = chunks[2];
    let status = Paragraph::new(format!("Mode: Normal | Line: {} Col: {}", workspace.editor.cursor.0 + 1, workspace.editor.cursor.1 + 1));
    frame.render_widget(status, status_area);

    let header = Paragraph::new("My TUI Editor").block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, header_area);
    
    // 渲染编辑区，支持滚动
    let editor_area = sub_chunks[1];
    let viewport_height = editor_area.height as usize;
    
    // 在渲染前更新视口状态（状态更新集中在此处）
    workspace.editor.update_viewport(viewport_height);
    
    // 只渲染可见行
    let total_lines = workspace.editor.rope.len_lines();
    let visible_start = workspace.editor.viewport_offset;
    let visible_end = (workspace.editor.viewport_offset + viewport_height).min(total_lines);
    
    let visible_text: String = (visible_start..visible_end)
        .map(|i| workspace.editor.rope.line(i).as_str().unwrap_or(""))
        .collect();
    
    let paragraph = Paragraph::new(visible_text).block(Block::default());
    frame.render_widget(paragraph, editor_area);

    // 计算光标位置（相对于视口）
    let cursor_x = editor_area.x + get_cursor_display_x(&workspace.editor);
    let cursor_y = editor_area.y + (workspace.editor.cursor.0 - workspace.editor.viewport_offset) as u16;
    
    // 确保光标在屏幕范围内
    if cursor_y >= editor_area.y && cursor_y < editor_area.y + editor_area.height {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}