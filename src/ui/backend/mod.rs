//! Rendering backends.
//!
//! Stage 1 keeps only a ratatui backend, but the trait helps isolate the rest
//! of the codebase from `ratatui` types.

use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::PaintCmd;

pub trait Backend {
    fn draw(&mut self, area: Rect, cmds: &[PaintCmd]);

    fn set_cursor(&mut self, pos: Option<Pos>);
}

// The concrete terminal backend lives in `ratatui.rs`, but we keep the module name generic so the
// rest of the codebase does not need to mention ratatui.
#[path = "ratatui.rs"]
pub mod terminal;
pub mod test;
