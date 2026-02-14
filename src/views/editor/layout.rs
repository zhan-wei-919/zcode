use crate::kernel::editor::EditorPaneState;
use crate::kernel::services::ports::EditorConfig;
use crate::ui::core::geom::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerticalScrollbarMetrics {
    pub track_area: Rect,
    pub thumb_area: Rect,
    pub max_offset: usize,
    thumb_range: u16,
}

impl VerticalScrollbarMetrics {
    pub fn thumb_top(&self) -> u16 {
        self.thumb_area.y.saturating_sub(self.track_area.y)
    }

    pub fn line_offset_for_thumb_top(&self, thumb_top: u16) -> usize {
        if self.max_offset == 0 || self.thumb_range == 0 {
            return 0;
        }

        let thumb_top = thumb_top.min(self.thumb_range) as u128;
        let max_offset = self.max_offset as u128;
        let denom = self.thumb_range as u128;
        ((thumb_top * max_offset + denom / 2) / denom) as usize
    }

    pub fn line_offset_for_pointer_row(&self, row: u16, grab_offset: u16) -> usize {
        let rel_row = row.saturating_sub(self.track_area.y);
        let thumb_top = rel_row.saturating_sub(grab_offset).min(self.thumb_range);
        self.line_offset_for_thumb_top(thumb_top)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EditorPaneLayout {
    pub area: Rect,
    pub tab_area: Rect,
    pub search_area: Option<Rect>,
    pub editor_area: Rect,
    pub gutter_area: Rect,
    pub content_area: Rect,
    pub v_scrollbar_area: Option<Rect>,
    pub gutter_width: u16,
}

pub fn compute_editor_pane_layout(
    area: Rect,
    pane: &EditorPaneState,
    config: &EditorConfig,
) -> EditorPaneLayout {
    const TAB_HEIGHT: u16 = 1;

    if area.is_empty() {
        return EditorPaneLayout {
            area,
            tab_area: Rect::default(),
            search_area: None,
            editor_area: Rect::default(),
            gutter_area: Rect::default(),
            content_area: Rect::default(),
            v_scrollbar_area: None,
            gutter_width: 0,
        };
    }

    let tab_height = TAB_HEIGHT.min(area.h);
    let tab_area = Rect::new(area.x, area.y, area.w, tab_height);

    let search_h = pane.search_bar.height();
    let available_after_tabs = area.h.saturating_sub(tab_height);
    let search_height = search_h.min(available_after_tabs);

    let search_area = (search_height > 0).then_some(Rect::new(
        area.x,
        area.y.saturating_add(tab_height),
        area.w,
        search_height,
    ));

    let chrome_h = tab_height.saturating_add(search_height);
    let editor_area = Rect::new(
        area.x,
        area.y.saturating_add(chrome_h),
        area.w,
        area.h.saturating_sub(chrome_h),
    );

    let (gutter_width, gutter_area, content_area) = compute_gutter(editor_area, pane, config);
    let (content_area, v_scrollbar_area) =
        compute_vertical_scrollbar(editor_area, content_area, pane);

    EditorPaneLayout {
        area,
        tab_area,
        search_area,
        editor_area,
        gutter_area,
        content_area,
        v_scrollbar_area,
        gutter_width,
    }
}

fn compute_gutter(
    editor_area: Rect,
    pane: &EditorPaneState,
    config: &EditorConfig,
) -> (u16, Rect, Rect) {
    if editor_area.is_empty() || !config.show_line_numbers {
        return (
            0,
            Rect::default(),
            Rect::new(editor_area.x, editor_area.y, editor_area.w, editor_area.h),
        );
    }

    let Some(tab) = pane.active_tab() else {
        return (
            0,
            Rect::default(),
            Rect::new(editor_area.x, editor_area.y, editor_area.w, editor_area.h),
        );
    };

    let total_lines = tab.buffer.len_lines().max(1);
    let digits = total_lines.to_string().len();
    let gutter_width = ((digits + 2) as u16).min(editor_area.w);

    if gutter_width == 0 || gutter_width >= editor_area.w {
        return (
            gutter_width,
            Rect::new(editor_area.x, editor_area.y, gutter_width, editor_area.h),
            Rect::default(),
        );
    }

    let gutter_area = Rect::new(editor_area.x, editor_area.y, gutter_width, editor_area.h);
    let content_area = Rect::new(
        editor_area.x + gutter_width,
        editor_area.y,
        editor_area.w - gutter_width,
        editor_area.h,
    );

    (gutter_width, gutter_area, content_area)
}

fn compute_vertical_scrollbar(
    editor_area: Rect,
    content_area: Rect,
    pane: &EditorPaneState,
) -> (Rect, Option<Rect>) {
    if content_area.is_empty() || content_area.w <= 1 {
        return (content_area, None);
    }

    let Some(tab) = pane.active_tab() else {
        return (content_area, None);
    };

    let total_lines = tab.buffer.len_lines().max(1);
    let viewport_lines = editor_area.h as usize;
    if viewport_lines == 0 || total_lines <= viewport_lines {
        return (content_area, None);
    }

    let scrollbar_area = Rect::new(
        content_area.right().saturating_sub(1),
        content_area.y,
        1,
        content_area.h,
    );
    let content = Rect::new(
        content_area.x,
        content_area.y,
        content_area.w.saturating_sub(1),
        content_area.h,
    );

    (content, Some(scrollbar_area))
}

pub fn vertical_scrollbar_metrics(
    layout: &EditorPaneLayout,
    total_lines: usize,
    viewport_lines: usize,
    line_offset: usize,
) -> Option<VerticalScrollbarMetrics> {
    let track_area = layout.v_scrollbar_area?;
    if track_area.is_empty() || total_lines == 0 || viewport_lines == 0 {
        return None;
    }

    let max_offset = total_lines.saturating_sub(viewport_lines);
    if max_offset == 0 {
        return None;
    }

    let track_h = track_area.h;
    if track_h == 0 {
        return None;
    }

    let thumb_h = (((track_h as usize) * viewport_lines + total_lines.saturating_sub(1))
        / total_lines)
        .max(1)
        .min(track_h as usize) as u16;
    let thumb_range = track_h.saturating_sub(thumb_h);

    let top = if thumb_range == 0 {
        0
    } else {
        let line_offset = line_offset.min(max_offset) as u128;
        let max_offset = max_offset as u128;
        let range = thumb_range as u128;
        ((line_offset * range + max_offset / 2) / max_offset) as u16
    };

    let thumb_area = Rect::new(track_area.x, track_area.y.saturating_add(top), 1, thumb_h);

    Some(VerticalScrollbarMetrics {
        track_area,
        thumb_area,
        max_offset,
        thumb_range,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/views/editor/layout.rs"]
mod tests;
