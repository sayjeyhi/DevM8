use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};

use crate::bot::state::PendingSlackAction;
use crate::bot::utils::escape_html;
use crate::bot::AppState;
use crate::claude::types::AskOptions;
use crate::slack::types::SlackNewMessage;

pub async fn create_slack_forward_handler(
    bot: Bot,
    allowed_user_ids: Vec<i64>,
    message: &SlackNewMessage,
) -> Result<()> {
    let sender = &message.sender_name;
    let text = &message.message.text;
    let channel_id = &message.channel.id;
    let ts = &message.message.ts;

    let body = format!(
        "📨 <b>Slack DM from @{}</b>\n{}",
        escape_html(sender),
        escape_html(text)
    );

    let keyboard = InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback("↩️ Reply", format!("slack:reply:{}:{}", channel_id, ts)),
        InlineKeyboardButton::callback(
            "🤖 Answer with AI",
            format!("slack:ai:{}:{}", channel_id, ts),
        ),
    ]]);

    for user_id in &allowed_user_ids {
        let _ = bot
            .send_message(ChatId(*user_id), &body)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard.clone())
            .await;
    }

    Ok(())
}

pub async fn handle_pending_slack_reply(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
) -> Result<()> {
    let text = msg.text().unwrap_or("").trim().to_string();
    if text.is_empty() {
        return Ok(());
    }

    let pending = {
        state
            .chat_states
            .get(&msg.chat.id.0)
            .and_then(|cs| cs.pending_slack_reply.clone())
    };

    let pending = match pending {
        Some(p) => p,
        None => return Ok(()),
    };

    {
        let mut entry = state.chat_states.entry(msg.chat.id.0).or_default();
        entry.pending_slack_reply = None;
    }

    state.logger.info(
        "slack: sending reply",
        Some(&json!({ "channel": &pending.channel_id })),
    );

    if let Some(slack) = state.slack.as_ref() {
        match slack
            .post_message(&pending.channel_id, &text, pending.thread_ts.as_deref())
            .await
        {
            Ok(()) => {
                state.logger.info(
                    "slack: reply sent",
                    Some(&json!({ "channel": &pending.channel_id })),
                );
                bot.send_message(msg.chat.id, "Slack reply sent.").await?;
            }
            Err(e) => {
                state.logger.error(
                    &format!("slack: failed to send reply: {e}"),
                    Some(&json!({ "channel": &pending.channel_id })),
                );
                bot.send_message(msg.chat.id, format!("Failed to send Slack reply: {e}"))
                    .await?;
            }
        }
    } else {
        bot.send_message(msg.chat.id, "Slack integration is not configured.")
            .await?;
    }

    Ok(())
}

pub async fn handle_slack_callback(bot: Bot, q: CallbackQuery, state: Arc<AppState>) -> Result<()> {
    let _ = bot.answer_callback_query(q.id.clone()).await;

    let data = q.data.as_deref().unwrap_or("");
    let chat_id = match q.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };

    let parts: Vec<&str> = data.splitn(4, ':').collect();
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
                let mut entry = state.chat_states.entry(chat_id.0).or_default();
                entry.pending_slack_reply = Some(PendingSlackAction {
                    channel_id,
                    thread_ts: Some(ts),
                    ai_draft: None,
                });
            }

            bot.send_message(chat_id, "Type your reply:").await?;
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

                    let thinking = bot.send_message(chat_id, "Generating AI draft...").await?;

                    match state.claude.ask(&prompt, AskOptions::default()).await {
                        Ok(draft) => {
                            state.logger.info(
                                "slack: AI draft generated",
                                Some(&json!({ "channel": &channel_id, "draft_len": draft.len() })),
                            );
                            bot.edit_message_text(
                                chat_id,
                                thinking.id,
                                format!("AI draft:\n\n<pre>{}</pre>", escape_html(&draft)),
                            )
                            .parse_mode(ParseMode::Html)
                            .await?;

                            let keyboard = InlineKeyboardMarkup::new(vec![vec![
                                InlineKeyboardButton::callback(
                                    "📤 Send",
                                    format!("slack:send:{}:{}", channel_id, ts),
                                ),
                                InlineKeyboardButton::callback(
                                    "✏️ Edit",
                                    format!("slack:edit:{}:{}", channel_id, ts),
                                ),
                                InlineKeyboardButton::callback(
                                    "❌ Cancel",
                                    format!("slack:cancel:{}:{}", channel_id, ts),
                                ),
                            ]]);

                            {
                                let mut entry = state.chat_states.entry(chat_id.0).or_default();
                                entry.pending_slack_reply = Some(PendingSlackAction {
                                    channel_id,
                                    thread_ts: Some(ts),
                                    ai_draft: Some(draft),
                                });
                            }

                            bot.send_message(chat_id, "Choose an action:")
                                .reply_markup(keyboard)
                                .await?;
                        }
                        Err(e) => {
                            state
                                .logger
                                .error(&format!("slack: Claude error generating draft: {e}"), None);
                            bot.edit_message_text(
                                chat_id,
                                thinking.id,
                                format!("Claude error: {e}"),
                            )
                            .await?;
                        }
                    }
                } else {
                    bot.send_message(chat_id, "Could not retrieve the original Slack message.")
                        .await?;
                }
            } else {
                bot.send_message(chat_id, "Slack integration is not configured.")
                    .await?;
            }
        }

        "send" => {
            let pending = {
                state
                    .chat_states
                    .get(&chat_id.0)
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
                                bot.send_message(chat_id, "Slack message sent.").await?;
                            }
                            Err(e) => {
                                state.logger.error(
                                    &format!("slack: failed to send AI draft: {e}"),
                                    Some(&json!({ "channel": &p.channel_id })),
                                );
                                bot.send_message(chat_id, format!("Failed: {e}")).await?;
                            }
                        }
                    }
                    {
                        let mut entry = state.chat_states.entry(chat_id.0).or_default();
                        entry.pending_slack_reply = None;
                    }
                } else {
                    bot.send_message(chat_id, "No draft available.").await?;
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
                let mut entry = state.chat_states.entry(chat_id.0).or_default();
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

            bot.send_message(chat_id, "Type your reply:").await?;
        }

        "cancel" => {
            state.logger.info("slack: reply cancelled", None);
            {
                let mut entry = state.chat_states.entry(chat_id.0).or_default();
                entry.pending_slack_reply = None;
            }
            bot.send_message(chat_id, "Cancelled.").await?;
        }

        _ => {}
    }

    Ok(())
}
