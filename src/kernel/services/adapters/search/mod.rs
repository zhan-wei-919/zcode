//! 搜索服务模块
//!
//! - StreamSearcher: 流式搜索器 (Literal 模式，8KB 栈 buffer)
//! - SearchService: 单文件搜索服务 (编辑器内搜索)
//! - GlobalSearchService: 全局多文件搜索服务

mod global;
mod searcher;
mod service;

#[inline]
fn count_byte(haystack: &[u8], needle: u8) -> usize {
    let mut n = 0usize;
    for &b in haystack {
        if b == needle {
            n += 1;
        }
    }
    n
}

pub use global::{GlobalSearchService, GlobalSearchTask};
pub use searcher::{search_regex_in_slice, RopeReader, SearchConfig, StreamSearcher};
pub use service::{SearchService, SearchTask};
