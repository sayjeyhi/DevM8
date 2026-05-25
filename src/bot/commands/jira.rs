use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};

use crate::bot::state::JiraPendingAction;
use crate::bot::utils::project_key_from_args;
use crate::bot::AppState;

use super::{handle_comment, handle_create, handle_move, handle_my_tickets, handle_solve};

pub async fn handle_jira(bot: Bot, msg: Message, _state: Arc<AppState>) -> Result<()> {
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("My Tickets", "jira:my_tickets"),
            InlineKeyboardButton::callback("Create Issue", "jira:create"),
        ],
        vec![
            InlineKeyboardButton::callback("Move Issue", "jira:move"),
            InlineKeyboardButton::callback("Add Comment", "jira:comment"),
        ],
        vec![InlineKeyboardButton::callback("Solve Issue", "jira:solve")],
    ]);

    bot.send_message(msg.chat.id, "Jira — choose an action:")
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

pub async fn handle_jira_callback(
    bot: Bot,
    query: CallbackQuery,
    state: Arc<AppState>,
) -> Result<()> {
    let data = query.data.as_deref().unwrap_or("");
    let user_id = query.from.id.0 as i64;
    let chat_id = match query.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => {
            let _ = bot.answer_callback_query(query.id).await;
            return Ok(());
        }
    };

    let _ = bot.answer_callback_query(query.id).await;

    match data {
        "jira:my_tickets" => handle_my_tickets(bot, chat_id, state, user_id).await,
        "jira:create" => {
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::Create);
            bot.send_message(
                chat_id,
                "Send the issue title (optionally add description after <code>--</code>):\n\
                 <code>New login page -- Add OAuth2 support</code>",
            )
            .parse_mode(ParseMode::Html)
            .await?;
            Ok(())
        }
        "jira:move" => {
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::Move);
            bot.send_message(
                chat_id,
                "Send the issue key and target status:\n\
                 <code>MYAPP-123 In Progress</code>",
            )
            .parse_mode(ParseMode::Html)
            .await?;
            Ok(())
        }
        "jira:comment" => {
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::Comment);
            bot.send_message(
                chat_id,
                "Send the issue key and comment text:\n\
                 <code>MYAPP-123 Fixed in PR #42</code>",
            )
            .parse_mode(ParseMode::Html)
            .await?;
            Ok(())
        }
        "jira:solve" => {
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::Solve);
            bot.send_message(chat_id, "Send the issue key:\n<code>MYAPP-123</code>")
                .parse_mode(ParseMode::Html)
                .await?;
            Ok(())
        }
        _ => Ok(()),
    }
}

pub async fn handle_jira_input(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    action: JiraPendingAction,
    is_authorized_for_project: impl Fn(&str) -> bool,
) -> Result<()> {
    let chat_id = msg.chat.id;
    let text = msg.text().unwrap_or("").trim().to_string();

    state
        .chat_states
        .entry(chat_id.0)
        .or_default()
        .pending_jira_action = None;

    // Check project authorization for issue-key actions
    let needs_project_check = matches!(
        action,
        JiraPendingAction::Move | JiraPendingAction::Comment | JiraPendingAction::Solve
    );
    if needs_project_check {
        if let Some(pk) = project_key_from_args(&text) {
            if !is_authorized_for_project(&pk) {
                bot.send_message(chat_id, "Access denied for that project.")
                    .await?;
                return Ok(());
            }
        }
    }

    match action {
        JiraPendingAction::Create => handle_create(bot, chat_id, state, text).await,
        JiraPendingAction::Move => handle_move(bot, chat_id, state, text).await,
        JiraPendingAction::Comment => handle_comment(bot, chat_id, state, text).await,
        JiraPendingAction::Solve => handle_solve(bot, chat_id, state, text).await,
    }
}
