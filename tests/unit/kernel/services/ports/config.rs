use super::*;

#[test]
fn test_default_config() {
    let config = EditorConfig::default();
    assert_eq!(config.tab_size, 4);
    assert!(config.show_line_numbers);
    assert!(config.show_indent_guides);
    assert!(!config.format_on_save);
}

#[test]
fn test_scroll_step() {
    let config = EditorConfig::default();
    assert_eq!(config.scroll_step(), 1);
}
