//! 搜索栏布局的唯一规范实现：match-info 文案、可视文本窗口、导航按钮原点。
//!
//! 绘制（`render::paint_search_bar`）与命中（`hit_test::hit_test_search_bar`）必须对导航
//! 按钮（▲ ▼ ✕）用完全相同的 x 计算，否则点击会漂移到错误按钮。两边共消费本模块的
//! `search_bar_nav_origin`，杜绝两份坐标走查各自漂移。

use crate::core::text_window;
use crate::kernel::editor::{SearchBarField, SearchBarState};
use unicode_width::UnicodeWidthStr;

/// 导航按钮区 " ▲ ▼ ✕" 的显示宽度。
pub(super) const SEARCH_NAV_BUTTONS_WIDTH: u16 = 8;

pub(super) fn search_bar_match_info(state: &SearchBarState) -> String {
    if state.searching {
        "Searching...".to_string()
    } else if let Some(err) = state.last_error.as_deref() {
        format!("Error: {}", err)
    } else if state.matches.is_empty() {
        if state.search_text.is_empty() {
            String::new()
        } else {
            "No results".to_string()
        }
    } else {
        let current = state.current_match_index.map(|i| i + 1).unwrap_or(0);
        format!("{}/{}", current, state.matches.len())
    }
}

pub(super) fn windowed_search_text<'a>(
    text: &'a str,
    cursor_pos: usize,
    focused: bool,
    area_width: u16,
    case_indicator: &str,
    regex_indicator: &str,
    match_info: &str,
) -> (&'a str, usize) {
    let prefix = "Find: ";
    let suffix_w = 1u16
        .saturating_add(case_indicator.width() as u16)
        .saturating_add(regex_indicator.width() as u16)
        .saturating_add(1)
        .saturating_add(match_info.width() as u16)
        .saturating_add(SEARCH_NAV_BUTTONS_WIDTH);
    let prefix_w = prefix.width() as u16;
    let available = area_width.saturating_sub(prefix_w).saturating_sub(suffix_w) as usize;
    let cursor = if focused { cursor_pos } else { text.len() }.min(text.len());
    let (start, end) = text_window::window(text, cursor, available);
    (&text[start..end], start)
}

/// 导航按钮（▲ ▼ ✕）的左上角原点。绘制据此摆放按钮、命中据此定位点击，二者同源。
pub(super) fn search_bar_nav_origin(
    area_x: u16,
    area_y: u16,
    area_w: u16,
    state: &SearchBarState,
) -> (u16, u16) {
    let match_info = search_bar_match_info(state);
    let case_indicator = if state.case_sensitive { "[Aa]" } else { "[aa]" };
    let regex_indicator = if state.use_regex { "[.*]" } else { "[  ]" };
    let (visible_text, _start) = windowed_search_text(
        state.search_text.as_str(),
        state.cursor_pos,
        state.focused_field == SearchBarField::Search,
        area_w,
        case_indicator,
        regex_indicator,
        &match_info,
    );

    let mut x = area_x;
    x = x.saturating_add("Find: ".width() as u16);
    x = x.saturating_add(visible_text.width().min(u16::MAX as usize) as u16);
    x = x.saturating_add(1);
    x = x.saturating_add(case_indicator.width().min(u16::MAX as usize) as u16);
    x = x.saturating_add(regex_indicator.width().min(u16::MAX as usize) as u16);
    x = x.saturating_add(1);
    x = x.saturating_add(match_info.width().min(u16::MAX as usize) as u16);
    (x, area_y)
}
