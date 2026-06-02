use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;

use crate::bot::state::PendingPermissions;
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender, Keyboard, SentMessageRef};
use crate::config::loader::{load_config, write_config};

// ---------------------------------------------------------------------------
// /permissions → show user list
// ---------------------------------------------------------------------------

pub async fn handle_permissions(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
) -> Result<()> {
    let text = user_list_text(&state);
    let keyboard = build_user_list_keyboard(&state);

    let sent = sender.send_with_keyboard(chat_id, &text, keyboard).await?;

    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.pending_permissions = Some(PendingPermissions {
            target_user_id: None,
            selected: HashSet::new(),
            message_id: Some(sent.message_id.clone()),
            awaiting_user_id_input: false,
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// perms:back → edit message to show user list
// ---------------------------------------------------------------------------

pub async fn handle_permissions_back(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    msg_ref: Option<SentMessageRef>,
) -> Result<()> {
    let message_id = pending_message_id(&state, chat_id);

    if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
        if let Some(p) = cs.pending_permissions.as_mut() {
            p.target_user_id = None;
            p.selected.clear();
            p.awaiting_user_id_input = false;
        }
    }

    let text = user_list_text(&state);
    let keyboard = build_user_list_keyboard(&state);

    let effective_ref = message_id
        .map(|mid| SentMessageRef::new(chat_id, mid))
        .or(msg_ref);

    if let Some(ref r) = effective_ref {
        sender.edit_with_keyboard(r, &text, keyboard).await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// perms:add → prompt admin to type a user ID
// ---------------------------------------------------------------------------

pub async fn handle_permissions_add(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
) -> Result<()> {
    let message_id = pending_message_id(&state, chat_id);

    if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
        if let Some(p) = cs.pending_permissions.as_mut() {
            p.awaiting_user_id_input = true;
            p.target_user_id = None;
        }
    }

    let keyboard: Keyboard = vec![vec![Button::new("\u{1f519} Back", "perms:back")]];

    if let Some(mid) = message_id {
        let r = SentMessageRef::new(chat_id, mid);
        sender
            .edit_with_keyboard(
                &r,
                "Enter the Telegram user ID to configure access for:",
                keyboard,
            )
            .await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// perms:user:<id> → show project picker for that user
// ---------------------------------------------------------------------------

pub async fn handle_permissions_user_select(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    target_id: i64,
) -> Result<()> {
    show_user_detail(sender, chat_id, state, target_id).await
}

// ---------------------------------------------------------------------------
// Text input: admin typed a user ID while awaiting_user_id_input
// ---------------------------------------------------------------------------

pub async fn handle_permissions_user_input(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    text: &str,
    state: Arc<AppState>,
) -> Result<()> {
    let target_id: i64 = match text.trim().parse::<i64>() {
        Ok(n) if n > 0 => n,
        _ => {
            sender
                .send(
                    chat_id,
                    "Invalid user ID \u{2014} must be a positive integer. Try again:",
                )
                .await?;
            return Ok(());
        }
    };

    show_user_detail(sender, chat_id, state, target_id).await
}

// ---------------------------------------------------------------------------
// perms:toggle:<key> → toggle project access
// ---------------------------------------------------------------------------

pub async fn handle_permissions_toggle(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    project_key: String,
) -> Result<()> {
    let (target_user_id_str, new_selected, message_id) = {
        let mut cs = state.chat_states.entry(chat_id.to_string()).or_default();
        let perm = match cs.pending_permissions.as_mut() {
            Some(p) if p.target_user_id.is_some() => p,
            _ => return Ok(()),
        };

        if perm.selected.contains(&project_key) {
            perm.selected.remove(&project_key);
        } else {
            perm.selected.insert(project_key);
        }

        (
            perm.target_user_id.clone(),
            perm.selected.clone(),
            perm.message_id.clone(),
        )
    };

    if let (Some(mid), Some(uid_str)) = (message_id, target_user_id_str) {
        let uid = uid_str.parse::<i64>().unwrap_or(0);
        let keyboard = build_user_detail_keyboard(&state, &new_selected, uid);
        let r = SentMessageRef::new(chat_id, mid);
        sender.edit_keyboard(&r, keyboard).await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// perms:done → persist + return to user list
// ---------------------------------------------------------------------------

pub async fn handle_permissions_done(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
) -> Result<()> {
    let (target_user_id_str, selected, message_id) = {
        let cs = state.chat_states.get(chat_id);
        match cs.as_ref().and_then(|c| c.pending_permissions.as_ref()) {
            Some(p) => (
                p.target_user_id.clone(),
                p.selected.clone(),
                p.message_id.clone(),
            ),
            None => return Ok(()),
        }
    };

    let target_user_id: i64 = match target_user_id_str.as_deref().and_then(|s| s.parse().ok()) {
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

    if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
        if let Some(p) = cs.pending_permissions.as_mut() {
            p.target_user_id = None;
            p.selected.clear();
            p.awaiting_user_id_input = false;
        }
    }

    let text = user_list_text(&state);
    let keyboard = build_user_list_keyboard(&state);

    if let Some(mid) = message_id {
        let r = SentMessageRef::new(chat_id, mid);
        sender.edit_with_keyboard(&r, &text, keyboard).await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// perms:revoke:<id> → remove from all project_access + return to user list
// ---------------------------------------------------------------------------

pub async fn handle_permissions_revoke(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    target_id: i64,
) -> Result<()> {
    let message_id = pending_message_id(&state, chat_id);

    {
        let mut access = state.project_access.write().unwrap();
        for ids in access.values_mut() {
            ids.retain(|&id| id != target_id);
        }
        access.retain(|_, ids| !ids.is_empty());
    }

    persist_project_access(&state).ok();

    if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
        if let Some(p) = cs.pending_permissions.as_mut() {
            p.target_user_id = None;
            p.selected.clear();
            p.awaiting_user_id_input = false;
        }
    }

    let text = user_list_text(&state);
    let keyboard = build_user_list_keyboard(&state);

    if let Some(mid) = message_id {
        let r = SentMessageRef::new(chat_id, mid);
        sender.edit_with_keyboard(&r, &text, keyboard).await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Shared: show user detail view
// ---------------------------------------------------------------------------

async fn show_user_detail(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
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

    let message_id = pending_message_id(&state, chat_id);

    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.pending_permissions = Some(PendingPermissions {
            target_user_id: Some(target_id.to_string()),
            selected: current_selected.clone(),
            message_id: message_id.clone(),
            awaiting_user_id_input: false,
        });
    }

    let name = user_display_name(&state, target_id);
    let is_admin_user = state.config.telegram.admin_user_id == Some(target_id);
    let is_in_allowed = state.config.telegram.allowed_user_ids.contains(&target_id);

    let mut header = format!(
        "\u{1f464} <b>{}</b> (<code>{}</code>)",
        html_escape(&name),
        target_id,
    );
    if is_admin_user {
        header.push_str("\n\u{1f511} <i>Admin \u{2014} full access to all features</i>");
    } else if is_in_allowed {
        header.push_str("\n\u{2705} <i>In allowed_user_ids \u{2014} base bot access</i>");
    }

    if all_projects.is_empty() {
        header.push_str("\n\n<i>No projects configured yet.</i>");
    } else {
        header.push_str("\n\nToggle project access, then tap <b>Done</b>.");
    }

    let keyboard = build_user_detail_keyboard(&state, &current_selected, target_id);

    if let Some(mid) = message_id {
        let r = SentMessageRef::new(chat_id, mid);
        sender.edit_with_keyboard(&r, &header, keyboard).await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pending_message_id(state: &AppState, chat_id: &str) -> Option<String> {
    state
        .chat_states
        .get(chat_id)
        .and_then(|c| c.pending_permissions.as_ref().and_then(|p| p.message_id.clone()))
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
        "\u{1f465} <b>Bot Users</b>\n\nNo users configured yet. Add one below.".to_string()
    } else {
        "\u{1f465} <b>Bot Users</b>\n\nSelect a user to manage their access:".to_string()
    }
}

fn build_user_list_keyboard(state: &AppState) -> Keyboard {
    let users = all_known_user_ids(state);
    let mut rows: Keyboard = Vec::new();

    for user_id in &users {
        let name = user_display_name(state, *user_id);
        let is_admin = state.config.telegram.admin_user_id == Some(*user_id);
        let is_allowed = state.config.telegram.allowed_user_ids.contains(user_id);

        let badge = if is_admin {
            " \u{1f511}"
        } else if is_allowed {
            " \u{2705}"
        } else {
            " \u{1f512}"
        };

        rows.push(vec![Button::new(
            format!("\u{1f464} {}{}", name, badge),
            format!("perms:user:{}", user_id),
        )]);
    }

    rows.push(vec![Button::new(
        "\u{2795} Add new user",
        "perms:add",
    )]);

    rows
}

fn build_user_detail_keyboard(
    state: &AppState,
    selected: &HashSet<String>,
    target_id: i64,
) -> Keyboard {
    let jira_keys = jira_project_keys(state);
    let git_keys = git_project_keys(state);
    let mut rows: Keyboard = Vec::new();

    if !jira_keys.is_empty() {
        rows.push(vec![Button::new(
            "\u{2500}\u{2500} \u{1f4cb} Jira projects \u{2500}\u{2500}",
            "perms:noop",
        )]);
        for key in &jira_keys {
            let label = if selected.contains(key) {
                format!("\u{2705} {}", key)
            } else {
                format!("\u{2b1c} {}", key)
            };
            rows.push(vec![Button::new(label, format!("perms:toggle:{}", key))]);
        }
    }

    if !git_keys.is_empty() {
        rows.push(vec![Button::new(
            "\u{2500}\u{2500} \u{1f4c1} Git projects \u{2500}\u{2500}",
            "perms:noop",
        )]);
        for key in &git_keys {
            let label = if selected.contains(key) {
                format!("\u{2705} {}", key)
            } else {
                format!("\u{2b1c} {}", key)
            };
            rows.push(vec![Button::new(label, format!("perms:toggle:{}", key))]);
        }
    }

    rows.push(vec![
        Button::new("\u{2714} Done", "perms:done"),
        Button::new(
            "\u{1f5d1} Revoke all",
            format!("perms:revoke:{}", target_id),
        ),
    ]);
    rows.push(vec![Button::new("\u{1f519} Back", "perms:back")]);

    rows
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Sorted Jira project keys — union of global config and all user_jira configs.
pub fn jira_project_keys(state: &AppState) -> Vec<String> {
    let mut keys: std::collections::HashSet<String> = state
        .config
        .jira
        .as_ref()
        .map(|j| j.project_keys.iter().cloned().collect())
        .unwrap_or_default();
    for ucfg in state.config.user_jira.values() {
        for k in &ucfg.project_keys {
            keys.insert(k.clone());
        }
    }
    let mut sorted: Vec<String> = keys.into_iter().collect();
    sorted.sort();
    sorted
}

/// Sorted git project keys (from git_map).
pub fn git_project_keys(state: &AppState) -> Vec<String> {
    let mut keys: Vec<String> = state.git_map.keys().cloned().collect();
    keys.sort();
    keys
}

/// Union of Jira project keys and git_map keys, sorted.
pub fn all_project_keys(state: &AppState) -> Vec<String> {
    let mut keys: HashSet<String> = state
        .config
        .jira
        .as_ref()
        .map(|j| j.project_keys.iter().cloned().collect())
        .unwrap_or_default();
    for ucfg in state.config.user_jira.values() {
        for k in &ucfg.project_keys {
            keys.insert(k.clone());
        }
    }
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
