//! 渲染缓存：viewport 内容尺寸、按 tab 的 markdown 视图状态、主题编辑器布局快照。
//! 这些是渲染产物的缓存，仅服务于绘制与滚动条等，不参与业务决策。

use super::super::{ThemeEditorLayoutCache, ViewportCache};
use crate::kernel::editor::TabId;
use crate::views::editor::markdown_cache::MarkdownViewState;
use rustc_hash::FxHashMap;

#[derive(Default)]
pub(in crate::app::workbench) struct RenderCache {
    pub(in crate::app::workbench) viewport: ViewportCache,
    pub(in crate::app::workbench) markdown_views: FxHashMap<TabId, MarkdownViewState>,
    pub(in crate::app::workbench) theme_editor_layout: ThemeEditorLayoutCache,
}
