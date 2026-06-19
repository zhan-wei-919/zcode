//! 帧布局契约：最近一帧渲染产出的几何区域。
//!
//! 仅由 render 写入；interaction / mouse / overlay 在其后的 tick 读取它来做命中测试与
//! 坐标映射。这是合法的跨帧单向数据流（immediate-mode 纪律）：输入事件映射到 N-1 帧的
//! 布局——render 与其后输入之间终端不会 resize（resize 会先触发重渲染再处理输入），
//! 因此读到的几何与屏幕一致。除 render 外不得写入。

use crate::ui::core::geom::Rect;

/// 每个编辑器 pane 的几何。`outer_areas` / `inner_areas` 与 pane 索引对齐。
/// render 消费 `outer_areas`（分隔线、pane 命中），interaction 消费 `inner_areas`（文本区映射）。
/// 二者目前恒等，但分属不同层、保留独立向量，便于将来 pane 内边距等扩展。
#[derive(Debug, Default)]
pub(in crate::app::workbench) struct EditorFrameLayout {
    pub(in crate::app::workbench) outer_areas: Vec<Rect>,
    pub(in crate::app::workbench) inner_areas: Vec<Rect>,
}

impl EditorFrameLayout {
    /// 指定 pane 的内层矩形；越界返回 None（不静默回退到 pane 0）。
    pub(in crate::app::workbench) fn inner(&self, pane: usize) -> Option<Rect> {
        self.inner_areas.get(pane).copied()
    }
}

#[derive(Debug, Default)]
pub(in crate::app::workbench) struct FrameLayout {
    pub(in crate::app::workbench) render_area: Option<Rect>,
    pub(in crate::app::workbench) sidebar_area: Option<Rect>,
    pub(in crate::app::workbench) sidebar_container_area: Option<Rect>,
    pub(in crate::app::workbench) overlay_area: Option<Rect>,
    pub(in crate::app::workbench) editor: EditorFrameLayout,
}
