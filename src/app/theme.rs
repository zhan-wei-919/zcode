//! UI 主题：把可配置的颜色集中管理，避免散落在渲染代码里。

use crate::kernel::services::ports::ThemeSettings;
use crate::ui::core::style::Color;
use crate::ui::core::theme::Theme as CoreTheme;

#[derive(Debug, Clone)]
pub struct UiTheme {
    pub focus_border: Color,
    pub inactive_border: Color,
    pub separator: Color,
    pub accent_fg: Color,
    pub syntax_comment_fg: Color,
    pub syntax_keyword_fg: Color,
    pub syntax_string_fg: Color,
    pub syntax_number_fg: Color,
    pub syntax_type_fg: Color,
    pub syntax_attribute_fg: Color,
    pub syntax_function_fg: Color,
    pub syntax_variable_fg: Color,
    pub syntax_constant_fg: Color,
    pub syntax_regex_fg: Color,
    pub error_fg: Color,
    pub warning_fg: Color,
    pub activity_bg: Color,
    pub activity_fg: Color,
    pub activity_active_bg: Color,
    pub activity_active_fg: Color,
    pub sidebar_tab_active_bg: Color,
    pub sidebar_tab_active_fg: Color,
    pub sidebar_tab_inactive_fg: Color,
    pub header_fg: Color,
    pub palette_border: Color,
    pub palette_bg: Color,
    pub palette_fg: Color,
    pub palette_selected_bg: Color,
    pub palette_selected_fg: Color,
    pub palette_muted_fg: Color,
    pub indent_guide_fg: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColorSupport {
    TrueColor,
    Ansi256,
    Ansi16,
}

pub fn detect_terminal_color_support() -> TerminalColorSupport {
    if let Ok(value) = std::env::var("ZCODE_COLOR_SUPPORT") {
        let value = value.trim().to_ascii_lowercase();
        match value.as_str() {
            "truecolor" | "24bit" | "rgb" => return TerminalColorSupport::TrueColor,
            "256" | "ansi256" => return TerminalColorSupport::Ansi256,
            "16" | "ansi16" | "basic" => return TerminalColorSupport::Ansi16,
            _ => {}
        }
    }

    let colorterm = std::env::var("COLORTERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let term = std::env::var("TERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if colorterm.contains("truecolor")
        || colorterm.contains("24bit")
        || colorterm.contains("direct")
        || term.contains("truecolor")
        || term.contains("24bit")
        || term.contains("direct")
    {
        return TerminalColorSupport::TrueColor;
    }

    if term.contains("256color") {
        return TerminalColorSupport::Ansi256;
    }

    TerminalColorSupport::Ansi16
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            focus_border: Color::Indexed(6),    // Cyan
            inactive_border: Color::Indexed(8), // DarkGray
            separator: Color::Indexed(8),       // DarkGray
            accent_fg: Color::Indexed(3),
            syntax_comment_fg: Color::Rgb(0x6A, 0x99, 0x55),
            syntax_keyword_fg: Color::Rgb(0x56, 0x9C, 0xD6),
            syntax_string_fg: Color::Rgb(0xCE, 0x91, 0x78),
            syntax_number_fg: Color::Rgb(0xB5, 0xCE, 0xA8),
            syntax_type_fg: Color::Rgb(0x4E, 0xC9, 0xB0),
            syntax_attribute_fg: Color::Rgb(0x4E, 0xC9, 0xB0),
            syntax_function_fg: Color::Rgb(0xDC, 0xDC, 0xAA),
            syntax_variable_fg: Color::Rgb(0x9C, 0xDC, 0xFE),
            syntax_constant_fg: Color::Rgb(0x4F, 0xC1, 0xFF),
            syntax_regex_fg: Color::Rgb(0xD1, 0x69, 0x69),
            error_fg: Color::Indexed(1),   // Red
            warning_fg: Color::Indexed(3), // Yellow
            activity_bg: Color::Reset,
            activity_fg: Color::Indexed(8),             // DarkGray
            activity_active_bg: Color::Indexed(8),      // DarkGray
            activity_active_fg: Color::Indexed(15),     // White
            sidebar_tab_active_bg: Color::Indexed(8),   // DarkGray
            sidebar_tab_active_fg: Color::Indexed(15),  // White
            sidebar_tab_inactive_fg: Color::Indexed(8), // DarkGray
            header_fg: Color::Indexed(6),               // Cyan
            palette_border: Color::Indexed(6),          // Cyan
            palette_bg: Color::Reset,
            palette_fg: Color::Indexed(15),          // White
            palette_selected_bg: Color::Indexed(8),  // DarkGray
            palette_selected_fg: Color::Indexed(15), // White
            palette_muted_fg: Color::Indexed(8),     // DarkGray
            indent_guide_fg: Color::Indexed(8),      // DarkGray
        }
    }
}

impl UiTheme {
    pub fn adapt_to_terminal_capabilities(&mut self) {
        self.apply_color_support(detect_terminal_color_support());
    }

    fn apply_color_support(&mut self, support: TerminalColorSupport) {
        if support == TerminalColorSupport::TrueColor {
            return;
        }

        self.focus_border = map_color_for_support(self.focus_border, support);
        self.inactive_border = map_color_for_support(self.inactive_border, support);
        self.separator = map_color_for_support(self.separator, support);
        self.accent_fg = map_color_for_support(self.accent_fg, support);
        self.syntax_comment_fg = map_color_for_support(self.syntax_comment_fg, support);
        self.syntax_keyword_fg = map_color_for_support(self.syntax_keyword_fg, support);
        self.syntax_string_fg = map_color_for_support(self.syntax_string_fg, support);
        self.syntax_number_fg = map_color_for_support(self.syntax_number_fg, support);
        self.syntax_type_fg = map_color_for_support(self.syntax_type_fg, support);
        self.syntax_attribute_fg = map_color_for_support(self.syntax_attribute_fg, support);
        self.syntax_function_fg = map_color_for_support(self.syntax_function_fg, support);
        self.syntax_variable_fg = map_color_for_support(self.syntax_variable_fg, support);
        self.syntax_constant_fg = map_color_for_support(self.syntax_constant_fg, support);
        self.syntax_regex_fg = map_color_for_support(self.syntax_regex_fg, support);
        self.error_fg = map_color_for_support(self.error_fg, support);
        self.warning_fg = map_color_for_support(self.warning_fg, support);
        self.activity_bg = map_color_for_support(self.activity_bg, support);
        self.activity_fg = map_color_for_support(self.activity_fg, support);
        self.activity_active_bg = map_color_for_support(self.activity_active_bg, support);
        self.activity_active_fg = map_color_for_support(self.activity_active_fg, support);
        self.sidebar_tab_active_bg = map_color_for_support(self.sidebar_tab_active_bg, support);
        self.sidebar_tab_active_fg = map_color_for_support(self.sidebar_tab_active_fg, support);
        self.sidebar_tab_inactive_fg = map_color_for_support(self.sidebar_tab_inactive_fg, support);
        self.header_fg = map_color_for_support(self.header_fg, support);
        self.palette_border = map_color_for_support(self.palette_border, support);
        self.palette_bg = map_color_for_support(self.palette_bg, support);
        self.palette_fg = map_color_for_support(self.palette_fg, support);
        self.palette_selected_bg = map_color_for_support(self.palette_selected_bg, support);
        self.palette_selected_fg = map_color_for_support(self.palette_selected_fg, support);
        self.palette_muted_fg = map_color_for_support(self.palette_muted_fg, support);
        self.indent_guide_fg = map_color_for_support(self.indent_guide_fg, support);

        self.apply_non_truecolor_syntax_palette(support);
    }

    fn apply_non_truecolor_syntax_palette(&mut self, support: TerminalColorSupport) {
        self.syntax_comment_fg = syntax_fallback_color(support, 65, 2);
        self.syntax_keyword_fg = syntax_fallback_color(support, 33, 4);
        self.syntax_string_fg = syntax_fallback_color(support, 114, 10);
        self.syntax_number_fg = syntax_fallback_color(support, 108, 10);
        self.syntax_type_fg = syntax_fallback_color(support, 44, 6);
        self.syntax_attribute_fg = syntax_fallback_color(support, 44, 6);
        self.syntax_function_fg = syntax_fallback_color(support, 179, 11);
        self.syntax_variable_fg = syntax_fallback_color(support, 81, 6);
        self.syntax_constant_fg = syntax_fallback_color(support, 39, 12);
        self.syntax_regex_fg = syntax_fallback_color(support, 167, 9);
    }

    pub fn apply_settings(&mut self, settings: &ThemeSettings) {
        if let Some(v) = &settings.focus_border {
            if let Some(c) = parse_color(v) {
                self.focus_border = c;
            }
        }
        if let Some(v) = &settings.inactive_border {
            if let Some(c) = parse_color(v) {
                self.inactive_border = c;
            }
        }
        if let Some(v) = &settings.separator {
            if let Some(c) = parse_color(v) {
                self.separator = c;
            }
        }
        if let Some(v) = &settings.accent_fg {
            if let Some(c) = parse_color(v) {
                self.accent_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_comment_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_comment_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_keyword_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_keyword_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_string_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_string_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_number_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_number_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_type_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_type_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_attribute_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_attribute_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_function_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_function_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_variable_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_variable_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_constant_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_constant_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_regex_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_regex_fg = c;
            }
        }
        if let Some(v) = &settings.error_fg {
            if let Some(c) = parse_color(v) {
                self.error_fg = c;
            }
        }
        if let Some(v) = &settings.warning_fg {
            if let Some(c) = parse_color(v) {
                self.warning_fg = c;
            }
        }
        if let Some(v) = &settings.activity_bg {
            if let Some(c) = parse_color(v) {
                self.activity_bg = c;
            }
        }
        if let Some(v) = &settings.activity_fg {
            if let Some(c) = parse_color(v) {
                self.activity_fg = c;
            }
        }
        if let Some(v) = &settings.activity_active_bg {
            if let Some(c) = parse_color(v) {
                self.activity_active_bg = c;
            }
        }
        if let Some(v) = &settings.activity_active_fg {
            if let Some(c) = parse_color(v) {
                self.activity_active_fg = c;
            }
        }
        if let Some(v) = &settings.sidebar_tab_active_bg {
            if let Some(c) = parse_color(v) {
                self.sidebar_tab_active_bg = c;
            }
        }
        if let Some(v) = &settings.sidebar_tab_active_fg {
            if let Some(c) = parse_color(v) {
                self.sidebar_tab_active_fg = c;
            }
        }
        if let Some(v) = &settings.sidebar_tab_inactive_fg {
            if let Some(c) = parse_color(v) {
                self.sidebar_tab_inactive_fg = c;
            }
        }
        if let Some(v) = &settings.header_fg {
            if let Some(c) = parse_color(v) {
                self.header_fg = c;
            }
        }
        if let Some(v) = &settings.palette_border {
            if let Some(c) = parse_color(v) {
                self.palette_border = c;
            }
        }
        if let Some(v) = &settings.palette_bg {
            if let Some(c) = parse_color(v) {
                self.palette_bg = c;
            }
        }
        if let Some(v) = &settings.palette_fg {
            if let Some(c) = parse_color(v) {
                self.palette_fg = c;
            }
        }
        if let Some(v) = &settings.palette_selected_bg {
            if let Some(c) = parse_color(v) {
                self.palette_selected_bg = c;
            }
        }
        if let Some(v) = &settings.palette_selected_fg {
            if let Some(c) = parse_color(v) {
                self.palette_selected_fg = c;
            }
        }
        if let Some(v) = &settings.palette_muted_fg {
            if let Some(c) = parse_color(v) {
                self.palette_muted_fg = c;
            }
        }
        if let Some(v) = &settings.indent_guide_fg {
            if let Some(c) = parse_color(v) {
                self.indent_guide_fg = c;
            }
        }
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
    let mut best_index = 0u8;
    let mut best_distance = u32::MAX;

    for index in 0u16..=255u16 {
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

pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (u16, u8, u8) {
    let rf = r as f64 / 255.0;
    let gf = g as f64 / 255.0;
    let bf = b as f64 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f64::EPSILON {
        return (0, 0, (l * 100.0).round() as u8);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - rf).abs() < f64::EPSILON {
        let mut h = (gf - bf) / d;
        if gf < bf {
            h += 6.0;
        }
        h
    } else if (max - gf).abs() < f64::EPSILON {
        (bf - rf) / d + 2.0
    } else {
        (rf - gf) / d + 4.0
    };

    let h = (h * 60.0).round() as u16 % 360;
    let s = (s * 100.0).round() as u8;
    let l = (l * 100.0).round() as u8;
    (h, s, l)
}

pub fn hsl_to_rgb(h: u16, s: u8, l: u8) -> (u8, u8, u8) {
    let h = (h % 360) as f64;
    let s = (s.min(100)) as f64 / 100.0;
    let l = (l.min(100)) as f64 / 100.0;

    if s < f64::EPSILON {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }

    let r = hue_to_rgb(p, q, h / 360.0 + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h / 360.0);
    let b = hue_to_rgb(p, q, h / 360.0 - 1.0 / 3.0);

    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

pub fn color_to_hex(color: Color) -> Option<String> {
    match color {
        Color::Rgb(r, g, b) => Some(format!("#{:02X}{:02X}{:02X}", r, g, b)),
        _ => None,
    }
}

pub fn parse_color(value: &str) -> Option<Color> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }

    if let Some(hex) = v.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }

    let v = v.to_ascii_lowercase();
    let c = match v.as_str() {
        "reset" => Color::Reset,
        "black" => Color::Indexed(0),
        "red" => Color::Indexed(1),
        "green" => Color::Indexed(2),
        "yellow" => Color::Indexed(3),
        "blue" => Color::Indexed(4),
        "magenta" => Color::Indexed(5),
        "cyan" => Color::Indexed(6),
        "gray" | "grey" => Color::Indexed(7),
        "dark_gray" | "darkgrey" => Color::Indexed(8),
        "white" => Color::Indexed(15),
        "light_red" => Color::Indexed(9),
        "light_green" => Color::Indexed(10),
        "light_yellow" => Color::Indexed(11),
        "light_blue" => Color::Indexed(12),
        "light_magenta" => Color::Indexed(13),
        "light_cyan" => Color::Indexed(14),
        _ => return None,
    };

    Some(c)
}

pub(crate) fn to_core_theme(theme: &UiTheme) -> CoreTheme {
    CoreTheme {
        focus_border: theme.focus_border,
        inactive_border: theme.inactive_border,
        separator: theme.separator,
        accent_fg: theme.accent_fg,
        syntax_comment_fg: theme.syntax_comment_fg,
        syntax_keyword_fg: theme.syntax_keyword_fg,
        syntax_string_fg: theme.syntax_string_fg,
        syntax_number_fg: theme.syntax_number_fg,
        syntax_type_fg: theme.syntax_type_fg,
        syntax_attribute_fg: theme.syntax_attribute_fg,
        syntax_function_fg: theme.syntax_function_fg,
        syntax_variable_fg: theme.syntax_variable_fg,
        syntax_constant_fg: theme.syntax_constant_fg,
        syntax_regex_fg: theme.syntax_regex_fg,
        error_fg: theme.error_fg,
        warning_fg: theme.warning_fg,
        activity_bg: theme.activity_bg,
        activity_fg: theme.activity_fg,
        activity_active_bg: theme.activity_active_bg,
        activity_active_fg: theme.activity_active_fg,
        sidebar_tab_active_bg: theme.sidebar_tab_active_bg,
        sidebar_tab_active_fg: theme.sidebar_tab_active_fg,
        sidebar_tab_inactive_fg: theme.sidebar_tab_inactive_fg,
        header_fg: theme.header_fg,
        palette_border: theme.palette_border,
        palette_bg: theme.palette_bg,
        palette_fg: theme.palette_fg,
        palette_selected_bg: theme.palette_selected_bg,
        palette_selected_fg: theme.palette_selected_fg,
        palette_muted_fg: theme.palette_muted_fg,
        indent_guide_fg: theme.indent_guide_fg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi256_fallback_converts_rgb_to_indexed_colors() {
        let mut theme = UiTheme::default();
        theme.apply_color_support(TerminalColorSupport::Ansi256);

        assert_eq!(theme.syntax_comment_fg, Color::Indexed(65));
        assert_eq!(theme.syntax_keyword_fg, Color::Indexed(33));
        assert_eq!(theme.syntax_string_fg, Color::Indexed(114));
        assert_eq!(theme.syntax_number_fg, Color::Indexed(108));
        assert_eq!(theme.syntax_type_fg, Color::Indexed(44));
        assert_eq!(theme.syntax_function_fg, Color::Indexed(179));
        assert_eq!(theme.syntax_variable_fg, Color::Indexed(81));
        assert_eq!(theme.syntax_constant_fg, Color::Indexed(39));
        assert_eq!(theme.syntax_regex_fg, Color::Indexed(167));
    }

    #[test]
    fn ansi16_fallback_converts_rgb_to_indexed_colors() {
        let mut theme = UiTheme::default();
        theme.apply_color_support(TerminalColorSupport::Ansi16);

        assert_eq!(theme.syntax_comment_fg, Color::Indexed(2));
        assert_eq!(theme.syntax_keyword_fg, Color::Indexed(4));
        assert_eq!(theme.syntax_string_fg, Color::Indexed(10));
        assert_eq!(theme.syntax_number_fg, Color::Indexed(10));
        assert_eq!(theme.syntax_type_fg, Color::Indexed(6));
        assert_eq!(theme.syntax_function_fg, Color::Indexed(11));
        assert_eq!(theme.syntax_variable_fg, Color::Indexed(6));
        assert_eq!(theme.syntax_constant_fg, Color::Indexed(12));
        assert_eq!(theme.syntax_regex_fg, Color::Indexed(9));
    }
}
