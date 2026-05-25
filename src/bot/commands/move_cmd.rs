use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode};

use crate::bot::utils::parse_first_and_rest;
use crate::bot::AppState;

pub async fn handle_move(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let args = args.trim().to_string();

    let (key, status) = match parse_first_and_rest(&args) {
        Some(pair) => pair,
        None => {
            bot.send_message(
                chat_id,
                "Send the issue key and target status:\n\
                 <code>MYAPP-123 In Progress</code>",
            )
            .parse_mode(ParseMode::Html)
            .await?;
            return Ok(());
        }
    };

    state.logger.info(
        "move: transitioning issue",
        Some(&json!({ "key": &key, "target_status": &status })),
    );

    match state.jira.transition_issue(&key, &status).await {
        Ok(()) => {
            state.logger.info(
                "move: transition complete",
                Some(&json!({ "key": &key, "status": &status })),
            );
            bot.send_message(chat_id, format!("Moved <b>{}</b> \u{2192} {}", key, status))
                .parse_mode(ParseMode::Html)
                .await?;
        }
        Err(e) => {
            state.logger.error(
                &format!("move: transition failed: {e}"),
                Some(&json!({ "key": &key, "target_status": &status })),
            );
            bot.send_message(chat_id, format!("Error: {e}")).await?;
        }
    }

    Ok(())
}
