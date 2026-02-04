use crate::ui::backend::Backend;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::painter::{BorderKind, PaintCmd};
use crate::ui::core::style::{Color, Mod, Style};
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect as RRect;
use ratatui::style::{Color as RColor, Modifier as RModifier, Style as RStyle};
use ratatui::widgets::Widget;
use ratatui::Frame;
use ratatui::Terminal;
use std::io;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub struct RatatuiBackend<'a, 'f> {
    frame: &'a mut Frame<'f>,
    cursor: Option<Pos>,
}

impl<'a, 'f> RatatuiBackend<'a, 'f> {
    pub fn new(frame: &'a mut Frame<'f>) -> Self {
        Self { frame, cursor: None }
    }
}

impl Drop for RatatuiBackend<'_, '_> {
    fn drop(&mut self) {
        if let Some(pos) = self.cursor {
            // If this method is not called, ratatui hides the cursor for this frame.
            self.frame.set_cursor_position((pos.x, pos.y));
        }
    }
}

impl From<RRect> for Rect {
    fn from(r: RRect) -> Self {
        Rect::new(r.x, r.y, r.width, r.height)
    }
}

impl From<Rect> for RRect {
    fn from(r: Rect) -> Self {
        RRect {
            x: r.x,
            y: r.y,
            width: r.w,
            height: r.h,
        }
    }
}

impl Backend for RatatuiBackend<'_, '_> {
    fn draw(&mut self, area: Rect, cmds: &[PaintCmd]) {
        let widget = PaintWidget { cmds };
        self.frame.render_widget(widget, area.into());
    }

    fn set_cursor(&mut self, pos: Option<Pos>) {
        self.cursor = pos;
    }
}

/// Opaque terminal wrapper so the rest of the crate does not need to reference `ratatui` types.
pub struct RatatuiTerminal {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl RatatuiTerminal {
    pub fn new(stdout: io::Stdout) -> io::Result<Self> {
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    pub fn draw<F>(&mut self, f: F) -> io::Result<()>
    where
        F: FnOnce(&mut dyn Backend, Rect),
    {
        self.terminal.draw(|frame| {
            let area: Rect = frame.area().into();
            let mut backend = RatatuiBackend::new(frame);
            f(&mut backend, area);
        })?;
        Ok(())
    }
}

struct PaintWidget<'a> {
    cmds: &'a [PaintCmd],
}

impl Widget for PaintWidget<'_> {
    fn render(self, _area: RRect, buf: &mut Buffer) {
        for cmd in self.cmds {
            match cmd {
                PaintCmd::FillRect { rect, style } => fill_rect(buf, *rect, *style),
                PaintCmd::StyleRect { rect, style } => style_rect(buf, *rect, *style),
                PaintCmd::HLine { pos, len, ch, style } => draw_hline(buf, *pos, *len, *ch, *style),
                PaintCmd::VLine { pos, len, ch, style } => draw_vline(buf, *pos, *len, *ch, *style),
                PaintCmd::Text {
                    pos,
                    text,
                    style,
                    clip,
                } => draw_text(buf, *pos, text, *style, *clip),
                PaintCmd::Border { rect, style, kind } => draw_border(buf, *rect, *style, *kind),
            }
        }
    }
}

fn to_ratatui_style(s: Style) -> RStyle {
    let mut out = RStyle::default();
    if let Some(fg) = s.fg {
        out = out.fg(to_ratatui_color(fg));
    }
    if let Some(bg) = s.bg {
        out = out.bg(to_ratatui_color(bg));
    }
    out = out.add_modifier(to_ratatui_mods(s.mods));
    out
}

fn to_ratatui_color(c: Color) -> RColor {
    match c {
        Color::Reset => RColor::Reset,
        Color::Rgb(r, g, b) => RColor::Rgb(r, g, b),
        Color::Indexed(i) => RColor::Indexed(i),
    }
}

fn to_ratatui_mods(m: Mod) -> RModifier {
    let mut out = RModifier::empty();
    if m.contains(Mod::BOLD) {
        out |= RModifier::BOLD;
    }
    if m.contains(Mod::DIM) {
        out |= RModifier::DIM;
    }
    if m.contains(Mod::ITALIC) {
        out |= RModifier::ITALIC;
    }
    if m.contains(Mod::UNDERLINE) {
        out |= RModifier::UNDERLINED;
    }
    if m.contains(Mod::REVERSE) {
        out |= RModifier::REVERSED;
    }
    out
}

fn fill_rect(buf: &mut Buffer, rect: Rect, style: Style) {
    if rect.w == 0 || rect.h == 0 {
        return;
    }
    let style = to_ratatui_style(style);
    let right = rect.x.saturating_add(rect.w);
    let bottom = rect.y.saturating_add(rect.h);
    for y in rect.y..bottom {
        for x in rect.x..right {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(' ').set_style(style);
            }
        }
    }
}

fn style_rect(buf: &mut Buffer, rect: Rect, style: Style) {
    if rect.w == 0 || rect.h == 0 {
        return;
    }
    let style = to_ratatui_style(style);
    let right = rect.x.saturating_add(rect.w);
    let bottom = rect.y.saturating_add(rect.h);
    for y in rect.y..bottom {
        for x in rect.x..right {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_style(style);
            }
        }
    }
}

fn draw_hline(buf: &mut Buffer, pos: Pos, len: u16, ch: char, style: Style) {
    if len == 0 {
        return;
    }
    let style = to_ratatui_style(style);
    let y = pos.y;
    let right = pos.x.saturating_add(len);
    for x in pos.x..right {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_char(ch).set_style(style);
        }
    }
}

fn draw_vline(buf: &mut Buffer, pos: Pos, len: u16, ch: char, style: Style) {
    if len == 0 {
        return;
    }
    let style = to_ratatui_style(style);
    let x = pos.x;
    let bottom = pos.y.saturating_add(len);
    for y in pos.y..bottom {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_char(ch).set_style(style);
        }
    }
}

fn draw_text(buf: &mut Buffer, pos: Pos, text: &str, style: Style, clip: Option<Rect>) {
    let style = to_ratatui_style(style);
    // Default clip is the buffer area so we never partially render wide glyphs at the edge.
    let clip = clip.unwrap_or_else(|| Rect::new(buf.area.x, buf.area.y, buf.area.width, buf.area.height));
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

        let Some(cell) = buf.cell_mut((x, y)) else {
            break;
        };
        if g.chars().count() == 1 {
            // Single unicode scalar value.
            if let Some(ch) = g.chars().next() {
                cell.set_char(ch).set_style(style);
            }
        } else {
            // Grapheme cluster (e.g. combining marks).
            cell.set_symbol(g).set_style(style);
        }

        // Basic wide-char handling: occupy next cells as spaces.
        for dx in 1..w {
            let xx = x.saturating_add(dx);
            if !clip.contains(Pos::new(xx, y)) {
                break;
            }
            let Some(cell) = buf.cell_mut((xx, y)) else {
                break;
            };
            cell.set_char(' ').set_style(style);
        }
        x = x.saturating_add(w);
    }
}

fn draw_border(buf: &mut Buffer, rect: Rect, style: Style, kind: BorderKind) {
    if rect.w < 2 || rect.h < 2 {
        return;
    }

    let style = to_ratatui_style(style);
    let right = rect.x.saturating_add(rect.w).saturating_sub(1);
    let bottom = rect.y.saturating_add(rect.h).saturating_sub(1);

    let (tl, tr, bl, br, h, v) = match kind {
        BorderKind::Plain => ('┌', '┐', '└', '┘', '─', '│'),
    };

    // Top/bottom.
    if let Some(cell) = buf.cell_mut((rect.x, rect.y)) {
        cell.set_char(tl).set_style(style);
    }
    if let Some(cell) = buf.cell_mut((right, rect.y)) {
        cell.set_char(tr).set_style(style);
    }
    for x in rect.x.saturating_add(1)..right {
        if let Some(cell) = buf.cell_mut((x, rect.y)) {
            cell.set_char(h).set_style(style);
        }
    }

    if let Some(cell) = buf.cell_mut((rect.x, bottom)) {
        cell.set_char(bl).set_style(style);
    }
    if let Some(cell) = buf.cell_mut((right, bottom)) {
        cell.set_char(br).set_style(style);
    }
    for x in rect.x.saturating_add(1)..right {
        if let Some(cell) = buf.cell_mut((x, bottom)) {
            cell.set_char(h).set_style(style);
        }
    }

    // Left/right.
    for y in rect.y.saturating_add(1)..bottom {
        if let Some(cell) = buf.cell_mut((rect.x, y)) {
            cell.set_char(v).set_style(style);
        }
        if let Some(cell) = buf.cell_mut((right, y)) {
            cell.set_char(v).set_style(style);
        }
    }
}

// Backend-specific tests live under `tests/unit/ui/backend/test.rs` to avoid depending on `ratatui`.
