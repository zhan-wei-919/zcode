use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub struct LoggingGuard {
    _guard: WorkerGuard,
    log_dir: PathBuf,
    log_rx: Option<Receiver<String>>,
}

impl LoggingGuard {
    pub fn log_dir(&self) -> &std::path::Path {
        &self.log_dir
    }

    pub fn take_log_rx(&mut self) -> Option<Receiver<String>> {
        self.log_rx.take()
    }
}

struct UiLogWriter {
    buf: Vec<u8>,
    tx: Sender<String>,
}

impl UiLogWriter {
    fn new(tx: Sender<String>) -> Self {
        Self {
            buf: Vec::with_capacity(256),
            tx,
        }
    }
}

impl Write for UiLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for UiLogWriter {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }

        let text = String::from_utf8_lossy(&self.buf);
        for line in text.lines() {
            let _ = self.tx.send(line.to_string());
        }
    }
}

#[derive(Clone)]
struct TeeMakeWriter {
    file: NonBlocking,
    tx: Sender<String>,
}

struct TeeWriter {
    file: NonBlocking,
    ui: UiLogWriter,
}

impl<'a> MakeWriter<'a> for TeeMakeWriter {
    type Writer = TeeWriter;

    fn make_writer(&'a self) -> Self::Writer {
        TeeWriter {
            file: self.file.make_writer(),
            ui: UiLogWriter::new(self.tx.clone()),
        }
    }
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.file.write(buf)?;
        let _ = self.ui.write_all(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.ui.flush()?;
        Ok(())
    }
}

pub fn init() -> Option<LoggingGuard> {
    let log_dir = zcode::kernel::services::adapters::ensure_log_dir()
        .or_else(|_| -> std::io::Result<PathBuf> {
            let dir = std::env::temp_dir().join("zcode").join("logs");
            std::fs::create_dir_all(&dir)?;
            Ok(dir)
        })
        .ok()?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "zcode.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let (log_tx, log_rx) = mpsc::channel::<String>();
    let writer = TeeMakeWriter {
        file: non_blocking,
        tx: log_tx,
    };

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("zcode=info"));

    let subscriber = tracing_subscriber::registry().with(env_filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .with_target(true)
            .with_file(true)
            .with_line_number(true),
    );

    if subscriber.try_init().is_err() {
        return None;
    }

    std::panic::set_hook(Box::new(|panic_info| {
        tracing::error!(panic = %panic_info, "panic");
    }));

    tracing::info!(log_dir = %log_dir.display(), "tracing initialized");

    Some(LoggingGuard {
        _guard: guard,
        log_dir,
        log_rx: Some(log_rx),
    })
}
