use crate::kernel::editor::EditorPaneState;
use crate::kernel::services::ports::EditorConfig;
use crate::ui::core::geom::Rect;

#[derive(Debug, Clone, Copy)]
pub struct EditorPaneLayout {
    pub area: Rect,
    pub tab_area: Rect,
    pub search_area: Option<Rect>,
    pub editor_area: Rect,
    pub gutter_area: Rect,
    pub content_area: Rect,
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

    EditorPaneLayout {
        area,
        tab_area,
        search_area,
        editor_area,
        gutter_area,
        content_area,
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
