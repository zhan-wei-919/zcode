use crate::app::theme::UiTheme;
use crate::core::text_window;
use crate::kernel::editor::{
    cursor_display_x_abs, EditorPaneState, EditorTabState, HighlightKind, HighlightSpan,
    SearchBarField, SearchBarMode, SearchBarState,
};
use crate::kernel::services::ports::EditorConfig;
use crate::models::slice_to_cow;
use memchr::memchr;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Tabs, Widget};
use ratatui::Frame;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::layout::EditorPaneLayout;

pub fn render_editor_pane(
    frame: &mut Frame,
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    config: &EditorConfig,
    theme: &UiTheme,
    hovered_tab: Option<usize>,
    workspace_empty: bool,
) {
    if layout.area.width == 0 || layout.area.height == 0 {
        return;
    }

    render_tabs(frame, layout.tab_area, pane, theme, hovered_tab);

    if let Some(search_area) = layout.search_area {
        render_search_bar(frame, search_area, &pane.search_bar, theme);
    }

    render_editor(frame, layout, pane, config, theme, workspace_empty);
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

    if layout.content_area.width == 0 || layout.content_area.height == 0 {
        return None;
    }

    let height = (layout.editor_area.height as usize).max(1);
    let visible_lines = tab.visible_lines_in_viewport(line_offset, height);
    let screen_row = visible_lines.iter().position(|&line| line == row)?;

    let cursor_x_abs = cursor_display_x_abs(&tab.buffer, config.tab_size);
    let cursor_x_rel = cursor_x_abs.saturating_sub(horiz_offset);

    let x = layout.content_area.x.saturating_add(
        cursor_x_rel.min(layout.content_area.width.saturating_sub(1) as u32) as u16,
    );
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

    if !tab.viewport.follow_cursor
        || layout.editor_area.height == 0
        || layout.content_area.width == 0
    {
        return (line_offset, horiz_offset);
    }

    let (row, _) = tab.buffer.cursor();
    let height = (layout.editor_area.height as usize).max(1);
    if row < line_offset {
        line_offset = row;
    } else if row >= line_offset + height {
        line_offset = row.saturating_sub(height.saturating_sub(1));
    }

    let cursor_x = cursor_display_x_abs(&tab.buffer, config.tab_size);
    let width = layout.content_area.width.max(1) as u32;
    if cursor_x < horiz_offset {
        horiz_offset = cursor_x;
    } else if cursor_x >= horiz_offset + width {
        horiz_offset = cursor_x.saturating_sub(width.saturating_sub(1));
    }

    (line_offset, horiz_offset)
}

fn render_tabs(
    frame: &mut Frame,
    area: Rect,
    pane: &EditorPaneState,
    theme: &UiTheme,
    hovered_tab: Option<usize>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if pane.tabs.is_empty() {
        frame.render_widget(
            Block::default().style(Style::default().bg(theme.palette_bg)),
            area,
        );
        return;
    }

    let titles: Vec<Line> = pane
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let active = i == pane.active;
            let is_hovered = hovered_tab == Some(i);
            let fg = if active {
                theme.header_fg
            } else {
                theme.palette_muted_fg
            };
            let mut spans = Vec::with_capacity(5);
            spans.push(Span::raw(" "));
            if tab.dirty {
                spans.push(Span::styled("● ", Style::default().fg(fg)));
            }
            spans.push(Span::styled(
                tab.title.as_str(),
                Style::default().fg(fg).add_modifier(if active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            ));
            spans.push(Span::raw(" "));
            if is_hovered {
                spans.push(Span::styled("×", Style::default().fg(theme.accent_fg)));
                spans.push(Span::raw(" "));
            }
            Line::from(spans)
        })
        .collect();

    let tabs_widget = Tabs::new(titles)
        .select(pane.active)
        .highlight_style(Style::default())
        .padding("", "");

    frame.render_widget(tabs_widget, area);
}

fn render_search_bar(frame: &mut Frame, area: Rect, state: &SearchBarState, theme: &UiTheme) {
    if !state.visible || area.width == 0 || area.height == 0 {
        return;
    }

    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.palette_bg).fg(theme.palette_fg)),
        area,
    );

    let match_info = search_bar_match_info(state);
    let case_indicator = if state.case_sensitive { "[Aa]" } else { "[aa]" };
    let regex_indicator = if state.use_regex { "[.*]" } else { "[  ]" };

    let sep_style = Style::default().bg(theme.separator);
    let label_style = Style::default().fg(theme.header_fg);
    let muted_style = Style::default().fg(theme.palette_muted_fg);

    match state.mode {
        SearchBarMode::Search => {
            let find_area = Rect::new(area.x, area.y, area.width, 1.min(area.height));
            let sep_area = (area.height >= 2).then_some(Rect::new(
                area.x,
                area.y.saturating_add(area.height.saturating_sub(1)),
                area.width,
                1,
            ));

            let (visible_text, _start) = windowed_search_text(
                state.search_text.as_str(),
                state.cursor_pos,
                state.focused_field == SearchBarField::Search,
                find_area.width,
                case_indicator,
                regex_indicator,
                &match_info,
            );
            let search_style = if state.focused_field == SearchBarField::Search {
                Style::default().fg(theme.palette_fg)
            } else {
                muted_style
            };

            let line = Line::from(vec![
                Span::styled("Find: ", label_style),
                Span::styled(visible_text, search_style),
                Span::raw(" "),
                Span::styled(case_indicator, muted_style),
                Span::styled(regex_indicator, muted_style),
                Span::raw(" "),
                Span::styled(match_info, label_style),
            ]);

            frame.render_widget(Paragraph::new(line), find_area);
            if let Some(sep) = sep_area {
                frame.render_widget(Block::default().style(sep_style), sep);
            }
        }
        SearchBarMode::Replace => {
            let top = Rect::new(area.x, area.y, area.width, 1.min(area.height));
            let replace_row = (area.height >= 2).then_some(Rect::new(
                area.x,
                area.y.saturating_add(1),
                area.width,
                1,
            ));
            let sep_area = (area.height >= 3).then_some(Rect::new(
                area.x,
                area.y.saturating_add(area.height.saturating_sub(1)),
                area.width,
                1,
            ));

            let (visible_search, _search_start) = windowed_search_text(
                state.search_text.as_str(),
                state.cursor_pos,
                state.focused_field == SearchBarField::Search,
                top.width,
                case_indicator,
                regex_indicator,
                &match_info,
            );
            let search_style = if state.focused_field == SearchBarField::Search {
                Style::default().fg(theme.palette_fg)
            } else {
                muted_style
            };

            let search_line = Line::from(vec![
                Span::styled("Find: ", label_style),
                Span::styled(visible_search, search_style),
                Span::raw(" "),
                Span::styled(case_indicator, muted_style),
                Span::styled(regex_indicator, muted_style),
                Span::raw(" "),
                Span::styled(match_info, label_style),
            ]);
            frame.render_widget(Paragraph::new(search_line), top);

            if let Some(replace_area) = replace_row {
                let (visible_replace, _replace_start) = windowed_replace_text(
                    state.replace_text.as_str(),
                    state.cursor_pos,
                    state.focused_field == SearchBarField::Replace,
                    replace_area.width,
                );
                let replace_style = if state.focused_field == SearchBarField::Replace {
                    Style::default().fg(theme.palette_fg)
                } else {
                    muted_style
                };

                let replace_line = Line::from(vec![
                    Span::styled("Replace: ", label_style),
                    Span::styled(visible_replace, replace_style),
                ]);

                frame.render_widget(Paragraph::new(replace_line), replace_area);
            }
            if let Some(sep) = sep_area {
                frame.render_widget(Block::default().style(sep_style), sep);
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

fn render_editor(
    frame: &mut Frame,
    layout: &EditorPaneLayout,
    pane: &EditorPaneState,
    config: &EditorConfig,
    theme: &UiTheme,
    workspace_empty: bool,
) {
    if layout.editor_area.width == 0 || layout.editor_area.height == 0 {
        return;
    }

    let Some(tab) = pane.active_tab() else {
        let base = Style::default()
            .bg(theme.palette_bg)
            .fg(theme.palette_muted_fg);
        frame.render_widget(Block::default().style(base), layout.editor_area);

        let msg = if workspace_empty {
            "Folder is empty"
        } else {
            "No file open"
        };
        let line_area = Rect::new(
            layout.editor_area.x,
            layout
                .editor_area
                .y
                .saturating_add(layout.editor_area.height / 2),
            layout.editor_area.width,
            1.min(layout.editor_area.height),
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(msg, base)))
                .alignment(Alignment::Center)
                .style(base),
            line_area,
        );
        return;
    };

    let (line_offset, horiz_offset) = effective_viewport(tab, layout, config);
    let height = layout.editor_area.height as usize;
    let visible_lines = tab.visible_lines_in_viewport(line_offset, height.max(1));
    let syntax = build_syntax_highlights(tab, &visible_lines);

    if config.show_line_numbers && layout.gutter_area.width > 0 {
        frame.render_widget(
            EditorGutterWidget {
                tab,
                lines: &visible_lines,
                active_row: tab.buffer.cursor().0,
                theme,
            },
            layout.gutter_area,
        );
    }

    if layout.content_area.width == 0 || layout.content_area.height == 0 {
        return;
    }

    frame.render_widget(
        EditorContentWidget {
            tab,
            visible_lines: &visible_lines,
            horiz_offset,
            highlight_lines: syntax.as_deref(),
            tab_size: config.tab_size,
            theme,
        },
        layout.content_area,
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

struct EditorGutterWidget<'a> {
    tab: &'a crate::kernel::editor::EditorTabState,
    lines: &'a [usize],
    active_row: usize,
    theme: &'a UiTheme,
}

impl Widget for EditorGutterWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let base_style = Style::default()
            .bg(self.theme.palette_bg)
            .fg(self.theme.palette_muted_fg);
        buf.set_style(area, base_style);

        let digits_width = area.width.saturating_sub(2) as usize;
        if digits_width == 0 {
            return;
        }

        let highlight_style = Style::default()
            .bg(self.theme.palette_bg)
            .fg(self.theme.header_fg)
            .add_modifier(Modifier::BOLD);

        let right = area.x.saturating_add(area.width);
        let bottom = area.y.saturating_add(area.height);
        for y in area.y..bottom {
            let row = (y - area.y) as usize;
            let Some(&line) = self.lines.get(row) else {
                continue;
            };
            let line_no = line.saturating_add(1);
            let style = if line == self.active_row {
                highlight_style
            } else {
                base_style
            };

            // Reserve last 2 columns: " " + gap.
            let mut x = right.saturating_sub(2);
            if area.width >= 2 {
                if let Some(marker) = self
                    .tab
                    .fold_marker_char(line.min(u32::MAX as usize) as u32)
                {
                    buf[(x, y)].set_char(marker).set_style(style);
                }
            }
            let mut n = line_no;
            for _ in 0..digits_width {
                if x <= area.x {
                    break;
                }
                x = x.saturating_sub(1);
                let ch = (b'0' + (n % 10) as u8) as char;
                buf[(x, y)].set_char(ch).set_style(style);
                n /= 10;
                if n == 0 {
                    break;
                }
            }
        }
    }
}

struct EditorContentWidget<'a> {
    tab: &'a crate::kernel::editor::EditorTabState,
    visible_lines: &'a [usize],
    horiz_offset: u32,
    highlight_lines: Option<&'a [Vec<HighlightSpan>]>,
    tab_size: u8,
    theme: &'a UiTheme,
}

impl Widget for EditorContentWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let base_style = Style::default()
            .bg(self.theme.palette_bg)
            .fg(self.theme.palette_fg);
        buf.set_style(area, base_style);

        let selection_style = Style::default()
            .bg(self.theme.palette_selected_bg)
            .fg(self.theme.palette_selected_fg);

        let horiz_offset = self.horiz_offset;
        let tab_size = self.tab_size.max(1) as u32;

        let bottom = area.y.saturating_add(area.height);
        for y in area.y..bottom {
            let screen_row = (y - area.y) as usize;
            let Some(&row) = self.visible_lines.get(screen_row) else {
                continue;
            };

            let selection_range =
                selection_range_for_row(self.tab.buffer.selection(), row).unwrap_or((0, 0));
            let has_selection = self.tab.buffer.selection().is_some_and(|s| !s.is_empty());

            let highlight_spans = self
                .highlight_lines
                .and_then(|lines| lines.get(screen_row))
                .map(|spans| spans.as_slice());
            let semantic_spans = self.tab.semantic_highlight_line(row);
            let inlay_hints = self.tab.inlay_hint_line(row);

            let line = self
                .tab
                .buffer
                .line_slice(row)
                .map(slice_to_cow)
                .unwrap_or_default();
            let line = line.strip_suffix('\n').unwrap_or(&line);
            let line = line.strip_suffix('\r').unwrap_or(line);

            let mut x = area.x;
            let right = area.x.saturating_add(area.width);
            let mut visible = line;
            let mut g_idx_base: usize = 0;
            let mut display_col: u32 = 0;
            let mut byte_offset: usize = 0;
            let mut semantic_idx: usize = 0;
            let mut highlight_idx: usize = 0;

            if horiz_offset > 0 {
                let start = (horiz_offset as usize).min(line.len());
                let prefix = &line.as_bytes()[..start];
                if line.is_char_boundary(start)
                    && prefix.is_ascii()
                    && memchr(b'\t', prefix).is_none()
                {
                    visible = &line[start..];
                    g_idx_base = start;
                    display_col = start.min(u32::MAX as usize) as u32;
                    byte_offset = start;
                }
            }

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
                    style_for_highlight(semantic_spans, &mut semantic_idx, g_start, self.theme)
                        .or_else(|| {
                            style_for_highlight(
                                highlight_spans,
                                &mut highlight_idx,
                                g_start,
                                self.theme,
                            )
                        })
                {
                    style = base_style.patch(hl);
                }

                if g == "\t" {
                    let mut spaces = width;
                    while spaces > 0 && x < right {
                        buf[(x, y)].set_char(' ').set_style(style);
                        x = x.saturating_add(1);
                        spaces -= 1;
                    }
                } else {
                    let w = width.min(u16::MAX as u32) as u16;
                    if x.saturating_add(w) > right {
                        break;
                    }
                    set_cell_symbol(buf, x, y, g, style);
                    x = x.saturating_add(w);
                }

                display_col = display_col.saturating_add(width);
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
                            .bg(self.theme.palette_bg)
                            .fg(self.theme.palette_muted_fg)
                            .add_modifier(Modifier::ITALIC);

                        let mut hx = x;
                        for g in visible_hint.graphemes(true) {
                            let w = g.width() as u16;
                            if w == 0 || hx.saturating_add(w) > right {
                                break;
                            }
                            set_cell_symbol(buf, hx, y, g, hint_style);
                            hx = hx.saturating_add(w);
                        }
                    }
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

fn set_cell_symbol(buf: &mut Buffer, x: u16, y: u16, symbol: &str, style: Style) {
    let mut chars = symbol.chars();
    let Some(first) = chars.next() else {
        return;
    };
    if chars.next().is_none() {
        buf[(x, y)].set_char(first).set_style(style);
    } else {
        buf[(x, y)].set_symbol(symbol).set_style(style);
    }
}

fn style_for_highlight(
    highlight_spans: Option<&[HighlightSpan]>,
    highlight_idx: &mut usize,
    byte_offset: usize,
    theme: &UiTheme,
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
        HighlightKind::Keyword => Style::default()
            .fg(theme.accent_fg)
            .add_modifier(Modifier::BOLD),
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
    if area.width == 0 || area.height == 0 {
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
                area.width,
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
                .min(area.x.saturating_add(area.width.saturating_sub(suffix_w)));
            Some((x, y))
        }
        SearchBarField::Replace => {
            let y = area.y.saturating_add(1);
            if y >= area.y.saturating_add(area.height) {
                return None;
            }

            let cursor = state.cursor_pos.min(state.replace_text.len());
            let (_visible, start) =
                windowed_replace_text(state.replace_text.as_str(), cursor, true, area.width);
            let before = state.replace_text.get(start..cursor).unwrap_or_default();

            let prefix_w = "Replace: ".width() as u16;
            let x = area
                .x
                .saturating_add(prefix_w)
                .saturating_add(before.width() as u16)
                .min(area.x.saturating_add(area.width.saturating_sub(1)));
            Some((x, y))
        }
    }
}
