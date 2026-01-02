use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub struct LoggingGuard {
    _guard: WorkerGuard,
    log_dir: PathBuf,
}

impl LoggingGuard {
    pub fn log_dir(&self) -> &std::path::Path {
        &self.log_dir
    }
}

pub fn init() -> Option<LoggingGuard> {
    let log_dir = zcode::services::ensure_log_dir()
        .or_else(|_| -> std::io::Result<PathBuf> {
            let dir = std::env::temp_dir().join("zcode").join("logs");
            std::fs::create_dir_all(&dir)?;
            Ok(dir)
        })
        .ok()?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "zcode.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("zcode=info"));

    let subscriber = tracing_subscriber::registry().with(env_filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
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
    })
}
