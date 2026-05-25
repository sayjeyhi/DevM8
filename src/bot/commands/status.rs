use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

use crate::bot::utils::escape_html;
use crate::bot::AppState;
use crate::daemon::agent_status;
use crate::daemon::pid::{is_process_running, read_pid};

pub async fn handle_status(bot: Bot, msg: Message, state: Arc<AppState>) -> Result<()> {
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

    let jira_url = escape_html(&state.config.jira.base_url);
    let jira_projects = escape_html(&state.config.jira.project_keys.join(", "));

    let mut git_keys: Vec<String> = state.git_map.keys().cloned().collect();
    git_keys.sort();
    let git_projects = if git_keys.is_empty() {
        "-".to_string()
    } else {
        escape_html(&git_keys.join(", "))
    };

    let text = format!(
        "<b>DevM8 Status</b>\n\n\
         State:         <code>{state_str}</code>\n\
         PID:           <code>{pid_display}</code>\n\
         Jira:          <code>{jira_url}</code>\n\
         Jira projects: <code>{jira_projects}</code>\n\
         Git projects:  <code>{git_projects}</code>"
    );

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Html)
        .await?;

    Ok(())
}
