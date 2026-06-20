use crate::kernel::editor::{SyntaxColorGroup, DEFAULT_CONFIGURABLE_SYNTAX_RGB_HEX};
use crate::ui::core::style::Color;

/// Semantic theme: render code reads these fields directly.
#[derive(Debug, Clone)]
pub struct Theme {
    pub focus_border: Color,
    pub separator: Color,
    pub accent_fg: Color,
    pub syntax_colors: [Color; SyntaxColorGroup::COUNT],
    pub error_fg: Color,
    pub warning_fg: Color,
    pub header_fg: Color,
    pub palette_fg: Color,
    pub palette_selected_bg: Color,
    pub palette_selected_fg: Color,
    pub palette_muted_fg: Color,
    pub indent_guide_fg: Color,
    pub editor_bg: Color,
    pub sidebar_bg: Color,
    pub popup_bg: Color,
    pub statusbar_bg: Color,
    /// 状态栏模式块底色 + 块上文字色（暂只用 INSERT；模态编辑落地后补其余）。
    pub mode_insert_bg: Color,
    pub mode_text_fg: Color,
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
    /// 光标所贴括号与其配对括号的前景色（加粗渲染）。
    pub bracket_match_fg: Color,
}

impl Theme {
    pub fn syntax_fg(&self, group: SyntaxColorGroup) -> Color {
        self.syntax_colors[group as usize]
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
            focus_border: Color::Indexed(6), // Cyan
            separator: Color::Indexed(8),    // DarkGray
            accent_fg: Color::Indexed(3),
            syntax_colors,
            error_fg: Color::Indexed(1),   // Red
            warning_fg: Color::Indexed(3), // Yellow
            header_fg: Color::Indexed(6),  // Cyan
            palette_fg,
            palette_selected_bg: Color::Indexed(8), // DarkGray
            palette_selected_fg: Color::Indexed(15), // White
            palette_muted_fg: Color::Indexed(8),    // DarkGray
            indent_guide_fg: Color::Indexed(8),     // DarkGray
            editor_bg: Color::Reset,
            sidebar_bg: Color::Reset,
            popup_bg: Color::Reset,
            statusbar_bg: Color::Reset,
            mode_insert_bg: Color::Indexed(2),             // Green
            mode_text_fg: Color::Indexed(0),               // Black（彩色块上的字）
            md_heading1_fg: Color::Rgb(0x56, 0x9C, 0xD6),  // Blue
            md_heading2_fg: Color::Rgb(0x4E, 0xC9, 0xB0),  // Teal
            md_heading3_fg: Color::Rgb(0xDC, 0xDC, 0xAA),  // Yellow
            md_heading4_fg: Color::Rgb(0xCE, 0x91, 0x78),  // Orange
            md_heading5_fg: Color::Rgb(0xC5, 0x86, 0xC0),  // Purple
            md_heading6_fg: Color::Rgb(0x6A, 0x99, 0x55),  // Green
            md_link_fg: Color::Rgb(0x56, 0x9C, 0xD6),      // Blue
            md_code_fg: Color::Rgb(0xCE, 0x91, 0x78),      // Orange
            md_code_bg: Color::Rgb(0x30, 0x30, 0x30),      // Dark gray
            md_blockquote_fg: Color::Indexed(8),           // DarkGray
            md_blockquote_bar: Color::Indexed(8),          // DarkGray
            md_hr_fg: Color::Indexed(8),                   // DarkGray
            md_marker_fg: Color::Indexed(8),               // DarkGray
            search_match_bg: Color::Rgb(0x5A, 0x4A, 0x1E), // Soft amber
            search_current_match_bg: Color::Rgb(0x80, 0x60, 0x10), // Bright amber
            bracket_match_fg: Color::Rgb(0xFF, 0xA5, 0x00), // Bright orange
        }
    }
}
