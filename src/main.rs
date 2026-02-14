use crossterm::event::{self, Event};
use std::os::unix::io::RawFd;
use std::{
    env, io,
    path::Path,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

mod logging;

use zcode::app::Workbench;
use zcode::core::event::InputEvent;
use zcode::kernel::services::adapters::AsyncRuntime;
use zcode::tui::view::{EventResult, View};
use zcode::tui::{self, terminal_guard::TerminationSignal};
use zcode::ui::backend::terminal::RatatuiTerminal;

#[derive(Debug)]
struct StartupPaths {
    root: PathBuf,
    open_file: Option<PathBuf>,
}

fn main() -> io::Result<()> {
    let mut logging_guard = logging::init();
    if cfg!(debug_assertions) {
        if let Some(guard) = &logging_guard {
            eprintln!("Log dir: {}", guard.log_dir().display());
        }
    }

    if !env_truthy("ZCODE_DISABLE_SETTINGS") {
        if let Ok(path) = zcode::kernel::services::adapters::ensure_settings_file() {
            tracing::info!(settings_path = %path.display(), "settings ready");
        }
    }

    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("Usage: zcode [path]\n\nIf no path is provided, zcode opens the current directory.\nThe path can be a directory or a file.");
        return Ok(());
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("zcode {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.len() > 1 {
        eprintln!("error: too many arguments\n\nUsage: zcode [path]");
        std::process::exit(2);
    }

    let cwd = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("error: cannot determine current directory: {e}");
        std::process::exit(1);
    });

    let startup =
        resolve_startup_paths(&cwd, args.first().map(String::as_str)).unwrap_or_else(|e| {
            eprintln!("error: invalid path: {e}");
            std::process::exit(1);
        });

    tracing::info!(
        root = %startup.root.display(),
        open_file = ?startup.open_file,
        "starting"
    );

    let guard = tui::terminal_guard::TerminalGuard::new()
        .inspect_err(|e| tracing::error!(error = ?e, "terminal setup failed"))?;
    let restorer = guard.restorer();

    {
        let prev = std::panic::take_hook();
        let restorer = restorer.clone();
        std::panic::set_hook(Box::new(move |info| {
            let _ = restorer.restore();
            prev(info);
        }));
    }

    let (term_tx, term_rx) = mpsc::channel::<TerminationSignal>();
    #[cfg(unix)]
    {
        let _ = tui::terminal_guard::install_termination_signals(restorer.clone(), term_tx);
    }
    #[cfg(not(unix))]
    {
        let _ = term_tx;
    }

    let mut terminal = RatatuiTerminal::new(io::stdout())
        .inspect_err(|e| tracing::error!(error = ?e, "terminal init failed"))?;
    let log_rx = logging_guard.as_mut().and_then(|guard| guard.take_log_rx());
    let result = run_app(
        &mut terminal,
        startup.root.as_path(),
        startup.open_file,
        log_rx,
        &term_rx,
    );
    drop(terminal);
    let _ = restorer
        .restore()
        .inspect_err(|e| tracing::error!(error = ?e, "terminal restore failed"));
    drop(guard);
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

fn env_truthy(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

fn run_app(
    terminal: &mut RatatuiTerminal,
    path: &Path,
    startup_file: Option<PathBuf>,
    log_rx: Option<mpsc::Receiver<String>>,
    term_rx: &mpsc::Receiver<TerminationSignal>,
) -> io::Result<()> {
    let mut root_path = path.to_path_buf();
    let (mut tx, mut rx) = mpsc::channel();

    let (wakeup_tx, wakeup_rx) = zcode::core::wakeup::wakeup_pipe()?;
    let mut workbench = Workbench::new(
        root_path.as_path(),
        AsyncRuntime::new(tx.clone())?,
        log_rx,
        Some(wakeup_tx.clone()),
    )?;
    if let Some(path) = startup_file {
        workbench.runtime().load_file(path);
    }

    let mut dirty = true;
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(50);

    'app: loop {
        if let Ok(signal) = term_rx.try_recv() {
            tracing::info!(?signal, "termination signal received");
            return Ok(());
        }

        if dirty {
            terminal.draw(|backend, area| workbench.render(backend, area))?;
            dirty = workbench.flush_post_render_sync();
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        let result = poll_events(timeout, wakeup_rx.raw_fd())?;

        // Always drain crossterm buffered events (handles SIGWINCH → Resize).
        while event::poll(Duration::ZERO)? {
            let event = event::read()?;
            let events = drain_pending_events(event);

            for ev in events {
                let input_event: InputEvent = zcode::tui::crossterm::into_input_event(ev);
                match workbench.handle_input(&input_event) {
                    EventResult::Quit => return Ok(()),
                    EventResult::Restart { path, hard } => {
                        let log_rx = workbench.take_log_rx();
                        root_path = path;
                        let (new_tx, new_rx) = mpsc::channel();
                        tx = new_tx;
                        rx = new_rx;
                        workbench = Workbench::new(
                            root_path.as_path(),
                            AsyncRuntime::new(tx.clone())?,
                            log_rx,
                            Some(wakeup_tx.clone()),
                        )?;
                        dirty = true;
                        last_tick = Instant::now();
                        if hard {
                            tracing::info!("hard reload completed");
                        }
                        continue 'app;
                    }
                    EventResult::Ignored => {}
                    _ => dirty = true,
                }
            }
        }

        // Kernel bus: drain if wakeup signalled.
        if result.wakeup_ready {
            wakeup_rx.drain();
            if workbench.poll_kernel_bus() {
                dirty = true;
            }
        }

        while let Ok(msg) = rx.try_recv() {
            workbench.handle_message(msg);
            dirty = true;
            if let Some((path, hard)) = workbench.take_pending_restart() {
                let log_rx = workbench.take_log_rx();
                root_path = path;
                let (new_tx, new_rx) = mpsc::channel();
                tx = new_tx;
                rx = new_rx;
                workbench = Workbench::new(
                    root_path.as_path(),
                    AsyncRuntime::new(tx.clone())?,
                    log_rx,
                    Some(wakeup_tx.clone()),
                )?;
                dirty = true;
                last_tick = Instant::now();
                if hard {
                    tracing::info!("hard reload completed");
                }
                continue 'app;
            }
        }

        // Tick (no longer includes poll_kernel_bus).
        if last_tick.elapsed() >= tick_rate {
            let tick_start = Instant::now();
            if workbench.tick() {
                dirty = true;
            }
            let tick_elapsed = tick_start.elapsed();
            if tick_elapsed.as_millis() > 5 {
                tracing::debug!(elapsed_ms = tick_elapsed.as_millis() as u64, "tick() slow");
            }
            last_tick = Instant::now();
        }
    }
}

struct PollResult {
    wakeup_ready: bool,
}

/// Use `libc::poll()` to simultaneously wait on stdin and the wakeup pipe fd.
/// Returns which fds are ready. On EINTR, returns both false (caller loops).
fn poll_events(timeout: Duration, wakeup_fd: RawFd) -> io::Result<PollResult> {
    poll_events_with(timeout, wakeup_fd, |fds, timeout_ms| {
        // SAFETY: fds is a valid array of pollfd structs, nfds=2, timeout_ms is valid.
        let ret = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout_ms) };

        if ret < 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                fds[0].revents = 0;
                fds[1].revents = 0;
                return Ok(());
            }
            return Err(err);
        }

        Ok(())
    })
}

fn poll_events_with<F>(timeout: Duration, wakeup_fd: RawFd, poller: F) -> io::Result<PollResult>
where
    F: FnOnce(&mut [libc::pollfd; 2], i32) -> io::Result<()>,
{
    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    let mut fds = [
        libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: wakeup_fd,
            events: libc::POLLIN,
            revents: 0,
        },
    ];

    poller(&mut fds, timeout_ms)?;

    let wake_revents = fds[1].revents;
    if wake_revents & libc::POLLNVAL != 0 {
        return Err(io::Error::from_raw_os_error(libc::EBADF));
    }
    if wake_revents & libc::POLLERR != 0 {
        return Err(io::Error::other("wakeup fd reported POLLERR"));
    }
    if wake_revents & libc::POLLHUP != 0 {
        return Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "wakeup pipe disconnected",
        ));
    }

    Ok(PollResult {
        wakeup_ready: wake_revents & libc::POLLIN != 0,
    })
}

fn resolve_startup_paths(cwd: &Path, arg: Option<&str>) -> io::Result<StartupPaths> {
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let cwd_for_default = cwd.clone();

    let path = match arg {
        None => cwd_for_default,
        Some(raw) => {
            let p = PathBuf::from(raw);
            if p.is_absolute() {
                p
            } else {
                cwd.join(p)
            }
        }
    };

    let meta = std::fs::metadata(&path)?;
    if meta.is_dir() {
        let root = path.canonicalize().unwrap_or(path);
        Ok(StartupPaths {
            root,
            open_file: None,
        })
    } else {
        let file = path.canonicalize().unwrap_or(path);
        let root = if file.starts_with(&cwd) {
            cwd
        } else {
            file.parent()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "file path has no parent directory",
                    )
                })?
                .to_path_buf()
        };
        Ok(StartupPaths {
            root,
            open_file: Some(file),
        })
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

#[cfg(test)]
#[path = "../tests/unit/cli_startup_paths.rs"]
mod tests;

/// 合并连续的滚轮事件，只保留最后一个方向的累计效果
fn coalesce_scroll_events(events: Vec<Event>) -> Vec<Event> {
    use crossterm::event::{MouseEvent, MouseEventKind};

    let mut result = Vec::new();
    let mut scroll_up_count: i32 = 0;
    let mut scroll_down_count: i32 = 0;
    let mut last_scroll_event: Option<MouseEvent> = None;
    let mut scroll_modifiers: Option<crossterm::event::KeyModifiers> = None;

    for ev in events {
        match &ev {
            Event::Mouse(mouse_ev) => match mouse_ev.kind {
                MouseEventKind::ScrollUp => {
                    if scroll_modifiers.is_some_and(|mods| mods != mouse_ev.modifiers) {
                        flush_scroll_events(
                            &mut result,
                            &mut scroll_up_count,
                            &mut scroll_down_count,
                            &last_scroll_event,
                        );
                    }
                    scroll_modifiers = Some(mouse_ev.modifiers);
                    scroll_up_count += 1;
                    last_scroll_event = Some(*mouse_ev);
                }
                MouseEventKind::ScrollDown => {
                    if scroll_modifiers.is_some_and(|mods| mods != mouse_ev.modifiers) {
                        flush_scroll_events(
                            &mut result,
                            &mut scroll_up_count,
                            &mut scroll_down_count,
                            &last_scroll_event,
                        );
                    }
                    scroll_modifiers = Some(mouse_ev.modifiers);
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
                    scroll_modifiers = None;
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
                scroll_modifiers = None;
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
