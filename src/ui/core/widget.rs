use super::geom::Rect;
use super::layout::Insets;
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

    pub fn with_rect<R>(&mut self, rect: Rect, f: impl FnOnce(&mut Ui<'_>) -> R) -> R {
        let mut child = Ui {
            rect,
            painter: self.painter,
            tree: self.tree,
        };
        f(&mut child)
    }

    pub fn inset(&mut self, insets: Insets) {
        self.rect = self.rect.inset(insets);
    }

    pub fn take_top(&mut self, h: u16) -> Rect {
        let (top, rest) = self.rect.split_top(h);
        self.rect = rest;
        top
    }

    pub fn take_bottom(&mut self, h: u16) -> Rect {
        let (rest, bottom) = self.rect.split_bottom(h);
        self.rect = rest;
        bottom
    }

    pub fn take_left(&mut self, w: u16) -> Rect {
        let (left, rest) = self.rect.split_left(w);
        self.rect = rest;
        left
    }

    pub fn take_right(&mut self, w: u16) -> Rect {
        let (rest, right) = self.rect.split_right(w);
        self.rect = rest;
        right
    }
}

pub trait Widget {
    fn ui(&mut self, ui: &mut Ui);
}
