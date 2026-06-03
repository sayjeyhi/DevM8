use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use crate::bot::state::PendingSlackAction;
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender};
use crate::claude::types::AskOptions;
use crate::slack::types::SlackNewMessage;

pub async fn create_slack_forward_handler(
    sender: Arc<dyn ChannelSender>,
    allowed_chat_ids: Vec<String>,
    message: &SlackNewMessage,
) -> Result<()> {
    let snd = &message.sender_name;
    let text = &message.message.text;
    let channel_id = &message.channel.id;
    let ts = &message.message.ts;

    let body = format!(
        "\u{1f4e8} <b>Slack DM from @{}</b>\n{}",
        sender.escape(snd),
        sender.escape(text)
    );

    let keyboard = vec![vec![
        Button::new(
            "\u{21a9}\u{fe0f} Reply",
            format!("slack:reply:{}:{}", channel_id, ts),
        ),
        Button::new(
            "\u{1f916} Answer with AI",
            format!("slack:ai:{}:{}", channel_id, ts),
        ),
    ]];

    for chat_id in &allowed_chat_ids {
        let _ = sender
            .send_with_keyboard(chat_id, &body, keyboard.clone())
            .await;
    }

    Ok(())
}

pub async fn handle_pending_slack_reply(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    text: &str,
    state: Arc<AppState>,
) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    let pending = {
        state
            .chat_states
            .get(chat_id)
            .and_then(|cs| cs.pending_slack_reply.clone())
    };

    let pending = match pending {
        Some(p) => p,
        None => return Ok(()),
    };

    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.pending_slack_reply = None;
    }

    state.logger.info(
        "slack: sending reply",
        Some(&json!({ "channel": &pending.channel_id })),
    );

    if let Some(slack) = state.slack.as_ref() {
        match slack
            .post_message(&pending.channel_id, text, pending.thread_ts.as_deref())
            .await
        {
            Ok(()) => {
                state.logger.info(
                    "slack: reply sent",
                    Some(&json!({ "channel": &pending.channel_id })),
                );
                sender.send(chat_id, "Slack reply sent.").await?;
            }
            Err(e) => {
                state.logger.error(
                    &format!("slack: failed to send reply: {e}"),
                    Some(&json!({ "channel": &pending.channel_id })),
                );
                sender
                    .send(chat_id, &format!("Failed to send Slack reply: {e}"))
                    .await?;
            }
        }
    } else {
        sender
            .send(chat_id, "Slack integration is not configured.")
            .await?;
    }

    Ok(())
}

pub async fn handle_slack_callback(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    action_data: &str,
    state: Arc<AppState>,
) -> Result<()> {
    let parts: Vec<&str> = action_data.splitn(4, ':').collect();
    if parts.len() < 2 || parts[0] != "slack" {
        return Ok(());
    }

    let action = parts[1];

    match action {
        "reply" => {
            if parts.len() < 4 {
                return Ok(());
            }
            let channel_id = parts[2].to_string();
            let ts = parts[3].to_string();

            state.logger.info(
                "slack: reply initiated",
                Some(&json!({ "channel": &channel_id })),
            );

            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_slack_reply = Some(PendingSlackAction {
                    channel_id,
                    thread_ts: Some(ts),
                    ai_draft: None,
                });
            }

            sender.send(chat_id, "Type your reply:").await?;
        }

        "ai" => {
            if parts.len() < 4 {
                return Ok(());
            }
            let channel_id = parts[2].to_string();
            let ts = parts[3].to_string();

            state.logger.info(
                "slack: generating AI draft",
                Some(&json!({ "channel": &channel_id })),
            );

            if let Some(slack) = state.slack.as_ref() {
                let msg_opt = slack
                    .get_message_by_ts(&channel_id, &ts)
                    .await
                    .ok()
                    .flatten();

                if let Some(slack_msg) = msg_opt {
                    let prompt = format!(
                        "You are drafting a professional reply to a Slack message.\n\nOriginal message:\n{}\n\nWrite a concise, helpful reply. Output only the reply text.",
                        slack_msg.text
                    );

                    let thinking_ref = sender.send(chat_id, "Generating AI draft...").await?;

                    match state.ai.ask(&prompt, AskOptions::default()).await {
                        Ok((draft, _)) => {
                            state.logger.info(
                                "slack: AI draft generated",
                                Some(&json!({ "channel": &channel_id, "draft_len": draft.len() })),
                            );
                            sender
                                .edit_text(
                                    &thinking_ref,
                                    &format!("AI draft:\n\n<pre>{}</pre>", sender.escape(&draft)),
                                )
                                .await?;

                            let keyboard = vec![vec![
                                Button::new(
                                    "\u{1f4e4} Send",
                                    format!("slack:send:{}:{}", channel_id, ts),
                                ),
                                Button::new(
                                    "\u{270f}\u{fe0f} Edit",
                                    format!("slack:edit:{}:{}", channel_id, ts),
                                ),
                                Button::new(
                                    "\u{274c} Cancel",
                                    format!("slack:cancel:{}:{}", channel_id, ts),
                                ),
                            ]];

                            {
                                let mut entry =
                                    state.chat_states.entry(chat_id.to_string()).or_default();
                                entry.pending_slack_reply = Some(PendingSlackAction {
                                    channel_id,
                                    thread_ts: Some(ts),
                                    ai_draft: Some(draft),
                                });
                            }

                            sender
                                .send_with_keyboard(chat_id, "Choose an action:", keyboard)
                                .await?;
                        }
                        Err(e) => {
                            state
                                .logger
                                .error(&format!("slack: Claude error generating draft: {e}"), None);
                            sender
                                .edit_text(&thinking_ref, &format!("Claude error: {e}"))
                                .await?;
                        }
                    }
                } else {
                    sender
                        .send(chat_id, "Could not retrieve the original Slack message.")
                        .await?;
                }
            } else {
                sender
                    .send(chat_id, "Slack integration is not configured.")
                    .await?;
            }
        }

        "send" => {
            let pending = {
                state
                    .chat_states
                    .get(chat_id)
                    .and_then(|cs| cs.pending_slack_reply.clone())
            };

            if let Some(p) = pending {
                if let Some(draft) = p.ai_draft.clone() {
                    if let Some(slack) = state.slack.as_ref() {
                        state.logger.info(
                            "slack: sending AI draft",
                            Some(&json!({ "channel": &p.channel_id })),
                        );
                        match slack
                            .post_message(&p.channel_id, &draft, p.thread_ts.as_deref())
                            .await
                        {
                            Ok(()) => {
                                state.logger.info(
                                    "slack: AI draft sent",
                                    Some(&json!({ "channel": &p.channel_id })),
                                );
                                sender.send(chat_id, "Slack message sent.").await?;
                            }
                            Err(e) => {
                                state.logger.error(
                                    &format!("slack: failed to send AI draft: {e}"),
                                    Some(&json!({ "channel": &p.channel_id })),
                                );
                                sender.send(chat_id, &format!("Failed: {e}")).await?;
                            }
                        }
                    }
                    {
                        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                        entry.pending_slack_reply = None;
                    }
                } else {
                    sender.send(chat_id, "No draft available.").await?;
                }
            }
        }

        "edit" => {
            if parts.len() < 4 {
                return Ok(());
            }
            let channel_id = parts[2].to_string();
            let ts = parts[3].to_string();

            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                if let Some(ref mut p) = entry.pending_slack_reply {
                    p.ai_draft = None;
                } else {
                    entry.pending_slack_reply = Some(PendingSlackAction {
                        channel_id,
                        thread_ts: Some(ts),
                        ai_draft: None,
                    });
                }
            }

            sender.send(chat_id, "Type your reply:").await?;
        }

        "cancel" => {
            state.logger.info("slack: reply cancelled", None);
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_slack_reply = None;
            }
            sender.send(chat_id, "Cancelled.").await?;
        }

        _ => {}
    }

    Ok(())
}
