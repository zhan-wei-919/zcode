//! Headless backend for tests and benchmarks.

use crate::ui::backend::Backend;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::{BorderKind, PaintCmd};
use crate::ui::core::style::Style;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub symbol: String,
    pub style: Style,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestBuffer {
    area: Rect,
    cells: Vec<Cell>,
}

impl TestBuffer {
    pub fn new(area: Rect) -> Self {
        let len = area.w as usize * area.h as usize;
        Self {
            area,
            cells: std::iter::repeat_with(|| Cell {
                symbol: " ".to_string(),
                style: Style::default(),
            })
            .take(len)
            .collect(),
        }
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    pub fn cell(&self, x: u16, y: u16) -> Option<&Cell> {
        let idx = self.idx(x, y)?;
        self.cells.get(idx)
    }

    pub fn cell_mut(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        let idx = self.idx(x, y)?;
        self.cells.get_mut(idx)
    }

    fn idx(&self, x: u16, y: u16) -> Option<usize> {
        if self.area.is_empty() {
            return None;
        }
        if x < self.area.x || y < self.area.y {
            return None;
        }
        if x >= self.area.right() || y >= self.area.bottom() {
            return None;
        }
        let rel_x = x - self.area.x;
        let rel_y = y - self.area.y;
        Some(rel_y as usize * self.area.w as usize + rel_x as usize)
    }
}

#[derive(Debug)]
pub struct TestBackend {
    buf: TestBuffer,
    cursor: Option<Pos>,
}

impl TestBackend {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            buf: TestBuffer::new(Rect::new(0, 0, width, height)),
            cursor: None,
        }
    }

    pub fn buffer(&self) -> &TestBuffer {
        &self.buf
    }

    pub fn cursor(&self) -> Option<Pos> {
        self.cursor
    }
}

impl Backend for TestBackend {
    fn draw(&mut self, _area: Rect, cmds: &[PaintCmd]) {
        for cmd in cmds {
            match cmd {
                PaintCmd::FillRect { rect, style } => fill_rect(&mut self.buf, *rect, *style),
                PaintCmd::StyleRect { rect, style } => style_rect(&mut self.buf, *rect, *style),
                PaintCmd::HLine { pos, len, ch, style } => {
                    draw_hline(&mut self.buf, *pos, *len, *ch, *style)
                }
                PaintCmd::VLine { pos, len, ch, style } => {
                    draw_vline(&mut self.buf, *pos, *len, *ch, *style)
                }
                PaintCmd::Text {
                    pos,
                    text,
                    style,
                    clip,
                } => draw_text(&mut self.buf, *pos, text, *style, *clip),
                PaintCmd::Border { rect, style, kind } => {
                    draw_border(&mut self.buf, *rect, *style, *kind)
                }
            }
        }
    }

    fn set_cursor(&mut self, pos: Option<Pos>) {
        self.cursor = pos;
    }
}

fn fill_rect(buf: &mut TestBuffer, rect: Rect, style: Style) {
    let clip = rect.intersect(buf.area());
    if clip.is_empty() {
        return;
    }
    for y in clip.y..clip.bottom() {
        for x in clip.x..clip.right() {
            let Some(cell) = buf.cell_mut(x, y) else {
                continue;
            };
            cell.symbol = " ".to_string();
            cell.style = style;
        }
    }
}

fn style_rect(buf: &mut TestBuffer, rect: Rect, style: Style) {
    let clip = rect.intersect(buf.area());
    if clip.is_empty() {
        return;
    }
    for y in clip.y..clip.bottom() {
        for x in clip.x..clip.right() {
            let Some(cell) = buf.cell_mut(x, y) else {
                continue;
            };
            cell.style = style;
        }
    }
}

fn draw_hline(buf: &mut TestBuffer, pos: Pos, len: u16, ch: char, style: Style) {
    if len == 0 {
        return;
    }
    let rect = Rect::new(pos.x, pos.y, len, 1);
    let clip = rect.intersect(buf.area());
    if clip.is_empty() {
        return;
    }
    for x in clip.x..clip.right() {
        let Some(cell) = buf.cell_mut(x, clip.y) else {
            continue;
        };
        cell.symbol = ch.to_string();
        cell.style = style;
    }
}

fn draw_vline(buf: &mut TestBuffer, pos: Pos, len: u16, ch: char, style: Style) {
    if len == 0 {
        return;
    }
    let rect = Rect::new(pos.x, pos.y, 1, len);
    let clip = rect.intersect(buf.area());
    if clip.is_empty() {
        return;
    }
    for y in clip.y..clip.bottom() {
        let Some(cell) = buf.cell_mut(clip.x, y) else {
            continue;
        };
        cell.symbol = ch.to_string();
        cell.style = style;
    }
}

fn draw_text(buf: &mut TestBuffer, pos: Pos, text: &str, style: Style, clip: Option<Rect>) {
    let clip = clip.unwrap_or_else(|| buf.area()).intersect(buf.area());
    if clip.is_empty() {
        return;
    }
    let mut x = pos.x;
    let y = pos.y;
    if y < clip.y || y >= clip.bottom() {
        return;
    }
    for g in text.graphemes(true) {
        let w = UnicodeWidthStr::width(g) as u16;
        if w == 0 {
            continue;
        }
        if x >= clip.right() {
            break;
        }
        // Do not partially render wide glyphs.
        if w > 1 && x.saturating_add(w).saturating_sub(1) >= clip.right() {
            break;
        }
        if !clip.contains(Pos::new(x, y)) {
            x = x.saturating_add(w);
            continue;
        }

        let Some(cell) = buf.cell_mut(x, y) else {
            break;
        };
        cell.symbol = g.to_string();
        cell.style = style;

        // Basic wide-char handling: occupy next cells as spaces.
        for dx in 1..w {
            let xx = x.saturating_add(dx);
            if !clip.contains(Pos::new(xx, y)) {
                break;
            }
            let Some(cell) = buf.cell_mut(xx, y) else {
                break;
            };
            cell.symbol = " ".to_string();
            cell.style = style;
        }

        x = x.saturating_add(w);
    }
}

fn draw_border(buf: &mut TestBuffer, rect: Rect, style: Style, kind: BorderKind) {
    let rect = rect.intersect(buf.area());
    if rect.w < 2 || rect.h < 2 {
        return;
    }

    let right = rect.x.saturating_add(rect.w).saturating_sub(1);
    let bottom = rect.y.saturating_add(rect.h).saturating_sub(1);

    let (tl, tr, bl, br, h, v) = match kind {
        BorderKind::Plain => ('┌', '┐', '└', '┘', '─', '│'),
    };

    // Top/bottom.
    if let Some(cell) = buf.cell_mut(rect.x, rect.y) {
        cell.symbol = tl.to_string();
        cell.style = style;
    }
    if let Some(cell) = buf.cell_mut(right, rect.y) {
        cell.symbol = tr.to_string();
        cell.style = style;
    }
    for x in rect.x.saturating_add(1)..right {
        if let Some(cell) = buf.cell_mut(x, rect.y) {
            cell.symbol = h.to_string();
            cell.style = style;
        }
    }

    if let Some(cell) = buf.cell_mut(rect.x, bottom) {
        cell.symbol = bl.to_string();
        cell.style = style;
    }
    if let Some(cell) = buf.cell_mut(right, bottom) {
        cell.symbol = br.to_string();
        cell.style = style;
    }
    for x in rect.x.saturating_add(1)..right {
        if let Some(cell) = buf.cell_mut(x, bottom) {
            cell.symbol = h.to_string();
            cell.style = style;
        }
    }

    // Left/right.
    for y in rect.y.saturating_add(1)..bottom {
        if let Some(cell) = buf.cell_mut(rect.x, y) {
            cell.symbol = v.to_string();
            cell.style = style;
        }
        if let Some(cell) = buf.cell_mut(right, y) {
            cell.symbol = v.to_string();
            cell.style = style;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/backend/test.rs"]
mod tests;

