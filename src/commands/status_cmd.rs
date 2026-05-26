use crate::config::loader::load_config;
use crate::daemon::agent_status;
use crate::daemon::pid::{is_process_running, read_pid};
use crate::shared::errors::AppError;
use crate::shared::paths::PATHS;
use crate::shared::utils::compute_uptime_from_pid_file;

pub async fn status_command() -> Result<(), AppError> {
    let paths = &*PATHS;

    // ------------------------------------------------------------------
    // Determine running state
    // ------------------------------------------------------------------
    let launchd = agent_status().await;
    let pid_from_file = read_pid(None).await?;

    let (state_str, effective_pid) = if launchd.running {
        ("running", launchd.pid)
    } else if let Some(pid) = pid_from_file {
        if is_process_running(pid) {
            ("running (dev)", Some(pid))
        } else {
            ("stopped", None)
        }
    } else {
        ("stopped", None)
    };

    // ------------------------------------------------------------------
    // Optional: load config for display (ignore errors)
    // ------------------------------------------------------------------
    let config = load_config(None).ok();

    // ------------------------------------------------------------------
    // Uptime (from PID file mtime)
    // ------------------------------------------------------------------
    let uptime_str = if state_str.starts_with("running") {
        compute_uptime_from_pid_file(paths).unwrap_or_else(|| "unknown".to_string())
    } else {
        "-".to_string()
    };

    // ------------------------------------------------------------------
    // Build output
    // ------------------------------------------------------------------
    let pid_display = effective_pid
        .map(|p| p.to_string())
        .unwrap_or_else(|| "-".to_string());

    let config_path = paths.config_file.display().to_string();
    let log_path = paths.log_file.display().to_string();

    let jira_url = config
        .as_ref()
        .map(|c| c.jira.base_url.as_str())
        .unwrap_or("-");

    let projects = config
        .as_ref()
        .map(|c| c.jira.project_keys.join(", "))
        .unwrap_or_else(|| "-".to_string());

    println!("devm8 status");
    println!("  State:    {}", state_str);
    println!("  PID:      {}", pid_display);
    println!("  Uptime:   {}", uptime_str);
    println!("  Config:   {}", config_path);
    println!("  Jira URL: {}", jira_url);
    println!("  Projects: {}", projects);
    println!("  Log:      {}", log_path);

    Ok(())
}
