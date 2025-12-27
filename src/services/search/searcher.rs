//! 流式搜索器
//!
//! 利用 Ropey 的 chunks() 迭代器实现流式搜索，
//! 使用栈上固定大小缓冲区处理跨 chunk 边界匹配

use memchr::memmem::Finder;
use ropey::Rope;

const BUFFER_SIZE: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_char: usize,
    pub end_char: usize,
    pub line: usize,
    pub col: usize,
}

impl Match {
    pub fn new(start_byte: usize, end_byte: usize, rope: &Rope) -> Self {
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);
        let line = rope.char_to_line(start_char);
        let line_start = rope.line_to_char(line);
        let col = start_char - line_start;

        Self {
            start_byte,
            end_byte,
            start_char,
            end_char,
            line,
            col,
        }
    }
}

pub struct StreamSearcher<'a> {
    rope: &'a Rope,
    pattern: Vec<u8>,
    case_sensitive: bool,
}

impl<'a> StreamSearcher<'a> {
    pub fn new(rope: &'a Rope, pattern: &str, case_sensitive: bool) -> Self {
        let pattern_bytes = if case_sensitive {
            pattern.as_bytes().to_vec()
        } else {
            pattern.to_lowercase().as_bytes().to_vec()
        };

        Self {
            rope,
            pattern: pattern_bytes,
            case_sensitive,
        }
    }

    /// 搜索所有匹配，返回迭代器
    pub fn find_all(&self) -> Vec<Match> {
        if self.pattern.is_empty() {
            return Vec::new();
        }

        let finder = Finder::new(&self.pattern);
        let mut matches = Vec::new();
        let mut buffer = [0u8; BUFFER_SIZE];
        let mut buffer_len = 0usize;
        let mut buffer_byte_offset = 0usize;
        let pattern_len = self.pattern.len();

        let mut chunks = self.rope.chunks();
        let mut search_start = 0usize;

        loop {
            // 在当前缓冲区中搜索
            let search_buf = &buffer[search_start..buffer_len];

            if let Some(pos) = finder.find(search_buf) {
                let global_byte_pos = buffer_byte_offset + search_start + pos;
                let match_end = global_byte_pos + pattern_len;
                matches.push(Match::new(global_byte_pos, match_end, self.rope));

                // 移动搜索起点，避免重叠匹配
                search_start += pos + 1;
                continue;
            }

            // 没找到，加载下一个 chunk
            let keep = if buffer_len > pattern_len - 1 {
                pattern_len - 1
            } else {
                buffer_len
            };

            // 移动保留的字节到缓冲区开头
            if keep > 0 && buffer_len > keep {
                buffer.copy_within((buffer_len - keep)..buffer_len, 0);
            }
            buffer_byte_offset += buffer_len - keep;
            buffer_len = keep;
            search_start = 0;

            // 加载新 chunk
            match chunks.next() {
                Some(chunk) => {
                    let bytes = chunk.as_bytes();
                    let copy_len = bytes.len().min(BUFFER_SIZE - buffer_len);

                    if self.case_sensitive {
                        buffer[buffer_len..buffer_len + copy_len]
                            .copy_from_slice(&bytes[..copy_len]);
                    } else {
                        // 大小写不敏感：转换为小写
                        for (i, &b) in bytes[..copy_len].iter().enumerate() {
                            buffer[buffer_len + i] = b.to_ascii_lowercase();
                        }
                    }
                    buffer_len += copy_len;
                }
                None => break,
            }
        }

        matches
    }

    /// 从指定位置开始向前搜索下一个匹配
    pub fn find_next(&self, from_byte: usize) -> Option<Match> {
        if self.pattern.is_empty() {
            return None;
        }

        let finder = Finder::new(&self.pattern);
        let mut buffer = [0u8; BUFFER_SIZE];
        let mut buffer_len = 0usize;
        let pattern_len = self.pattern.len();

        // 计算起始 chunk
        let start_byte = from_byte;
        let mut current_byte = 0usize;
        let mut chunks = self.rope.chunks().peekable();

        // 跳过 from_byte 之前的 chunks
        while let Some(chunk) = chunks.peek() {
            let chunk_len = chunk.len();
            if current_byte + chunk_len > start_byte {
                break;
            }
            current_byte += chunk_len;
            chunks.next();
        }

        let mut buffer_byte_offset = current_byte;
        let mut search_start = 0usize;
        let mut first_chunk = true;

        loop {
            // 在当前缓冲区中搜索
            let search_buf = &buffer[search_start..buffer_len];

            if let Some(pos) = finder.find(search_buf) {
                let global_byte_pos = buffer_byte_offset + search_start + pos;

                // 确保匹配位置在 from_byte 之后
                if global_byte_pos >= from_byte {
                    let match_end = global_byte_pos + pattern_len;
                    return Some(Match::new(global_byte_pos, match_end, self.rope));
                }

                search_start += pos + 1;
                continue;
            }

            // 保留末尾可能跨边界的部分
            let keep = if buffer_len > pattern_len - 1 {
                pattern_len - 1
            } else {
                buffer_len
            };

            if keep > 0 && buffer_len > keep {
                buffer.copy_within((buffer_len - keep)..buffer_len, 0);
            }
            buffer_byte_offset += buffer_len - keep;
            buffer_len = keep;
            search_start = 0;

            match chunks.next() {
                Some(chunk) => {
                    let bytes = chunk.as_bytes();

                    // 第一个 chunk 需要从 from_byte 开始
                    let skip = if first_chunk && buffer_byte_offset < start_byte {
                        start_byte - buffer_byte_offset
                    } else {
                        0
                    };
                    first_chunk = false;

                    let available = &bytes[skip.min(bytes.len())..];
                    let copy_len = available.len().min(BUFFER_SIZE - buffer_len);

                    if self.case_sensitive {
                        buffer[buffer_len..buffer_len + copy_len]
                            .copy_from_slice(&available[..copy_len]);
                    } else {
                        for (i, &b) in available[..copy_len].iter().enumerate() {
                            buffer[buffer_len + i] = b.to_ascii_lowercase();
                        }
                    }
                    buffer_len += copy_len;

                    if skip > 0 {
                        buffer_byte_offset += skip;
                    }
                }
                None => return None,
            }
        }
    }

    /// 从指定位置开始向后搜索上一个匹配
    pub fn find_prev(&self, from_byte: usize) -> Option<Match> {
        if self.pattern.is_empty() {
            return None;
        }

        // 向后搜索：收集所有匹配，找到 from_byte 之前的最后一个
        let all_matches = self.find_all();

        all_matches
            .into_iter()
            .filter(|m| m.start_byte < from_byte)
            .last()
    }
}

/// 批量搜索器，用于同时搜索多个模式
pub struct MultiPatternSearcher<'a> {
    rope: &'a Rope,
    ac: aho_corasick::AhoCorasick,
    patterns_len: Vec<usize>,
}

impl<'a> MultiPatternSearcher<'a> {
    pub fn new(rope: &'a Rope, patterns: &[&str], case_sensitive: bool) -> Self {
        let patterns: Vec<String> = if case_sensitive {
            patterns.iter().map(|s| s.to_string()).collect()
        } else {
            patterns.iter().map(|s| s.to_lowercase()).collect()
        };

        let patterns_len: Vec<usize> = patterns.iter().map(|p| p.len()).collect();

        let ac = aho_corasick::AhoCorasick::builder()
            .ascii_case_insensitive(!case_sensitive)
            .build(&patterns)
            .expect("Failed to build AhoCorasick");

        Self {
            rope,
            ac,
            patterns_len,
        }
    }

    pub fn find_all(&self) -> Vec<(usize, Match)> {
        let mut matches = Vec::new();
        let text = self.rope.to_string();
        let bytes = text.as_bytes();

        for mat in self.ac.find_iter(bytes) {
            let pattern_idx = mat.pattern().as_usize();
            let start_byte = mat.start();
            let end_byte = mat.end();
            matches.push((pattern_idx, Match::new(start_byte, end_byte, self.rope)));
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_search() {
        let rope = Rope::from_str("hello world hello");
        let searcher = StreamSearcher::new(&rope, "hello", true);
        let matches = searcher.find_all();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].start_byte, 0);
        assert_eq!(matches[1].start_byte, 12);
    }

    #[test]
    fn test_case_insensitive() {
        let rope = Rope::from_str("Hello HELLO hello");
        let searcher = StreamSearcher::new(&rope, "hello", false);
        let matches = searcher.find_all();

        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_find_next() {
        let rope = Rope::from_str("hello world hello");
        let searcher = StreamSearcher::new(&rope, "hello", true);

        let m1 = searcher.find_next(0).unwrap();
        assert_eq!(m1.start_byte, 0);

        let m2 = searcher.find_next(1).unwrap();
        assert_eq!(m2.start_byte, 12);

        let m3 = searcher.find_next(13);
        assert!(m3.is_none());
    }

    #[test]
    fn test_find_prev() {
        let rope = Rope::from_str("hello world hello");
        let searcher = StreamSearcher::new(&rope, "hello", true);

        let m1 = searcher.find_prev(17).unwrap();
        assert_eq!(m1.start_byte, 12);

        let m2 = searcher.find_prev(12).unwrap();
        assert_eq!(m2.start_byte, 0);

        let m3 = searcher.find_prev(0);
        assert!(m3.is_none());
    }

    #[test]
    fn test_line_col() {
        let rope = Rope::from_str("line1\nline2 hello\nline3");
        let searcher = StreamSearcher::new(&rope, "hello", true);
        let matches = searcher.find_all();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line, 1);
        assert_eq!(matches[0].col, 6);
    }

    #[test]
    fn test_empty_pattern() {
        let rope = Rope::from_str("hello");
        let searcher = StreamSearcher::new(&rope, "", true);
        let matches = searcher.find_all();

        assert!(matches.is_empty());
    }

    #[test]
    fn test_multi_pattern() {
        let rope = Rope::from_str("hello world foo bar");
        let searcher = MultiPatternSearcher::new(&rope, &["hello", "foo"], true);
        let matches = searcher.find_all();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].0, 0); // pattern index for "hello"
        assert_eq!(matches[1].0, 1); // pattern index for "foo"
    }
}
