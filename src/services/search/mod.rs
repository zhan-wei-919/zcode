//! 搜索服务模块
//!
//! 提供流式搜索和异步搜索功能：
//! - StreamSearcher: 基于 Ropey chunks 的流式搜索器
//! - SearchService: 异步搜索服务
//! - GlobalSearchService: 全局（多文件）搜索服务

mod global;
mod searcher;
mod service;

pub use global::{FileMatches, GlobalSearchMessage, GlobalSearchService, GlobalSearchTask};
pub use searcher::{Match, SearchDirection, StreamSearcher};
pub use service::{SearchMessage, SearchService, SearchTask};
