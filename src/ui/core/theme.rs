use crate::kernel::editor::{SyntaxColorGroup, DEFAULT_CONFIGURABLE_SYNTAX_RGB_HEX};
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
    SyntaxCommentFg,
    SyntaxKeywordFg,
    SyntaxKeywordControlFg,
    SyntaxStringFg,
    SyntaxNumberFg,
    SyntaxTypeFg,
    SyntaxAttributeFg,
    SyntaxNamespaceFg,
    SyntaxMacroFg,
    SyntaxFunctionFg,
    SyntaxVariableFg,
    SyntaxConstantFg,
    SyntaxRegexFg,
    SyntaxOperatorFg,
    SyntaxTagFg,
    EditorBg,
    SidebarBg,
    PopupBg,
    StatusbarBg,
    MdHeading1Fg,
    MdHeading2Fg,
    MdHeading3Fg,
    MdHeading4Fg,
    MdHeading5Fg,
    MdHeading6Fg,
    MdLinkFg,
    MdCodeFg,
    MdCodeBg,
    MdBlockquoteFg,
    MdBlockquoteBar,
    MdHrFg,
    MdMarkerFg,
    SearchMatchBg,
    SearchCurrentMatchBg,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub focus_border: Color,
    pub inactive_border: Color,
    pub separator: Color,
    pub accent_fg: Color,
    pub syntax_colors: [Color; SyntaxColorGroup::COUNT],
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
    pub md_heading1_fg: Color,
    pub md_heading2_fg: Color,
    pub md_heading3_fg: Color,
    pub md_heading4_fg: Color,
    pub md_heading5_fg: Color,
    pub md_heading6_fg: Color,
    pub md_link_fg: Color,
    pub md_code_fg: Color,
    pub md_code_bg: Color,
    pub md_blockquote_fg: Color,
    pub md_blockquote_bar: Color,
    pub md_hr_fg: Color,
    pub md_marker_fg: Color,
    pub search_match_bg: Color,
    pub search_current_match_bg: Color,
}

impl Theme {
    pub fn syntax_fg(&self, group: SyntaxColorGroup) -> Color {
        self.syntax_colors[group as usize]
    }

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
            Token::SyntaxCommentFg => self.syntax_fg(SyntaxColorGroup::Comment),
            Token::SyntaxKeywordFg => self.syntax_fg(SyntaxColorGroup::Keyword),
            Token::SyntaxKeywordControlFg => self.syntax_fg(SyntaxColorGroup::KeywordControl),
            Token::SyntaxStringFg => self.syntax_fg(SyntaxColorGroup::String),
            Token::SyntaxNumberFg => self.syntax_fg(SyntaxColorGroup::Number),
            Token::SyntaxTypeFg => self.syntax_fg(SyntaxColorGroup::Type),
            Token::SyntaxAttributeFg => self.syntax_fg(SyntaxColorGroup::Attribute),
            Token::SyntaxNamespaceFg => self.syntax_fg(SyntaxColorGroup::Namespace),
            Token::SyntaxMacroFg => self.syntax_fg(SyntaxColorGroup::Macro),
            Token::SyntaxFunctionFg => self.syntax_fg(SyntaxColorGroup::Function),
            Token::SyntaxVariableFg => self.syntax_fg(SyntaxColorGroup::Variable),
            Token::SyntaxConstantFg => self.syntax_fg(SyntaxColorGroup::Constant),
            Token::SyntaxRegexFg => self.syntax_fg(SyntaxColorGroup::Regex),
            Token::SyntaxOperatorFg => self.syntax_fg(SyntaxColorGroup::Operator),
            Token::SyntaxTagFg => self.syntax_fg(SyntaxColorGroup::Tag),
            Token::EditorBg => self.editor_bg,
            Token::SidebarBg => self.sidebar_bg,
            Token::PopupBg => self.popup_bg,
            Token::StatusbarBg => self.statusbar_bg,
            Token::MdHeading1Fg => self.md_heading1_fg,
            Token::MdHeading2Fg => self.md_heading2_fg,
            Token::MdHeading3Fg => self.md_heading3_fg,
            Token::MdHeading4Fg => self.md_heading4_fg,
            Token::MdHeading5Fg => self.md_heading5_fg,
            Token::MdHeading6Fg => self.md_heading6_fg,
            Token::MdLinkFg => self.md_link_fg,
            Token::MdCodeFg => self.md_code_fg,
            Token::MdCodeBg => self.md_code_bg,
            Token::MdBlockquoteFg => self.md_blockquote_fg,
            Token::MdBlockquoteBar => self.md_blockquote_bar,
            Token::MdHrFg => self.md_hr_fg,
            Token::MdMarkerFg => self.md_marker_fg,
            Token::SearchMatchBg => self.search_match_bg,
            Token::SearchCurrentMatchBg => self.search_current_match_bg,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        let palette_fg = Color::Indexed(15); // White
        let mut syntax_colors = [palette_fg; SyntaxColorGroup::COUNT];
        for (idx, group) in SyntaxColorGroup::CONFIGURABLE.iter().copied().enumerate() {
            let rgb = DEFAULT_CONFIGURABLE_SYNTAX_RGB_HEX[idx];
            let r = ((rgb >> 16) & 0xFF) as u8;
            let g = ((rgb >> 8) & 0xFF) as u8;
            let b = (rgb & 0xFF) as u8;
            syntax_colors[group as usize] = Color::Rgb(r, g, b);
        }

        syntax_colors[SyntaxColorGroup::Operator as usize] = palette_fg;
        syntax_colors[SyntaxColorGroup::Tag as usize] =
            syntax_colors[SyntaxColorGroup::Keyword as usize];
        Self {
            focus_border: Color::Indexed(6),    // Cyan
            inactive_border: Color::Indexed(8), // DarkGray
            separator: Color::Indexed(8),       // DarkGray
            accent_fg: Color::Indexed(3),
            syntax_colors,
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
            palette_fg,
            palette_selected_bg: Color::Indexed(8), // DarkGray
            palette_selected_fg: Color::Indexed(15), // White
            palette_muted_fg: Color::Indexed(8),    // DarkGray
            indent_guide_fg: Color::Indexed(8),     // DarkGray
            editor_bg: Color::Reset,
            sidebar_bg: Color::Reset,
            popup_bg: Color::Reset,
            statusbar_bg: Color::Reset,
            md_heading1_fg: Color::Rgb(0x56, 0x9C, 0xD6), // Blue
            md_heading2_fg: Color::Rgb(0x4E, 0xC9, 0xB0), // Teal
            md_heading3_fg: Color::Rgb(0xDC, 0xDC, 0xAA), // Yellow
            md_heading4_fg: Color::Rgb(0xCE, 0x91, 0x78), // Orange
            md_heading5_fg: Color::Rgb(0xC5, 0x86, 0xC0), // Purple
            md_heading6_fg: Color::Rgb(0x6A, 0x99, 0x55), // Green
            md_link_fg: Color::Rgb(0x56, 0x9C, 0xD6),     // Blue
            md_code_fg: Color::Rgb(0xCE, 0x91, 0x78),     // Orange
            md_code_bg: Color::Rgb(0x30, 0x30, 0x30),     // Dark gray
            md_blockquote_fg: Color::Indexed(8),          // DarkGray
            md_blockquote_bar: Color::Indexed(8),         // DarkGray
            md_hr_fg: Color::Indexed(8),                  // DarkGray
            md_marker_fg: Color::Indexed(8),              // DarkGray
            search_match_bg: Color::Rgb(0x5A, 0x4A, 0x1E), // Soft amber
            search_current_match_bg: Color::Rgb(0x80, 0x60, 0x10), // Bright amber
        }
    }
}
