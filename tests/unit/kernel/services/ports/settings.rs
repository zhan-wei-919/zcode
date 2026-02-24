use super::*;

#[test]
fn theme_settings_default_syntax_colors_match_kernel_defaults() {
    let settings = ThemeSettings::default();
    for (idx, group) in SyntaxColorGroup::CONFIGURABLE.iter().copied().enumerate() {
        let expected = format!("#{:06X}", DEFAULT_CONFIGURABLE_SYNTAX_RGB_HEX[idx]);
        assert_eq!(settings.syntax_color_for(group), Some(expected.as_str()));
    }
}

#[test]
fn theme_settings_serde_roundtrip_preserves_defaults() {
    let original = ThemeSettings::default();
    let json = serde_json::to_string(&original).expect("serialize ThemeSettings");
    let decoded: ThemeSettings = serde_json::from_str(&json).expect("deserialize ThemeSettings");

    for group in SyntaxColorGroup::CONFIGURABLE {
        assert_eq!(
            decoded.syntax_color_for(group),
            original.syntax_color_for(group)
        );
    }
    assert_eq!(decoded.focus_border, original.focus_border);
    assert_eq!(decoded.palette_fg, original.palette_fg);
}
