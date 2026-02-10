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
    pub syntax_namespace_fg: Color,
    pub syntax_macro_fg: Color,
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
    pub editor_bg: Color,
    pub sidebar_bg: Color,
    pub popup_bg: Color,
    pub statusbar_bg: Color,
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
            syntax_namespace_fg: Color::Rgb(0x4E, 0xC9, 0xB0),
            syntax_macro_fg: Color::Rgb(0x56, 0x9C, 0xD6),
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
            editor_bg: Color::Reset,
            sidebar_bg: Color::Reset,
            popup_bg: Color::Reset,
            statusbar_bg: Color::Reset,
        }
    }
}

impl UiTheme {
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
        if let Some(v) = &settings.syntax_namespace_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_namespace_fg = c;
            }
        }
        if let Some(v) = &settings.syntax_macro_fg {
            if let Some(c) = parse_color(v) {
                self.syntax_macro_fg = c;
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
        if let Some(v) = &settings.editor_bg {
            if let Some(c) = parse_color(v) {
                self.editor_bg = c;
            }
        }
        if let Some(v) = &settings.sidebar_bg {
            if let Some(c) = parse_color(v) {
                self.sidebar_bg = c;
            }
        }
        if let Some(v) = &settings.popup_bg {
            if let Some(c) = parse_color(v) {
                self.popup_bg = c;
            }
        }
        if let Some(v) = &settings.statusbar_bg {
            if let Some(c) = parse_color(v) {
                self.statusbar_bg = c;
            }
        }
    }
}

pub use crate::ui::core::color::{hsl_to_rgb, rgb_to_hsl};

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
        syntax_namespace_fg: theme.syntax_namespace_fg,
        syntax_macro_fg: theme.syntax_macro_fg,
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
        editor_bg: theme.editor_bg,
        sidebar_bg: theme.sidebar_bg,
        popup_bg: theme.popup_bg,
        statusbar_bg: theme.statusbar_bg,
    }
}
