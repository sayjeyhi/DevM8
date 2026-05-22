use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

use crate::bot::utils::parse_first_and_rest;
use crate::bot::AppState;

pub async fn handle_comment(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let args = args.trim().to_string();

    let (key, text) = match parse_first_and_rest(&args) {
        Some(pair) => pair,
        None => {
            bot.send_message(
                msg.chat.id,
                "Usage: /comment &lt;issue-key&gt; &lt;text&gt;",
            )
            .parse_mode(ParseMode::Html)
            .await?;
            return Ok(());
        }
    };

    match state.jira.add_comment(&key, &text).await {
        Ok(()) => {
            bot.send_message(
                msg.chat.id,
                format!("Comment added to <b>{}</b>", key),
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

/// Handle a pending comment that the user typed in free-text mode.
pub async fn handle_pending_comment(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    issue_key: String,
) -> Result<()> {
    let text = msg.text().unwrap_or("").trim().to_string();
    if text.is_empty() {
        bot.send_message(msg.chat.id, "Comment cannot be empty.").await?;
        return Ok(());
    }

    // Clear the pending state
    if let Some(mut chat_state) = state.chat_states.get_mut(&msg.chat.id.0) {
        chat_state.pending_comment = None;
    }

    match state.jira.add_comment(&issue_key, &text).await {
        Ok(()) => {
            bot.send_message(
                msg.chat.id,
                format!("Comment added to <b>{}</b>", issue_key),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("Error adding comment: {e}"))
                .await?;
        }
    }

    Ok(())
}
