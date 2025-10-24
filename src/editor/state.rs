//zcode/src/editor/state.rs
use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

pub struct Editor {
    pub rope: Rope,
    pub cursor: (usize, usize), // (row: 行索引, col: 行内字形簇索引)
    pub viewport_offset: usize,  // 视口顶部行号（用于滚动）
    pub viewport_height: usize,  // 视口高度（行数），由渲染层更新
    pub cached_char_pos: Option<usize>,
}

impl Editor {
    pub fn new() -> Self {
        Editor{
            rope: Rope::new(),
            cursor: (0,0),
            viewport_offset: 0,
            viewport_height: 20, // 默认高度，将在首次渲染时更新
            cached_char_pos: Some(0),
        }
    }

    pub(super) fn grapheme_to_char_index(&self, row: usize, grapheme_index: usize) -> usize {
        self.rope.line(row)
            .as_str().unwrap_or("")
            .graphemes(true)
            .take(grapheme_index)
            .map(|g| g.chars().count())
            .sum()
    }

    pub(super) fn line_grapheme_len(&self, row: usize) -> usize {
        // 排除行尾换行符，返回实际可见字符数
        let line = self.rope.line(row).as_str().unwrap_or("");
        let without_newline = line.strip_suffix('\n').unwrap_or(line);
        without_newline.graphemes(true).count()
    }


    pub(super) fn get_char_pos(&mut self) -> usize{
        if self.cached_char_pos.is_none() {
            self.cached_char_pos = Some(
                self.grapheme_to_char_index(self.cursor.0, self.cursor.1) 
                + self.rope.line_to_char(self.cursor.0)
            )
        };
        self.cached_char_pos.unwrap()
    }

    pub(super) fn invalidate_pos_cache(&mut self) {
        self.cached_char_pos = None;
    }


    // 在渲染前更新视口状态
    pub fn update_viewport(&mut self, viewport_height: usize) {
        self.viewport_height = viewport_height;
        
        // 调整视口偏移，确保光标可见
        if self.cursor.0 < self.viewport_offset {
            self.viewport_offset = self.cursor.0;
        } else if self.cursor.0 >= self.viewport_offset + viewport_height {
            self.viewport_offset = self.cursor.0.saturating_sub(viewport_height.saturating_sub(1));
        }
    }
}