//! Per-pane mouse state machine for click granularity detection.
//!
//! Tracks double-click / triple-click using screen coordinates and timestamps,
//! keeping this input-device concern out of the kernel.

use crate::models::Granularity;
use std::time::Instant;

#[derive(Debug)]
pub(crate) struct EditorMouseTracker {
    last_click: Option<(u16, u16, Instant)>,
    click_count: u8,
    dragging: bool,
}

impl EditorMouseTracker {
    pub fn new() -> Self {
        Self {
            last_click: None,
            click_count: 0,
            dragging: false,
        }
    }

    /// Register a click at screen coordinates, returning the detected granularity.
    pub fn click(
        &mut self,
        x: u16,
        y: u16,
        now: Instant,
        slop: u16,
        triple_click_ms: u64,
    ) -> Granularity {
        if let Some((lx, ly, lt)) = self.last_click {
            let dx = (x as i32 - lx as i32).abs();
            let dy = (y as i32 - ly as i32).abs();
            let dt = now.duration_since(lt).as_millis() as u64;

            if dx <= slop as i32 && dy <= slop as i32 && dt < triple_click_ms {
                self.click_count = (self.click_count % 3) + 1;
            } else {
                self.click_count = 1;
            }
        } else {
            self.click_count = 1;
        }

        self.last_click = Some((x, y, now));
        self.dragging = true;

        match self.click_count {
            1 => Granularity::Char,
            2 => Granularity::Word,
            _ => Granularity::Line,
        }
    }

    pub fn dragging(&self) -> bool {
        self.dragging
    }

    pub fn stop_drag(&mut self) {
        self.dragging = false;
    }
}
