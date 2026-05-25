use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode};

use crate::bot::state::PendingPermissions;
use crate::bot::AppState;
use crate::config::loader::{load_config, write_config};

// ---------------------------------------------------------------------------
// Step 1: /permissions → ask for target user ID
// ---------------------------------------------------------------------------

pub async fn handle_permissions(bot: Bot, msg: Message, state: Arc<AppState>) -> Result<()> {
    {
        let mut entry = state.chat_states.entry(msg.chat.id.0).or_default();
        entry.pending_permissions = Some(PendingPermissions {
            target_user_id: None,
            selected: HashSet::new(),
            message_id: None,
        });
    }
    bot.send_message(
        msg.chat.id,
        "Enter the Telegram user ID to configure access for:",
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Step 2: admin typed a user ID → show project picker
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

    let all_projects = all_project_keys(&state);

    if all_projects.is_empty() {
        bot.send_message(msg.chat.id, "No projects configured.")
            .await?;
        if let Some(mut cs) = state.chat_states.get_mut(&msg.chat.id.0) {
            cs.pending_permissions = None;
        }
        return Ok(());
    }

    // Derive which projects this user currently has access to.
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

    let keyboard = build_keyboard(&all_projects, &current_selected);

    let sent = bot
        .send_message(
            msg.chat.id,
            format!(
                "Select projects for user <code>{}</code>:\n\
                 Tap a project to toggle access, then tap <b>Done</b>.",
                target_id
            ),
        )
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    {
        let mut entry = state.chat_states.entry(msg.chat.id.0).or_default();
        entry.pending_permissions = Some(PendingPermissions {
            target_user_id: Some(target_id),
            selected: current_selected,
            message_id: Some(sent.id.0),
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Step 3: toggle a project button
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

    let all_projects = all_project_keys(&state);
    let keyboard = build_keyboard(&all_projects, &new_selected);

    if let (Some(msg_id), Some(_)) = (message_id, target_user_id) {
        let _ = bot
            .edit_message_reply_markup(chat_id, MessageId(msg_id))
            .reply_markup(keyboard)
            .await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Step 4: Done → persist
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

    // Update the live in-memory map.
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

    // Persist to config file.
    let persist_ok = persist_project_access(&state).is_ok();

    // Clear pending state.
    if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
        cs.pending_permissions = None;
    }

    let status_line = if persist_ok {
        "Permissions saved."
    } else {
        "Permissions updated in memory but could not write to disk."
    };

    let reply = format!(
        "✅ {} Access for <code>{}</code>:\n{}",
        status_line,
        target_user_id,
        if selected.is_empty() {
            "— no projects".to_string()
        } else {
            let mut sorted: Vec<&String> = selected.iter().collect();
            sorted.sort();
            sorted
                .iter()
                .map(|k| format!("• {}", k))
                .collect::<Vec<_>>()
                .join("\n")
        }
    );

    if let Some(mid) = message_id {
        let _ = bot
            .edit_message_text(chat_id, MessageId(mid), reply)
            .parse_mode(ParseMode::Html)
            .await;
    } else {
        bot.send_message(chat_id, reply)
            .parse_mode(ParseMode::Html)
            .await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn build_keyboard(all_projects: &[String], selected: &HashSet<String>) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = all_projects
        .iter()
        .map(|key| {
            let label = if selected.contains(key) {
                format!("✅ {}", key)
            } else {
                format!("⬜ {}", key)
            };
            vec![InlineKeyboardButton::callback(
                label,
                format!("perms:toggle:{}", key),
            )]
        })
        .collect();

    rows.push(vec![InlineKeyboardButton::callback("✔ Done", "perms:done")]);

    InlineKeyboardMarkup::new(rows)
}

fn persist_project_access(state: &AppState) -> anyhow::Result<()> {
    let mut config = load_config(None)?;
    config.telegram.project_access = state.project_access.read().unwrap().clone();
    write_config(&config, None)?;
    Ok(())
}
