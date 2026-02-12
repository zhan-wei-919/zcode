use crate::ui::core::color_support::TerminalColorSupport;
use crate::ui::core::style::Color;
use crate::ui::core::theme::Theme;

pub fn adapt_theme(theme: &Theme, support: TerminalColorSupport) -> Theme {
    if support == TerminalColorSupport::TrueColor {
        return theme.clone();
    }

    let mut adapted = Theme {
        focus_border: map_color_for_support(theme.focus_border, support),
        inactive_border: map_color_for_support(theme.inactive_border, support),
        separator: map_color_for_support(theme.separator, support),
        accent_fg: map_color_for_support(theme.accent_fg, support),
        syntax_comment_fg: map_color_for_support(theme.syntax_comment_fg, support),
        syntax_keyword_fg: map_color_for_support(theme.syntax_keyword_fg, support),
        syntax_string_fg: map_color_for_support(theme.syntax_string_fg, support),
        syntax_number_fg: map_color_for_support(theme.syntax_number_fg, support),
        syntax_type_fg: map_color_for_support(theme.syntax_type_fg, support),
        syntax_attribute_fg: map_color_for_support(theme.syntax_attribute_fg, support),
        syntax_namespace_fg: map_color_for_support(theme.syntax_namespace_fg, support),
        syntax_macro_fg: map_color_for_support(theme.syntax_macro_fg, support),
        syntax_function_fg: map_color_for_support(theme.syntax_function_fg, support),
        syntax_variable_fg: map_color_for_support(theme.syntax_variable_fg, support),
        syntax_constant_fg: map_color_for_support(theme.syntax_constant_fg, support),
        syntax_regex_fg: map_color_for_support(theme.syntax_regex_fg, support),
        error_fg: map_color_for_support(theme.error_fg, support),
        warning_fg: map_color_for_support(theme.warning_fg, support),
        activity_bg: map_color_for_support(theme.activity_bg, support),
        activity_fg: map_color_for_support(theme.activity_fg, support),
        activity_active_bg: map_color_for_support(theme.activity_active_bg, support),
        activity_active_fg: map_color_for_support(theme.activity_active_fg, support),
        sidebar_tab_active_bg: map_color_for_support(theme.sidebar_tab_active_bg, support),
        sidebar_tab_active_fg: map_color_for_support(theme.sidebar_tab_active_fg, support),
        sidebar_tab_inactive_fg: map_color_for_support(theme.sidebar_tab_inactive_fg, support),
        header_fg: map_color_for_support(theme.header_fg, support),
        palette_border: map_color_for_support(theme.palette_border, support),
        palette_bg: map_color_for_support(theme.palette_bg, support),
        palette_fg: map_color_for_support(theme.palette_fg, support),
        palette_selected_bg: map_color_for_support(theme.palette_selected_bg, support),
        palette_selected_fg: map_color_for_support(theme.palette_selected_fg, support),
        palette_muted_fg: map_color_for_support(theme.palette_muted_fg, support),
        indent_guide_fg: map_color_for_support(theme.indent_guide_fg, support),
        editor_bg: map_color_for_support(theme.editor_bg, support),
        sidebar_bg: map_color_for_support(theme.sidebar_bg, support),
        popup_bg: map_color_for_support(theme.popup_bg, support),
        statusbar_bg: map_color_for_support(theme.statusbar_bg, support),
        md_heading1_fg: map_color_for_support(theme.md_heading1_fg, support),
        md_heading2_fg: map_color_for_support(theme.md_heading2_fg, support),
        md_heading3_fg: map_color_for_support(theme.md_heading3_fg, support),
        md_heading4_fg: map_color_for_support(theme.md_heading4_fg, support),
        md_heading5_fg: map_color_for_support(theme.md_heading5_fg, support),
        md_heading6_fg: map_color_for_support(theme.md_heading6_fg, support),
        md_link_fg: map_color_for_support(theme.md_link_fg, support),
        md_code_fg: map_color_for_support(theme.md_code_fg, support),
        md_code_bg: map_color_for_support(theme.md_code_bg, support),
        md_blockquote_fg: map_color_for_support(theme.md_blockquote_fg, support),
        md_blockquote_bar: map_color_for_support(theme.md_blockquote_bar, support),
        md_hr_fg: map_color_for_support(theme.md_hr_fg, support),
        md_marker_fg: map_color_for_support(theme.md_marker_fg, support),
    };

    apply_non_truecolor_syntax_palette(&mut adapted, theme, support);
    adapted
}

pub fn map_color_to_support(color: Color, support: TerminalColorSupport) -> Color {
    map_color_for_support(color, support)
}

pub fn color_to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Reset => None,
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Indexed(i) => Some(ansi256_index_to_rgb(i)),
    }
}

pub fn color_to_hex(color: Color) -> Option<String> {
    color_to_rgb(color).map(|(r, g, b)| format!("#{:02X}{:02X}{:02X}", r, g, b))
}

fn apply_non_truecolor_syntax_palette(
    adapted: &mut Theme,
    original: &Theme,
    support: TerminalColorSupport,
) {
    let defaults = Theme::default();

    maybe_apply_syntax_fallback(
        &mut adapted.syntax_comment_fg,
        original.syntax_comment_fg,
        defaults.syntax_comment_fg,
        support,
        65,
        2,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_keyword_fg,
        original.syntax_keyword_fg,
        defaults.syntax_keyword_fg,
        support,
        33,
        4,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_string_fg,
        original.syntax_string_fg,
        defaults.syntax_string_fg,
        support,
        114,
        10,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_number_fg,
        original.syntax_number_fg,
        defaults.syntax_number_fg,
        support,
        108,
        10,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_type_fg,
        original.syntax_type_fg,
        defaults.syntax_type_fg,
        support,
        44,
        6,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_attribute_fg,
        original.syntax_attribute_fg,
        defaults.syntax_attribute_fg,
        support,
        44,
        6,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_namespace_fg,
        original.syntax_namespace_fg,
        defaults.syntax_namespace_fg,
        support,
        44,
        6,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_macro_fg,
        original.syntax_macro_fg,
        defaults.syntax_macro_fg,
        support,
        68,
        4,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_function_fg,
        original.syntax_function_fg,
        defaults.syntax_function_fg,
        support,
        179,
        11,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_variable_fg,
        original.syntax_variable_fg,
        defaults.syntax_variable_fg,
        support,
        81,
        6,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_constant_fg,
        original.syntax_constant_fg,
        defaults.syntax_constant_fg,
        support,
        39,
        12,
    );
    maybe_apply_syntax_fallback(
        &mut adapted.syntax_regex_fg,
        original.syntax_regex_fg,
        defaults.syntax_regex_fg,
        support,
        167,
        9,
    );
}

fn maybe_apply_syntax_fallback(
    out: &mut Color,
    original: Color,
    default: Color,
    support: TerminalColorSupport,
    ansi256_index: u8,
    ansi16_index: u8,
) {
    if original == default {
        *out = syntax_fallback_color(support, ansi256_index, ansi16_index);
    }
}

fn map_color_for_support(color: Color, support: TerminalColorSupport) -> Color {
    match (support, color) {
        (TerminalColorSupport::TrueColor, value) => value,
        (_, Color::Reset) => Color::Reset,
        (TerminalColorSupport::Ansi256, Color::Rgb(r, g, b)) => {
            Color::Indexed(rgb_to_ansi256_index(r, g, b))
        }
        (TerminalColorSupport::Ansi256, Color::Indexed(i)) => Color::Indexed(i),
        (TerminalColorSupport::Ansi16, Color::Rgb(r, g, b)) => {
            Color::Indexed(rgb_to_ansi16_index(r, g, b))
        }
        (TerminalColorSupport::Ansi16, Color::Indexed(i)) if i <= 15 => Color::Indexed(i),
        (TerminalColorSupport::Ansi16, Color::Indexed(i)) => {
            let (r, g, b) = ansi256_index_to_rgb(i);
            Color::Indexed(rgb_to_ansi16_index(r, g, b))
        }
    }
}

fn syntax_fallback_color(
    support: TerminalColorSupport,
    ansi256_index: u8,
    ansi16_index: u8,
) -> Color {
    match support {
        TerminalColorSupport::TrueColor => {
            unreachable!("syntax fallback palette should only apply in non-truecolor mode")
        }
        TerminalColorSupport::Ansi256 => Color::Indexed(ansi256_index),
        TerminalColorSupport::Ansi16 => Color::Indexed(ansi16_index),
    }
}

fn rgb_to_ansi256_index(r: u8, g: u8, b: u8) -> u8 {
    // Note: ANSI 0..15 colors are terminal-theme-dependent. For predictable results across
    // terminals (notably macOS Terminal.app), prefer the standardized 16..255 palette.
    let mut best_index = 16u8;
    let mut best_distance = u32::MAX;

    for index in 16u16..=255u16 {
        let index_u8 = index as u8;
        let (pr, pg, pb) = ansi256_index_to_rgb(index_u8);
        let distance = color_distance_sq(r, g, b, pr, pg, pb);
        if distance < best_distance {
            best_distance = distance;
            best_index = index_u8;
        }
    }

    best_index
}

fn rgb_to_ansi16_index(r: u8, g: u8, b: u8) -> u8 {
    let mut best_index = 0u8;
    let mut best_distance = u32::MAX;

    for (index, (pr, pg, pb)) in ANSI16_RGB.iter().copied().enumerate() {
        let distance = color_distance_sq(r, g, b, pr, pg, pb);
        if distance < best_distance {
            best_distance = distance;
            best_index = index as u8;
        }
    }

    best_index
}

fn ansi256_index_to_rgb(index: u8) -> (u8, u8, u8) {
    if index <= 15 {
        return ANSI16_RGB[index as usize];
    }

    if (16..=231).contains(&index) {
        let level = [0u8, 95, 135, 175, 215, 255];
        let offset = index - 16;
        let r = level[(offset / 36) as usize];
        let g = level[((offset / 6) % 6) as usize];
        let b = level[(offset % 6) as usize];
        return (r, g, b);
    }

    let gray = 8u8.saturating_add((index - 232).saturating_mul(10));
    (gray, gray, gray)
}

fn color_distance_sq(r1: u8, g1: u8, b1: u8, r2: u8, g2: u8, b2: u8) -> u32 {
    let dr = i32::from(r1) - i32::from(r2);
    let dg = i32::from(g1) - i32::from(g2);
    let db = i32::from(b1) - i32::from(b2);
    (dr * dr + dg * dg + db * db) as u32
}

const ANSI16_RGB: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 0, 0),
    (0, 205, 0),
    (205, 205, 0),
    (0, 0, 238),
    (205, 0, 205),
    (0, 205, 205),
    (229, 229, 229),
    (127, 127, 127),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (92, 92, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

#[cfg(test)]
#[path = "../../../tests/unit/ui/core/theme_adapter.rs"]
mod tests;
