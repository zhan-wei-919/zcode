//! 视图系统：View trait 定义
//!
//! 所有可渲染、可交互的视图组件都实现此 trait

use ratatui::layout::Rect;
use ratatui::Frame;
use std::path::PathBuf;
use super::event::InputEvent;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveArea {
    Explorer,
    Editor,
}

impl Default for ActiveArea {
    fn default() -> Self {
        ActiveArea::Editor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_result() {
        assert!(EventResult::Consumed.is_consumed());
        assert!(EventResult::Ignored.is_ignored());
        assert!(EventResult::Quit.is_quit());
    }

    #[test]
    fn test_active_area_default() {
        let area = ActiveArea::default();
        assert_eq!(area, ActiveArea::Editor);
    }
}
