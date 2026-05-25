use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode};

use crate::bot::state::PendingPermissions;
use crate::bot::AppState;
use crate::config::loader::{load_config, write_config};

// ---------------------------------------------------------------------------
// /permissions → show user list
// ---------------------------------------------------------------------------

pub async fn handle_permissions(bot: Bot, chat_id: ChatId, state: Arc<AppState>) -> Result<()> {
    let text = user_list_text(&state);
    let keyboard = build_user_list_keyboard(&state);

    let sent = bot
        .send_message(chat_id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        entry.pending_permissions = Some(PendingPermissions {
            target_user_id: None,
            selected: HashSet::new(),
            message_id: Some(sent.id.0),
            awaiting_user_id_input: false,
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// perms:back → edit message to show user list
// ---------------------------------------------------------------------------

pub async fn handle_permissions_back(
    bot: Bot,
    query: CallbackQuery,
    state: Arc<AppState>,
) -> Result<()> {
    let _ = bot.answer_callback_query(query.id.clone()).await;

    let chat_id = match query.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };

    let message_id = pending_message_id(&state, chat_id.0);

    if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
        if let Some(p) = cs.pending_permissions.as_mut() {
            p.target_user_id = None;
            p.selected.clear();
            p.awaiting_user_id_input = false;
        }
    }

    let text = user_list_text(&state);
    let keyboard = build_user_list_keyboard(&state);

    if let Some(mid) = message_id {
        let _ = bot
            .edit_message_text(chat_id, MessageId(mid), text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// perms:add → prompt admin to type a user ID
// ---------------------------------------------------------------------------

pub async fn handle_permissions_add(
    bot: Bot,
    query: CallbackQuery,
    state: Arc<AppState>,
) -> Result<()> {
    let _ = bot.answer_callback_query(query.id.clone()).await;

    let chat_id = match query.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };

    let message_id = pending_message_id(&state, chat_id.0);

    if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
        if let Some(p) = cs.pending_permissions.as_mut() {
            p.awaiting_user_id_input = true;
            p.target_user_id = None;
        }
    }

    let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "🔙 Back",
        "perms:back",
    )]]);

    if let Some(mid) = message_id {
        let _ = bot
            .edit_message_text(
                chat_id,
                MessageId(mid),
                "Enter the Telegram user ID to configure access for:",
            )
            .reply_markup(keyboard)
            .await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// perms:user:<id> → show project picker for that user
// ---------------------------------------------------------------------------

pub async fn handle_permissions_user_select(
    bot: Bot,
    query: CallbackQuery,
    state: Arc<AppState>,
    target_id: i64,
) -> Result<()> {
    let _ = bot.answer_callback_query(query.id.clone()).await;

    let chat_id = match query.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };

    show_user_detail(bot, chat_id, state, target_id).await
}

// ---------------------------------------------------------------------------
// Text input: admin typed a user ID while awaiting_user_id_input
// ---------------------------------------------------------------------------

pub async fn handle_permissions_user_input(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
) -> Result<()> {
    let text = msg.text().unwrap_or("").trim().to_string();

    let target_id: i64 = match text.parse::<i64>() {
        Ok(n) if n > 0 => n,
        _ => {
            bot.send_message(
                msg.chat.id,
                "Invalid user ID — must be a positive integer. Try again:",
            )
            .await?;
            return Ok(());
        }
    };

    // Best-effort: delete the admin's typed message to keep the chat clean.
    let _ = bot.delete_message(msg.chat.id, msg.id).await;

    show_user_detail(bot, msg.chat.id, state, target_id).await
}

// ---------------------------------------------------------------------------
// perms:toggle:<key> → toggle project access
// ---------------------------------------------------------------------------

pub async fn handle_permissions_toggle(
    bot: Bot,
    query: CallbackQuery,
    state: Arc<AppState>,
    project_key: String,
) -> Result<()> {
    let _ = bot.answer_callback_query(query.id.clone()).await;

    let chat_id = match query.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };

    let (target_user_id, new_selected, message_id) = {
        let mut cs = state.chat_states.entry(chat_id.0).or_default();
        let perm = match cs.pending_permissions.as_mut() {
            Some(p) if p.target_user_id.is_some() => p,
            _ => return Ok(()),
        };

        if perm.selected.contains(&project_key) {
            perm.selected.remove(&project_key);
        } else {
            perm.selected.insert(project_key);
        }

        (perm.target_user_id, perm.selected.clone(), perm.message_id)
    };

    if let (Some(mid), Some(uid)) = (message_id, target_user_id) {
        let keyboard = build_user_detail_keyboard(&state, &new_selected, uid);
        let _ = bot
            .edit_message_reply_markup(chat_id, MessageId(mid))
            .reply_markup(keyboard)
            .await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// perms:done → persist + return to user list
// ---------------------------------------------------------------------------

pub async fn handle_permissions_done(
    bot: Bot,
    query: CallbackQuery,
    state: Arc<AppState>,
) -> Result<()> {
    let _ = bot.answer_callback_query(query.id.clone()).await;

    let chat_id = match query.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };

    let (target_user_id, selected, message_id) = {
        let cs = state.chat_states.get(&chat_id.0);
        match cs.as_ref().and_then(|c| c.pending_permissions.as_ref()) {
            Some(p) => (p.target_user_id, p.selected.clone(), p.message_id),
            None => return Ok(()),
        }
    };

    let target_user_id = match target_user_id {
        Some(id) => id,
        None => return Ok(()),
    };

    let all_projects = all_project_keys(&state);

    {
        let mut access = state.project_access.write().unwrap();
        for project in &all_projects {
            if selected.contains(project) {
                let ids = access.entry(project.clone()).or_default();
                if !ids.contains(&target_user_id) {
                    ids.push(target_user_id);
                }
            } else if let Some(ids) = access.get_mut(project) {
                ids.retain(|&id| id != target_user_id);
                if ids.is_empty() {
                    access.remove(project);
                }
            }
        }
    }

    persist_project_access(&state).ok();

    if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
        if let Some(p) = cs.pending_permissions.as_mut() {
            p.target_user_id = None;
            p.selected.clear();
            p.awaiting_user_id_input = false;
        }
    }

    let text = user_list_text(&state);
    let keyboard = build_user_list_keyboard(&state);

    if let Some(mid) = message_id {
        let _ = bot
            .edit_message_text(chat_id, MessageId(mid), text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// perms:revoke:<id> → remove from all project_access + return to user list
// ---------------------------------------------------------------------------

pub async fn handle_permissions_revoke(
    bot: Bot,
    query: CallbackQuery,
    state: Arc<AppState>,
    target_id: i64,
) -> Result<()> {
    let _ = bot.answer_callback_query(query.id.clone()).await;

    let chat_id = match query.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };

    let message_id = pending_message_id(&state, chat_id.0);

    {
        let mut access = state.project_access.write().unwrap();
        for ids in access.values_mut() {
            ids.retain(|&id| id != target_id);
        }
        access.retain(|_, ids| !ids.is_empty());
    }

    persist_project_access(&state).ok();

    if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
        if let Some(p) = cs.pending_permissions.as_mut() {
            p.target_user_id = None;
            p.selected.clear();
            p.awaiting_user_id_input = false;
        }
    }

    let text = user_list_text(&state);
    let keyboard = build_user_list_keyboard(&state);

    if let Some(mid) = message_id {
        let _ = bot
            .edit_message_text(chat_id, MessageId(mid), text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Shared: show user detail view
// ---------------------------------------------------------------------------

async fn show_user_detail(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    target_id: i64,
) -> Result<()> {
    let all_projects = all_project_keys(&state);

    let current_selected: HashSet<String> = {
        let access = state.project_access.read().unwrap();
        all_projects
            .iter()
            .filter(|pk| {
                access
                    .get(pk.as_str())
                    .map(|ids| ids.contains(&target_id))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    };

    let message_id = pending_message_id(&state, chat_id.0);

    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        entry.pending_permissions = Some(PendingPermissions {
            target_user_id: Some(target_id),
            selected: current_selected.clone(),
            message_id,
            awaiting_user_id_input: false,
        });
    }

    let name = user_display_name(&state, target_id);
    let is_admin_user = state.config.telegram.admin_user_id == Some(target_id);
    let is_in_allowed = state.config.telegram.allowed_user_ids.contains(&target_id);

    let mut header = format!(
        "👤 <b>{}</b> (<code>{}</code>)",
        html_escape(&name),
        target_id,
    );
    if is_admin_user {
        header.push_str("\n🔑 <i>Admin — full access to all features</i>");
    } else if is_in_allowed {
        header.push_str("\n✅ <i>In allowed_user_ids — base bot access</i>");
    }

    if all_projects.is_empty() {
        header.push_str("\n\n<i>No projects configured yet.</i>");
    } else {
        header.push_str("\n\nToggle project access, then tap <b>Done</b>.");
    }

    let keyboard = build_user_detail_keyboard(&state, &current_selected, target_id);

    if let Some(mid) = message_id {
        let _ = bot
            .edit_message_text(chat_id, MessageId(mid), header)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pending_message_id(state: &AppState, chat_id: i64) -> Option<i32> {
    state
        .chat_states
        .get(&chat_id)
        .and_then(|c| c.pending_permissions.as_ref().and_then(|p| p.message_id))
}

/// All user IDs known to the bot: union of allowed_user_ids and project_access values, sorted.
fn all_known_user_ids(state: &AppState) -> Vec<i64> {
    let mut ids: HashSet<i64> = state
        .config
        .telegram
        .allowed_user_ids
        .iter()
        .copied()
        .collect();
    let access = state.project_access.read().unwrap();
    for user_ids in access.values() {
        for &id in user_ids {
            ids.insert(id);
        }
    }
    let mut sorted: Vec<i64> = ids.into_iter().collect();
    sorted.sort();
    sorted
}

fn user_display_name(state: &AppState, user_id: i64) -> String {
    state
        .user_names
        .get(&user_id)
        .map(|n| n.clone())
        .unwrap_or_else(|| format!("User {}", user_id))
}

fn user_list_text(state: &AppState) -> String {
    let users = all_known_user_ids(state);
    if users.is_empty() {
        "👥 <b>Bot Users</b>\n\nNo users configured yet. Add one below.".to_string()
    } else {
        "👥 <b>Bot Users</b>\n\nSelect a user to manage their access:".to_string()
    }
}

fn build_user_list_keyboard(state: &AppState) -> InlineKeyboardMarkup {
    let users = all_known_user_ids(state);
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    for user_id in &users {
        let name = user_display_name(state, *user_id);
        let is_admin = state.config.telegram.admin_user_id == Some(*user_id);
        let is_allowed = state.config.telegram.allowed_user_ids.contains(user_id);

        let badge = if is_admin {
            " 🔑"
        } else if is_allowed {
            " ✅"
        } else {
            " 🔒"
        };

        rows.push(vec![InlineKeyboardButton::callback(
            format!("👤 {}{}", name, badge),
            format!("perms:user:{}", user_id),
        )]);
    }

    rows.push(vec![InlineKeyboardButton::callback(
        "➕ Add new user",
        "perms:add",
    )]);

    InlineKeyboardMarkup::new(rows)
}

fn build_user_detail_keyboard(
    state: &AppState,
    selected: &HashSet<String>,
    target_id: i64,
) -> InlineKeyboardMarkup {
    let jira_keys = jira_project_keys(state);
    let git_keys = git_project_keys(state);
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    if !jira_keys.is_empty() {
        rows.push(vec![InlineKeyboardButton::callback(
            "── 📋 Jira projects ──",
            "perms:noop",
        )]);
        for key in &jira_keys {
            let label = if selected.contains(key) {
                format!("✅ {}", key)
            } else {
                format!("⬜ {}", key)
            };
            rows.push(vec![InlineKeyboardButton::callback(
                label,
                format!("perms:toggle:{}", key),
            )]);
        }
    }

    if !git_keys.is_empty() {
        rows.push(vec![InlineKeyboardButton::callback(
            "── 📁 Git projects ──",
            "perms:noop",
        )]);
        for key in &git_keys {
            let label = if selected.contains(key) {
                format!("✅ {}", key)
            } else {
                format!("⬜ {}", key)
            };
            rows.push(vec![InlineKeyboardButton::callback(
                label,
                format!("perms:toggle:{}", key),
            )]);
        }
    }

    rows.push(vec![
        InlineKeyboardButton::callback("✔ Done", "perms:done"),
        InlineKeyboardButton::callback("🗑 Revoke all", format!("perms:revoke:{}", target_id)),
    ]);
    rows.push(vec![InlineKeyboardButton::callback(
        "🔙 Back",
        "perms:back",
    )]);

    InlineKeyboardMarkup::new(rows)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Sorted Jira project keys.
pub fn jira_project_keys(state: &AppState) -> Vec<String> {
    let mut keys: Vec<String> = state.config.jira.project_keys.clone();
    keys.sort();
    keys
}

/// Sorted git project keys (from git_map).
pub fn git_project_keys(state: &AppState) -> Vec<String> {
    let mut keys: Vec<String> = state.git_map.keys().cloned().collect();
    keys.sort();
    keys
}

/// Union of Jira project keys and git_map keys, sorted.
pub fn all_project_keys(state: &AppState) -> Vec<String> {
    let mut keys: HashSet<String> = state.config.jira.project_keys.iter().cloned().collect();
    for k in state.git_map.keys() {
        keys.insert(k.clone());
    }
    let mut sorted: Vec<String> = keys.into_iter().collect();
    sorted.sort();
    sorted
}

fn persist_project_access(state: &AppState) -> anyhow::Result<()> {
    let mut config = load_config(None)?;
    config.telegram.project_access = state.project_access.read().unwrap().clone();
    write_config(&config, None)?;
    Ok(())
}
