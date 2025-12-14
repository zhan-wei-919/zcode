//zcode/src/editor/input/input.rs
//! 输入处理：键盘和鼠标事件的顶层入口
//! 
//! 架构：
//! - handle_input: 主入口，使用 Keybindings 系统
//! - handle_mouse: 鼠标事件处理
//! - execute_command: 命令执行（在 executor.rs 中实现）

use crate::editor::core::Editor;
use super::selection::{Selection, Granularity};
use super::command::{Command, Key};
use std::io;
use std::time::Instant;
use crossterm::event::{self, Event, KeyCode, KeyEvent, MouseEvent, MouseEventKind, MouseButton};

impl Editor {
    /// 主输入处理函数（使用命令系统）
    /// 
    /// 架构：
    /// 1. 读取事件（键盘/鼠标）
    /// 2. 将按键转换为 Key
    /// 3. 通过 Keybindings 查找对应的 Command
    /// 4. 执行命令（通过 execute_command）
    /// 
    pub fn handle_input(&mut self) -> io::Result<bool> {
        match event::read()? {
            Event::Key(key_event) => {
                self.handle_key_event(key_event)
            }
            Event::Mouse(mouse) => {
                self.handle_mouse(mouse)?;
                Ok(false)
            }
            _ => Ok(false)
        }
    }
    
    /// 处理键盘事件（通过键位绑定系统）
    fn handle_key_event(&mut self, key_event: KeyEvent) -> io::Result<bool> {
        let key = Key::new(key_event.code, key_event.modifiers);
        
        // 先查找键位绑定
        if let Some(command) = self.keybindings.get(&key).cloned() {
            // 找到绑定的命令，执行它
            self.execute_command(&command);
            
            // 特殊处理退出命令
            if matches!(command, Command::Quit) {
                return Ok(true);
            }
            
            return Ok(false);
        }
        
        // 没有绑定的按键，尝试处理普通字符输入
        if let KeyCode::Char(c) = key_event.code {
            if key_event.modifiers.is_empty() || key_event.modifiers == crossterm::event::KeyModifiers::SHIFT {
                // 无修饰键或只有 Shift 的字符输入
                self.execute_command(&Command::InsertChar(c));
            }
        }
        
        Ok(false)
    }
    
    /// 处理鼠标事件
    fn handle_mouse(&mut self, mouse: MouseEvent) -> io::Result<()> {
        // 获取编辑器区域（用于坐标转换）
        let editor_area = match self.view.editor_area() {
            Some(area) => area,
            None => return Ok(()), // 还没有渲染过，忽略鼠标事件
        };
        
        // 转换鼠标坐标到编辑器内部坐标
        let x = mouse.column.saturating_sub(editor_area.x);
        let y = mouse.row.saturating_sub(editor_area.y);
        
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.on_mouse_down(x, y, editor_area);
            }
            
            MouseEventKind::Drag(MouseButton::Left) => {
                self.on_mouse_drag(x, y, editor_area);
            }
            
            MouseEventKind::Up(MouseButton::Left) => {
                self.mouse_controller.on_mouse_up();
            }
            
            MouseEventKind::ScrollDown => {
                let step = self.config.scroll_step(self.view.viewport_height());
                self.view.scroll_vertical(step as isize, self.model.len_lines());
            }
            
            MouseEventKind::ScrollUp => {
                let step = self.config.scroll_step(self.view.viewport_height());
                self.view.scroll_vertical(-(step as isize), self.model.len_lines());
            }
            
            _ => {}
        }
        
        Ok(())
    }
    
    /// 鼠标按下
    fn on_mouse_down(&mut self, x: u16, y: u16, _area: ratatui::layout::Rect) {
        let granularity = self.mouse_controller.on_mouse_down(x, y, Instant::now());
        
        if let Some(pos) = self.view.screen_to_pos(x, y, &self.model) {
            // 移动光标
            self.model.set_cursor(pos.0, pos.1);
            
            // 创建新选区
            let mut selection = Selection::new(pos, granularity);
            
            // 对于双击/三击，立即扩展选区
            if granularity != Granularity::Char {
                selection.update_cursor(pos, self.model.rope());
            }
            
            self.model.set_selection(Some(selection));
        }
    }
    
    /// 鼠标拖拽
    fn on_mouse_drag(&mut self, x: u16, y: u16, _area: ratatui::layout::Rect) {
        if !self.mouse_controller.is_dragging() {
            return;
        }
        
        if let Some(pos) = self.view.screen_to_pos(x, y, &self.model) {
            // 更新选区
            self.model.update_selection_cursor(pos);
            
            // 移动光标
            self.model.set_cursor(pos.0, pos.1);
            self.ensure_cursor_visible();
        }
    }
}
