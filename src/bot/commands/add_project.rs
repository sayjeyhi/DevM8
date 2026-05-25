use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode};

use crate::bot::utils::{escape_html, parse_first_and_rest};
use crate::bot::AppState;
use crate::commands::add_project_cmd::register_project;

pub async fn handle_add_project(
    bot: Bot,
    chat_id: ChatId,
    _state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let Some((path, project_name)) = parse_first_and_rest(&args) else {
        bot.send_message(
            chat_id,
            "Send the local path and project name:\n\
             <code>/home/user/my-app MY_APP</code>",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    };

    match register_project(&path, &project_name) {
        Ok(()) => {
            bot.send_message(
                chat_id,
                format!(
                    "Registered <code>{}</code> as project <code>{}</code>",
                    escape_html(&path),
                    escape_html(&project_name)
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }
        Err(e) => {
            bot.send_message(chat_id, format!("Failed: {}", escape_html(&e.to_string())))
                .await?;
        }
    }

    Ok(())
}
