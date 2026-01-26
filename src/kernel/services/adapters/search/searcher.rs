//! 流式搜索器
//!
//! - Literal 模式：流式搜索 + 8KB 栈 buffer (memchr)
//! - Regex 模式：需要全量数据，由调用方提供 &[u8]

use crate::kernel::services::ports::search::Match;
use memchr::memmem::Finder;
use std::io::Read;

const BUFFER_SIZE: usize = 8192;

/// 搜索配置，缓存编译好的搜索引擎
#[derive(Clone)]
pub enum SearchConfig {
    Literal {
        pattern: Vec<u8>,
        case_sensitive: bool,
    },
    Regex {
        regex: regex::Regex,
    },
}

impl SearchConfig {
    pub fn literal(pattern: &str, case_sensitive: bool) -> Self {
        let pattern_bytes = if case_sensitive {
            pattern.as_bytes().to_vec()
        } else {
            pattern.to_ascii_lowercase().into_bytes()
        };

        Self::Literal {
            pattern: pattern_bytes,
            case_sensitive,
        }
    }

    pub fn regex(pattern: &str, case_sensitive: bool) -> Result<Self, regex::Error> {
        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .build()?;
        Ok(Self::Regex { regex })
    }

    pub fn is_regex(&self) -> bool {
        matches!(self, Self::Regex { .. })
    }

    pub fn pattern_len(&self) -> usize {
        match self {
            Self::Literal { pattern, .. } => pattern.len(),
            Self::Regex { .. } => 0, // Regex 长度不固定
        }
    }
}

/// Literal 模式的流式搜索器
/// 使用 8KB 栈 buffer，处理跨 chunk 边界匹配
pub struct StreamSearcher<'a, R> {
    reader: R,
    config: &'a SearchConfig,
}

impl<'a, R: Read> StreamSearcher<'a, R> {
    pub fn new(reader: R, config: &'a SearchConfig) -> Self {
        Self { reader, config }
    }

    /// 执行流式搜索（仅支持 Literal 模式）
    pub fn search(self) -> std::io::Result<Vec<Match>> {
        let mut matches = Vec::new();
        let _ = self.search_with(
            |m| {
                matches.push(m);
                Ok(())
            },
            || false,
        )?;
        Ok(matches)
    }

    pub fn search_with<F, C>(mut self, mut on_match: F, mut cancelled: C) -> std::io::Result<usize>
    where
        F: FnMut(Match) -> std::io::Result<()>,
        C: FnMut() -> bool,
    {
        let (pattern, case_sensitive) = match self.config {
            SearchConfig::Literal {
                pattern,
                case_sensitive,
            } => {
                if pattern.is_empty() {
                    return Ok(0);
                }
                (pattern.as_slice(), *case_sensitive)
            }
            SearchConfig::Regex { .. } => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "StreamSearcher does not support regex mode",
                ));
            }
        };

        let finder = Finder::new(pattern);
        let pattern_len = pattern.len();

        fn search_stream<R, F, C>(
            reader: &mut R,
            finder: &Finder<'_>,
            pattern_len: usize,
            case_sensitive: bool,
            buffer: &mut [u8],
            on_match: &mut F,
            cancelled: &mut C,
        ) -> std::io::Result<usize>
        where
            R: Read,
            F: FnMut(Match) -> std::io::Result<()>,
            C: FnMut() -> bool,
        {
            let mut total = 0usize;
            let mut filled_len = 0usize;
            let mut global_offset = 0usize;
            let mut search_start = 0usize;

            // 行号计算
            let mut current_line = 0usize;
            let mut line_start_offset = 0usize;

            loop {
                if cancelled() {
                    return Ok(total);
                }

                // 在当前缓冲区中搜索
                let search_buf = &buffer[search_start..filled_len];

                if let Some(pos) = finder.find(search_buf) {
                    let match_start = global_offset + search_start + pos;
                    let match_end = match_start + pattern_len;

                    // 计算行号：统计从 line_start_offset 到 match_start 之间的换行符
                    let count_from = line_start_offset.saturating_sub(global_offset);
                    let count_to = search_start + pos;

                    if count_to > count_from {
                        let newlines = super::count_byte(&buffer[count_from..count_to], b'\n');
                        if newlines > 0 {
                            current_line += newlines;
                            // 找到最后一个换行符的位置
                            for i in (count_from..count_to).rev() {
                                if buffer[i] == b'\n' {
                                    line_start_offset = global_offset + i + 1;
                                    break;
                                }
                            }
                        }
                    }

                    let col = match_start - line_start_offset;
                    on_match(Match::new(match_start, match_end, current_line, col))?;
                    total += 1;

                    search_start += pos + 1;
                    continue;
                }

                if cancelled() {
                    return Ok(total);
                }

                // 没找到，准备加载下一块数据
                // 保留末尾 pattern_len - 1 字节以处理跨边界匹配
                let keep = if filled_len > pattern_len - 1 {
                    pattern_len - 1
                } else {
                    filled_len
                };

                // 更新行号信息：统计即将丢弃部分的换行符
                let discard_end = filled_len - keep;
                if discard_end > 0 {
                    let newlines = super::count_byte(&buffer[..discard_end], b'\n');
                    if newlines > 0 {
                        current_line += newlines;
                        for i in (0..discard_end).rev() {
                            if buffer[i] == b'\n' {
                                line_start_offset = global_offset + i + 1;
                                break;
                            }
                        }
                    }
                }

                // 移动保留的字节到缓冲区开头
                if keep > 0 && filled_len > keep {
                    buffer.copy_within((filled_len - keep)..filled_len, 0);
                }
                global_offset += filled_len - keep;
                filled_len = keep;
                search_start = 0;

                // 读取新数据
                let bytes_read = reader.read(&mut buffer[filled_len..])?;
                if bytes_read == 0 {
                    break;
                }

                // 大小写不敏感时转换为小写
                if !case_sensitive {
                    for b in &mut buffer[filled_len..filled_len + bytes_read] {
                        *b = b.to_ascii_lowercase();
                    }
                }
                filled_len += bytes_read;
            }

            Ok(total)
        }

        if pattern_len >= BUFFER_SIZE {
            let buffer_size = pattern_len.checked_add(BUFFER_SIZE).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "pattern too large")
            })?;
            let mut buffer = vec![0u8; buffer_size];
            search_stream(
                &mut self.reader,
                &finder,
                pattern_len,
                case_sensitive,
                &mut buffer,
                &mut on_match,
                &mut cancelled,
            )
        } else {
            let mut buffer = [0u8; BUFFER_SIZE];
            search_stream(
                &mut self.reader,
                &finder,
                pattern_len,
                case_sensitive,
                &mut buffer,
                &mut on_match,
                &mut cancelled,
            )
        }
    }
}

/// 在 slice 上执行 Regex 搜索
pub fn search_regex_in_slice(data: &[u8], regex: &regex::Regex) -> Vec<Match> {
    // 尝试转换为 UTF-8
    let text = match std::str::from_utf8(data) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let mut matches = Vec::new();
    let mut current_line = 0usize;
    let mut line_start = 0usize;
    let mut last_pos = 0usize;

    for mat in regex.find_iter(text) {
        let start = mat.start();
        let end = mat.end();

        // 计算行号
        let newlines = super::count_byte(&data[last_pos..start], b'\n');
        if newlines > 0 {
            current_line += newlines;
            // 找最后一个换行符
            for i in (last_pos..start).rev() {
                if data[i] == b'\n' {
                    line_start = i + 1;
                    break;
                }
            }
        }
        last_pos = start;

        let col = start - line_start;
        matches.push(Match::new(start, end, current_line, col));
    }

    matches
}

/// Rope 的 Read 适配器
pub struct RopeReader<'a> {
    chunks: ropey::iter::Chunks<'a>,
    current_chunk: &'a [u8],
    pos: usize,
}

impl<'a> RopeReader<'a> {
    pub fn new(rope: &'a ropey::Rope) -> Self {
        let mut chunks = rope.chunks();
        let current_chunk = chunks.next().map(|s| s.as_bytes()).unwrap_or(&[]);
        Self {
            chunks,
            current_chunk,
            pos: 0,
        }
    }
}

impl<'a> Read for RopeReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut total_read = 0;

        while total_read < buf.len() {
            let remaining = &self.current_chunk[self.pos..];
            if remaining.is_empty() {
                // 获取下一个 chunk
                match self.chunks.next() {
                    Some(chunk) => {
                        self.current_chunk = chunk.as_bytes();
                        self.pos = 0;
                        continue;
                    }
                    None => break,
                }
            }

            let to_copy = remaining.len().min(buf.len() - total_read);
            buf[total_read..total_read + to_copy].copy_from_slice(&remaining[..to_copy]);
            self.pos += to_copy;
            total_read += to_copy;
        }

        Ok(total_read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_literal_search() {
        let data = b"hello world hello";
        let config = SearchConfig::literal("hello", true);
        let reader = Cursor::new(data);
        let matches = StreamSearcher::new(reader, &config).search().unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[1].start, 12);
    }

    #[test]
    fn test_case_insensitive() {
        let data = b"Hello HELLO hello";
        let config = SearchConfig::literal("hello", false);
        let reader = Cursor::new(data);
        let matches = StreamSearcher::new(reader, &config).search().unwrap();

        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_line_numbers() {
        let data = b"line1\nline2 hello\nline3";
        let config = SearchConfig::literal("hello", true);
        let reader = Cursor::new(data);
        let matches = StreamSearcher::new(reader, &config).search().unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line, 1);
        assert_eq!(matches[0].col, 6);
    }

    #[test]
    fn test_regex_search() {
        let data = b"hello123 world456";
        let config = SearchConfig::regex(r"\w+\d+", true).unwrap();
        let SearchConfig::Regex { regex } = &config else {
            panic!("expected Regex config");
        };
        let matches = search_regex_in_slice(data, regex);

        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_rope_reader() {
        let rope = ropey::Rope::from_str("hello world hello");
        let config = SearchConfig::literal("hello", true);
        let reader = RopeReader::new(&rope);
        let matches = StreamSearcher::new(reader, &config).search().unwrap();

        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_empty_pattern() {
        let data = b"hello";
        let config = SearchConfig::literal("", true);
        let reader = Cursor::new(data);
        let matches = StreamSearcher::new(reader, &config).search().unwrap();

        assert!(matches.is_empty());
    }

    #[test]
    fn test_long_pattern_over_buffer_size() {
        let pattern = "a".repeat(BUFFER_SIZE + 10);
        let config = SearchConfig::literal(&pattern, true);
        let reader = Cursor::new(pattern.into_bytes());
        let matches = StreamSearcher::new(reader, &config).search().unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[0].end, BUFFER_SIZE + 10);
    }
}
