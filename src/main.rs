//zcode/src/main.rs
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::{EnableMouseCapture, DisableMouseCapture},
    cursor,
};
use ratatui::{
    prelude::*,
};
use std::{io, env, path::Path};

mod editor;
mod file_system;
mod workspace;

use editor::render;
use file_system::build_from_path;
use workspace::Workspace; 

fn main() -> io::Result<()> {
    //以下为测试内容
    let args: Vec<String> = env::args().collect();
    let path_to_open = &args[1];
    //======================

    let file_tree = build_from_path(Path::new(path_to_open))?;
    
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, cursor::SetCursorStyle::BlinkingBar)?; // 启用鼠标捕获并设置光标为闪烁竖线
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut workspace = Workspace::new(file_tree);
    let mut should_quit = false;

    while !should_quit {
        terminal.draw(|f| render(&mut workspace, f))?;
        should_quit = workspace.editor.handle_input()?;
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture, cursor::SetCursorStyle::DefaultUserShape)?; // 禁用鼠标捕获并恢复默认光标
    Ok(())
}
