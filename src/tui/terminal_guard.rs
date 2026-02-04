use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub trait TerminalOps: Send + Sync + 'static {
    fn setup(&self) -> io::Result<()>;
    fn restore(&self) -> io::Result<()>;
}

#[derive(Debug, Default)]
pub struct CrosstermTerminalOps;

impl TerminalOps for CrosstermTerminalOps {
    fn setup(&self) -> io::Result<()> {
        use crossterm::{
            cursor,
            event::EnableMouseCapture,
            execute,
            terminal::{enable_raw_mode, EnterAlternateScreen},
        };

        enable_raw_mode()?;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            cursor::SetCursorStyle::BlinkingBar
        )?;
        Ok(())
    }

    fn restore(&self) -> io::Result<()> {
        use crossterm::{
            cursor,
            event::DisableMouseCapture,
            execute,
            terminal::{disable_raw_mode, LeaveAlternateScreen},
        };

        // Best-effort restore: try all steps even if one fails.
        let mut first_err: Option<io::Error> = None;

        if let Err(err) = disable_raw_mode() {
            first_err.get_or_insert(err);
        }
        if let Err(err) = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            cursor::SetCursorStyle::DefaultUserShape
        ) {
            first_err.get_or_insert(err);
        }

        match first_err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

#[derive(Clone)]
pub struct TerminalRestorer {
    restored: Arc<AtomicBool>,
    ops: Arc<dyn TerminalOps>,
}

impl TerminalRestorer {
    pub fn restore(&self) -> io::Result<()> {
        if self.restored.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.ops.restore()
    }
}

pub struct TerminalGuard {
    restorer: TerminalRestorer,
}

impl TerminalGuard {
    pub fn new() -> io::Result<Self> {
        Self::with_ops(Arc::new(CrosstermTerminalOps))
    }

    pub fn with_ops(ops: Arc<dyn TerminalOps>) -> io::Result<Self> {
        ops.setup()?;
        Ok(Self {
            restorer: TerminalRestorer {
                restored: Arc::new(AtomicBool::new(false)),
                ops,
            },
        })
    }

    pub fn restorer(&self) -> TerminalRestorer {
        self.restorer.clone()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restorer.restore();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationSignal {
    SigInt,
    SigTerm,
}

impl TerminationSignal {
    pub fn exit_code(self) -> i32 {
        match self {
            TerminationSignal::SigInt => 130,
            TerminationSignal::SigTerm => 143,
        }
    }
}

#[cfg(unix)]
pub fn install_termination_signals(
    restorer: TerminalRestorer,
    tx: std::sync::mpsc::Sender<TerminationSignal>,
) -> io::Result<std::thread::JoinHandle<()>> {
    use signal_hook::consts::signal::{SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;
    use std::time::Duration;

    let mut signals = Signals::new([SIGINT, SIGTERM])?;
    Ok(std::thread::spawn(move || {
        for sig in signals.forever() {
            let signal = match sig {
                SIGINT => TerminationSignal::SigInt,
                SIGTERM => TerminationSignal::SigTerm,
                _ => continue,
            };

            let _ = tx.send(signal);

            // Grace period: if the main loop is wedged, restore + hard-exit.
            std::thread::sleep(Duration::from_secs(2));
            let _ = restorer.restore();
            std::process::exit(signal.exit_code());
        }
    }))
}

#[cfg(test)]
#[path = "../../tests/unit/tui/terminal_guard.rs"]
mod tests;
