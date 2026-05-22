use crate::daemon::launchd::unload_agent;
use crate::daemon::pid::remove_pid;
use crate::logger::{Level, append_to_log_file};
use crate::shared::errors::AppError;
use crate::shared::paths::PATHS;

pub async fn stop_command() -> Result<(), AppError> {
    let paths = &*PATHS;

    // Unload the launchd agent (ignore errors so `stop` is idempotent).
    if let Err(e) = unload_agent().await {
        eprintln!("Warning: could not unload launchd agent: {e}");
    }

    // Remove the PID file.
    remove_pid(None).await?;

    // Append a log entry.
    append_to_log_file(&paths.log_file, Level::Info, "service stopped", None);

    println!("devm8 stopped");

    Ok(())
}
