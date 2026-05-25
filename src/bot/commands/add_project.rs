use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

use crate::bot::utils::{escape_html, parse_first_and_rest};
use crate::bot::AppState;
use crate::commands::add_project_cmd::register_project;

pub async fn handle_add_project(
    bot: Bot,
    msg: Message,
    _state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let Some((path, project_name)) = parse_first_and_rest(&args) else {
        bot.send_message(
            msg.chat.id,
            "Usage: /add_project &lt;path&gt; &lt;project_name&gt;\n\
             Example: /add_project /home/user/my-app MY_APP",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    };

    match register_project(&path, &project_name) {
        Ok(()) => {
            bot.send_message(
                msg.chat.id,
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
            bot.send_message(
                msg.chat.id,
                format!("Failed: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }

    Ok(())
}
