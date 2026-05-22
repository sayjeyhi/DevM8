use crate::config::loader::{load_config, write_config};
use crate::config::wizard::run_wizard;
use crate::logger::{Level, append_to_log_file};
use crate::shared::errors::AppError;
use crate::shared::paths::PATHS;

pub async fn config_command() -> Result<(), AppError> {
    let paths = &*PATHS;

    // Load existing config if available (ignore errors).
    let existing = load_config(None).ok();

    // Run the interactive wizard (pre-filled with existing values if any).
    let config = run_wizard(existing.as_ref())?;

    // Write updated config to disk.
    write_config(&config, None)?;

    // Append a log entry (best-effort).
    append_to_log_file(
        &paths.log_file,
        Level::Info,
        "config updated",
        Some(&serde_json::json!({ "path": paths.config_file.display().to_string() })),
    );

    println!("Config written to {}", paths.config_file.display());

    Ok(())
}
