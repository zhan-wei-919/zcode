use super::paint_drag_chip;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::{PaintCmd, Painter};
use crate::ui::core::style::Mod;
use crate::ui::core::theme::Theme;

#[test]
fn drag_chip_is_borderless_and_has_accent_bar() {
    let mut painter = Painter::new();
    let screen = Rect::new(0, 0, 80, 24);
    let mouse = Pos::new(10, 10);
    let theme = Theme::default();

    paint_drag_chip(&mut painter, screen, mouse, "tracing_subscriber", &theme);

    let cmds = painter.cmds();
    assert!(
        cmds.iter().any(|c| matches!(c, PaintCmd::FillRect { .. })),
        "expected the chip background to be filled"
    );
    assert!(
        !cmds.iter().any(|c| matches!(c, PaintCmd::Border { .. })),
        "chip should not draw a border"
    );
    assert!(
        cmds.iter()
            .any(|c| matches!(c, PaintCmd::VLine { ch: '\u{258F}', .. })),
        "expected a thin accent bar"
    );
    assert!(
        cmds.iter().any(|c| {
            matches!(c, PaintCmd::Text { style, .. } if style.mods.contains(Mod::BOLD))
        }),
        "expected bold chip label"
    );
}
