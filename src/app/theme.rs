//! UI 主题：把可配置的颜色集中管理，避免散落在渲染代码里。

use ratatui::style::Color;

use crate::kernel::services::ports::ThemeSettings;

#[derive(Debug, Clone)]
pub struct UiTheme {
    pub focus_border: Color,
    pub inactive_border: Color,
    pub separator: Color,
    pub accent_fg: Color,
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
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            focus_border: Color::Cyan,
            inactive_border: Color::DarkGray,
            separator: Color::DarkGray,
            accent_fg: Color::Yellow,
            activity_bg: Color::Reset,
            activity_fg: Color::DarkGray,
            activity_active_bg: Color::DarkGray,
            activity_active_fg: Color::White,
            sidebar_tab_active_bg: Color::DarkGray,
            sidebar_tab_active_fg: Color::White,
            sidebar_tab_inactive_fg: Color::DarkGray,
            header_fg: Color::Cyan,
            palette_border: Color::Cyan,
            palette_bg: Color::Reset,
            palette_fg: Color::White,
            palette_selected_bg: Color::DarkGray,
            palette_selected_fg: Color::White,
            palette_muted_fg: Color::DarkGray,
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
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "dark_gray" | "darkgrey" => Color::DarkGray,
        "white" => Color::White,
        "light_red" => Color::LightRed,
        "light_green" => Color::LightGreen,
        "light_yellow" => Color::LightYellow,
        "light_blue" => Color::LightBlue,
        "light_magenta" => Color::LightMagenta,
        "light_cyan" => Color::LightCyan,
        _ => return None,
    };

    Some(c)
}
