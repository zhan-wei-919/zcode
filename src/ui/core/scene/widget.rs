use super::geom::Rect;
use super::painter::Painter;
use super::tree::UiTree;

pub struct Ui<'a> {
    pub rect: Rect,
    pub painter: &'a mut Painter,
    pub tree: &'a mut UiTree,
}

impl<'a> Ui<'a> {
    pub fn new(rect: Rect, painter: &'a mut Painter, tree: &'a mut UiTree) -> Self {
        Self {
            rect,
            painter,
            tree,
        }
    }
}

pub trait Widget {
    fn ui(&mut self, ui: &mut Ui);
}
