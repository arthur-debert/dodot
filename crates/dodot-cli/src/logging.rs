//! Logging setup — file + optional stdout subscriber.
//!
//! By default, dodot logs to a daily-rotating file under
//! `~/.cache/dodot/logs/`. The `--verbose` and `--debug` flags
//! additionally enable stdout output at the respective level.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use tracing_appender::rolling;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;

/// Verbosity level requested via CLI flags.
pub enum Verbosity {
    /// No stdout logging (file only).
    Quiet,
    /// INFO and above to stdout.
    Verbose,
    /// DEBUG and above to stdout.
    Debug,
}

/// Initialize the tracing subscriber.
///
/// Always logs to a daily-rotating file. Optionally also logs to
/// stdout based on `verbosity`.
pub fn init(log_dir: &Path, verbosity: Verbosity) {
    // Ensure the log directory exists
    let _ = fs::create_dir_all(log_dir);

    // File layer: always active, DEBUG level (captures everything)
    let file_appender = rolling::daily(log_dir, "dodot.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Leak the guard so the writer lives for the process lifetime.
    // This is fine for a CLI that runs and exits.
    std::mem::forget(_guard);

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true);

    let file_filter = EnvFilter::new("dodot_lib=debug,dodot=debug");

    match verbosity {
        Verbosity::Quiet => {
            tracing_subscriber::registry()
                .with(file_layer.with_filter(file_filter))
                .init();
        }
        Verbosity::Verbose => {
            let stdout_layer = fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false)
                .compact();
            let stdout_filter = EnvFilter::new("dodot_lib=info,dodot=info");

            tracing_subscriber::registry()
                .with(file_layer.with_filter(file_filter))
                .with(stdout_layer.with_filter(stdout_filter))
                .init();
        }
        Verbosity::Debug => {
            let stdout_layer = fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(true)
                .compact();
            let stdout_filter = EnvFilter::new("dodot_lib=debug,dodot=debug");

            tracing_subscriber::registry()
                .with(file_layer.with_filter(file_filter))
                .with(stdout_layer.with_filter(stdout_filter))
                .init();
        }
    }

    cleanup_old_logs(log_dir, 7);
}

/// Remove log files older than `max_age_days` days.
fn cleanup_old_logs(log_dir: &Path, max_age_days: u64) {
    let cutoff = SystemTime::now() - std::time::Duration::from_secs(max_age_days * 24 * 60 * 60);

    let entries = match fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Only clean up dodot log files
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.starts_with("dodot.log") => n,
            _ => continue,
        };

        // Don't remove the base file name (current day's log)
        if name == "dodot.log" {
            continue;
        }

        if let Ok(meta) = fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if modified < cutoff {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
}
