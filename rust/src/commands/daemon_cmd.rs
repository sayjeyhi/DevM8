use std::sync::Arc;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::config::loader::load_config;
use crate::daemon::pid::{remove_pid, write_pid};
use crate::daemon::restart_tracker::RestartTracker;
use crate::logger::{self, Level, OutputMode, append_to_log_file};
use crate::logger::rotate::rotate_if_needed;
use crate::shared::errors::{AppError, FriendlyError};
use crate::shared::paths::PATHS;

// Current binary version — overridden by build script or env.
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn daemon_command() -> Result<(), AppError> {
    // ------------------------------------------------------------------
    // Load configuration (exit 1 on error)
    // ------------------------------------------------------------------
    let config = load_config(None).map_err(|e| {
        if let AppError::ConfigMissing(_) = &e {
            eprintln!("devm8: config not found. Run `devm8 config` to set up.");
        } else {
            eprintln!("devm8: failed to load config: {e}");
        }
        e
    })?;

    let paths = &*PATHS;

    // ------------------------------------------------------------------
    // Create structured JSON logger → log file
    // ------------------------------------------------------------------
    let log_level = match config.app.log_level {
        crate::config::schema::LogLevel::Debug => Level::Debug,
        crate::config::schema::LogLevel::Info  => Level::Info,
        crate::config::schema::LogLevel::Error => Level::Error,
    };

    let logger: Arc<dyn crate::logger::Logger> = Arc::new(logger::create_logger(
        log_level,
        Some(OutputMode::Json),
        Some(&paths.log_file),
    ));

    // ------------------------------------------------------------------
    // Restart tracker
    // ------------------------------------------------------------------
    let tracker = RestartTracker::default();

    // ------------------------------------------------------------------
    // Rotate log if needed, then set up an hourly rotation task
    // ------------------------------------------------------------------
    if let Err(e) = rotate_if_needed(&paths.log_file, None, None) {
        logger.warn(&format!("Log rotation failed: {e}"), None);
    }

    let log_file_clone = paths.log_file.clone();
    let logger_clone = Arc::clone(&logger);
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(3600));
        interval.tick().await; // skip the immediate first tick
        loop {
            interval.tick().await;
            if let Err(e) = rotate_if_needed(&log_file_clone, None, None) {
                logger_clone.warn(&format!("Scheduled log rotation failed: {e}"), None);
            }
        }
    });

    // ------------------------------------------------------------------
    // Log startup banner
    // ------------------------------------------------------------------
    let pid = std::process::id();
    logger.info(
        "daemon starting",
        Some(&json!({
            "version": VERSION,
            "pid": pid,
            "config": paths.config_file.display().to_string(),
        })),
    );

    // ------------------------------------------------------------------
    // Write PID file
    // ------------------------------------------------------------------
    write_pid(pid, None).await?;
    logger.info("daemon ready", Some(&json!({ "pid": pid })));

    // ------------------------------------------------------------------
    // SIGTERM handler via CancellationToken
    // ------------------------------------------------------------------
    let ct = CancellationToken::new();
    let ct_term = ct.clone();

    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate())
                .expect("Failed to install SIGTERM handler");
            sigterm.recv().await;
            ct_term.cancel();
        }
        #[cfg(not(unix))]
        {
            // On non-Unix platforms just wait for Ctrl-C.
            let _ = tokio::signal::ctrl_c().await;
            ct_term.cancel();
        }
    });

    // ------------------------------------------------------------------
    // Run the bot / polling loop
    // ------------------------------------------------------------------
    let poll_result = crate::bot::polling::start_polling(
        ct.clone(),
        &logger,
        &config,
    )
    .await;

    // ------------------------------------------------------------------
    // Graceful shutdown
    // ------------------------------------------------------------------
    let _ = remove_pid(None).await;

    match poll_result {
        Ok(()) => {
            logger.info("shutdown complete", None);
            Ok(())
        }
        Err(e) => {
            logger.error(&format!("polling error: {e}"), None);

            // Check restart limit — if exceeded, exit 0 so launchd stops retrying.
            match tracker.record_restart().await {
                Ok(true) => {
                    logger.warn(
                        "restart limit exceeded — exiting with 0 to stop launchd retries",
                        None,
                    );
                    std::process::exit(0);
                }
                Ok(false) => {
                    std::process::exit(1);
                }
                Err(te) => {
                    logger.warn(&format!("restart tracker error: {te}"), None);
                    std::process::exit(1);
                }
            }
        }
    }
}
