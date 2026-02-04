use super::*;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::id::IdPath;
use crate::ui::core::painter::{PaintCmd, Painter};
use crate::ui::core::style::Style;
use crate::ui::core::tree::{NodeKind, Sense, UiTree};
use crate::ui::core::widget::Ui;

#[test]
fn menu_registers_overlay_and_item_nodes() {
    let mut painter = Painter::new();
    let mut tree = UiTree::new();
    let screen = Rect::new(0, 0, 30, 10);
    let mut ui = Ui::new(screen, &mut painter, &mut tree);

    let items = ["One", "Two", "Three"];
    let styles = MenuStyles {
        base: Style::default(),
        border: Style::default(),
        selected: Style::default(),
    };
    let mut menu = Menu {
        id_base: IdPath::root("test_menu"),
        menu_id: 7,
        layer: 10,
        anchor: Pos::new(1, 1),
        items: &items,
        selected: 1,
        styles,
    };

    menu.ui(&mut ui);

    // Overlay should cover the full screen and be clickable.
    assert!(tree.nodes().iter().any(|n| {
        n.rect == screen
            && n.layer == 10
            && n.sense.contains(Sense::CLICK)
            && matches!(n.kind, NodeKind::Unknown)
    }));

    // One MenuItem node per item (when it fits vertically).
    let item_nodes = tree
        .nodes()
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::MenuItem { menu_id: 7, .. }))
        .count();
    assert_eq!(item_nodes, items.len());

    // Should paint a background and a border.
    assert!(painter
        .cmds()
        .iter()
        .any(|c| matches!(c, PaintCmd::FillRect { .. })));
    assert!(painter
        .cmds()
        .iter()
        .any(|c| matches!(c, PaintCmd::Border { .. })));
}

