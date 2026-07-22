use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use crate::bot::utils::parse_first_and_rest;
use crate::bot::AppState;
use crate::channel::ChannelSender;

pub async fn handle_comment(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
    args: String,
) -> Result<()> {
    let args = args.trim().to_string();

    let (key, text) = match parse_first_and_rest(&args) {
        Some(pair) => pair,
        None => {
            sender
                .send(
                    chat_id,
                    "Send the issue key and comment text:\n\
                     <code>MYAPP-123 Fixed in PR #42</code>",
                )
                .await?;
            return Ok(());
        }
    };

    state
        .logger
        .info("comment: adding comment", Some(&json!({ "key": &key })));

    let Some(jira) = state.jira_for_user(user_id).await else {
        sender
            .send(
                chat_id,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    match jira.add_comment(&key, &text).await {
        Ok(()) => {
            state
                .logger
                .info("comment: comment added", Some(&json!({ "key": &key })));
            sender
                .send(chat_id, &format!("Comment added to <b>{}</b>", key))
                .await?;
        }
        Err(e) => {
            state.logger.error(
                &format!("comment: failed to add comment: {e}"),
                Some(&json!({ "key": &key })),
            );
            sender.send(chat_id, &format!("Error: {e}")).await?;
        }
    }

    Ok(())
}

pub async fn handle_pending_comment(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    state: Arc<AppState>,
    issue_key: String,
) -> Result<()> {
    if text.is_empty() {
        sender.send(chat_id, "Comment cannot be empty.").await?;
        return Ok(());
    }

    if let Some(mut chat_state) = state.chat_states.get_mut(chat_id) {
        chat_state.pending_comment = None;
    }

    state.logger.info(
        "comment: adding pending comment",
        Some(&json!({ "key": &issue_key })),
    );

    let Some(jira) = state.jira_for_user(user_id).await else {
        sender
            .send(
                chat_id,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    match jira.add_comment(&issue_key, text).await {
        Ok(()) => {
            state.logger.info(
                "comment: pending comment added",
                Some(&json!({ "key": &issue_key })),
            );
            sender
                .send(chat_id, &format!("Comment added to <b>{}</b>", issue_key))
                .await?;
        }
        Err(e) => {
            state.logger.error(
                &format!("comment: failed to add pending comment: {e}"),
                Some(&json!({ "key": &issue_key })),
            );
            sender
                .send(chat_id, &format!("Error adding comment: {e}"))
                .await?;
        }
    }

    Ok(())
}
