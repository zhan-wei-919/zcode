use super::geom::{Pos, Rect};
use super::style::Style;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderKind {
    Plain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaintCmd {
    FillRect { rect: Rect, style: Style },
    /// Apply a style to the existing buffer cells without changing their symbols.
    StyleRect { rect: Rect, style: Style },
    HLine {
        pos: Pos,
        len: u16,
        ch: char,
        style: Style,
    },
    VLine {
        pos: Pos,
        len: u16,
        ch: char,
        style: Style,
    },
    Text {
        pos: Pos,
        text: String,
        style: Style,
        clip: Option<Rect>,
    },
    Border {
        rect: Rect,
        style: Style,
        kind: BorderKind,
    },
}

#[derive(Debug, Default)]
pub struct Painter {
    cmds: Vec<PaintCmd>,
}

impl Painter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.cmds.clear();
    }

    pub fn cmds(&self) -> &[PaintCmd] {
        &self.cmds
    }

    pub fn push(&mut self, cmd: PaintCmd) {
        self.cmds.push(cmd);
    }

    pub fn fill_rect(&mut self, rect: Rect, style: Style) {
        self.cmds.push(PaintCmd::FillRect { rect, style });
    }

    pub fn style_rect(&mut self, rect: Rect, style: Style) {
        self.cmds.push(PaintCmd::StyleRect { rect, style });
    }

    pub fn hline(&mut self, pos: Pos, len: u16, ch: char, style: Style) {
        self.cmds.push(PaintCmd::HLine { pos, len, ch, style });
    }

    pub fn vline(&mut self, pos: Pos, len: u16, ch: char, style: Style) {
        self.cmds.push(PaintCmd::VLine { pos, len, ch, style });
    }

    pub fn text(&mut self, pos: Pos, text: impl Into<String>, style: Style) {
        self.cmds.push(PaintCmd::Text {
            pos,
            text: text.into(),
            style,
            clip: None,
        });
    }

    pub fn text_clipped(
        &mut self,
        pos: Pos,
        text: impl Into<String>,
        style: Style,
        clip: Rect,
    ) {
        self.cmds.push(PaintCmd::Text {
            pos,
            text: text.into(),
            style,
            clip: Some(clip),
        });
    }

    pub fn border(&mut self, rect: Rect, style: Style, kind: BorderKind) {
        self.cmds.push(PaintCmd::Border { rect, style, kind });
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/core/painter.rs"]
mod tests;
