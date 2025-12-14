//zcode/src/editor/selection.rs
//! 选区模型：支持字符/单词/整行三种粒度

use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;
use unicode_xid::UnicodeXID;

/// 选择粒度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    /// 字符级别（单击）
    Char,
    /// 单词级别（双击）
    Word,
    /// 整行级别（三击）
    Line,
}

/// 选区
#[derive(Debug, Clone)]
pub struct Selection {
    /// 锚点（拖拽起点，固定不变）
    anchor: (usize, usize),
    
    /// 光标（拖拽终点，跟随鼠标）
    cursor: (usize, usize),
    
    /// 当前粒度
    granularity: Granularity,
}

impl Selection {
    /// 创建新选区
    pub fn new(pos: (usize, usize), granularity: Granularity) -> Self {
        Self {
            anchor: pos,
            cursor: pos,
            granularity,
        }
    }
    
    /// 更新光标位置（拖拽时调用）
    pub fn update_cursor(&mut self, pos: (usize, usize), rope: &Rope) {
        self.cursor = match self.granularity {
            Granularity::Char => pos,
            Granularity::Word => self.snap_to_word(pos, rope),
            Granularity::Line => self.snap_to_line(pos, rope),
        };
    }
    
    /// 对齐到单词边界
    fn snap_to_word(&self, pos: (usize, usize), rope: &Rope) -> (usize, usize) {
        let line = rope.line(pos.0).as_str().unwrap_or("");
        let (start, end) = Self::word_bounds_at(line, pos.1);
        
        // 根据距离决定对齐到词首还是词尾
        if pos.1 - start < end - pos.1 {
            (pos.0, start)
        } else {
            (pos.0, end)
        }
    }
    
    /// 对齐到整行
    fn snap_to_line(&self, pos: (usize, usize), rope: &Rope) -> (usize, usize) {
        let line_len = Self::line_grapheme_len(rope, pos.0);
        (pos.0, line_len)
    }
    
    /// 获取单词边界（改进版：支持 Unicode 标识符）
    /// 返回：(word_start, word_end)
    /// 
    /// 词边界规则：
    /// - 代码模式：使用 Unicode XID (eXtended Identifier) 标准
    ///   - XID_Start: 可以作为标识符开头（字母、下划线等）
    ///   - XID_Continue: 可以作为标识符后续字符（包括数字）
    /// - 符号/空白：各自成独立词
    /// 
    /// 示例：
    /// - `hello_world` → 一个词
    /// - `中文标识符` → 一个词
    /// - `café` → 一个词（支持 Unicode）
    /// - `a + b` → 三个词（a, +, b）
    pub fn word_bounds_at(line: &str, col: usize) -> (usize, usize) {
        #[derive(PartialEq, Eq, Clone, Copy)]
        enum CharType {
            Identifier,  // 标识符字符（XID_Start 或 XID_Continue）
            Whitespace,  // 空白
            Other,       // 其他符号
        }
        
        // 判断字符类型（使用 Unicode XID 标准）
        let classify_char = |s: &str| -> CharType {
            let mut chars = s.chars();
            if let Some(c) = chars.next() {
                if c.is_whitespace() {
                    CharType::Whitespace
                } else if c.is_xid_start() || c.is_xid_continue() || c == '_' {
                    // XID + 特殊处理下划线（Rust/C/Python 风格）
                    CharType::Identifier
                } else {
                    CharType::Other
                }
            } else {
                CharType::Other
            }
        };
        
        // 收集字形簇和类型
        let graphemes: Vec<_> = line.graphemes(true).enumerate().collect();
        let len = graphemes.len();
        
        if col >= len {
            return (len, len);
        }
        
        let current_type = classify_char(graphemes[col].1);
        
        // 向左查找边界
        let mut start = col;
        for i in (0..col).rev() {
            if classify_char(graphemes[i].1) != current_type {
                start = i + 1;
                break;
            }
            if i == 0 {
                start = 0;
                break;
            }
        }
        
        // 向右查找边界
        let mut end = len; // 默认到行尾
        for i in (col + 1)..len {
            if classify_char(graphemes[i].1) != current_type {
                end = i;
                break;
            }
        }
        
        (start, end)
    }
    
    /// 获取行的字形簇长度（不含换行符）
    fn line_grapheme_len(rope: &Rope, row: usize) -> usize {
        let line = rope.line(row).as_str().unwrap_or("");
        let without_newline = line.strip_suffix('\n').unwrap_or(line);
        without_newline.graphemes(true).count()
    }
    
    /// 获取规范化的选区范围：(start, end)，保证 start <= end
    pub fn range(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }
    
    /// 检查选区是否为空
    pub fn is_empty(&self) -> bool {
        self.anchor == self.cursor
    }
    
    /// 检查某个位置是否在选区内
    pub fn contains(&self, pos: (usize, usize)) -> bool {
        let (start, end) = self.range();
        start <= pos && pos < end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_word_bounds() {
        let line = "hello_world foo bar";
        
        // 在 "hello_world" 中间
        assert_eq!(Selection::word_bounds_at(line, 5), (0, 11));
        
        // 在空格上
        assert_eq!(Selection::word_bounds_at(line, 11), (11, 12));
        
        // 在 "foo" 上
        assert_eq!(Selection::word_bounds_at(line, 12), (12, 15));
    }
    
    #[test]
    fn test_word_bounds_unicode() {
        // Unicode 标识符（中文）
        let line = "let 变量名 = value;";
        assert_eq!(Selection::word_bounds_at(line, 4), (4, 7)); // "变量名"
        
        // 带重音符的字符（café）
        let line = "café";
        assert_eq!(Selection::word_bounds_at(line, 0), (0, 4)); // 整个词
        
        // 符号分隔
        let line = "a+b-c";
        assert_eq!(Selection::word_bounds_at(line, 0), (0, 1)); // "a"
        assert_eq!(Selection::word_bounds_at(line, 1), (1, 2)); // "+"
        assert_eq!(Selection::word_bounds_at(line, 2), (2, 3)); // "b"
        assert_eq!(Selection::word_bounds_at(line, 3), (3, 4)); // "-"
        assert_eq!(Selection::word_bounds_at(line, 4), (4, 5)); // "c"
    }
    
    #[test]
    fn test_word_bounds_mixed() {
        // 混合 ASCII 和 Unicode（字形簇计数）
        let line = "hello世界world";
        let grapheme_count = line.graphemes(true).count();
        assert_eq!(Selection::word_bounds_at(line, 0), (0, grapheme_count)); // 整个词
        
        // 数字在标识符中
        let line = "var123";
        assert_eq!(Selection::word_bounds_at(line, 3), (0, 6)); // 整个词包括数字
    }
    
    #[test]
    fn test_selection_range() {
        let sel = Selection::new((1, 5), Granularity::Char);
        assert_eq!(sel.range(), ((1, 5), (1, 5)));
        
        let mut sel = Selection::new((2, 3), Granularity::Char);
        sel.cursor = (5, 7);
        assert_eq!(sel.range(), ((2, 3), (5, 7)));
        
        // 反向选择
        let mut sel = Selection::new((5, 7), Granularity::Char);
        sel.cursor = (2, 3);
        assert_eq!(sel.range(), ((2, 3), (5, 7)));
    }
}
