//! 编辑器视口管理
//!
//! 负责：
//! - 视口状态（滚动偏移、尺寸）
//! - 布局计算
//! - 坐标转换

use crate::models::{slice_to_cow, TextBuffer};
use ratatui::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub struct Viewport {
    viewport_offset: usize,
    viewport_height: usize,
    horiz_offset: u32,
    viewport_width: usize,
    tab_size: u8,
    area: Option<Rect>,
    /// 是否跟随光标滚动（鼠标滚轮滚动时禁用）
    follow_cursor: bool,
}

impl Viewport {
    pub fn new(tab_size: u8) -> Self {
        Self {
            viewport_offset: 0,
            viewport_height: 20,
            horiz_offset: 0,
            viewport_width: 80,
            tab_size,
            area: None,
            follow_cursor: true,
        }
    }

    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    pub fn viewport_height(&self) -> usize {
        self.viewport_height
    }

    pub fn horiz_offset(&self) -> u32 {
        self.horiz_offset
    }

    pub fn viewport_width(&self) -> usize {
        self.viewport_width
    }

    pub fn tab_size(&self) -> u8 {
        self.tab_size
    }

    pub fn area(&self) -> Option<Rect> {
        self.area
    }

    pub fn set_area(&mut self, area: Rect) {
        self.area = Some(area);
    }

    pub fn update(&mut self, buffer: &TextBuffer, height: usize, width: usize) {
        self.viewport_height = height;
        self.viewport_width = width;

        // 只有在跟随光标模式下才自动滚动到光标位置
        if self.follow_cursor {
            let cursor = buffer.cursor();

            if cursor.0 < self.viewport_offset {
                self.viewport_offset = cursor.0;
            } else if cursor.0 >= self.viewport_offset + height {
                self.viewport_offset = cursor.0.saturating_sub(height.saturating_sub(1));
            }

            let cursor_x = self.get_cursor_display_x(buffer);
            if (cursor_x as u32) < self.horiz_offset {
                self.horiz_offset = cursor_x as u32;
            } else if (cursor_x as u32) >= self.horiz_offset + width as u32 {
                self.horiz_offset = (cursor_x as u32).saturating_sub(width.saturating_sub(1) as u32);
            }
        }
    }

    pub fn scroll_vertical(&mut self, delta: isize, total_lines: usize) {
        // 用户主动滚动时，禁用光标跟随
        self.follow_cursor = false;

        if delta > 0 {
            let max_offset = total_lines.saturating_sub(self.viewport_height);
            self.viewport_offset = (self.viewport_offset + delta as usize).min(max_offset);
        } else {
            self.viewport_offset = self.viewport_offset.saturating_sub((-delta) as usize);
        }
    }

    /// 重新启用光标跟随（当用户进行编辑操作时调用）
    pub fn enable_follow_cursor(&mut self) {
        self.follow_cursor = true;
    }

    pub fn scroll_horizontal(&mut self, delta: isize) {
        if delta > 0 {
            self.horiz_offset = self.horiz_offset.saturating_add(delta as u32);
        } else {
            self.horiz_offset = self.horiz_offset.saturating_sub((-delta) as u32);
        }
    }

    pub fn get_cursor_display_x(&self, buffer: &TextBuffer) -> u16 {
        let (row, col) = buffer.cursor();
        if let Some(slice) = buffer.line_slice(row) {
            let line = slice_to_cow(slice);
            let expanded = self.expand_tabs_cow(&line);

            let graphemes: Vec<&str> = expanded.graphemes(true).collect();
            let mut x = 0u32;
            for (i, g) in graphemes.iter().enumerate() {
                if i >= col {
                    break;
                }
                x += g.width() as u32;
            }

            x.saturating_sub(self.horiz_offset) as u16
        } else {
            0
        }
    }

    pub fn screen_to_pos(&self, x: u16, y: u16, buffer: &TextBuffer) -> Option<(usize, usize)> {
        let area = self.area?;

        if x >= area.width || y >= area.height {
            return None;
        }

        let row = (self.viewport_offset + y as usize).min(buffer.len_lines().saturating_sub(1));

        let slice = buffer.line_slice(row)?;
        let line = slice_to_cow(slice);
        let expanded = self.expand_tabs_cow(&line);
        let graphemes: Vec<&str> = expanded.graphemes(true).collect();

        let target_x = self.horiz_offset + x as u32;
        let mut accumulated_x = 0u32;
        let mut col = 0;

        for (i, g) in graphemes.iter().enumerate() {
            let w = g.width() as u32;
            if accumulated_x + w / 2 >= target_x {
                col = i;
                break;
            }
            accumulated_x += w;
            col = i + 1;
        }

        Some((row, col.min(graphemes.len())))
    }

    /// 展开 tab，接受 Cow<str> 避免不必要的分配
    pub fn expand_tabs_cow(&self, line: &str) -> String {
        let mut expanded = String::new();
        let mut display_col = 0u32;
        let tab_size = self.tab_size as u32;

        for ch in line.chars() {
            if ch == '\t' {
                let remainder = display_col % tab_size;
                let spaces = if remainder == 0 {
                    tab_size
                } else {
                    tab_size - remainder
                };
                for _ in 0..spaces {
                    expanded.push(' ');
                }
                display_col += spaces;
            } else if ch == '\n' {
                break;
            } else {
                expanded.push(ch);
                display_col += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) as u32;
            }
        }

        expanded
    }

    pub fn visible_range(&self, total_lines: usize) -> (usize, usize) {
        let start = self.viewport_offset;
        let end = (start + self.viewport_height).min(total_lines);
        (start, end)
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_scroll() {
        let mut viewport = Viewport::new(4);

        viewport.scroll_vertical(5, 100);
        assert_eq!(viewport.viewport_offset(), 5);

        viewport.scroll_vertical(-2, 100);
        assert_eq!(viewport.viewport_offset(), 3);
    }

    #[test]
    fn test_expand_tabs() {
        let viewport = Viewport::new(4);

        let expanded = viewport.expand_tabs_cow("\thello");
        assert_eq!(expanded, "    hello");

        let expanded = viewport.expand_tabs_cow("a\tb");
        assert_eq!(expanded, "a   b");
    }

    #[test]
    fn test_visible_range() {
        let mut viewport = Viewport::new(4);
        viewport.viewport_height = 10;
        viewport.viewport_offset = 5;

        let (start, end) = viewport.visible_range(100);
        assert_eq!(start, 5);
        assert_eq!(end, 15);

        let (start, end) = viewport.visible_range(8);
        assert_eq!(start, 5);
        assert_eq!(end, 8);
    }
}
