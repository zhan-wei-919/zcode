use super::*;
use crate::ui::core::geom::{Pos, Rect};
use crate::ui::core::style::{Color, Style};

#[test]
fn draw_text_clips_wide_glyphs_that_do_not_fit() {
    let mut buf = TestBuffer::new(Rect::new(0, 0, 1, 1));
    draw_text(&mut buf, Pos::new(0, 0), "👍", Style::default(), None);
    assert_eq!(buf.cell(0, 0).unwrap().symbol, " ");
}

#[test]
fn draw_text_renders_wide_glyphs_when_they_fit() {
    let mut buf = TestBuffer::new(Rect::new(0, 0, 2, 1));
    draw_text(&mut buf, Pos::new(0, 0), "👍", Style::default(), None);
    assert_eq!(buf.cell(0, 0).unwrap().symbol, "👍");
    assert_eq!(buf.cell(1, 0).unwrap().symbol, " ");
}

#[test]
fn fill_rect_clips_to_buffer_area() {
    let mut buf = TestBuffer::new(Rect::new(0, 0, 2, 2));
    buf.cell_mut(0, 0).unwrap().symbol = "A".to_string();
    fill_rect(&mut buf, Rect::new(0, 0, 10, 10), Style::default());
    assert_eq!(buf.cell(0, 0).unwrap().symbol, " ");
}

#[test]
fn style_rect_preserves_existing_symbols() {
    let mut buf = TestBuffer::new(Rect::new(0, 0, 1, 1));
    buf.cell_mut(0, 0).unwrap().symbol = "A".to_string();
    let style = Style::default().fg(Color::Indexed(2));
    style_rect(&mut buf, Rect::new(0, 0, 1, 1), style);
    let cell = buf.cell(0, 0).unwrap();
    assert_eq!(cell.symbol, "A");
    assert_eq!(cell.style, style);
}

#[test]
fn draw_hline_writes_characters() {
    let mut buf = TestBuffer::new(Rect::new(0, 0, 3, 1));
    draw_hline(&mut buf, Pos::new(0, 0), 3, '-', Style::default());
    assert_eq!(buf.cell(0, 0).unwrap().symbol, "-");
    assert_eq!(buf.cell(1, 0).unwrap().symbol, "-");
    assert_eq!(buf.cell(2, 0).unwrap().symbol, "-");
}

#[test]
fn draw_vline_writes_characters() {
    let mut buf = TestBuffer::new(Rect::new(0, 0, 1, 3));
    draw_vline(&mut buf, Pos::new(0, 0), 3, '|', Style::default());
    assert_eq!(buf.cell(0, 0).unwrap().symbol, "|");
    assert_eq!(buf.cell(0, 1).unwrap().symbol, "|");
    assert_eq!(buf.cell(0, 2).unwrap().symbol, "|");
}
