use crate::core::text_window;
use crate::kernel::editor::{
    cursor_display_x_abs, EditorPaneState, EditorTabState, HighlightKind, HighlightSpan,
    SearchBarField, SearchBarMode, SearchBarState,
};
use crate::kernel::services::ports::EditorConfig;
use crate::models::slice_to_cow;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Mod, Style};
use crate::ui::core::theme::Theme;
use memchr::memchr;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::layout::EditorPaneLayout;

pub fn paint_editor_pane(
    painter: &mut Painter,
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    config: &EditorConfig,
    theme: &Theme,
    hovered_tab: Option<usize>,
    workspace_empty: bool,
) {
    if layout.area.is_empty() {
        return;
    }

    paint_tabs(painter, layout.tab_area, pane, theme, hovered_tab);

    if let Some(search_area) = layout.search_area {
        paint_search_bar(painter, search_area, &pane.search_bar, theme);
    }

    paint_editor_body(painter, layout, pane, config, theme, workspace_empty);
}

pub fn cursor_position_editor(
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    config: &EditorConfig,
) -> Option<(u16, u16)> {
    if pane.search_bar.visible {
        let area = layout.search_area?;
        return cursor_position_search_bar(area, &pane.search_bar);
    }

    let tab = pane.active_tab()?;
    let (row, _col) = tab.buffer.cursor();
    let (line_offset, horiz_offset) = effective_viewport(tab, layout, config);

    if layout.content_area.is_empty() {
        return None;
    }

    let height = (layout.editor_area.h as usize).max(1);
    let visible_lines = tab.visible_lines_in_viewport(line_offset, height);
    let screen_row = visible_lines.iter().position(|&line| line == row)?;

    let cursor_x_abs = cursor_display_x_abs(&tab.buffer, config.tab_size);
    let cursor_x_rel = cursor_x_abs.saturating_sub(horiz_offset);

    let x = layout
        .content_area
        .x
        .saturating_add(cursor_x_rel.min(layout.content_area.w.saturating_sub(1) as u32) as u16);
    let y = layout
        .content_area
        .y
        .saturating_add(screen_row.min(u16::MAX as usize) as u16);

    Some((x, y))
}

fn effective_viewport(
    tab: &EditorTabState,
    layout: &EditorPaneLayout,
    config: &EditorConfig,
) -> (usize, u32) {
    let mut line_offset = tab.viewport.line_offset;
    let mut horiz_offset = tab.viewport.horiz_offset;

    if !tab.viewport.follow_cursor || layout.editor_area.h == 0 || layout.content_area.w == 0 {
        return (line_offset, horiz_offset);
    }

    let (row, _) = tab.buffer.cursor();
    let height = (layout.editor_area.h as usize).max(1);
    if row < line_offset {
        line_offset = row;
    } else if row >= line_offset + height {
        line_offset = row.saturating_sub(height.saturating_sub(1));
    }

    let cursor_x = cursor_display_x_abs(&tab.buffer, config.tab_size);
    let width = layout.content_area.w.max(1) as u32;
    if cursor_x < horiz_offset {
        horiz_offset = cursor_x;
    } else if cursor_x >= horiz_offset + width {
        horiz_offset = cursor_x.saturating_sub(width.saturating_sub(1));
    }

    (line_offset, horiz_offset)
}

fn paint_tabs(
    painter: &mut Painter,
    area: Rect,
    pane: &EditorPaneState,
    theme: &Theme,
    hovered_tab: Option<usize>,
) {
    if area.is_empty() {
        return;
    }

    // Clear tab bar background to avoid leaking old content on partial redraws.
    let base = Style::default().bg(theme.palette_bg).fg(theme.palette_fg);
    painter.fill_rect(area, base);

    if pane.tabs.is_empty() {
        return;
    }

    const PADDING_LEFT: u16 = 1;
    const PADDING_RIGHT: u16 = 1;
    const CLOSE_BUTTON_WIDTH: u16 = 2;
    const DIVIDER: u16 = 1;

    let y = area.y;
    let right = area.right();
    let row_clip = Rect::new(area.x, y, area.w, 1.min(area.h));
    let mut x = area.x;
    for (i, tab) in pane.tabs.iter().enumerate() {
        if x >= right {
            break;
        }

        // Left padding.
        painter.text_clipped(Pos::new(x, y), " ", Style::default(), row_clip);
        x = x.saturating_add(PADDING_LEFT).min(right);

        let active = i == pane.active;
        let is_hovered = hovered_tab == Some(i);
        let fg = if active {
            theme.header_fg
        } else {
            theme.palette_muted_fg
        };

        if tab.dirty && x < right {
            let style = Style::default().fg(fg);
            painter.text_clipped(Pos::new(x, y), "● ", style, row_clip);
            x = x.saturating_add(2).min(right);
        }

        let mut style = Style::default().fg(fg);
        if active {
            style = style.add_mod(Mod::BOLD);
        }
        painter.text_clipped(Pos::new(x, y), tab.title.as_str(), style, row_clip);
        x = x.saturating_add(tab.title.width().min(u16::MAX as usize) as u16);
        x = x.min(right);

        // Right padding.
        if x < right {
            painter.text_clipped(Pos::new(x, y), " ", Style::default(), row_clip);
        }
        x = x.saturating_add(PADDING_RIGHT).min(right);

        if is_hovered {
            if x < right {
                let close_style = Style::default().fg(theme.accent_fg);
                painter.text_clipped(Pos::new(x, y), "×", close_style, row_clip);
            }
            x = x.saturating_add(1).min(right);
            if x < right {
                painter.text_clipped(Pos::new(x, y), " ", Style::default(), row_clip);
            }
            x = x
                .saturating_add(CLOSE_BUTTON_WIDTH.saturating_sub(1))
                .min(right);
        }

        if i + 1 == pane.tabs.len() {
            break;
        }

        if x < right {
            painter.text_clipped(Pos::new(x, y), " ", Style::default(), row_clip);
        }
        x = x.saturating_add(DIVIDER).min(right);
    }
}

fn paint_search_bar(painter: &mut Painter, area: Rect, state: &SearchBarState, theme: &Theme) {
    if !state.visible || area.is_empty() {
        return;
    }

    let base = Style::default().bg(theme.palette_bg).fg(theme.palette_fg);
    painter.fill_rect(area, base);

    let match_info = search_bar_match_info(state);
    let case_indicator = if state.case_sensitive { "[Aa]" } else { "[aa]" };
    let regex_indicator = if state.use_regex { "[.*]" } else { "[  ]" };

    let label_style = Style::default().fg(theme.header_fg);
    let muted_style = Style::default().fg(theme.palette_muted_fg);

    match state.mode {
        SearchBarMode::Search => {
            if area.h == 0 {
                return;
            }
            let row = Rect::new(area.x, area.y, area.w, 1);
            let (visible_text, _start) = windowed_search_text(
                state.search_text.as_str(),
                state.cursor_pos,
                state.focused_field == SearchBarField::Search,
                row.w,
                case_indicator,
                regex_indicator,
                &match_info,
            );
            let input_style = if state.focused_field == SearchBarField::Search {
                Style::default().fg(theme.palette_fg)
            } else {
                muted_style
            };

            let mut x = row.x;
            painter.text_clipped(Pos::new(x, row.y), "Find: ", label_style, row);
            x = x.saturating_add("Find: ".width() as u16);

            painter.text_clipped(Pos::new(x, row.y), visible_text, input_style, row);
            x = x.saturating_add(visible_text.width().min(u16::MAX as usize) as u16);

            painter.text_clipped(Pos::new(x, row.y), " ", Style::default(), row);
            x = x.saturating_add(1);

            painter.text_clipped(Pos::new(x, row.y), case_indicator, muted_style, row);
            x = x.saturating_add(case_indicator.width().min(u16::MAX as usize) as u16);

            painter.text_clipped(Pos::new(x, row.y), regex_indicator, muted_style, row);
            x = x.saturating_add(regex_indicator.width().min(u16::MAX as usize) as u16);

            painter.text_clipped(Pos::new(x, row.y), " ", Style::default(), row);
            x = x.saturating_add(1);

            painter.text_clipped(Pos::new(x, row.y), match_info, label_style, row);
        }
        SearchBarMode::Replace => {
            if area.h == 0 {
                return;
            }

            let top = Rect::new(area.x, area.y, area.w, 1);
            let (visible_search, _search_start) = windowed_search_text(
                state.search_text.as_str(),
                state.cursor_pos,
                state.focused_field == SearchBarField::Search,
                top.w,
                case_indicator,
                regex_indicator,
                &match_info,
            );
            let search_style = if state.focused_field == SearchBarField::Search {
                Style::default().fg(theme.palette_fg)
            } else {
                muted_style
            };

            let mut x = top.x;
            painter.text_clipped(Pos::new(x, top.y), "Find: ", label_style, top);
            x = x.saturating_add("Find: ".width() as u16);

            painter.text_clipped(Pos::new(x, top.y), visible_search, search_style, top);
            x = x.saturating_add(visible_search.width().min(u16::MAX as usize) as u16);

            painter.text_clipped(Pos::new(x, top.y), " ", Style::default(), top);
            x = x.saturating_add(1);

            painter.text_clipped(Pos::new(x, top.y), case_indicator, muted_style, top);
            x = x.saturating_add(case_indicator.width().min(u16::MAX as usize) as u16);

            painter.text_clipped(Pos::new(x, top.y), regex_indicator, muted_style, top);
            x = x.saturating_add(regex_indicator.width().min(u16::MAX as usize) as u16);

            painter.text_clipped(Pos::new(x, top.y), " ", Style::default(), top);
            x = x.saturating_add(1);

            painter.text_clipped(Pos::new(x, top.y), match_info, label_style, top);

            if area.h >= 2 {
                let replace_area = Rect::new(area.x, area.y.saturating_add(1), area.w, 1);
                let (visible_replace, _replace_start) = windowed_replace_text(
                    state.replace_text.as_str(),
                    state.cursor_pos,
                    state.focused_field == SearchBarField::Replace,
                    replace_area.w,
                );
                let replace_style = if state.focused_field == SearchBarField::Replace {
                    Style::default().fg(theme.palette_fg)
                } else {
                    muted_style
                };

                let mut x = replace_area.x;
                painter.text_clipped(
                    Pos::new(x, replace_area.y),
                    "Replace: ",
                    label_style,
                    replace_area,
                );
                x = x.saturating_add("Replace: ".width() as u16);
                painter.text_clipped(
                    Pos::new(x, replace_area.y),
                    visible_replace,
                    replace_style,
                    replace_area,
                );
            }
        }
    }
}

fn search_bar_match_info(state: &SearchBarState) -> String {
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

fn windowed_search_text<'a>(
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
        .saturating_add(match_info.width() as u16);
    let prefix_w = prefix.width() as u16;
    let available = area_width.saturating_sub(prefix_w).saturating_sub(suffix_w) as usize;
    let cursor = if focused { cursor_pos } else { text.len() }.min(text.len());
    let (start, end) = text_window::window(text, cursor, available);
    (&text[start..end], start)
}

fn windowed_replace_text(
    text: &str,
    cursor_pos: usize,
    focused: bool,
    area_width: u16,
) -> (&str, usize) {
    let prefix = "Replace: ";
    let prefix_w = prefix.width() as u16;
    let available = area_width.saturating_sub(prefix_w) as usize;
    let cursor = if focused { cursor_pos } else { text.len() }.min(text.len());
    let (start, end) = text_window::window(text, cursor, available);
    (&text[start..end], start)
}

fn paint_editor_body(
    painter: &mut Painter,
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    config: &EditorConfig,
    theme: &Theme,
    workspace_empty: bool,
) {
    if layout.editor_area.is_empty() {
        return;
    }

    let base_style = Style::default().bg(theme.palette_bg).fg(theme.palette_fg);
    painter.fill_rect(layout.editor_area, base_style);

    let Some(tab) = pane.active_tab() else {
        let style = Style::default()
            .bg(theme.palette_bg)
            .fg(theme.palette_muted_fg);
        let msg = if workspace_empty {
            "Folder is empty"
        } else {
            "No file open"
        };

        let y = layout
            .editor_area
            .y
            .saturating_add(layout.editor_area.h / 2);
        let msg_w = UnicodeWidthStr::width(msg).min(u16::MAX as usize) as u16;
        let x = layout
            .editor_area
            .x
            .saturating_add(layout.editor_area.w.saturating_sub(msg_w) / 2);
        let row_clip = Rect::new(layout.editor_area.x, y, layout.editor_area.w, 1);
        painter.text_clipped(Pos::new(x, y), msg, style, row_clip);
        return;
    };

    let (line_offset, horiz_offset) = effective_viewport(tab, layout, config);
    let height = layout.editor_area.h as usize;
    let visible_lines = tab.visible_lines_in_viewport(line_offset, height.max(1));
    let syntax = build_syntax_highlights(tab, &visible_lines);

    if config.show_line_numbers && !layout.gutter_area.is_empty() {
        paint_gutter(
            painter,
            layout.gutter_area,
            tab,
            &visible_lines,
            tab.buffer.cursor().0,
            theme,
        );
    }

    if layout.content_area.is_empty() {
        return;
    }

    paint_content(
        painter,
        tab,
        ContentPaintCtx {
            area: layout.content_area,
            visible_lines: &visible_lines,
            horiz_offset,
            highlight_lines: syntax.as_deref(),
            tab_size: config.tab_size,
            theme,
            show_indent_guides: config.show_indent_guides,
        },
    );
}

fn build_syntax_highlights(
    tab: &EditorTabState,
    visible_lines: &[usize],
) -> Option<Vec<Vec<HighlightSpan>>> {
    if visible_lines.is_empty() {
        return Some(Vec::new());
    }

    let mut out: Vec<Vec<HighlightSpan>> = Vec::with_capacity(visible_lines.len());
    let mut idx = 0usize;
    while idx < visible_lines.len() {
        let start = visible_lines[idx];
        let mut end = start.saturating_add(1);
        let mut next = idx.saturating_add(1);
        while next < visible_lines.len() && visible_lines[next] == end {
            end = end.saturating_add(1);
            next = next.saturating_add(1);
        }

        let segment = tab.highlight_lines(start, end)?;
        out.extend(segment);
        idx = next;
    }

    Some(out)
}

fn paint_gutter(
    painter: &mut Painter,
    area: Rect,
    tab: &EditorTabState,
    lines: &[usize],
    active_row: usize,
    theme: &Theme,
) {
    if area.is_empty() {
        return;
    }

    let base_style = Style::default()
        .bg(theme.palette_bg)
        .fg(theme.palette_muted_fg);
    painter.fill_rect(area, base_style);

    let digits_width = area.w.saturating_sub(2) as usize;
    if digits_width == 0 {
        return;
    }

    let highlight_style = Style::default()
        .bg(theme.palette_bg)
        .fg(theme.header_fg)
        .add_mod(Mod::BOLD);

    let right = area.right();
    let bottom = area.bottom();
    for y in area.y..bottom {
        let row = (y - area.y) as usize;
        let Some(&line) = lines.get(row) else {
            continue;
        };

        let line_no = line.saturating_add(1);
        let style = if line == active_row {
            highlight_style
        } else {
            base_style
        };

        // Reserve last 2 columns: fold marker + git marker.
        if area.w >= 2 {
            let x = right.saturating_sub(2);
            if let Some(marker) = tab.fold_marker_char(line.min(u32::MAX as usize) as u32) {
                painter.text_clipped(
                    Pos::new(x, y),
                    marker.to_string(),
                    style,
                    Rect::new(x, y, 1, 1),
                );
            }
        }
        if area.w >= 1 {
            let git_x = right.saturating_sub(1);
            if let Some(marker) = tab.git_gutter_marker(line) {
                let marker_style = match marker {
                    '+' => style.fg(theme.syntax_string_fg),
                    '~' => style.fg(theme.warning_fg),
                    '-' => style.fg(theme.error_fg),
                    _ => style,
                };
                painter.text_clipped(
                    Pos::new(git_x, y),
                    marker.to_string(),
                    marker_style,
                    Rect::new(git_x, y, 1, 1),
                );
            }
        }

        let num = line_no.to_string();
        let digits = if num.len() > digits_width {
            &num[num.len().saturating_sub(digits_width)..]
        } else {
            num.as_str()
        };
        if digits.is_empty() {
            continue;
        }

        let x_end = right.saturating_sub(2);
        let x_start = x_end.saturating_sub(digits.len().min(u16::MAX as usize) as u16);
        painter.text_clipped(
            Pos::new(x_start, y),
            digits,
            style,
            Rect::new(area.x, y, area.w.saturating_sub(2), 1),
        );
    }
}

#[derive(Clone, Copy, Debug)]
struct TextSegment {
    start: usize,
    end: usize,
    x: u16,
    style: Style,
}

fn flush_text_segment(
    painter: &mut Painter,
    line: &str,
    y: u16,
    clip: Rect,
    seg: &mut Option<TextSegment>,
) {
    let Some(seg) = seg.take() else {
        return;
    };

    let end = seg.end.min(line.len());
    let start = seg.start.min(end);
    if start >= end {
        return;
    }

    painter.text_clipped(Pos::new(seg.x, y), &line[start..end], seg.style, clip);
}

struct ContentPaintCtx<'a> {
    area: Rect,
    visible_lines: &'a [usize],
    horiz_offset: u32,
    highlight_lines: Option<&'a [Vec<HighlightSpan>]>,
    tab_size: u8,
    theme: &'a Theme,
    show_indent_guides: bool,
}

fn paint_content(painter: &mut Painter, tab: &EditorTabState, ctx: ContentPaintCtx<'_>) {
    let ContentPaintCtx {
        area,
        visible_lines,
        horiz_offset,
        highlight_lines,
        tab_size,
        theme,
        show_indent_guides,
    } = ctx;
    if area.is_empty() {
        return;
    }

    let base_style = Style::default().bg(theme.palette_bg).fg(theme.palette_fg);
    painter.fill_rect(area, base_style);

    let selection_style = Style::default()
        .bg(theme.palette_selected_bg)
        .fg(theme.palette_selected_fg);

    let indent_guide_style = Style::default().fg(theme.indent_guide_fg);

    let tab_size = tab_size.max(1) as u32;

    let bottom = area.bottom();
    for y in area.y..bottom {
        let screen_row = (y - area.y) as usize;
        let Some(&row) = visible_lines.get(screen_row) else {
            continue;
        };

        let selection_range =
            selection_range_for_row(tab.buffer.selection(), row).unwrap_or((0, 0));
        let has_selection = tab.buffer.selection().is_some_and(|s| !s.is_empty());

        let highlight_spans = highlight_lines
            .and_then(|lines| lines.get(screen_row))
            .map(|spans| spans.as_slice());
        let semantic_spans = tab.semantic_highlight_line(row);
        let inlay_hints = tab.inlay_hint_line(row);

        let line = tab
            .buffer
            .line_slice(row)
            .map(slice_to_cow)
            .unwrap_or_default();
        let line = line.strip_suffix('\n').unwrap_or(&line);
        let line = line.strip_suffix('\r').unwrap_or(line);

        let mut x = area.x;
        let right = area.right();
        let row_clip = Rect::new(area.x, y, area.w, 1);

        let mut visible = line;
        let mut g_idx_base: usize = 0;
        let mut display_col: u32 = 0;
        let mut byte_offset: usize = 0;
        let mut semantic_idx: usize = 0;
        let mut highlight_idx: usize = 0;

        if horiz_offset > 0 {
            let start = (horiz_offset as usize).min(line.len());
            let prefix = &line.as_bytes()[..start];
            if line.is_char_boundary(start) && prefix.is_ascii() && memchr(b'\t', prefix).is_none()
            {
                visible = &line[start..];
                g_idx_base = start;
                display_col = start.min(u32::MAX as usize) as u32;
                byte_offset = start;
            }
        }

        let mut seg: Option<TextSegment> = None;

        for (g_rel_idx, g) in visible.graphemes(true).enumerate() {
            let g_idx = g_idx_base.saturating_add(g_rel_idx);
            let g_start = byte_offset;
            byte_offset = byte_offset.saturating_add(g.len());

            let width = if g == "\t" {
                let rem = display_col % tab_size;
                if rem == 0 {
                    tab_size
                } else {
                    tab_size - rem
                }
            } else {
                g.width() as u32
            };

            if width == 0 {
                continue;
            }

            if display_col < horiz_offset {
                display_col = display_col.saturating_add(width);
                continue;
            }

            if x >= right {
                flush_text_segment(painter, line, y, row_clip, &mut seg);
                break;
            }

            let mut style = base_style;
            if has_selection
                && g_idx >= selection_range.0
                && g_idx < selection_range.1
                && selection_range.0 != selection_range.1
            {
                style = selection_style;
            } else if let Some(hl) =
                style_for_highlight(semantic_spans, &mut semantic_idx, g_start, theme).or_else(
                    || style_for_highlight(highlight_spans, &mut highlight_idx, g_start, theme),
                )
            {
                style = base_style.patch(hl);
            }

            if g == "\t" {
                flush_text_segment(painter, line, y, row_clip, &mut seg);

                let w = width.min(u16::MAX as u32) as u16;
                let visible_w = w.min(right.saturating_sub(x));
                if visible_w == 0 {
                    break;
                }
                if style != base_style {
                    painter.style_rect(Rect::new(x, y, visible_w, 1), style);
                }
                x = x.saturating_add(visible_w);
                display_col = display_col.saturating_add(width);
                continue;
            }

            let w = width.min(u16::MAX as u32) as u16;
            if x.saturating_add(w) > right {
                flush_text_segment(painter, line, y, row_clip, &mut seg);
                break;
            }

            if let Some(mut cur) = seg {
                if cur.style == style {
                    cur.end = byte_offset;
                    seg = Some(cur);
                } else {
                    flush_text_segment(painter, line, y, row_clip, &mut seg);
                    seg = Some(TextSegment {
                        start: g_start,
                        end: byte_offset,
                        x,
                        style,
                    });
                }
            } else {
                seg = Some(TextSegment {
                    start: g_start,
                    end: byte_offset,
                    x,
                    style,
                });
            }

            x = x.saturating_add(w);
            display_col = display_col.saturating_add(width);
        }

        flush_text_segment(painter, line, y, row_clip, &mut seg);

        // Draw indent guides after the line has been rendered so we can overlay on whitespace.
        if show_indent_guides {
            let indent_len = line.len().saturating_sub(line.trim_start().len());
            if indent_len > 0 {
                let indent_prefix = &line[..indent_len];
                let mut col: u32 = 0;
                for (g_idx, ch) in indent_prefix.chars().enumerate() {
                    let w = if ch == '\t' {
                        let rem = col % tab_size;
                        if rem == 0 {
                            tab_size
                        } else {
                            tab_size - rem
                        }
                    } else {
                        1
                    };

                    col = col.saturating_add(w);
                    if col == 0 || !col.is_multiple_of(tab_size) {
                        continue;
                    }

                    // Use the last cell within the indent level so we don't overwrite code.
                    let guide_col = col.saturating_sub(1);
                    let rel = guide_col.saturating_sub(horiz_offset);
                    if rel >= area.w as u32 {
                        continue;
                    }
                    let guide_x = area.x.saturating_add(rel as u16);
                    if guide_x >= right {
                        continue;
                    }

                    let selected = has_selection
                        && g_idx >= selection_range.0
                        && g_idx < selection_range.1
                        && selection_range.0 != selection_range.1;
                    let style = if selected {
                        indent_guide_style.bg(theme.palette_selected_bg)
                    } else {
                        indent_guide_style.bg(theme.palette_bg)
                    };

                    painter.text_clipped(Pos::new(guide_x, y), "│", style, row_clip);
                }
            }
        }

        if let Some(hints) = inlay_hints {
            if x < right {
                let mut hint_text = String::new();
                for hint in hints {
                    let hint = hint.trim();
                    if hint.is_empty() {
                        continue;
                    }
                    if !hint_text.is_empty() {
                        hint_text.push(' ');
                    }
                    hint_text.push_str(hint);
                }

                if !hint_text.is_empty() {
                    hint_text.insert(0, ' ');
                    let avail = right.saturating_sub(x) as usize;
                    let end = text_window::truncate_to_width(&hint_text, avail);
                    let visible_hint = hint_text.get(..end).unwrap_or_default();

                    let hint_style = Style::default()
                        .fg(theme.palette_muted_fg)
                        .add_mod(Mod::ITALIC);
                    painter.text_clipped(Pos::new(x, y), visible_hint, hint_style, row_clip);
                }
            }
        }
    }
}

fn selection_range_for_row(
    selection: Option<&crate::models::Selection>,
    row: usize,
) -> Option<(usize, usize)> {
    let s = selection?;
    let ((start_row, start_col), (end_row, end_col)) = s.range();
    if row < start_row || row > end_row {
        return None;
    }

    let (sel_start, sel_end) = if row == start_row && row == end_row {
        (start_col, end_col)
    } else if row == start_row {
        (start_col, usize::MAX)
    } else if row == end_row {
        (0, end_col)
    } else {
        (0, usize::MAX)
    };

    Some((sel_start, sel_end))
}

fn style_for_highlight(
    highlight_spans: Option<&[HighlightSpan]>,
    highlight_idx: &mut usize,
    byte_offset: usize,
    theme: &Theme,
) -> Option<Style> {
    let spans = highlight_spans?;

    while *highlight_idx < spans.len() && spans[*highlight_idx].end <= byte_offset {
        *highlight_idx += 1;
    }

    let span = spans.get(*highlight_idx)?;
    if byte_offset < span.start || byte_offset >= span.end {
        return None;
    }

    let style = match span.kind {
        HighlightKind::Comment => Style::default().fg(theme.palette_muted_fg),
        HighlightKind::String => Style::default().fg(theme.syntax_string_fg),
        HighlightKind::Keyword => Style::default().fg(theme.accent_fg).add_mod(Mod::BOLD),
        HighlightKind::Type => Style::default().fg(theme.header_fg),
        HighlightKind::Number => Style::default().fg(theme.syntax_number_fg),
        HighlightKind::Attribute => Style::default().fg(theme.syntax_attribute_fg),
        HighlightKind::Lifetime => Style::default().fg(theme.syntax_number_fg),
        HighlightKind::Function => Style::default().fg(theme.accent_fg),
        HighlightKind::Macro => Style::default().fg(theme.syntax_attribute_fg),
        HighlightKind::Variable => Style::default().fg(theme.palette_fg),
    };

    Some(style)
}

fn cursor_position_search_bar(area: Rect, state: &SearchBarState) -> Option<(u16, u16)> {
    if area.is_empty() {
        return None;
    }

    let match_info = search_bar_match_info(state);
    let case_indicator = if state.case_sensitive { "[Aa]" } else { "[aa]" };
    let regex_indicator = if state.use_regex { "[.*]" } else { "[  ]" };

    match state.focused_field {
        SearchBarField::Search => {
            let y = area.y;
            let cursor = state.cursor_pos.min(state.search_text.len());
            let (_visible, start) = windowed_search_text(
                state.search_text.as_str(),
                cursor,
                true,
                area.w,
                case_indicator,
                regex_indicator,
                &match_info,
            );
            let before = state.search_text.get(start..cursor).unwrap_or_default();

            let prefix_w = "Find: ".width() as u16;
            let suffix_w = 1u16
                .saturating_add(case_indicator.width() as u16)
                .saturating_add(regex_indicator.width() as u16)
                .saturating_add(1)
                .saturating_add(match_info.width() as u16);

            let x = area
                .x
                .saturating_add(prefix_w)
                .saturating_add(before.width() as u16)
                .min(area.x.saturating_add(area.w.saturating_sub(suffix_w)));
            Some((x, y))
        }
        SearchBarField::Replace => {
            let y = area.y.saturating_add(1);
            if y >= area.bottom() {
                return None;
            }

            let cursor = state.cursor_pos.min(state.replace_text.len());
            let (_visible, start) =
                windowed_replace_text(state.replace_text.as_str(), cursor, true, area.w);
            let before = state.replace_text.get(start..cursor).unwrap_or_default();

            let prefix_w = "Replace: ".width() as u16;
            let x = area
                .x
                .saturating_add(prefix_w)
                .saturating_add(before.width() as u16)
                .min(area.x.saturating_add(area.w.saturating_sub(1)));
            Some((x, y))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/views/editor/render.rs"]
mod tests;
