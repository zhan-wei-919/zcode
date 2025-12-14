//zcode/src/editor/mod.rs
use ratatui::{prelude::*, widgets::{Block, Borders, Paragraph}, text::{Line, Span}};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use unicode_segmentation::UnicodeSegmentation;

// 子模块
pub mod core;
pub mod layout;
pub mod input;
pub mod config;

// 重新导出常用类型（保持向后兼容）
pub use core::{Editor, TextModel, EditorView};
pub use layout::{LayoutEngine, LineLayout};
pub use input::{MouseController, Command, Key, Keybindings, Selection, Granularity};
pub use config::EditorConfig;

use crate::workspace::Workspace;

// UI Layout Constants
const HEADER_HEIGHT: u16 = 3;
const STATUS_HEIGHT: u16 = 1;
const FILE_LIST_WIDTH_PERCENT: u16 = 20;
const EDITOR_AREA_WIDTH_PERCENT: u16 = 80; 

// 使用 View 获取光标 x 坐标（缓存友好，考虑水平滚动）
fn get_cursor_display_x(editor: &mut Editor) -> u16 {
    editor.get_cursor_display_x()
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
    let cursor = workspace.editor.cursor();
    let status = Paragraph::new(format!("Mode: Normal | Line: {} Col: {}", cursor.0 + 1, cursor.1 + 1));
    frame.render_widget(status, status_area);

    let header = Paragraph::new("My TUI Editor").block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, header_area);
    
    // 渲染编辑区，支持滚动
    let editor_area = sub_chunks[1];
    
    // 1. 计算行号区宽度
    let total_lines = workspace.editor.model.len_lines();
    let max_line_width = total_lines.to_string().len(); //最大行号位数
    let gutter_width = (max_line_width + 2) as u16; // +2 for padding (e.g. " 10 ")
    
    // 2. 分割布局：左边行号，右边内容
    let editor_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(gutter_width),
            Constraint::Min(0),
        ])
        .split(editor_area);
        
    let gutter_area = editor_layout[0];
    let content_area = editor_layout[1];

    let viewport_height = content_area.height as usize;
    let viewport_width = content_area.width as usize;
    
    // 保存编辑器区域（用于鼠标事件，注意鼠标点击现在主要针对内容区）
    workspace.editor.view.set_editor_area(content_area);
    
    // 在渲染前更新视口状态
    workspace.editor.update_viewport(viewport_height, viewport_width);
    
    // 只渲染可见行
    let visible_start = workspace.editor.viewport_offset();
    let visible_end = (visible_start + viewport_height).min(total_lines);
    
    // --- 渲染行号 ---
    let gutter_lines: Vec<Line> = (visible_start..visible_end)
        .map(|i| {
            let line_num = i + 1;
            // 右对齐行号，使用暗灰色
            Line::from(Span::styled(
                format!("{:>width$} ", line_num, width = max_line_width),
                Style::default().fg(Color::DarkGray)
            ))
        })
        .collect();
    
    // 渲染行号栏（可以加个右边框或者背景色来区分）
    let gutter_widget = Paragraph::new(gutter_lines)
        .block(Block::default().style(Style::default().bg(Color::Reset))); 
    frame.render_widget(gutter_widget, gutter_area);
    
    // --- 渲染内容 ---
    
    // 获取水平滚动偏移
    let horiz_offset = workspace.editor.view.horiz_offset();
    let tab_size = workspace.editor.view.tab_size() as u32;
    
    // 获取选区（如果有）
    let selection = workspace.editor.model.selection();
    let selection_range = selection.as_ref().map(|s| s.range());
    
    // 渲染可见行
    let visible_lines: Vec<Line> = (visible_start..visible_end)
        .map(|i| {
            render_line_with_selection(
                workspace.editor.model.rope().line(i).as_str().unwrap_or(""),
                i,
                tab_size,
                horiz_offset,
                selection_range,
            )
        })
        .collect();
    
    let paragraph = Paragraph::new(visible_lines).block(Block::default());
    frame.render_widget(paragraph, content_area);

    // 计算光标位置（相对于内容区）
    let cursor = workspace.editor.cursor();
    let cursor_x = content_area.x + get_cursor_display_x(&mut workspace.editor);
    let cursor_y = content_area.y + (cursor.0 - workspace.editor.viewport_offset()) as u16;
    
    // 确保光标在屏幕范围内
    if cursor_y >= content_area.y && cursor_y < content_area.y + content_area.height {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

/// 渲染单行文本，支持选区高亮
/// 
/// # 参数
/// - `line_str`: 原始行文本（可能包含Tab和换行符）
/// - `row`: 行号
/// - `tab_size`: Tab大小
/// - `horiz_offset`: 水平滚动偏移
/// - `selection_range`: 选区范围（如果有）
fn render_line_with_selection(
    line_str: &str,
    row: usize,
    tab_size: u32,
    horiz_offset: u32,
    selection_range: Option<((usize, usize), (usize, usize))>,
) -> Line<'static> {
    // 1. 展开Tab为空格
    let expanded = expand_tabs(line_str, tab_size);
    
    // 2. 将字符串转换为grapheme索引映射
    let graphemes: Vec<&str> = expanded.graphemes(true).collect();
    
    // 3. 如果没有选区，直接渲染
    if selection_range.is_none() {
        return render_line_with_scroll(&graphemes, horiz_offset);
    }
    
    // 4. 有选区：分割成多个Span
    let ((start_row, start_col), (end_row, end_col)) = selection_range.unwrap();
    
    // 判断当前行是否与选区相交
    let (sel_start, sel_end) = if row < start_row || row > end_row {
        // 当前行不在选区内
        return render_line_with_scroll(&graphemes, horiz_offset);
    } else if row == start_row && row == end_row {
        // 选区在单行内
        (start_col, end_col)
    } else if row == start_row {
        // 选区起始行
        (start_col, graphemes.len())
    } else if row == end_row {
        // 选区结束行
        (0, end_col)
    } else {
        // 选区中间行（全选）
        (0, graphemes.len())
    };
    
    // 5. 构建Span列表
    render_line_with_highlight(&graphemes, horiz_offset, sel_start, sel_end)
}

/// 展开Tab为空格
fn expand_tabs(line_str: &str, tab_size: u32) -> String {
    let mut expanded = String::new();
    let mut display_col = 0u32;
    
    for ch in line_str.chars() {
        if ch == '\t' {
            // Tab展开：对齐到下一个tab_size的倍数
            let remainder = display_col % tab_size;
            let spaces_to_add = if remainder == 0 { tab_size } else { tab_size - remainder };
            for _ in 0..spaces_to_add {
                expanded.push(' ');
            }
            display_col += spaces_to_add;
        } else if ch == '\n' {
            // 跳过换行符
            break;
        } else {
            expanded.push(ch);
            display_col += ch.width().unwrap_or(0) as u32;
        }
    }
    
    expanded
}

/// 渲染不带高亮的行（仅水平滚动）
fn render_line_with_scroll(graphemes: &[&str], horiz_offset: u32) -> Line<'static> {
    if horiz_offset == 0 {
        return Line::from(graphemes.join(""));
    }
    
    // 计算跳过的字符数
    let mut accumulated_width = 0u32;
    let mut skip_count = 0;
    
    for g in graphemes {
        if accumulated_width >= horiz_offset {
            break;
        }
        accumulated_width += (*g).width() as u32;
        skip_count += 1;
    }
    
    let visible: String = graphemes.iter().skip(skip_count).copied().collect();
    Line::from(visible)
}

/// 渲染带高亮的行
fn render_line_with_highlight(
    graphemes: &[&str],
    horiz_offset: u32,
    sel_start: usize,
    sel_end: usize,
) -> Line<'static> {
    // 计算水平滚动后的起始索引
    let mut accumulated_width = 0u32;
    let mut skip_count = 0;
    
    for g in graphemes {
        if accumulated_width >= horiz_offset {
            break;
        }
        accumulated_width += (*g).width() as u32;
        skip_count += 1;
    }
    
    // 构建Span列表
    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut in_selection = false;
    
    for (idx, g) in graphemes.iter().enumerate().skip(skip_count) {
        let should_highlight = idx >= sel_start && idx < sel_end;
        
        // 状态转换：进入或离开选区
        if should_highlight != in_selection {
            if !current_text.is_empty() {
                // 保存当前累积的文本
                if in_selection {
                    spans.push(Span::styled(
                        current_text.clone(),
                        Style::default().bg(Color::Blue).fg(Color::White),
                    ));
                } else {
                    spans.push(Span::raw(current_text.clone()));
                }
                current_text.clear();
            }
            in_selection = should_highlight;
        }
        
        current_text.push_str(g);
    }
    
    // 处理最后的文本
    if !current_text.is_empty() {
        if in_selection {
            spans.push(Span::styled(
                current_text,
                Style::default().bg(Color::Blue).fg(Color::White),
            ));
        } else {
            spans.push(Span::raw(current_text));
        }
    }
    
    Line::from(spans)
}