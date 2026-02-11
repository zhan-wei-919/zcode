use super::*;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::id::IdPath;
use crate::ui::core::painter::{PaintCmd, Painter};
use crate::ui::core::style::Style;
use crate::ui::core::tree::{NodeKind, Sense, UiTree};
use crate::ui::core::widget::Ui;

fn test_styles(border: Option<Style>) -> MenuStyles {
    MenuStyles {
        base: Style::default(),
        border,
        selected: Style::default(),
        disabled: Style::default(),
        separator: Style::default(),
    }
}

#[test]
fn menu_registers_overlay_and_item_nodes() {
    let mut painter = Painter::new();
    let mut tree = UiTree::new();
    let screen = Rect::new(0, 0, 30, 10);
    let mut ui = Ui::new(screen, &mut painter, &mut tree);

    let items = [
        MenuItem::action("One"),
        MenuItem::action("Two"),
        MenuItem::action("Three"),
    ];
    let mut menu = Menu {
        id_base: IdPath::root("test_menu"),
        menu_id: 7,
        layer: 10,
        anchor: Pos::new(1, 1),
        items: &items,
        selected: 1,
        styles: test_styles(Some(Style::default())),
    };

    menu.ui(&mut ui);

    assert!(tree.nodes().iter().any(|n| {
        n.rect == screen
            && n.layer == 10
            && n.sense.contains(Sense::CLICK)
            && matches!(n.kind, NodeKind::Unknown)
    }));

    let item_nodes = tree
        .nodes()
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::MenuItem { menu_id: 7, .. }))
        .count();
    assert_eq!(item_nodes, items.len());

    assert!(painter
        .cmds()
        .iter()
        .any(|c| matches!(c, PaintCmd::FillRect { .. })));
    assert!(painter
        .cmds()
        .iter()
        .any(|c| matches!(c, PaintCmd::Border { .. })));
}

#[test]
fn menu_without_border_does_not_emit_border_paint() {
    let mut painter = Painter::new();
    let mut tree = UiTree::new();
    let screen = Rect::new(0, 0, 30, 10);
    let mut ui = Ui::new(screen, &mut painter, &mut tree);

    let items = [MenuItem::action("One"), MenuItem::action("Two")];
    let mut menu = Menu {
        id_base: IdPath::root("test_menu"),
        menu_id: 8,
        layer: 10,
        anchor: Pos::new(1, 1),
        items: &items,
        selected: 0,
        styles: test_styles(None),
    };

    menu.ui(&mut ui);

    assert!(painter
        .cmds()
        .iter()
        .any(|c| matches!(c, PaintCmd::FillRect { .. })));
    assert!(!painter
        .cmds()
        .iter()
        .any(|c| matches!(c, PaintCmd::Border { .. })));
}

#[test]
fn menu_disabled_and_separator_rows_do_not_register_click_nodes() {
    let mut painter = Painter::new();
    let mut tree = UiTree::new();
    let screen = Rect::new(0, 0, 30, 10);
    let mut ui = Ui::new(screen, &mut painter, &mut tree);

    let items = [
        MenuItem::action("One"),
        MenuItem::disabled_action("Disabled"),
        MenuItem::separator(),
        MenuItem::action("Two"),
    ];
    let mut menu = Menu {
        id_base: IdPath::root("test_menu"),
        menu_id: 9,
        layer: 10,
        anchor: Pos::new(1, 1),
        items: &items,
        selected: 0,
        styles: test_styles(None),
    };

    menu.ui(&mut ui);

    let mut indices = tree
        .nodes()
        .iter()
        .filter_map(|node| match node.kind {
            NodeKind::MenuItem { menu_id: 9, index } => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();
    indices.sort_unstable();
    assert_eq!(indices, vec![0, 3]);
}
