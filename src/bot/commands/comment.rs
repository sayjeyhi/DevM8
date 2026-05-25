use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode};

use crate::bot::utils::parse_first_and_rest;
use crate::bot::AppState;

pub async fn handle_comment(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let args = args.trim().to_string();

    let (key, text) = match parse_first_and_rest(&args) {
        Some(pair) => pair,
        None => {
            bot.send_message(
                chat_id,
                "Send the issue key and comment text:\n\
                 <code>MYAPP-123 Fixed in PR #42</code>",
            )
            .parse_mode(ParseMode::Html)
            .await?;
            return Ok(());
        }
    };

    state
        .logger
        .info("comment: adding comment", Some(&json!({ "key": &key })));

    match state.jira.add_comment(&key, &text).await {
        Ok(()) => {
            state
                .logger
                .info("comment: comment added", Some(&json!({ "key": &key })));
            bot.send_message(chat_id, format!("Comment added to <b>{}</b>", key))
                .parse_mode(ParseMode::Html)
                .await?;
        }
        Err(e) => {
            state.logger.error(
                &format!("comment: failed to add comment: {e}"),
                Some(&json!({ "key": &key })),
            );
            bot.send_message(chat_id, format!("Error: {e}")).await?;
        }
    }

    Ok(())
}

pub async fn handle_pending_comment(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    issue_key: String,
) -> Result<()> {
    let text = msg.text().unwrap_or("").trim().to_string();
    if text.is_empty() {
        bot.send_message(msg.chat.id, "Comment cannot be empty.")
            .await?;
        return Ok(());
    }

    if let Some(mut chat_state) = state.chat_states.get_mut(&msg.chat.id.0) {
        chat_state.pending_comment = None;
    }

    state.logger.info(
        "comment: adding pending comment",
        Some(&json!({ "key": &issue_key })),
    );

    match state.jira.add_comment(&issue_key, &text).await {
        Ok(()) => {
            state.logger.info(
                "comment: pending comment added",
                Some(&json!({ "key": &issue_key })),
            );
            bot.send_message(
                msg.chat.id,
                format!("Comment added to <b>{}</b>", issue_key),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }
        Err(e) => {
            state.logger.error(
                &format!("comment: failed to add pending comment: {e}"),
                Some(&json!({ "key": &issue_key })),
            );
            bot.send_message(msg.chat.id, format!("Error adding comment: {e}"))
                .await?;
        }
    }

    Ok(())
}
