//zcode/src/editor/core/text_model.rs
//! 文本模型层：纯数据，不关心渲染
//! 
//! 职责：
//! - 文本存储（Rope）
//! - 光标和选区管理
//! - 行列 ↔ 字符偏移映射
//! - 编辑历史（未来支持 undo/redo）

use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;
use crate::editor::input::Selection;

/// 文本模型：管理文档内容和编辑状态
#[derive(Clone)]
pub struct TextModel {
    /// 文本内容（Rope 数据结构，支持高效的大文件编辑）
    rope: Rope,
    
    /// 主光标位置 (row, col)
    /// 未来支持多光标时，这是主光标
    cursor: (usize, usize),
    
    /// 选区（可选）
    /// 未来支持多选区时，改为 Vec<Selection>
    selection: Option<Selection>,
    
    /// 光标字符位置缓存（性能优化）
    cached_char_pos: Option<usize>,
    
    // TODO: 未来添加
    // edit_history: EditHistory,  // 撤销/重做
    // cursors: Vec<Cursor>,        // 多光标
    // selections: Vec<Selection>,  // 多选区
}

impl TextModel {
    /// 创建空文档
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            cursor: (0, 0),
            selection: None,
            cached_char_pos: Some(0),
        }
    }
    
    /// 从文本创建
    pub fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            cursor: (0, 0),
            selection: None,
            cached_char_pos: Some(0),
        }
    }
    
    // ==================== 只读访问 ====================
    
    /// 获取 Rope 引用
    pub fn rope(&self) -> &Rope {
        &self.rope
    }
    
    /// 获取可变 Rope 引用（仅内部使用）
    pub(super) fn rope_mut(&mut self) -> &mut Rope {
        &mut self.rope
    }
    
    /// 获取光标位置
    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }
    
    /// 设置光标位置
    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.cursor = (row, col);
        self.invalidate_char_pos_cache();
    }
    
    /// 移动光标
    pub fn move_cursor(&mut self, row: usize, col: usize) {
        self.set_cursor(row, col);
    }
    
    /// 获取选区
    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }
    
    /// 获取可变选区
    pub fn selection_mut(&mut self) -> Option<&mut Selection> {
        self.selection.as_mut()
    }
    
    /// 设置选区
    pub fn set_selection(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }
    
    /// 更新选区的光标位置（避免借用冲突）
    pub fn update_selection_cursor(&mut self, pos: (usize, usize)) {
        if let Some(sel) = &mut self.selection {
            sel.update_cursor(pos, &self.rope);
        }
    }
    
    /// 总行数
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }
    
    /// 获取指定行的内容
    pub fn line(&self, row: usize) -> Option<String> {
        if row < self.rope.len_lines() {
            self.rope.line(row).as_str().map(|s| s.to_string())
        } else {
            None
        }
    }
    
    // ==================== 坐标转换 ====================
    
    /// 获取光标的字符偏移量（带缓存）
    pub fn cursor_char_offset(&mut self) -> usize {
        if self.cached_char_pos.is_none() {
            let char_offset = self.rope.line_to_char(self.cursor.0)
                + self.grapheme_to_char_index(self.cursor.0, self.cursor.1);
            self.cached_char_pos = Some(char_offset);
        }
        self.cached_char_pos.unwrap()
    }
    
    /// 位置 → 字符偏移
    pub fn pos_to_char(&self, pos: (usize, usize)) -> usize {
        self.rope.line_to_char(pos.0) + self.grapheme_to_char_index(pos.0, pos.1)
    }
    
    /// 字形簇索引 → 字符索引
    pub fn grapheme_to_char_index(&self, row: usize, grapheme_index: usize) -> usize {
        self.rope
            .line(row)
            .as_str()
            .unwrap_or("")
            .graphemes(true)
            .take(grapheme_index)
            .map(|g| g.chars().count())
            .sum()
    }
    
    /// 获取行的字形簇长度（不含换行符）
    pub fn line_grapheme_len(&self, row: usize) -> usize {
        let line = self.rope.line(row).as_str().unwrap_or("");
        let without_newline = line.strip_suffix('\n').unwrap_or(line);
        without_newline.graphemes(true).count()
    }
    
    // ==================== 编辑操作 ====================
    
    /// 插入字符
    pub fn insert_char(&mut self, c: char) {
        let pos = self.cursor_char_offset();
        self.rope.insert_char(pos, c);
        self.invalidate_char_pos_cache();
    }
    
    /// 插入字符串
    pub fn insert_str(&mut self, s: &str) {
        let pos = self.cursor_char_offset();
        self.rope.insert(pos, s);
        self.invalidate_char_pos_cache();
    }
    
    /// 删除范围
    pub fn remove_range(&mut self, start: usize, end: usize) {
        self.rope.remove(start..end);
        self.invalidate_char_pos_cache();
    }
    
    /// 删除选区内容
    /// 返回：是否删除了内容
    pub fn delete_selection(&mut self) -> bool {
        if let Some(selection) = &self.selection {
            if !selection.is_empty() {
                let (start, end) = selection.range();
                let start_char = self.pos_to_char(start);
                let end_char = self.pos_to_char(end);
                
                self.rope.remove(start_char..end_char);
                self.cursor = start;
                self.selection = None;
                self.invalidate_char_pos_cache();
                
                return true;
            }
        }
        false
    }
    
    // ==================== 内部辅助 ====================
    
    /// 使缓存失效
    fn invalidate_char_pos_cache(&mut self) {
        self.cached_char_pos = None;
    }
}

impl Default for TextModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_text_model_basic() {
        let mut model = TextModel::from_text("hello\nworld");
        
        assert_eq!(model.len_lines(), 2);
        assert_eq!(model.cursor(), (0, 0));
        
        model.set_cursor(1, 2);
        assert_eq!(model.cursor(), (1, 2));
    }
    
    #[test]
    fn test_pos_to_char() {
        let model = TextModel::from_text("hello\nworld");
        
        // 第一行开头
        assert_eq!(model.pos_to_char((0, 0)), 0);
        
        // 第二行开头（"hello\n" = 6 字符）
        assert_eq!(model.pos_to_char((1, 0)), 6);
    }
    
    #[test]
    fn test_insert_char() {
        let mut model = TextModel::new();
        model.insert_char('a');
        
        let text: String = model.rope().into();
        assert_eq!(text, "a");
    }
}

