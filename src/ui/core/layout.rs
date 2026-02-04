use super::geom::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    pub left: u16,
    pub right: u16,
    pub top: u16,
    pub bottom: u16,
}

impl Insets {
    pub const fn all(v: u16) -> Self {
        Self {
            left: v,
            right: v,
            top: v,
            bottom: v,
        }
    }

    pub const fn xy(x: u16, y: u16) -> Self {
        Self {
            left: x,
            right: x,
            top: y,
            bottom: y,
        }
    }
}

impl Rect {
    pub fn inset(self, insets: Insets) -> Self {
        let x = self.x.saturating_add(insets.left);
        let y = self.y.saturating_add(insets.top);
        let w = self
            .w
            .saturating_sub(insets.left.saturating_add(insets.right));
        let h = self
            .h
            .saturating_sub(insets.top.saturating_add(insets.bottom));
        Rect::new(x, y, w, h)
    }

    pub fn intersect(self, other: Rect) -> Rect {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.right().min(other.right());
        let y2 = self.bottom().min(other.bottom());
        Rect::new(x1, y1, x2.saturating_sub(x1), y2.saturating_sub(y1))
    }

    pub fn split_top(self, h: u16) -> (Rect, Rect) {
        let top_h = h.min(self.h);
        let top = Rect::new(self.x, self.y, self.w, top_h);
        let rest = Rect::new(
            self.x,
            self.y.saturating_add(top_h),
            self.w,
            self.h.saturating_sub(top_h),
        );
        (top, rest)
    }

    pub fn split_bottom(self, h: u16) -> (Rect, Rect) {
        let bottom_h = h.min(self.h);
        let rest_h = self.h.saturating_sub(bottom_h);
        let rest = Rect::new(self.x, self.y, self.w, rest_h);
        let bottom = Rect::new(self.x, self.y.saturating_add(rest_h), self.w, bottom_h);
        (rest, bottom)
    }

    pub fn split_left(self, w: u16) -> (Rect, Rect) {
        let left_w = w.min(self.w);
        let left = Rect::new(self.x, self.y, left_w, self.h);
        let rest = Rect::new(
            self.x.saturating_add(left_w),
            self.y,
            self.w.saturating_sub(left_w),
            self.h,
        );
        (left, rest)
    }

    pub fn split_right(self, w: u16) -> (Rect, Rect) {
        let right_w = w.min(self.w);
        let rest_w = self.w.saturating_sub(right_w);
        let rest = Rect::new(self.x, self.y, rest_w, self.h);
        let right = Rect::new(self.x.saturating_add(rest_w), self.y, right_w, self.h);
        (rest, right)
    }

    pub fn centered(self, w: u16, h: u16) -> Rect {
        let w = w.min(self.w);
        let h = h.min(self.h);
        let x = self.x.saturating_add(self.w.saturating_sub(w) / 2);
        let y = self.y.saturating_add(self.h.saturating_sub(h) / 2);
        Rect::new(x, y, w, h)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/core/layout.rs"]
mod tests;
