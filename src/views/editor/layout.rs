use crate::kernel::editor::EditorPaneState;
use crate::kernel::services::ports::EditorConfig;
use ratatui::layout::Rect;

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

    if area.width == 0 || area.height == 0 {
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

    let tab_height = TAB_HEIGHT.min(area.height);
    let tab_area = Rect::new(area.x, area.y, area.width, tab_height);

    let search_h = pane.search_bar.height();
    let available_after_tabs = area.height.saturating_sub(tab_height);
    let search_height = search_h.min(available_after_tabs);

    let search_area = (search_height > 0).then_some(Rect::new(
        area.x,
        area.y.saturating_add(tab_height),
        area.width,
        search_height,
    ));

    let chrome_h = tab_height.saturating_add(search_height);
    let editor_area = Rect::new(
        area.x,
        area.y.saturating_add(chrome_h),
        area.width,
        area.height.saturating_sub(chrome_h),
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
    if editor_area.width == 0 || editor_area.height == 0 || !config.show_line_numbers {
        return (
            0,
            Rect::default(),
            Rect::new(
                editor_area.x,
                editor_area.y,
                editor_area.width,
                editor_area.height,
            ),
        );
    }

    let Some(tab) = pane.active_tab() else {
        return (
            0,
            Rect::default(),
            Rect::new(
                editor_area.x,
                editor_area.y,
                editor_area.width,
                editor_area.height,
            ),
        );
    };

    let total_lines = tab.buffer.len_lines().max(1);
    let digits = total_lines.to_string().len();
    let gutter_width = ((digits + 2) as u16).min(editor_area.width);

    if gutter_width == 0 || gutter_width >= editor_area.width {
        return (
            gutter_width,
            Rect::new(
                editor_area.x,
                editor_area.y,
                gutter_width,
                editor_area.height,
            ),
            Rect::default(),
        );
    }

    let gutter_area = Rect::new(
        editor_area.x,
        editor_area.y,
        gutter_width,
        editor_area.height,
    );
    let content_area = Rect::new(
        editor_area.x + gutter_width,
        editor_area.y,
        editor_area.width - gutter_width,
        editor_area.height,
    );

    (gutter_width, gutter_area, content_area)
}
