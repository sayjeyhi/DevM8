use std::sync::Arc;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::config::loader::load_config;
use crate::daemon::pid::{remove_pid, write_pid};
use crate::daemon::restart_tracker::RestartTracker;
use crate::logger::rotate::rotate_if_needed;
use crate::logger::{self, Level, OutputMode};
use crate::shared::errors::AppError;
use crate::shared::paths::PATHS;

const VERSION: &str = env!("CARGO_PKG_VERSION");

enum LoopControl {
    Exit(anyhow::Result<()>),
    Reload,
}

pub async fn daemon_command() -> Result<(), AppError> {
    // ------------------------------------------------------------------
    // Load configuration (exit 1 on error)
    // ------------------------------------------------------------------
    let mut config = load_config(None).map_err(|e| {
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
        crate::config::schema::LogLevel::Info => Level::Info,
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
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(e) = rotate_if_needed(&log_file_clone, None, None) {
                logger_clone.warn(&format!("Scheduled log rotation failed: {e}"), None);
            }
        }
    });

    // ------------------------------------------------------------------
    // Log startup banner + write PID
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

    write_pid(pid, None).await?;
    logger.info("daemon ready", Some(&json!({ "pid": pid })));

    // ------------------------------------------------------------------
    // Signal streams (created once, reused across reload iterations)
    // ------------------------------------------------------------------
    #[cfg(unix)]
    let (mut sigterm_stream, mut sighup_stream) = {
        use tokio::signal::unix::{signal, SignalKind};
        (
            signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler"),
            signal(SignalKind::hangup()).expect("Failed to install SIGHUP handler"),
        )
    };

    // ------------------------------------------------------------------
    // Polling loop — restarts on SIGHUP, exits on SIGTERM or error
    // ------------------------------------------------------------------
    loop {
        let ct = CancellationToken::new();

        #[cfg(unix)]
        let control = {
            let ct_clone = ct.clone();
            tokio::select! {
                r = crate::bot::polling::start_polling(ct, &logger, &config) => {
                    LoopControl::Exit(r.map_err(anyhow::Error::from))
                }
                _ = sigterm_stream.recv() => {
                    ct_clone.cancel();
                    LoopControl::Exit(Ok(()))
                }
                _ = sighup_stream.recv() => {
                    ct_clone.cancel();
                    LoopControl::Reload
                }
            }
        };

        #[cfg(not(unix))]
        let control = {
            // On non-Unix: no SIGHUP, just wait for Ctrl-C or natural end.
            let ct_clone = ct.clone();
            tokio::select! {
                r = crate::bot::polling::start_polling(ct, &logger, &config) => {
                    LoopControl::Exit(r.map_err(anyhow::Error::from))
                }
                _ = tokio::signal::ctrl_c() => {
                    ct_clone.cancel();
                    LoopControl::Exit(Ok(()))
                }
            }
        };

        match control {
            LoopControl::Reload => {
                logger.info("SIGHUP received — reloading config", None);
                match load_config(None) {
                    Ok(new_cfg) => {
                        config = new_cfg;
                        logger.info("config reloaded, restarting bot", None);
                    }
                    Err(e) => {
                        logger.warn(
                            &format!("config reload failed, keeping previous config: {e}"),
                            None,
                        );
                    }
                }
                // continue loop with (possibly updated) config
            }

            LoopControl::Exit(Ok(())) => {
                let _ = remove_pid(None).await;
                logger.info("shutdown complete", None);
                return Ok(());
            }

            LoopControl::Exit(Err(e)) => {
                let _ = remove_pid(None).await;
                logger.error(&format!("polling error: {e}"), None);

                match tracker.record_restart().await {
                    Ok(true) => {
                        logger.warn(
                            "restart limit exceeded — exiting with 0 to stop launchd retries",
                            None,
                        );
                        std::process::exit(0);
                    }
                    Ok(false) => std::process::exit(1),
                    Err(te) => {
                        logger.warn(&format!("restart tracker error: {te}"), None);
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}
