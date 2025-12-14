//zcode/src/editor/core/view.rs
//! 视图层：管理渲染、布局、滚动
//! 
//! 职责：
//! - 视口管理（垂直/水平滚动）
//! - 布局计算（行宽、光标位置）
//! - 屏幕坐标 ↔ 文档位置转换
//! - 渲染区域信息

use crate::editor::layout::LayoutEngine;
use super::text_model::TextModel;
use ropey::Rope;

/// 编辑器视图：管理渲染和视口状态
pub struct EditorView {
    /// 布局引擎（管理行布局缓存）
    layout_engine: LayoutEngine,
    
    /// 垂直滚动偏移（视口顶部行号）
    viewport_offset: usize,
    
    /// 视口高度（行数）
    viewport_height: usize,
    
    /// 水平滚动偏移（列）
    horiz_offset: u32,
    
    /// 视口宽度（列数）
    viewport_width: usize,
    
    /// 编辑器区域（由渲染层设置）
    editor_area: Option<ratatui::layout::Rect>,
}

impl EditorView {
    /// 创建新视图
    pub fn new(tab_size: u8) -> Self {
        Self {
            layout_engine: LayoutEngine::new(tab_size),
            viewport_offset: 0,
            viewport_height: 20,
            horiz_offset: 0,
            viewport_width: 80,
            editor_area: None,
        }
    }
    
    // ==================== 视口状态访问 ====================
    
    /// 获取垂直滚动偏移
    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }
    
    /// 获取Tab大小
    pub fn tab_size(&self) -> u8 {
        self.layout_engine.tab_size()
    }
    
    /// 获取视口高度
    pub fn viewport_height(&self) -> usize {
        self.viewport_height
    }
    
    /// 获取水平滚动偏移
    pub fn horiz_offset(&self) -> u32 {
        self.horiz_offset
    }
    
    /// 获取视口宽度
    pub fn viewport_width(&self) -> usize {
        self.viewport_width
    }
    
    /// 获取编辑器区域
    pub fn editor_area(&self) -> Option<ratatui::layout::Rect> {
        self.editor_area
    }
    
    /// 设置编辑器区域
    pub fn set_editor_area(&mut self, area: ratatui::layout::Rect) {
        self.editor_area = Some(area);
    }
    
    /// 获取布局引擎的可变引用
    pub fn layout_engine_mut(&mut self) -> &mut LayoutEngine {
        &mut self.layout_engine
    }
    
    // ==================== 视口更新 ====================
    
    /// 更新视口尺寸并确保光标可见
    pub fn update_viewport(&mut self, model: &TextModel, viewport_height: usize, viewport_width: usize) {
        self.viewport_height = viewport_height;
        self.viewport_width = viewport_width;
        
        let cursor = model.cursor();
        
        // 垂直滚动：确保光标行可见
        if cursor.0 < self.viewport_offset {
            self.viewport_offset = cursor.0;
        } else if cursor.0 >= self.viewport_offset + viewport_height {
            self.viewport_offset = cursor.0.saturating_sub(viewport_height.saturating_sub(1));
        }
        
        // 水平滚动：确保光标列可见
        let layout = self.layout_engine.layout_line(model.rope(), cursor.0);
        let cursor_doc_x = if cursor.1 >= layout.cell_x.len() {
            layout.display_width
        } else {
            layout.cell_x[cursor.1]
        };
        
        if cursor_doc_x < self.horiz_offset {
            self.horiz_offset = cursor_doc_x;
        } else if cursor_doc_x >= self.horiz_offset + viewport_width as u32 {
            self.horiz_offset = cursor_doc_x.saturating_sub(viewport_width.saturating_sub(1) as u32);
        }
    }
    
    /// 垂直滚动
    pub fn scroll_vertical(&mut self, delta: isize, total_lines: usize) {
        if delta > 0 {
            let max_offset = total_lines.saturating_sub(self.viewport_height);
            self.viewport_offset = (self.viewport_offset + delta as usize).min(max_offset);
        } else {
            self.viewport_offset = self.viewport_offset.saturating_sub((-delta) as usize);
        }
    }
    
    /// 水平滚动
    pub fn scroll_horizontal(&mut self, delta: isize) {
        if delta > 0 {
            self.horiz_offset = self.horiz_offset.saturating_add(delta as u32);
        } else {
            self.horiz_offset = self.horiz_offset.saturating_sub((-delta) as u32);
        }
    }
    
    // ==================== 坐标转换 ====================
    
    /// 获取光标的显示 x 坐标（考虑水平滚动）
    pub fn get_cursor_display_x(&mut self, model: &TextModel) -> u16 {
        let (row, col) = model.cursor();
        self.layout_engine.get_cursor_x(model.rope(), row, col, self.horiz_offset)
    }
    
    /// 屏幕坐标 → 文档位置
    /// 
    /// 参数 x, y 应该是**相对于编辑器区域**的坐标（已经减去 area.x 和 area.y）
    pub fn screen_to_pos(&mut self, x: u16, y: u16, model: &TextModel) -> Option<(usize, usize)> {
        let area = self.editor_area?;
        
        // 检查是否在编辑器区域内（x, y 已经是相对坐标）
        if x >= area.width || y >= area.height {
            return None;
        }
        
        // 计算行号
        let relative_y = y as usize;
        let row = (self.viewport_offset + relative_y).min(model.len_lines().saturating_sub(1));
        
        // 计算列号（考虑水平偏移）
        let col = self.layout_engine.hit_test_x(model.rope(), row, x, self.horiz_offset);
        
        Some((row, col))
    }
    
    // ==================== 布局失效 ====================
    
    /// 使指定行范围的布局失效
    pub fn invalidate_layout_range(&mut self, start_row: usize, end_row: usize) {
        self.layout_engine.invalidate_range(start_row, end_row);
    }
    
    /// 使所有布局失效
    pub fn invalidate_all_layout(&mut self) {
        self.layout_engine.invalidate_all();
    }
    
    // ==================== 字符索引（O(1)，性能关键） ====================
    
    /// 位置 → 字符偏移（O(1)）
    /// 
    /// 将 (row, col) 转换为文档中的绝对字符偏移。
    /// 使用布局缓存实现 O(1) 查询，避免 O(N) 扫描。
    /// 
    /// # 参数
    /// - `model`: TextModel（用于获取 rope 和行数）
    /// - `pos`: (row, col) 位置
    /// 
    /// # 返回
    /// 文档中的绝对字符偏移
    /// 
    /// # 性能
    /// - O(1) 如果布局已缓存
    /// - O(行长度) 如果需要计算布局（首次访问该行）
    pub fn pos_to_char(&mut self, model: &TextModel, pos: (usize, usize)) -> usize {
        let (row, col) = pos;
        
        // 行首的字符偏移
        let line_char_offset = model.rope().line_to_char(row);
        
        // 列的字符偏移（O(1) 通过布局缓存）
        let col_char_offset = self.layout_engine.grapheme_to_char_index(model.rope(), row, col);
        
        line_char_offset + col_char_offset
    }
}

impl Default for EditorView {
    fn default() -> Self {
        Self::new(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_viewport_update() {
        let mut view = EditorView::new(4);
        let mut model = TextModel::from_text("line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12");
        
        // 初始状态
        assert_eq!(view.viewport_offset(), 0);
        
        // 光标在视口外（向下，第10行）
        model.set_cursor(10, 0);
        view.update_viewport(&model, 5, 80); // 视口高度5行
        
        // 视口应该滚动，使光标可见
        assert!(view.viewport_offset() > 0);
        assert!(view.viewport_offset() <= 10);
        assert!(10 < view.viewport_offset() + 5); // 光标在视口内
    }
    
    #[test]
    fn test_scroll() {
        let mut view = EditorView::new(4);
        
        // 向下滚动
        view.scroll_vertical(5, 100);
        assert_eq!(view.viewport_offset(), 5);
        
        // 向上滚动
        view.scroll_vertical(-2, 100);
        assert_eq!(view.viewport_offset(), 3);
    }
}

