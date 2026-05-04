use std::env;
use std::fs;
use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;

pub struct LoggerGuard {
    _worker_guard: WorkerGuard,
}

pub fn init() -> Result<LoggerGuard, String> {
    let log_dir = log_dir()?;
    fs::create_dir_all(&log_dir)
        .map_err(|err| format!("failed to create log dir {}: {err}", log_dir.display()))?;

    tracing_log::LogTracer::init().map_err(|err| format!("failed to initialize log bridge: {err}"))?;

    let appender = tracing_appender::rolling::daily(log_dir, "metre.log");
    let (writer, worker_guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_max_level(tracing::Level::INFO)
        .try_init()
        .map_err(|err| format!("failed to initialize tracing subscriber: {err}"))?;

    Ok(LoggerGuard {
        _worker_guard: worker_guard,
    })
}

fn log_dir() -> Result<PathBuf, String> {
    let home = env::var_os("METRE_HOME")
        .ok_or_else(|| "METRE_HOME is not set; cannot resolve ~/.metre/log".to_string())?;
    Ok(PathBuf::from(home).join(".metre").join("log"))
}
