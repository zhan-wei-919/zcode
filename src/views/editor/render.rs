use crate::core::text_window;
use crate::kernel::editor::{
    cursor_display_x_abs, EditorPaneState, EditorTabState, HighlightKind, HighlightSpan,
    SearchBarField, SearchBarMode, SearchBarState,
};
use crate::kernel::services::ports::{EditorConfig, Match};
use crate::models::slice_to_cow;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Color, Mod, Style};
use crate::ui::core::theme::Theme;
use crate::views::doc::{self, DocLine, DocSpan, DocSpanKind};
use memchr::memchr;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::layout::{vertical_scrollbar_metrics, EditorPaneLayout, VerticalScrollbarMetrics};
use super::markdown::{self, MarkdownDocument};
use super::tab_row::{compute_tab_row_layout, ellipsize_title};

// U+250A "BOX DRAWINGS LIGHT QUADRUPLE DASH VERTICAL" keeps guides subtle.
const INDENT_GUIDE_SYMBOL: &str = "\u{250A}";

/// Width of the search bar navigation buttons: " ▲ ▼ ✕"
const SEARCH_NAV_BUTTONS: &str = " \u{25B2} \u{25BC} \u{2715}";
const SEARCH_NAV_BUTTONS_WIDTH: u16 = 8;
const V_SCROLL_TRACK_SYMBOL: char = '│';
const V_SCROLL_THUMB_SYMBOL: char = '█';

#[derive(Debug, Clone, Copy, Default)]
pub struct EditorPaneRenderOptions {
    pub hovered_tab: Option<usize>,
    pub workspace_empty: bool,
    pub show_vertical_scrollbar: bool,
}

pub fn paint_editor_pane(
    painter: &mut Painter,
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    config: &EditorConfig,
    theme: &Theme,
    options: EditorPaneRenderOptions,
    markdown: Option<&MarkdownDocument>,
) {
    if layout.area.is_empty() {
        return;
    }

    paint_tabs(painter, layout.tab_area, pane, theme, options.hovered_tab);

    if let Some(search_area) = layout.search_area {
        paint_search_bar(painter, search_area, &pane.search_bar, theme);
    }

    paint_editor_body(painter, layout, pane, config, theme, options, markdown);
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
    let base = Style::default().bg(theme.editor_bg).fg(theme.palette_fg);
    painter.fill_rect(area, base);

    if pane.tabs.is_empty() {
        return;
    }
    let y = area.y;
    let row_clip = Rect::new(area.x, y, area.w, 1.min(area.h));
    let row_layout = compute_tab_row_layout(area, pane, hovered_tab);

    for slot in row_layout.slots {
        let tab = &pane.tabs[slot.index];
        let active = slot.index == pane.active;
        let is_hovered = hovered_tab == Some(slot.index);

        if active && slot.end > slot.start {
            let active_bg = Style::default()
                .bg(theme.palette_selected_bg)
                .fg(theme.palette_selected_fg);
            painter.fill_rect(
                Rect::new(slot.start, y, slot.end - slot.start, 1),
                active_bg,
            );
        }

        let text_style = if active {
            Style::default()
                .fg(theme.palette_selected_fg)
                .bg(theme.palette_selected_bg)
                .add_mod(Mod::BOLD)
        } else {
            Style::default().fg(theme.palette_muted_fg)
        };

        if let Some(dirty_x) = slot.dirty_x {
            painter.text_clipped(Pos::new(dirty_x, y), "● ", text_style, row_clip);
        }

        if slot.title_width > 0 {
            let title = ellipsize_title(tab.title.as_str(), slot.title_width);
            painter.text_clipped(Pos::new(slot.title_x, y), title, text_style, row_clip);
        }

        if is_hovered {
            if let Some(close_x) = slot.close_start {
                if close_x < slot.close_end {
                    let close_style = if active {
                        text_style
                    } else {
                        Style::default().fg(theme.accent_fg)
                    };
                    painter.text_clipped(Pos::new(close_x, y), "×", close_style, row_clip);
                }
            }
        }
    }
}

fn paint_search_bar(painter: &mut Painter, area: Rect, state: &SearchBarState, theme: &Theme) {
    if !state.visible || area.is_empty() {
        return;
    }

    let base = Style::default().bg(theme.editor_bg).fg(theme.palette_fg);
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

            painter.text_clipped(Pos::new(x, row.y), &match_info, label_style, row);
            x = x.saturating_add(match_info.width().min(u16::MAX as usize) as u16);

            paint_search_bar_nav_buttons(painter, x, row.y, row, theme);
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

            painter.text_clipped(Pos::new(x, top.y), &match_info, label_style, top);
            x = x.saturating_add(match_info.width().min(u16::MAX as usize) as u16);

            paint_search_bar_nav_buttons(painter, x, top.y, top, theme);

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

/// Paint ▲ ▼ ✕ navigation buttons after match info. Returns the x position after buttons.
fn paint_search_bar_nav_buttons(
    painter: &mut Painter,
    x: u16,
    y: u16,
    row_clip: Rect,
    theme: &Theme,
) -> u16 {
    let style = Style::default().fg(theme.palette_fg);
    painter.text_clipped(Pos::new(x, y), SEARCH_NAV_BUTTONS, style, row_clip);
    x.saturating_add(SEARCH_NAV_BUTTONS_WIDTH)
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
        .saturating_add(match_info.width() as u16)
        .saturating_add(SEARCH_NAV_BUTTONS_WIDTH);
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
    options: EditorPaneRenderOptions,
    markdown: Option<&MarkdownDocument>,
) {
    if layout.editor_area.is_empty() {
        return;
    }

    let base_style = Style::default().bg(theme.editor_bg).fg(theme.palette_fg);
    painter.fill_rect(layout.editor_area, base_style);

    let Some(tab) = pane.active_tab() else {
        let style = Style::default()
            .bg(theme.editor_bg)
            .fg(theme.palette_muted_fg);
        let msg = if options.workspace_empty {
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
    let scrollbar_metrics = options.show_vertical_scrollbar.then(|| {
        vertical_scrollbar_metrics(
            layout,
            tab.buffer.len_lines().max(1),
            layout.editor_area.h as usize,
            line_offset,
        )
    });
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
            search_matches: &pane.search_bar.matches,
            current_match_index: pane.search_bar.current_match_index,
            markdown,
        },
    );

    if let Some(metrics) = scrollbar_metrics.flatten() {
        paint_vertical_scrollbar(painter, &metrics, theme);
    }
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
        .bg(theme.editor_bg)
        .fg(theme.palette_muted_fg);
    painter.fill_rect(area, base_style);

    let digits_width = area.w.saturating_sub(2) as usize;
    if digits_width == 0 {
        return;
    }

    let highlight_style = Style::default()
        .bg(theme.editor_bg)
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
    search_matches: &'a [Match],
    current_match_index: Option<usize>,
    markdown: Option<&'a MarkdownDocument>,
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
        search_matches,
        current_match_index,
        markdown,
    } = ctx;
    if area.is_empty() {
        return;
    }

    let base_style = Style::default().bg(theme.editor_bg).fg(theme.palette_fg);
    painter.fill_rect(area, base_style);

    let selection_style = Style::default()
        .bg(theme.palette_selected_bg)
        .fg(theme.palette_selected_fg);

    // Indent guides should be subtle like VSCode/Helix (no "full bar" feeling).
    let indent_guide_style = Style::default().fg(theme.indent_guide_fg).add_mod(Mod::DIM);

    let tab_size = tab_size.max(1) as u32;

    let is_markdown = tab.is_markdown();
    let cursor_row = tab.buffer.cursor().0;
    let mut match_cursor = visible_lines
        .first()
        .map(|first_line| search_matches.partition_point(|m| m.line < *first_line))
        .unwrap_or(0);

    let bottom = area.bottom();
    for y in area.y..bottom {
        let screen_row = (y - area.y) as usize;
        let Some(&row) = visible_lines.get(screen_row) else {
            continue;
        };

        while match_cursor < search_matches.len() && search_matches[match_cursor].line < row {
            match_cursor = match_cursor.saturating_add(1);
        }
        let line_match_start = match_cursor;
        while match_cursor < search_matches.len() && search_matches[match_cursor].line == row {
            match_cursor = match_cursor.saturating_add(1);
        }
        let line_matches = &search_matches[line_match_start..match_cursor];

        // For markdown non-cursor lines, use WYSIWYG rendering
        if is_markdown && row != cursor_row {
            if let Some(md) = markdown {
                let rendered = md.render_line(row, tab.buffer.rope(), area.w as usize);
                let mut doc_line = doc::from_markdown_rendered(rendered);
                append_markdown_selection_spans(&mut doc_line, tab, row);
                let row_clip = Rect::new(area.x, y, area.w, 1);
                doc::paint_doc_line(
                    painter,
                    Pos::new(area.x, y),
                    area.w,
                    doc::DocPaintLineParams {
                        line: &doc_line,
                        theme,
                        base_style,
                        clip: row_clip,
                        horiz_offset,
                    },
                );
                continue;
            }
        }

        let selection_range =
            selection_range_for_row(tab.buffer.selection(), row).unwrap_or((0, 0));
        let has_selection = tab.buffer.selection().is_some_and(|s| !s.is_empty());

        // For markdown cursor lines, use markdown source highlighting if no tree-sitter
        let highlight_spans = if is_markdown {
            None // We'll use md_source_spans below
        } else {
            highlight_lines
                .and_then(|lines| lines.get(screen_row))
                .map(|spans| spans.as_slice())
        };

        let md_source_spans: Vec<HighlightSpan>;
        let highlight_spans = if is_markdown && highlight_spans.is_none() {
            if let Some(md) = markdown {
                md_source_spans = md.highlight_source_line(row, tab.buffer.rope());
                if md_source_spans.is_empty() {
                    None
                } else {
                    Some(md_source_spans.as_slice())
                }
            } else {
                highlight_spans
            }
        } else {
            highlight_spans
        };

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
            } else {
                if let Some(hl) =
                    style_for_highlight(semantic_spans, &mut semantic_idx, g_start, theme).or_else(
                        || style_for_highlight(highlight_spans, &mut highlight_idx, g_start, theme),
                    )
                {
                    style = base_style.patch(hl);
                }
                if let Some(bg) = search_match_bg(
                    line_matches,
                    line_match_start,
                    g_start,
                    current_match_index,
                    theme,
                ) {
                    style = style.bg(bg);
                }
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
                let mut spans_vec: Vec<(u32, u32, usize)> = Vec::new();

                let mut col: u32 = 0;
                for (g_idx, ch) in indent_prefix.chars().enumerate() {
                    let start = col;
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

                    if w == 0 {
                        continue;
                    }

                    col = col.saturating_add(w);
                    spans_vec.push((start, col, g_idx));
                }

                let indent_width = col;
                let mut level_start_col: u32 = 0;
                while level_start_col.saturating_add(tab_size) <= indent_width {
                    let guide_col = level_start_col;
                    level_start_col = level_start_col.saturating_add(tab_size);

                    if guide_col < horiz_offset {
                        continue;
                    }
                    let rel = guide_col - horiz_offset;
                    if rel >= area.w as u32 {
                        continue;
                    }
                    let guide_x = area.x.saturating_add(rel as u16);
                    if guide_x >= right {
                        continue;
                    }

                    let guide_g_idx = spans_vec
                        .iter()
                        .find(|(start, end, _)| *start <= guide_col && guide_col < *end)
                        .map(|(_, _, g_idx)| *g_idx);

                    let selected = has_selection
                        && guide_g_idx.is_some_and(|g_idx| {
                            g_idx >= selection_range.0 && g_idx < selection_range.1
                        })
                        && selection_range.0 != selection_range.1;
                    let style = if selected {
                        indent_guide_style.bg(theme.palette_selected_bg)
                    } else {
                        indent_guide_style.bg(theme.editor_bg)
                    };

                    painter.text_clipped(
                        Pos::new(guide_x, y),
                        INDENT_GUIDE_SYMBOL,
                        style,
                        row_clip,
                    );
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

fn append_markdown_selection_spans(line: &mut DocLine, tab: &EditorTabState, row: usize) {
    let selection = tab.buffer.selection();
    let Some((sel_start, sel_end)) = selection_range_for_row(selection, row) else {
        return;
    };
    if sel_start == sel_end {
        return;
    }

    let Some(offset_map) = line.offset_map.as_deref() else {
        return;
    };
    if offset_map.is_empty() || line.text.is_empty() {
        return;
    }

    let rope = tab.buffer.rope();
    let src_line_start_char = rope.line_to_char(row);
    let mut byte_offset = 0usize;
    let mut current_start: Option<usize> = None;

    for g in line.text.graphemes(true) {
        let g_start = byte_offset;
        byte_offset = byte_offset.saturating_add(g.len());

        let src_byte = markdown::display_to_source_byte(offset_map, g_start);
        let selected = if src_byte < rope.len_bytes() {
            let src_char = rope.byte_to_char(src_byte);
            let col = src_char.saturating_sub(src_line_start_char);
            col >= sel_start && col < sel_end
        } else {
            false
        };

        match (current_start, selected) {
            (None, true) => current_start = Some(g_start),
            (Some(start), false) => {
                line.spans.push(DocSpan {
                    start,
                    end: g_start,
                    kind: DocSpanKind::Selection,
                });
                current_start = None;
            }
            _ => {}
        }
    }

    if let Some(start) = current_start {
        line.spans.push(DocSpan {
            start,
            end: line.text.len(),
            kind: DocSpanKind::Selection,
        });
    }

    if !line.spans.is_empty() {
        line.spans.sort_by_key(|span| (span.start, span.end));
    }
}

fn paint_vertical_scrollbar(
    painter: &mut Painter,
    metrics: &VerticalScrollbarMetrics,
    theme: &Theme,
) {
    if metrics.track_area.is_empty() || metrics.thumb_area.is_empty() {
        return;
    }

    let track_style = Style::default()
        .bg(theme.editor_bg)
        .fg(theme.palette_muted_fg);
    painter.vline(
        Pos::new(metrics.track_area.x, metrics.track_area.y),
        metrics.track_area.h,
        V_SCROLL_TRACK_SYMBOL,
        track_style,
    );

    let thumb_style = Style::default().bg(theme.editor_bg).fg(theme.header_fg);
    painter.vline(
        Pos::new(metrics.thumb_area.x, metrics.thumb_area.y),
        metrics.thumb_area.h,
        V_SCROLL_THUMB_SYMBOL,
        thumb_style,
    );
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
        HighlightKind::Comment => Style::default().fg(theme.syntax_comment_fg),
        HighlightKind::String => Style::default().fg(theme.syntax_string_fg),
        HighlightKind::Regex => Style::default().fg(theme.syntax_regex_fg),
        HighlightKind::Keyword => Style::default().fg(theme.syntax_keyword_fg),
        HighlightKind::Type => Style::default().fg(theme.syntax_type_fg),
        HighlightKind::Number => Style::default().fg(theme.syntax_number_fg),
        HighlightKind::Attribute => Style::default().fg(theme.syntax_attribute_fg),
        HighlightKind::Lifetime => Style::default().fg(theme.syntax_keyword_fg),
        HighlightKind::Function => Style::default().fg(theme.syntax_function_fg),
        HighlightKind::Macro => Style::default().fg(theme.syntax_macro_fg),
        HighlightKind::Namespace => Style::default().fg(theme.syntax_namespace_fg),
        HighlightKind::Variable => Style::default().fg(theme.syntax_variable_fg),
        HighlightKind::Constant => Style::default().fg(theme.syntax_constant_fg),
    };

    Some(style)
}

fn search_match_bg(
    line_matches: &[Match],
    global_start_idx: usize,
    byte_in_line: usize,
    current_match_index: Option<usize>,
    theme: &Theme,
) -> Option<Color> {
    for (offset, m) in line_matches.iter().enumerate() {
        let len = m.end.saturating_sub(m.start);
        if len == 0 {
            continue;
        }

        let start = m.col;
        let end = start.saturating_add(len);
        if byte_in_line < start || byte_in_line >= end {
            continue;
        }

        let idx = global_start_idx.saturating_add(offset);
        return Some(if current_match_index == Some(idx) {
            theme.search_current_match_bg
        } else {
            theme.search_match_bg
        });
    }

    None
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
                .saturating_add(match_info.width() as u16)
                .saturating_add(SEARCH_NAV_BUTTONS_WIDTH);

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
