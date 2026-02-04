use super::*;

#[test]
fn painter_collects_commands_and_clears() {
    let mut p = Painter::new();
    assert!(p.cmds().is_empty());

    p.fill_rect(Rect::new(0, 0, 1, 1), Style::default());
    p.style_rect(Rect::new(0, 0, 1, 1), Style::default());
    p.text(Pos::new(0, 0), "hi", Style::default());
    p.border(Rect::new(0, 0, 3, 3), Style::default(), BorderKind::Plain);
    assert_eq!(p.cmds().len(), 4);

    p.clear();
    assert!(p.cmds().is_empty());
}
