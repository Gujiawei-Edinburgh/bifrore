use crate::paths;
use std::fs;
use tracing_appender::non_blocking::WorkerGuard;

pub struct LoggerGuard {
    _worker_guard: WorkerGuard,
}

pub fn init() -> Result<LoggerGuard, String> {
    let log_dir = paths::log_dir()?;
    fs::create_dir_all(&log_dir)
        .map_err(|err| format!("failed to create log dir {}: {err}", log_dir.display()))?;

    let appender = tracing_appender::rolling::daily(log_dir, "tenon.log");
    let (writer, worker_guard) = tracing_appender::non_blocking(appender);
    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|err| format!("failed to initialize tracing subscriber: {err}"))?;
    tracing_log::LogTracer::init().map_err(|err| format!("failed to initialize log bridge: {err}"))?;

    Ok(LoggerGuard {
        _worker_guard: worker_guard,
    })
}
