use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode};

use crate::bot::state::JiraPendingAction;
use crate::bot::utils::project_key_from_args;
use crate::bot::AppState;

use super::my_tickets::accessible_project_keys;
use super::{
    handle_comment, handle_create_confirm, handle_create_suggest, handle_move, handle_my_tickets,
    handle_solve,
};
use crate::bot::commands::jira_setup::{
    handle_jira_clear, handle_jira_fav_status_done, handle_jira_fav_status_toggle,
    handle_jira_fav_statuses_start, handle_jira_manage_project_done,
    handle_jira_manage_project_toggle, handle_jira_projects_start, handle_jira_setup_input,
    handle_jira_setup_project_done, handle_jira_setup_project_toggle, handle_jira_setup_start,
    start_url_step,
};

pub async fn handle_jira(bot: Bot, msg: Message, state: Arc<AppState>) -> Result<()> {
    let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    let has_user_jira = state.has_user_jira(user_id);
    let jira_account_label = if has_user_jira {
        "⚙️ Settings ✓"
    } else {
        "⚙️ Settings"
    };

    let mut rows = vec![
        vec![
            InlineKeyboardButton::callback("🎫 My Tickets", "jira:my_tickets"),
            InlineKeyboardButton::callback("✏️ Create Ticket", "jira:create"),
        ],
        vec![
            InlineKeyboardButton::callback("🔀 Move Ticket", "jira:move"),
            InlineKeyboardButton::callback("💬 Add Comment", "jira:comment"),
        ],
        vec![InlineKeyboardButton::callback(
            "✅ Solve Ticket",
            "jira:solve",
        )],
    ];

    rows.push(vec![InlineKeyboardButton::callback(
        jira_account_label,
        "jira:setup",
    )]);

    let keyboard = InlineKeyboardMarkup::new(rows);

    bot.send_message(msg.chat.id, "Jira — choose an action:")
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

async fn prompt_for_title(bot: &Bot, chat_id: ChatId, project_key: &str) -> Result<()> {
    bot.send_message(
        chat_id,
        format!("Project: <code>{project_key}</code>\n\nSend the issue title:"),
    )
    .parse_mode(ParseMode::Html)
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

    // Setup callbacks
    if data == "jira:setup" {
        return handle_jira_setup_start(bot, chat_id, state, user_id).await;
    }
    if data == "jira:setup_reconnect" {
        return start_url_step(&bot, chat_id, &state).await;
    }
    if data == "jira:setup_clear" {
        return handle_jira_clear(bot, chat_id, state, user_id).await;
    }

    // Project picker callbacks (step 4 of setup)
    if let Some(key) = data.strip_prefix("jira:setup_proj_toggle:") {
        let message_id = query
            .message
            .as_ref()
            .map(|m| m.id())
            .unwrap_or(MessageId(0));
        return handle_jira_setup_project_toggle(bot, chat_id, message_id, state, user_id, key)
            .await;
    }
    if data == "jira:setup_proj_done" {
        let message_id = query
            .message
            .as_ref()
            .map(|m| m.id())
            .unwrap_or(MessageId(0));
        return handle_jira_setup_project_done(bot, chat_id, message_id, state, user_id).await;
    }

    // Manage-projects callbacks (post-setup project picker)
    if data == "jira:projects" {
        return handle_jira_projects_start(bot, chat_id, state, user_id).await;
    }
    if let Some(key) = data.strip_prefix("jira:manage_proj_toggle:") {
        let message_id = query
            .message
            .as_ref()
            .map(|m| m.id())
            .unwrap_or(MessageId(0));
        return handle_jira_manage_project_toggle(bot, chat_id, message_id, state, key).await;
    }
    if data == "jira:manage_proj_done" {
        let message_id = query
            .message
            .as_ref()
            .map(|m| m.id())
            .unwrap_or(MessageId(0));
        return handle_jira_manage_project_done(bot, chat_id, message_id, state, user_id).await;
    }

    // Favorite-statuses callbacks
    if data == "jira:fav_statuses" {
        return handle_jira_fav_statuses_start(bot, chat_id, state, user_id).await;
    }
    if let Some(name) = data.strip_prefix("jira:fav_status_toggle:") {
        let message_id = query
            .message
            .as_ref()
            .map(|m| m.id())
            .unwrap_or(MessageId(0));
        return handle_jira_fav_status_toggle(bot, chat_id, message_id, state, name).await;
    }
    if data == "jira:fav_status_done" {
        let message_id = query
            .message
            .as_ref()
            .map(|m| m.id())
            .unwrap_or(MessageId(0));
        return handle_jira_fav_status_done(bot, chat_id, message_id, state, user_id).await;
    }

    // Step 3a: user confirmed Claude's description
    if data == "jira:create_confirm" {
        let pending = state
            .chat_states
            .get(&chat_id.0)
            .and_then(|s| s.pending_jira_action.clone());

        if let Some(JiraPendingAction::CreateDescription(pk, title, suggested)) = pending {
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_jira_action = None;
            return handle_create_confirm(bot, chat_id, state, user_id, &pk, &title, &suggested)
                .await;
        }
        return Ok(());
    }

    // Step 1b: project selected from picker
    if let Some(pk) = data.strip_prefix("jira:create_project:") {
        state
            .chat_states
            .entry(chat_id.0)
            .or_default()
            .pending_jira_action = Some(JiraPendingAction::CreateTitle(pk.to_string()));
        return prompt_for_title(&bot, chat_id, pk).await;
    }

    match data {
        "jira:my_tickets" => handle_my_tickets(bot, chat_id, state, user_id).await,

        // Step 1a: show project picker (or skip if single project)
        "jira:create" => {
            let projects = accessible_project_keys(user_id, &state);

            if projects.is_empty() {
                bot.send_message(chat_id, "No Jira projects configured.")
                    .await?;
                return Ok(());
            }

            if projects.len() == 1 {
                let pk = projects.into_iter().next().unwrap();
                state
                    .chat_states
                    .entry(chat_id.0)
                    .or_default()
                    .pending_jira_action = Some(JiraPendingAction::CreateTitle(pk.clone()));
                return prompt_for_title(&bot, chat_id, &pk).await;
            }

            let buttons: Vec<Vec<InlineKeyboardButton>> = projects
                .iter()
                .map(|k| {
                    vec![InlineKeyboardButton::callback(
                        k.clone(),
                        format!("jira:create_project:{k}"),
                    )]
                })
                .collect();

            bot.send_message(chat_id, "Select a project:")
                .reply_markup(InlineKeyboardMarkup::new(buttons))
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
    user_id: i64,
    action: JiraPendingAction,
    is_authorized_for_project: impl Fn(&str) -> bool,
) -> Result<()> {
    let chat_id = msg.chat.id;
    let text = msg.text().unwrap_or("").trim().to_string();

    // Setup steps don't clear pending action — they re-set it for the next step
    match &action {
        JiraPendingAction::JiraSetupUrl
        | JiraPendingAction::JiraSetupEmail(_)
        | JiraPendingAction::JiraSetupToken(_, _) => {
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_jira_action = None;
            return handle_jira_setup_input(bot, chat_id, state, user_id, action, text).await;
        }
        JiraPendingAction::JiraSetupProjects(_, _, _, _, _)
        | JiraPendingAction::JiraManageProjects(_, _)
        | JiraPendingAction::JiraFavoriteStatuses(_, _) => {
            // Selection is via inline buttons; restore state and guide user.
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_jira_action = Some(action.clone());
            bot.send_message(
                chat_id,
                "Use the buttons above to make your selection, then tap ✓ Done.",
            )
            .await?;
            return Ok(());
        }
        _ => {}
    }

    // Clear current pending state (suggest step will re-set it to CreateDescription)
    state
        .chat_states
        .entry(chat_id.0)
        .or_default()
        .pending_jira_action = None;

    match action {
        // Step 2: title received → suggest description
        JiraPendingAction::CreateTitle(project_key) => {
            handle_create_suggest(bot, chat_id, state, user_id, project_key, text).await
        }

        // Step 3b: user sent their own description instead of using Claude's
        JiraPendingAction::CreateDescription(pk, title, _suggested) => {
            handle_create_confirm(bot, chat_id, state, user_id, &pk, &title, &text).await
        }

        JiraPendingAction::Move => {
            if let Some(pk) = project_key_from_args(&text) {
                if !is_authorized_for_project(&pk) {
                    bot.send_message(chat_id, "Access denied for that project.")
                        .await?;
                    return Ok(());
                }
            }
            handle_move(bot, chat_id, state, user_id, text).await
        }

        JiraPendingAction::Comment => {
            if let Some(pk) = project_key_from_args(&text) {
                if !is_authorized_for_project(&pk) {
                    bot.send_message(chat_id, "Access denied for that project.")
                        .await?;
                    return Ok(());
                }
            }
            handle_comment(bot, chat_id, state, user_id, text).await
        }

        JiraPendingAction::Solve => {
            if let Some(pk) = project_key_from_args(&text) {
                if !is_authorized_for_project(&pk) {
                    bot.send_message(chat_id, "Access denied for that project.")
                        .await?;
                    return Ok(());
                }
            }
            handle_solve(bot, chat_id, state, user_id, text).await
        }

        // Setup/manage/picker steps handled above
        JiraPendingAction::JiraSetupUrl
        | JiraPendingAction::JiraSetupEmail(_)
        | JiraPendingAction::JiraSetupToken(_, _)
        | JiraPendingAction::JiraSetupProjects(_, _, _, _, _)
        | JiraPendingAction::JiraManageProjects(_, _)
        | JiraPendingAction::JiraFavoriteStatuses(_, _) => Ok(()),
    }
}
