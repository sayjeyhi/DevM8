use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use crate::bot::utils::parse_first_and_rest;
use crate::bot::AppState;
use crate::channel::ChannelSender;

pub async fn handle_move(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
    args: String,
) -> Result<()> {
    let args = args.trim().to_string();

    let (key, status) = match parse_first_and_rest(&args) {
        Some(pair) => pair,
        None => {
            sender
                .send(
                    chat_id,
                    "Send the issue key and target status:\n\
                     <code>MYAPP-123 In Progress</code>",
                )
                .await?;
            return Ok(());
        }
    };

    state.logger.info(
        "move: transitioning issue",
        Some(&json!({ "key": &key, "target_status": &status })),
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
    match jira.transition_issue(&key, &status).await {
        Ok(()) => {
            state.logger.info(
                "move: transition complete",
                Some(&json!({ "key": &key, "status": &status })),
            );
            sender
                .send(
                    chat_id,
                    &format!("Moved <b>{}</b> \u{2192} {}", key, status),
                )
                .await?;
        }
        Err(e) => {
            state.logger.error(
                &format!("move: transition failed: {e}"),
                Some(&json!({ "key": &key, "target_status": &status })),
            );
            sender.send(chat_id, &format!("Error: {e}")).await?;
        }
    }

    Ok(())
}
