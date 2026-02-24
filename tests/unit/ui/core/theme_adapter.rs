use super::*;
use crate::ui::core::color_support::TerminalColorSupport;

#[test]
fn ansi256_fallback_converts_syntax_colors_to_indexed_palette() {
    let theme = Theme::default();
    let adapted = adapt_theme(&theme, TerminalColorSupport::Ansi256);

    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Comment),
        Color::Indexed(65)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Keyword),
        Color::Indexed(33)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::KeywordControl),
        Color::Indexed(5)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::String),
        Color::Indexed(114)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Number),
        Color::Indexed(108)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Type),
        Color::Indexed(44)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Attribute),
        Color::Indexed(44)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Namespace),
        Color::Indexed(44)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Macro),
        Color::Indexed(68)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Function),
        Color::Indexed(179)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Variable),
        Color::Indexed(81)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Constant),
        Color::Indexed(39)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Regex),
        Color::Indexed(167)
    );
}

#[test]
fn ansi16_fallback_converts_syntax_colors_to_indexed_palette() {
    let theme = Theme::default();
    let adapted = adapt_theme(&theme, TerminalColorSupport::Ansi16);

    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Comment),
        Color::Indexed(2)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Keyword),
        Color::Indexed(4)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::KeywordControl),
        Color::Indexed(5)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::String),
        Color::Indexed(10)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Number),
        Color::Indexed(10)
    );
    assert_eq!(adapted.syntax_fg(SyntaxColorGroup::Type), Color::Indexed(6));
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Attribute),
        Color::Indexed(6)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Namespace),
        Color::Indexed(6)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Macro),
        Color::Indexed(4)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Function),
        Color::Indexed(11)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Variable),
        Color::Indexed(6)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Constant),
        Color::Indexed(12)
    );
    assert_eq!(
        adapted.syntax_fg(SyntaxColorGroup::Regex),
        Color::Indexed(9)
    );
}

#[test]
fn color_to_rgb_supports_indexed_colors() {
    assert_eq!(color_to_rgb(Color::Indexed(16)), Some((0, 0, 0)));
    assert_eq!(color_to_rgb(Color::Indexed(21)), Some((0, 0, 255)));
    assert_eq!(color_to_rgb(Color::Indexed(244)), Some((128, 128, 128)));
    assert_eq!(color_to_rgb(Color::Reset), None);
}

#[test]
fn color_to_hex_supports_indexed_colors() {
    assert_eq!(
        color_to_hex(Color::Indexed(21)),
        Some("#0000FF".to_string())
    );
    assert_eq!(
        color_to_hex(Color::Indexed(244)),
        Some("#808080".to_string())
    );
    assert_eq!(color_to_hex(Color::Reset), None);
}

#[test]
fn ansi256_rgb_mapping_avoids_theme_dependent_base16_palette() {
    // ANSI 0..15 colors can be customized by terminal themes (notably on macOS Terminal.app),
    // so RGB→ANSI256 mapping should prefer the standardized 16..255 palette for stability.
    assert_eq!(
        map_color_to_support(Color::Rgb(255, 0, 0), TerminalColorSupport::Ansi256),
        Color::Indexed(196)
    );
    assert_eq!(
        map_color_to_support(Color::Rgb(0, 255, 0), TerminalColorSupport::Ansi256),
        Color::Indexed(46)
    );
    assert_eq!(
        map_color_to_support(Color::Rgb(0, 0, 255), TerminalColorSupport::Ansi256),
        Color::Indexed(21)
    );
    assert_eq!(
        map_color_to_support(Color::Rgb(0, 0, 0), TerminalColorSupport::Ansi256),
        Color::Indexed(16)
    );
    assert_eq!(
        map_color_to_support(Color::Rgb(255, 255, 255), TerminalColorSupport::Ansi256),
        Color::Indexed(231)
    );
}
