use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode};
use tokio::process::Command;

use crate::bot::utils::escape_html;
use crate::bot::AppState;
use crate::commands::add_project_cmd::register_project;

fn repo_name_from_url(url: &str) -> String {
    url.rsplit(['/', ':'])
        .next()
        .unwrap_or(url)
        .trim_end_matches(".git")
        .to_string()
}

pub async fn handle_clone(
    bot: Bot,
    chat_id: ChatId,
    _state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let mut tokens = args.trim().splitn(2, char::is_whitespace);
    let ssh_url = match tokens.next().filter(|s| !s.is_empty()) {
        Some(u) => u.to_string(),
        None => {
            bot.send_message(
                chat_id,
                "Send the SSH URL and destination path:\n\
                 <code>git@github.com:org/repo.git /home/user/projects</code>",
            )
            .parse_mode(ParseMode::Html)
            .await?;
            return Ok(());
        }
    };

    let dest_parent = match tokens.next().map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => p.to_string(),
        None => {
            bot.send_message(
                chat_id,
                "Send the SSH URL and destination path:\n\
                 <code>git@github.com:org/repo.git /home/user/projects</code>",
            )
            .parse_mode(ParseMode::Html)
            .await?;
            return Ok(());
        }
    };

    let repo_name = repo_name_from_url(&ssh_url);
    let dest_path = std::path::Path::new(&dest_parent)
        .join(&repo_name)
        .to_string_lossy()
        .into_owned();

    let progress = bot
        .send_message(
            chat_id,
            format!(
                "Cloning <code>{}</code> → <code>{}</code>…",
                escape_html(&ssh_url),
                escape_html(&dest_path)
            ),
        )
        .parse_mode(ParseMode::Html)
        .await?;

    let output = Command::new("git")
        .args(["clone", &ssh_url, &dest_path])
        .output()
        .await;

    let reply = match output {
        Err(e) => format!("Failed to run git: {}", escape_html(&e.to_string())),
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            format!("git clone failed:\n<pre>{}</pre>", escape_html(&stderr))
        }
        Ok(_) => match register_project(&dest_path, &repo_name) {
            Ok(()) => format!(
                "Cloned and registered as project <code>{}</code>\nPath: <code>{}</code>",
                escape_html(&repo_name),
                escape_html(&dest_path)
            ),
            Err(e) => format!(
                "Cloned to <code>{}</code> but failed to register: {}",
                escape_html(&dest_path),
                escape_html(&e.to_string())
            ),
        },
    };

    bot.edit_message_text(chat_id, progress.id, reply)
        .parse_mode(ParseMode::Html)
        .await?;

    Ok(())
}
