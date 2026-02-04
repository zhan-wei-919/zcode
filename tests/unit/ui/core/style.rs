use super::*;

#[test]
fn mod_bit_ops_and_contains() {
    let m = Mod::BOLD | Mod::UNDERLINE | Mod::ITALIC;
    assert!(m.contains(Mod::BOLD));
    assert!(m.contains(Mod::UNDERLINE));
    assert!(m.contains(Mod::ITALIC));
    assert!(!m.contains(Mod::REVERSE));
}

#[test]
fn style_builder_sets_fields() {
    let s = Style::default().fg(Color::Rgb(1, 2, 3)).bg(Color::Indexed(8));
    assert_eq!(s.fg, Some(Color::Rgb(1, 2, 3)));
    assert_eq!(s.bg, Some(Color::Indexed(8)));
}
