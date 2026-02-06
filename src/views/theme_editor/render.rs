use crate::app::theme::{hsl_to_rgb, UiTheme};
use crate::kernel::editor::{highlight_snippet, HighlightKind, HighlightSpan, LanguageId};
use crate::kernel::state::{PreviewLanguage, ThemeEditorFocus, ThemeEditorState, ThemeEditorToken};
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Color, Mod as UiMod, Style};
use crate::ui::core::theme::Theme;

use super::snippets;

/// Areas returned by the theme editor for mouse hit-testing.
pub struct ThemeEditorAreas {
    pub token_list: Option<Rect>,
    pub hue_bar: Option<Rect>,
    pub sv_palette: Option<Rect>,
}

pub fn paint_theme_editor(
    painter: &mut Painter,
    area: Rect,
    state: &ThemeEditorState,
    theme: &Theme,
    ui_theme: &UiTheme,
) -> ThemeEditorAreas {
    let mut areas = ThemeEditorAreas {
        token_list: None,
        hue_bar: None,
        sv_palette: None,
    };

    if area.is_empty() || area.h < 5 || area.w < 20 {
        return areas;
    }

    let bg = Style::default().bg(Color::Reset).fg(Color::Reset);
    painter.fill_rect(area, bg);

    // Title bar
    let (title_area, body) = area.split_top(1);
    paint_title_bar(painter, title_area, theme);

    // Split body into left panel and right preview
    let left_w = (body.w * 40 / 100).max(30).min(body.w.saturating_sub(10));
    let (left_area, right_area) = body.split_left(left_w);

    // Separator
    if right_area.w > 0 {
        let sep_style = Style::default().fg(theme.separator);
        painter.vline(
            Pos::new(right_area.x, right_area.y),
            right_area.h,
            '\u{2502}',
            sep_style,
        );
    }

    let right_inner = if right_area.w > 1 {
        Rect::new(
            right_area.x.saturating_add(1),
            right_area.y,
            right_area.w.saturating_sub(1),
            right_area.h,
        )
    } else {
        right_area
    };

    paint_left_panel(painter, left_area, state, theme, ui_theme, &mut areas);
    paint_code_preview(painter, right_inner, state, theme, ui_theme);
    areas
}

fn paint_title_bar(painter: &mut Painter, area: Rect, theme: &Theme) {
    if area.is_empty() {
        return;
    }
    let style = Style::default()
        .bg(theme.palette_selected_bg)
        .fg(theme.palette_selected_fg)
        .add_mod(UiMod::BOLD);
    painter.fill_rect(area, style);
    painter.text_clipped(
        Pos::new(area.x.saturating_add(1), area.y),
        "Theme Editor",
        style,
        area,
    );
    let esc_label = "[Esc to close]";
    let esc_x = area.right().saturating_sub(esc_label.len() as u16 + 1);
    if esc_x > area.x + 14 {
        let muted = Style::default()
            .bg(theme.palette_selected_bg)
            .fg(theme.palette_muted_fg);
        painter.text_clipped(Pos::new(esc_x, area.y), esc_label, muted, area);
    }
}

fn paint_left_panel(
    painter: &mut Painter,
    area: Rect,
    state: &ThemeEditorState,
    theme: &Theme,
    ui_theme: &UiTheme,
    areas: &mut ThemeEditorAreas,
) {
    if area.is_empty() {
        return;
    }

    let inner = Rect::new(
        area.x.saturating_add(1),
        area.y,
        area.w.saturating_sub(2),
        area.h,
    );
    if inner.is_empty() {
        return;
    }

    // Token list
    let token_list_h = ThemeEditorToken::ALL.len() as u16;
    let (token_area, rest) = inner.split_top(token_list_h.min(inner.h));
    areas.token_list = Some(token_area);
    paint_token_list(painter, token_area, state, theme, ui_theme);

    if rest.h < 2 {
        return;
    }

    // Blank line
    let (_, rest) = rest.split_top(1);
    if rest.h < 4 {
        return;
    }

    // Reserve 2 rows at bottom for hex color + language label
    let (picker_area, bottom_area) = rest.split_bottom(2);

    if picker_area.h >= 3 && picker_area.w >= 6 {
        paint_color_picker(painter, picker_area, state, theme, areas);
    }

    // Hex color + language label
    if bottom_area.h >= 1 {
        let (r, g, b) = hsl_to_rgb(state.hue, state.saturation, state.lightness);
        let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
        let color_preview = Style::default()
            .fg(Color::Rgb(r, g, b))
            .add_mod(UiMod::BOLD);
        let (hex_row, rest2) = bottom_area.split_top(1);
        painter.text_clipped(Pos::new(hex_row.x, hex_row.y), &hex, color_preview, hex_row);

        if !rest2.is_empty() {
            let (lang_row, _) = rest2.split_top(1);
            let lang_label = format!("[{}]", state.preview_language.label());
            let muted = Style::default().fg(theme.palette_muted_fg);
            painter.text_clipped(
                Pos::new(lang_row.x, lang_row.y),
                &lang_label,
                muted,
                lang_row,
            );
        }
    }
}

fn token_color(token: ThemeEditorToken, ui_theme: &UiTheme) -> Color {
    match token {
        ThemeEditorToken::Comment => ui_theme.syntax_comment_fg,
        ThemeEditorToken::Keyword => ui_theme.syntax_keyword_fg,
        ThemeEditorToken::String => ui_theme.syntax_string_fg,
        ThemeEditorToken::Number => ui_theme.syntax_number_fg,
        ThemeEditorToken::Type => ui_theme.syntax_type_fg,
        ThemeEditorToken::Attribute => ui_theme.syntax_attribute_fg,
        ThemeEditorToken::Function => ui_theme.syntax_function_fg,
        ThemeEditorToken::Variable => ui_theme.syntax_variable_fg,
        ThemeEditorToken::Constant => ui_theme.syntax_constant_fg,
        ThemeEditorToken::Regex => ui_theme.syntax_regex_fg,
    }
}

fn paint_token_list(
    painter: &mut Painter,
    area: Rect,
    state: &ThemeEditorState,
    theme: &Theme,
    ui_theme: &UiTheme,
) {
    for (i, token) in ThemeEditorToken::ALL.iter().enumerate() {
        let y = area.y.saturating_add(i as u16);
        if y >= area.bottom() {
            break;
        }
        let row = Rect::new(area.x, y, area.w, 1);
        let is_selected = *token == state.selected_token;

        if is_selected && state.focus == ThemeEditorFocus::TokenList {
            let sel_bg = Style::default()
                .bg(theme.palette_selected_bg)
                .fg(theme.palette_selected_fg);
            painter.fill_rect(row, sel_bg);
        }

        let indicator = if is_selected { "\u{25B8} " } else { "  " };
        let color = token_color(*token, ui_theme);
        let label_style = if is_selected && state.focus == ThemeEditorFocus::TokenList {
            Style::default()
                .bg(theme.palette_selected_bg)
                .fg(color)
                .add_mod(UiMod::BOLD)
        } else {
            Style::default().fg(color)
        };

        let text = format!("{}{}", indicator, token.label());
        painter.text_clipped(Pos::new(row.x, row.y), &text, label_style, row);
    }
}

fn paint_color_picker(
    painter: &mut Painter,
    area: Rect,
    state: &ThemeEditorState,
    theme: &Theme,
    areas: &mut ThemeEditorAreas,
) {
    // Layout: [Hue Bar (2 cols)] [1 col gap] [SV Palette (rest)]
    let hue_bar_w: u16 = 2;
    let gap: u16 = 1;
    let min_sv_w: u16 = 3;

    if area.w < hue_bar_w + gap + min_sv_w {
        return;
    }

    let hue_bar_area = Rect::new(area.x, area.y, hue_bar_w, area.h);
    let sv_area = Rect::new(
        area.x + hue_bar_w + gap,
        area.y,
        area.w - hue_bar_w - gap,
        area.h,
    );

    areas.hue_bar = Some(hue_bar_area);
    areas.sv_palette = Some(sv_area);

    paint_hue_bar(painter, hue_bar_area, state, theme);
    paint_sv_palette(painter, sv_area, state, theme);
}

fn paint_hue_bar(painter: &mut Painter, area: Rect, state: &ThemeEditorState, _theme: &Theme) {
    for row in 0..area.h {
        let y = area.y + row;
        // Map row to hue: top=0, bottom=359
        let hue = if area.h > 1 {
            (row as u32 * 359) / (area.h as u32 - 1)
        } else {
            0
        } as u16;

        let (r, g, b) = hsl_to_rgb(hue, 100, 50);
        let bg_color = Color::Rgb(r, g, b);

        // Check if this row is the current hue position
        let cur_row = hue_to_row(state.hue, area.h);

        if row == cur_row {
            // Draw marker for current hue — use ASCII to avoid wide-char issues
            let marker_style = Style::default()
                .bg(bg_color)
                .fg(Color::Rgb(0, 0, 0))
                .add_mod(UiMod::BOLD);
            painter.text_clipped(Pos::new(area.x, y), "<>", marker_style, area);
        } else {
            let style = Style::default().bg(bg_color);
            painter.text_clipped(Pos::new(area.x, y), "  ", style, area);
        }
    }
}

fn paint_sv_palette(painter: &mut Painter, area: Rect, state: &ThemeEditorState, _theme: &Theme) {
    let hue = state.hue;

    // Current marker position — use same mapping as mouse handler
    let marker_col = saturation_to_col(state.saturation, area.w);
    let marker_row = lightness_to_row(state.lightness, area.h);

    for row in 0..area.h {
        let y = area.y + row;
        // Map row to lightness: top=100 (bright), bottom=0 (dark)
        let l = row_to_lightness(row, area.h);

        for col in 0..area.w {
            let x = area.x + col;
            // Map col to saturation: left=0, right=100
            let s = col_to_saturation(col, area.w);

            let (r, g, b) = hsl_to_rgb(hue, s, l);
            let bg_color = Color::Rgb(r, g, b);

            if row == marker_row && col == marker_col {
                // Draw crosshair marker — use ASCII "+" to avoid wide-char issues
                let luma = (r as u16 + g as u16 + b as u16) / 3;
                let fg = if luma > 128 {
                    Color::Rgb(0, 0, 0)
                } else {
                    Color::Rgb(255, 255, 255)
                };
                let style = Style::default().bg(bg_color).fg(fg).add_mod(UiMod::BOLD);
                painter.text_clipped(Pos::new(x, y), "+", style, area);
            } else {
                let style = Style::default().bg(bg_color);
                painter.text_clipped(Pos::new(x, y), " ", style, area);
            }
        }
    }
}

// ── Coordinate mapping helpers (shared between rendering and mouse handler) ──
//
// The pixel→value→pixel round-trip must be lossless: clicking a cell and
// re-rendering the marker must land on the same cell.  We achieve this by
// using *rounding* (+ half-divisor) in the value→pixel direction.

/// Map a hue value (0..359) to a row index in the hue bar.
fn hue_to_row(hue: u16, height: u16) -> u16 {
    if height > 1 {
        let h = height as u32 - 1;
        ((hue as u32 * h + 359 / 2) / 359) as u16
    } else {
        0
    }
}

/// Map a row index in the hue bar to a hue value (0..359).
pub fn row_to_hue(row: u16, height: u16) -> u16 {
    if height > 1 {
        (row as u32 * 359 / (height as u32 - 1)) as u16
    } else {
        0
    }
}

/// Map a saturation value (0..100) to a column index in the SV palette.
fn saturation_to_col(saturation: u8, width: u16) -> u16 {
    if width > 1 {
        let w = width as u32 - 1;
        ((saturation as u32 * w + 50) / 100) as u16
    } else {
        0
    }
}

/// Map a column index in the SV palette to a saturation value (0..100).
pub fn col_to_saturation(col: u16, width: u16) -> u8 {
    if width > 1 {
        (col as u32 * 100 / (width as u32 - 1)) as u8
    } else {
        50
    }
}

/// Map a lightness value (0..100) to a row index in the SV palette.
/// Top row = lightness 100, bottom row = lightness 0.
fn lightness_to_row(lightness: u8, height: u16) -> u16 {
    if height > 1 {
        let h = height as u32 - 1;
        (((100 - lightness) as u32 * h + 50) / 100) as u16
    } else {
        0
    }
}

/// Map a row index in the SV palette to a lightness value (0..100).
/// Top row = lightness 100, bottom row = lightness 0.
pub fn row_to_lightness(row: u16, height: u16) -> u8 {
    if height > 1 {
        (100 - (row as u32 * 100 / (height as u32 - 1))) as u8
    } else {
        50
    }
}

fn paint_code_preview(
    painter: &mut Painter,
    area: Rect,
    state: &ThemeEditorState,
    theme: &Theme,
    ui_theme: &UiTheme,
) {
    if area.is_empty() {
        return;
    }

    let (lang_id, snippet) = match state.preview_language {
        PreviewLanguage::Rust => (LanguageId::Rust, snippets::RUST_SNIPPET),
        PreviewLanguage::Python => (LanguageId::Python, snippets::PYTHON_SNIPPET),
        PreviewLanguage::Go => (LanguageId::Go, snippets::GO_SNIPPET),
        PreviewLanguage::JavaScript => (LanguageId::JavaScript, snippets::JS_SNIPPET),
    };

    let highlights = highlight_snippet(lang_id, snippet);
    let lines: Vec<&str> = snippet.lines().collect();

    let base_style = Style::default().fg(theme.palette_fg);

    for (i, line) in lines.iter().enumerate() {
        let y = area.y.saturating_add(i as u16);
        if y >= area.bottom() {
            break;
        }
        let row = Rect::new(area.x.saturating_add(1), y, area.w.saturating_sub(1), 1);
        if row.is_empty() {
            continue;
        }

        let spans = highlights.get(i).map(|s| s.as_slice());
        paint_highlighted_line(painter, row, line, spans, base_style, ui_theme);
    }
}

fn style_for_highlight(kind: HighlightKind, ui_theme: &UiTheme) -> Style {
    match kind {
        HighlightKind::Comment => Style::default().fg(ui_theme.syntax_comment_fg),
        HighlightKind::String => Style::default().fg(ui_theme.syntax_string_fg),
        HighlightKind::Regex => Style::default().fg(ui_theme.syntax_regex_fg),
        HighlightKind::Keyword => Style::default().fg(ui_theme.syntax_keyword_fg),
        HighlightKind::Type => Style::default().fg(ui_theme.syntax_type_fg),
        HighlightKind::Number => Style::default().fg(ui_theme.syntax_number_fg),
        HighlightKind::Attribute => Style::default().fg(ui_theme.syntax_attribute_fg),
        HighlightKind::Lifetime => Style::default().fg(ui_theme.syntax_keyword_fg),
        HighlightKind::Function => Style::default().fg(ui_theme.syntax_function_fg),
        HighlightKind::Macro => Style::default().fg(ui_theme.syntax_attribute_fg),
        HighlightKind::Variable => Style::default().fg(ui_theme.syntax_variable_fg),
        HighlightKind::Constant => Style::default().fg(ui_theme.syntax_constant_fg),
    }
}

fn paint_highlighted_line(
    painter: &mut Painter,
    clip: Rect,
    line: &str,
    spans: Option<&[HighlightSpan]>,
    base_style: Style,
    ui_theme: &UiTheme,
) {
    if clip.is_empty() || line.is_empty() {
        return;
    }

    let Some(spans) = spans else {
        painter.text_clipped(Pos::new(clip.x, clip.y), line, base_style, clip);
        return;
    };

    if spans.is_empty() {
        painter.text_clipped(Pos::new(clip.x, clip.y), line, base_style, clip);
        return;
    }

    let mut x = clip.x;
    let mut byte_pos = 0usize;
    let mut span_idx = 0usize;

    while byte_pos < line.len() && x < clip.right() {
        // Find the next span that covers or starts after byte_pos
        while span_idx < spans.len() && spans[span_idx].end <= byte_pos {
            span_idx += 1;
        }

        if span_idx < spans.len() && spans[span_idx].start <= byte_pos {
            // Inside a highlighted span
            let span = &spans[span_idx];
            let end = span.end.min(line.len());
            let seg = &line[byte_pos..end];
            let style = style_for_highlight(span.kind, ui_theme);
            painter.text_clipped(Pos::new(x, clip.y), seg, style, clip);
            let seg_w = unicode_width::UnicodeWidthStr::width(seg) as u16;
            x = x.saturating_add(seg_w);
            byte_pos = end;
        } else {
            // Before the next span or no more spans
            let next_start = if span_idx < spans.len() {
                spans[span_idx].start.min(line.len())
            } else {
                line.len()
            };
            let seg = &line[byte_pos..next_start];
            painter.text_clipped(Pos::new(x, clip.y), seg, base_style, clip);
            let seg_w = unicode_width::UnicodeWidthStr::width(seg) as u16;
            x = x.saturating_add(seg_w);
            byte_pos = next_start;
        }
    }
}
