use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::bot::state::AdminPendingAction;
use crate::bot::AppState;

use super::{handle_add_project, handle_clone, handle_logs, handle_permissions, handle_status};

pub async fn handle_admin(bot: Bot, msg: Message, _state: Arc<AppState>) -> Result<()> {
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("Permissions", "admin:permissions"),
            InlineKeyboardButton::callback("Logs", "admin:logs"),
        ],
        vec![
            InlineKeyboardButton::callback("Add Project", "admin:add_project"),
            InlineKeyboardButton::callback("Clone Repo", "admin:clone"),
        ],
        vec![InlineKeyboardButton::callback("Status", "admin:status")],
    ]);

    bot.send_message(msg.chat.id, "Admin Panel — choose an action:")
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

pub async fn handle_admin_callback(
    bot: Bot,
    query: CallbackQuery,
    state: Arc<AppState>,
) -> Result<()> {
    let data = query.data.as_deref().unwrap_or("");
    let chat_id = match query.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => {
            let _ = bot.answer_callback_query(query.id).await;
            return Ok(());
        }
    };

    let _ = bot.answer_callback_query(query.id).await;

    match data {
        "admin:permissions" => handle_permissions(bot, chat_id, state).await,
        "admin:logs" => handle_logs(bot, chat_id, state, String::new()).await,
        "admin:status" => handle_status(bot, chat_id, state).await,
        "admin:clone" => {
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_admin_action = Some(AdminPendingAction::Clone);
            bot.send_message(
                chat_id,
                "Send the SSH URL and destination path:\n\
                 <code>git@github.com:org/repo.git /home/user/projects</code>",
            )
            .parse_mode(teloxide::types::ParseMode::Html)
            .await?;
            Ok(())
        }
        "admin:add_project" => {
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_admin_action = Some(AdminPendingAction::AddProject);
            bot.send_message(
                chat_id,
                "Send the local path and project name:\n\
                 <code>/home/user/my-app MY_APP</code>",
            )
            .parse_mode(teloxide::types::ParseMode::Html)
            .await?;
            Ok(())
        }
        _ => Ok(()),
    }
}

pub async fn handle_admin_input(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    action: AdminPendingAction,
) -> Result<()> {
    let chat_id = msg.chat.id;
    let text = msg.text().unwrap_or("").trim().to_string();

    state
        .chat_states
        .entry(chat_id.0)
        .or_default()
        .pending_admin_action = None;

    match action {
        AdminPendingAction::Clone => handle_clone(bot, chat_id, state, text).await,
        AdminPendingAction::AddProject => handle_add_project(bot, chat_id, state, text).await,
    }
}
