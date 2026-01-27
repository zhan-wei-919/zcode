use crate::core::event::InputEvent;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::path::PathBuf;

pub trait View {
    fn handle_input(&mut self, event: &InputEvent) -> EventResult;

    fn render(&mut self, frame: &mut Frame, area: Rect);

    fn focusable(&self) -> bool {
        true
    }

    fn on_focus(&mut self) {}

    fn on_blur(&mut self) {}

    fn cursor_position(&self) -> Option<(u16, u16)> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventResult {
    Consumed,
    Ignored,
    Quit,
    /// 请求打开当前选中的文件
    OpenFile,
    /// 请求异步加载目录内容
    LoadDir(PathBuf),
}

impl EventResult {
    pub fn is_consumed(&self) -> bool {
        matches!(self, EventResult::Consumed)
    }

    pub fn is_ignored(&self) -> bool {
        matches!(self, EventResult::Ignored)
    }

    pub fn is_quit(&self) -> bool {
        matches!(self, EventResult::Quit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveArea {
    Explorer,
    #[default]
    Editor,
    GlobalSearch,
}

#[cfg(test)]
#[path = "../../tests/unit/tui/view.rs"]
mod tests;
