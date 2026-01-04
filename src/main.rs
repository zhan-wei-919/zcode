//! zcode - TUI 文本编辑器

use crossterm::{
    cursor,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::{
    env, io,
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};

mod logging;

use zcode::app::Workbench;
use zcode::core::event::InputEvent;
use zcode::core::view::{EventResult, View};
use zcode::kernel::services::adapters::AsyncRuntime;

fn main() -> io::Result<()> {
    let mut logging_guard = logging::init();
    if cfg!(debug_assertions) {
        if let Some(guard) = &logging_guard {
            eprintln!("Log dir: {}", guard.log_dir().display());
        }
    }

    if let Ok(path) = zcode::kernel::services::adapters::ensure_settings_file() {
        tracing::info!(settings_path = %path.display(), "settings ready");
    }

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <path>", args[0]);
        std::process::exit(1);
    }
    let path_to_open = Path::new(&args[1]);

    tracing::info!(path = %path_to_open.display(), "starting");

    enable_raw_mode().inspect_err(|e| tracing::error!(error = ?e, "enable_raw_mode failed"))?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        cursor::SetCursorStyle::BlinkingBar
    )
    .inspect_err(|e| tracing::error!(error = ?e, "enter alternate screen failed"))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .inspect_err(|e| tracing::error!(error = ?e, "terminal init failed"))?;

    let log_rx = logging_guard.as_mut().and_then(|guard| guard.take_log_rx());
    let result = run_app(&mut terminal, path_to_open, log_rx);

    disable_raw_mode().inspect_err(|e| tracing::error!(error = ?e, "disable_raw_mode failed"))?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        cursor::SetCursorStyle::DefaultUserShape
    )
    .inspect_err(|e| tracing::error!(error = ?e, "leave alternate screen failed"))?;

    if let Err(e) = result {
        tracing::error!(error = ?e, "application error");
        if let Some(guard) = &logging_guard {
            eprintln!("Log dir: {}", guard.log_dir().display());
        }
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    path: &Path,
    log_rx: Option<mpsc::Receiver<String>>,
) -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    let async_runtime = AsyncRuntime::new(tx);
    let mut workbench = Workbench::new(path, async_runtime, log_rx)?;

    let mut dirty = true;
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(50);

    loop {
        if dirty {
            terminal.draw(|frame| {
                workbench.render(frame, frame.area());
            })?;
            dirty = false;
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            let event = event::read()?;
            let events = drain_pending_events(event);

            for ev in events {
                let input_event: InputEvent = ev.into();
                match workbench.handle_input(&input_event) {
                    EventResult::Quit => return Ok(()),
                    EventResult::Ignored => {}
                    _ => dirty = true,
                }
            }
        }

        while let Ok(msg) = rx.try_recv() {
            workbench.handle_message(msg);
            dirty = true;
        }

        // 定时检查是否需要刷盘
        if last_tick.elapsed() >= tick_rate {
            if workbench.tick() {
                dirty = true;
            }
            last_tick = Instant::now();
        }
    }
}

/// 从事件队列中取出所有待处理事件，合并连续的滚轮事件
fn drain_pending_events(first: Event) -> Vec<Event> {
    let mut events = vec![first];

    // 非阻塞地读取队列中的所有事件
    while event::poll(Duration::ZERO).unwrap_or(false) {
        if let Ok(ev) = event::read() {
            events.push(ev);
        }
    }

    // 合并连续的滚轮事件
    coalesce_scroll_events(events)
}

/// 合并连续的滚轮事件，只保留最后一个方向的累计效果
fn coalesce_scroll_events(events: Vec<Event>) -> Vec<Event> {
    use crossterm::event::{MouseEvent, MouseEventKind};

    let mut result = Vec::new();
    let mut scroll_up_count: i32 = 0;
    let mut scroll_down_count: i32 = 0;
    let mut last_scroll_event: Option<MouseEvent> = None;

    for ev in events {
        match &ev {
            Event::Mouse(mouse_ev) => match mouse_ev.kind {
                MouseEventKind::ScrollUp => {
                    scroll_up_count += 1;
                    last_scroll_event = Some(*mouse_ev);
                }
                MouseEventKind::ScrollDown => {
                    scroll_down_count += 1;
                    last_scroll_event = Some(*mouse_ev);
                }
                _ => {
                    // 遇到非滚轮事件，先 flush 累积的滚轮事件
                    flush_scroll_events(
                        &mut result,
                        &mut scroll_up_count,
                        &mut scroll_down_count,
                        &last_scroll_event,
                    );
                    result.push(ev);
                }
            },
            _ => {
                // 非鼠标事件，先 flush 累积的滚轮事件
                flush_scroll_events(
                    &mut result,
                    &mut scroll_up_count,
                    &mut scroll_down_count,
                    &last_scroll_event,
                );
                result.push(ev);
            }
        }
    }

    // 处理剩余的滚轮事件
    flush_scroll_events(
        &mut result,
        &mut scroll_up_count,
        &mut scroll_down_count,
        &last_scroll_event,
    );

    result
}

fn flush_scroll_events(
    result: &mut Vec<Event>,
    scroll_up_count: &mut i32,
    scroll_down_count: &mut i32,
    last_scroll_event: &Option<crossterm::event::MouseEvent>,
) {
    use crossterm::event::{MouseEvent, MouseEventKind};

    let net_scroll = *scroll_down_count - *scroll_up_count;

    if net_scroll != 0 {
        if let Some(base_event) = last_scroll_event {
            let kind = if net_scroll > 0 {
                MouseEventKind::ScrollDown
            } else {
                MouseEventKind::ScrollUp
            };

            // 生成合并后的滚轮事件（只生成一个，但滚动步长会在 viewport 中处理）
            // 这里我们生成 |net_scroll| 个事件，让滚动量正确
            let count = net_scroll.unsigned_abs() as usize;
            for _ in 0..count {
                result.push(Event::Mouse(MouseEvent {
                    kind,
                    column: base_event.column,
                    row: base_event.row,
                    modifiers: base_event.modifiers,
                }));
            }
        }
    }

    *scroll_up_count = 0;
    *scroll_down_count = 0;
}
