//! 搜索服务模块
//!
//! - StreamSearcher: 流式搜索器 (Literal 模式，8KB 栈 buffer)
//! - SearchService: 单文件搜索服务 (编辑器内搜索)
//! - GlobalSearchService: 全局多文件搜索服务

mod global;
mod searcher;
mod service;

pub use global::{GlobalSearchService, GlobalSearchTask};
pub use searcher::{search_regex_in_slice, RopeReader, SearchConfig, StreamSearcher};
pub use service::{SearchService, SearchTask};
