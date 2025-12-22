//! 选区模型：支持字符/单词/整行三种粒度

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
        let line = rope.line(pos.0).as_str().unwrap_or("");
        let (start, end) = Self::word_bounds_at(line, pos.1);

        if pos.1 - start < end - pos.1 {
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

        let graphemes: Vec<_> = line.graphemes(true).enumerate().collect();
        let len = graphemes.len();

        if col >= len {
            return (len, len);
        }

        let current_type = classify_char(graphemes[col].1);

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

        let mut end = len;
        for i in (col + 1)..len {
            if classify_char(graphemes[i].1) != current_type {
                end = i;
                break;
            }
        }

        (start, end)
    }

    fn line_grapheme_len(rope: &Rope, row: usize) -> usize {
        let line = rope.line(row).as_str().unwrap_or("");
        let without_newline = line.strip_suffix('\n').unwrap_or(line);
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
}
