use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

use crate::bot::utils::parse_first_and_rest;
use crate::bot::AppState;

pub async fn handle_move(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let args = args.trim().to_string();

    let (key, status) = match parse_first_and_rest(&args) {
        Some(pair) => pair,
        None => {
            bot.send_message(
                msg.chat.id,
                "Usage: /move &lt;issue-key&gt; &lt;status&gt;",
            )
            .parse_mode(ParseMode::Html)
            .await?;
            return Ok(());
        }
    };

    match state.jira.transition_issue(&key, &status).await {
        Ok(()) => {
            bot.send_message(
                msg.chat.id,
                format!("Moved <b>{}</b> \u{2192} {}", key, status),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("Error: {e}"))
                .await?;
        }
    }

    Ok(())
}
