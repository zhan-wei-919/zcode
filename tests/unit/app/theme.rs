use super::*;

#[test]
fn recompute_syntax_derived_groups_keeps_operator_and_tag_in_sync() {
    let mut theme = UiTheme {
        palette_fg: Color::Rgb(0x01, 0x02, 0x03),
        ..Default::default()
    };
    theme.syntax_colors[SyntaxColorGroup::Operator as usize] = Color::Rgb(0x09, 0x09, 0x09);

    theme.syntax_colors[SyntaxColorGroup::Keyword as usize] = Color::Rgb(0x10, 0x11, 0x12);
    theme.syntax_colors[SyntaxColorGroup::Tag as usize] = Color::Rgb(0x09, 0x09, 0x09);

    theme.recompute_syntax_derived_groups();

    assert_eq!(
        theme.syntax_colors[SyntaxColorGroup::Operator as usize],
        theme.palette_fg
    );
    assert_eq!(
        theme.syntax_colors[SyntaxColorGroup::Tag as usize],
        theme.syntax_colors[SyntaxColorGroup::Keyword as usize]
    );
}

#[test]
fn set_syntax_color_recomputes_derived_groups() {
    let mut theme = UiTheme::default();
    let keyword = Color::Rgb(0x01, 0x02, 0x03);
    theme.set_syntax_color(SyntaxColorGroup::Keyword, keyword);

    assert_eq!(theme.syntax_colors[SyntaxColorGroup::Tag as usize], keyword);
}
