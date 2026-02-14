use crate::kernel::editor::EditorPaneState;
use crate::ui::core::geom::Rect;
use unicode_width::UnicodeWidthStr;

const PADDING_LEFT: u16 = 1;
const PADDING_RIGHT: u16 = 1;
const DIRTY_WIDTH: u16 = 2;
const CLOSE_BUTTON_WIDTH: u16 = 2;
const DIVIDER_WIDTH: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabRowSlot {
    pub index: usize,
    pub start: u16,
    pub end: u16,
    pub hit_end: u16,
    pub dirty_x: Option<u16>,
    pub title_x: u16,
    pub title_width: u16,
    pub close_start: Option<u16>,
    pub close_end: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRowLayout {
    pub slots: Vec<TabRowSlot>,
}

pub fn compute_tab_row_layout(
    area: Rect,
    pane: &EditorPaneState,
    hovered_tab: Option<usize>,
) -> TabRowLayout {
    if area.is_empty() || pane.tabs.is_empty() {
        return TabRowLayout { slots: Vec::new() };
    }

    let preferred_title_widths: Vec<usize> = pane
        .tabs
        .iter()
        .map(|tab| UnicodeWidthStr::width(tab.title.as_str()))
        .collect();
    let title_budget = (area.w as usize).saturating_sub(total_fixed_width(pane, hovered_tab));
    let allocated_title_widths = allocate_title_widths(&preferred_title_widths, title_budget);

    let right = area.right();
    let mut x = area.x;
    let mut slots = Vec::with_capacity(pane.tabs.len());

    for (index, tab) in pane.tabs.iter().enumerate() {
        if x >= right {
            break;
        }

        let start = x;
        x = x.saturating_add(PADDING_LEFT).min(right);

        let dirty_x = if tab.dirty && x < right {
            let pos = x;
            x = x.saturating_add(DIRTY_WIDTH).min(right);
            Some(pos)
        } else {
            None
        };

        let title_x = x;
        let requested_title_width = allocated_title_widths
            .get(index)
            .copied()
            .unwrap_or(0)
            .min(u16::MAX as usize) as u16;
        let title_width = requested_title_width.min(right.saturating_sub(title_x));
        x = x.saturating_add(title_width).min(right);

        x = x.saturating_add(PADDING_RIGHT).min(right);

        let mut close_start = None;
        let mut close_end = x;
        if hovered_tab == Some(index) {
            close_start = Some(x);
            close_end = x.saturating_add(CLOSE_BUTTON_WIDTH).min(right);
            x = close_end;
        }

        let end = x;
        let hit_end = if hovered_tab == Some(index) {
            close_start.unwrap_or(end).min(end)
        } else {
            end
        };

        slots.push(TabRowSlot {
            index,
            start,
            end,
            hit_end,
            dirty_x,
            title_x,
            title_width,
            close_start,
            close_end,
        });

        if index + 1 < pane.tabs.len() {
            x = x.saturating_add(DIVIDER_WIDTH).min(right);
        }
    }

    TabRowLayout { slots }
}

pub fn ellipsize_title(title: &str, max_width: u16) -> String {
    let max_width = max_width as usize;
    if max_width == 0 {
        return String::new();
    }

    if UnicodeWidthStr::width(title) <= max_width {
        return title.to_string();
    }

    if max_width == 1 {
        return "…".to_string();
    }

    let keep = crate::core::text_window::truncate_to_width(title, max_width - 1);
    if keep == 0 {
        return "…".to_string();
    }

    let mut out = String::with_capacity(keep + 3);
    out.push_str(&title[..keep]);
    out.push('…');
    out
}

fn total_fixed_width(pane: &EditorPaneState, hovered_tab: Option<usize>) -> usize {
    let mut total = pane.tabs.len().saturating_sub(1) * (DIVIDER_WIDTH as usize);
    for (index, tab) in pane.tabs.iter().enumerate() {
        total = total
            .saturating_add(PADDING_LEFT as usize)
            .saturating_add(PADDING_RIGHT as usize);
        if tab.dirty {
            total = total.saturating_add(DIRTY_WIDTH as usize);
        }
        if hovered_tab == Some(index) {
            total = total.saturating_add(CLOSE_BUTTON_WIDTH as usize);
        }
    }
    total
}

fn allocate_title_widths(preferred: &[usize], budget: usize) -> Vec<usize> {
    let mut widths = vec![0; preferred.len()];
    let mut remaining = budget;

    for width in &mut widths {
        if remaining == 0 {
            break;
        }
        *width = 1;
        remaining -= 1;
    }

    if remaining == 0 {
        return widths;
    }

    let mut needs: Vec<usize> = preferred
        .iter()
        .zip(widths.iter())
        .map(|(preferred, assigned)| preferred.saturating_sub(*assigned))
        .collect();

    while remaining > 0 {
        let mut progressed = false;
        for (width, need) in widths.iter_mut().zip(needs.iter_mut()) {
            if *need == 0 {
                continue;
            }
            *width += 1;
            *need -= 1;
            remaining -= 1;
            progressed = true;
            if remaining == 0 {
                break;
            }
        }
        if !progressed {
            break;
        }
    }

    widths
}
