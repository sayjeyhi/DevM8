use std::sync::Arc;

use anyhow::Result;
use sysinfo::{Disks, System};
use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode};

use crate::bot::utils::escape_html;
use crate::bot::AppState;
use crate::daemon::agent_status;
use crate::daemon::pid::{is_process_running, read_pid};
use crate::shared::paths::PATHS;
use crate::shared::utils::{compute_uptime_from_pid_file, dir_size_bytes, format_bytes};

pub async fn handle_status(bot: Bot, chat_id: ChatId, state: Arc<AppState>) -> Result<()> {
    let launchd = agent_status().await;
    let pid_from_file = read_pid(None).await.ok().flatten();

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

    let pid_display = effective_pid
        .map(|p| p.to_string())
        .unwrap_or_else(|| "-".to_string());

    let uptime_str = if state_str.starts_with("running") {
        compute_uptime_from_pid_file(&PATHS).unwrap_or_else(|| "unknown".to_string())
    } else {
        "-".to_string()
    };

    // Jira / git config
    let jira_url = escape_html(&state.config.jira.base_url);
    let jira_projects = escape_html(&state.config.jira.project_keys.join(", "));
    let mut git_keys: Vec<String> = state.git_map.keys().cloned().collect();
    git_keys.sort();
    let git_projects = if git_keys.is_empty() {
        "-".to_string()
    } else {
        escape_html(&git_keys.join(", "))
    };

    // Users
    let configured_users = state.config.telegram.allowed_user_ids.len();
    let active_users = state.user_names.len();

    // Log sizes
    let log_size = dir_size_bytes(&PATHS.logs_dir);
    let log_size_str = format_bytes(log_size);

    // System info (CPU/RAM) — run blocking in a spawn_blocking to avoid blocking the async runtime
    let (os_name, total_ram, free_ram, free_disk, total_disk) = tokio::task::spawn_blocking(|| {
        let mut sys = System::new_all();
        sys.refresh_all();

        let os_name = format!(
            "{} {}",
            System::name().unwrap_or_else(|| "Unknown".to_string()),
            System::os_version().unwrap_or_default(),
        )
        .trim()
        .to_string();

        let total_ram = sys.total_memory();
        let free_ram = sys.available_memory();

        let disks = Disks::new_with_refreshed_list();
        let (free_disk, total_disk) = disks.iter().fold((0u64, 0u64), |(f, t), d| {
            (f + d.available_space(), t + d.total_space())
        });

        (os_name, total_ram, free_ram, free_disk, total_disk)
    })
    .await
    .unwrap_or_else(|_| ("Unknown".to_string(), 0, 0, 0, 0));

    let ram_str = format!(
        "{} free / {} total",
        format_bytes(free_ram),
        format_bytes(total_ram)
    );
    let disk_str = format!(
        "{} free / {} total",
        format_bytes(free_disk),
        format_bytes(total_disk)
    );

    let text = format!(
        "<b>DevM8 Status</b>\n\n\
         State:          <code>{state_str}</code>\n\
         PID:            <code>{pid_display}</code>\n\
         Uptime:         <code>{uptime_str}</code>\n\
         \n\
         OS:             <code>{os_name}</code>\n\
         RAM:            <code>{ram_str}</code>\n\
         Disk:           <code>{disk_str}</code>\n\
         \n\
         Logs size:      <code>{log_size_str}</code>\n\
         Users (cfg):    <code>{configured_users}</code>\n\
         Users (seen):   <code>{active_users}</code>\n\
         \n\
         Jira:           <code>{jira_url}</code>\n\
         Jira projects:  <code>{jira_projects}</code>\n\
         Git projects:   <code>{git_projects}</code>"
    );

    bot.send_message(chat_id, text)
        .parse_mode(ParseMode::Html)
        .await?;

    Ok(())
}
