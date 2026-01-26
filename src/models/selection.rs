//! 选区模型：支持字符/单词/整行三种粒度

use super::text_buffer::slice_to_cow;
use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;
use unicode_xid::UnicodeXID;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Char,
    Word,
    Line,
}

#[derive(Debug, Clone)]
pub struct Selection {
    anchor: (usize, usize),
    cursor: (usize, usize),
    granularity: Granularity,
}

impl Selection {
    pub fn new(pos: (usize, usize), granularity: Granularity) -> Self {
        Self {
            anchor: pos,
            cursor: pos,
            granularity,
        }
    }

    pub fn anchor(&self) -> (usize, usize) {
        self.anchor
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    pub fn granularity(&self) -> Granularity {
        self.granularity
    }

    pub fn update_cursor(&mut self, pos: (usize, usize), rope: &Rope) {
        self.cursor = match self.granularity {
            Granularity::Char => pos,
            Granularity::Word => self.snap_to_word(pos, rope),
            Granularity::Line => self.snap_to_line(pos, rope),
        };
    }

    fn snap_to_word(&self, pos: (usize, usize), rope: &Rope) -> (usize, usize) {
        let slice = rope.line(pos.0);
        let line = slice_to_cow(slice);
        let (start, end) = Self::word_bounds_at(&line, pos.1);

        if start == end {
            return (pos.0, end);
        }

        let left_dist = pos.1.saturating_sub(start);
        let right_dist = end.saturating_sub(pos.1);

        if left_dist < right_dist {
            (pos.0, start)
        } else {
            (pos.0, end)
        }
    }

    fn snap_to_line(&self, pos: (usize, usize), rope: &Rope) -> (usize, usize) {
        let line_len = Self::line_grapheme_len(rope, pos.0);
        (pos.0, line_len)
    }

    pub fn word_bounds_at(line: &str, col: usize) -> (usize, usize) {
        #[derive(PartialEq, Eq, Clone, Copy)]
        enum CharType {
            Identifier,
            Whitespace,
            Other,
        }

        let classify_char = |s: &str| -> CharType {
            let mut chars = s.chars();
            if let Some(c) = chars.next() {
                if c.is_whitespace() {
                    CharType::Whitespace
                } else if c.is_xid_start() || c.is_xid_continue() || c == '_' {
                    CharType::Identifier
                } else {
                    CharType::Other
                }
            } else {
                CharType::Other
            }
        };

        let mut len = 0usize;
        let mut prev_type: Option<CharType> = None;
        let mut segment_start = 0usize;

        let mut start = 0usize;
        let mut current_type: Option<CharType> = None;

        for (idx, grapheme) in line.graphemes(true).enumerate() {
            let ty = classify_char(grapheme);
            if prev_type.is_some_and(|prev| prev != ty) {
                segment_start = idx;
            }
            prev_type = Some(ty);

            if idx == col {
                current_type = Some(ty);
                start = segment_start;
            } else if current_type.is_some_and(|ct| idx > col && ty != ct) {
                return (start, idx);
            }

            len = idx + 1;
        }

        match current_type {
            Some(_) => (start, len),
            None => (len, len),
        }
    }

    fn line_grapheme_len(rope: &Rope, row: usize) -> usize {
        let slice = rope.line(row);
        let line = slice_to_cow(slice);
        let without_newline = line.strip_suffix('\n').unwrap_or(&line);
        let without_newline = without_newline
            .strip_suffix('\r')
            .unwrap_or(without_newline);
        without_newline.graphemes(true).count()
    }

    pub fn range(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.cursor
    }

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

        assert_eq!(Selection::word_bounds_at(line, 5), (0, 11));
        assert_eq!(Selection::word_bounds_at(line, 11), (11, 12));
        assert_eq!(Selection::word_bounds_at(line, 12), (12, 15));
    }

    #[test]
    fn test_word_bounds_unicode() {
        let line = "let 变量名 = value;";
        assert_eq!(Selection::word_bounds_at(line, 4), (4, 7));

        let line = "café";
        assert_eq!(Selection::word_bounds_at(line, 0), (0, 4));

        let line = "a+b-c";
        assert_eq!(Selection::word_bounds_at(line, 0), (0, 1));
        assert_eq!(Selection::word_bounds_at(line, 1), (1, 2));
        assert_eq!(Selection::word_bounds_at(line, 2), (2, 3));
    }

    #[test]
    fn test_selection_range() {
        let sel = Selection::new((1, 5), Granularity::Char);
        assert_eq!(sel.range(), ((1, 5), (1, 5)));

        let mut sel = Selection::new((2, 3), Granularity::Char);
        sel.cursor = (5, 7);
        assert_eq!(sel.range(), ((2, 3), (5, 7)));

        let mut sel = Selection::new((5, 7), Granularity::Char);
        sel.cursor = (2, 3);
        assert_eq!(sel.range(), ((2, 3), (5, 7)));
    }

    #[test]
    fn test_word_selection_out_of_bounds_cursor() {
        let rope = Rope::from_str("hello");
        let mut sel = Selection::new((0, 0), Granularity::Word);
        sel.update_cursor((0, 100), &rope);
        assert_eq!(sel.cursor(), (0, 5));
    }
}
