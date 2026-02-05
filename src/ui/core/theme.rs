use crate::ui::core::style::Color;

/// Semantic theme tokens for the UI layer.
///
/// This keeps the UI code independent from backend-specific color types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    FocusBorder,
    InactiveBorder,
    Separator,
    AccentFg,
    HeaderFg,
    PaletteBorder,
    PaletteBg,
    PaletteFg,
    PaletteSelectedBg,
    PaletteSelectedFg,
    PaletteMutedFg,
    ErrorFg,
    WarningFg,
    ActivityBg,
    ActivityFg,
    ActivityActiveBg,
    ActivityActiveFg,
    SidebarTabActiveBg,
    SidebarTabActiveFg,
    SidebarTabInactiveFg,
    SyntaxStringFg,
    SyntaxNumberFg,
    SyntaxAttributeFg,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub focus_border: Color,
    pub inactive_border: Color,
    pub separator: Color,
    pub accent_fg: Color,
    pub syntax_string_fg: Color,
    pub syntax_number_fg: Color,
    pub syntax_attribute_fg: Color,
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

impl Theme {
    pub fn color(&self, token: Token) -> Color {
        match token {
            Token::FocusBorder => self.focus_border,
            Token::InactiveBorder => self.inactive_border,
            Token::Separator => self.separator,
            Token::AccentFg => self.accent_fg,
            Token::HeaderFg => self.header_fg,
            Token::PaletteBorder => self.palette_border,
            Token::PaletteBg => self.palette_bg,
            Token::PaletteFg => self.palette_fg,
            Token::PaletteSelectedBg => self.palette_selected_bg,
            Token::PaletteSelectedFg => self.palette_selected_fg,
            Token::PaletteMutedFg => self.palette_muted_fg,
            Token::ErrorFg => self.error_fg,
            Token::WarningFg => self.warning_fg,
            Token::ActivityBg => self.activity_bg,
            Token::ActivityFg => self.activity_fg,
            Token::ActivityActiveBg => self.activity_active_bg,
            Token::ActivityActiveFg => self.activity_active_fg,
            Token::SidebarTabActiveBg => self.sidebar_tab_active_bg,
            Token::SidebarTabActiveFg => self.sidebar_tab_active_fg,
            Token::SidebarTabInactiveFg => self.sidebar_tab_inactive_fg,
            Token::SyntaxStringFg => self.syntax_string_fg,
            Token::SyntaxNumberFg => self.syntax_number_fg,
            Token::SyntaxAttributeFg => self.syntax_attribute_fg,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            focus_border: Color::Indexed(6),        // Cyan
            inactive_border: Color::Indexed(8),     // DarkGray
            separator: Color::Indexed(8),           // DarkGray
            accent_fg: Color::Indexed(3),           // Yellow
            syntax_string_fg: Color::Indexed(2),    // Green
            syntax_number_fg: Color::Indexed(5),    // Magenta
            syntax_attribute_fg: Color::Indexed(4), // Blue
            error_fg: Color::Indexed(1),            // Red
            warning_fg: Color::Indexed(3),          // Yellow
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
