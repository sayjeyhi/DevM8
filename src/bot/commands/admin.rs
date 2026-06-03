use std::sync::Arc;

use anyhow::Result;

use crate::bot::state::AdminPendingAction;
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender};

use super::{
    handle_add_project, handle_audit_logs, handle_clone, handle_logs, handle_permissions,
    handle_status,
};

pub async fn handle_admin(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    _state: Arc<AppState>,
) -> Result<()> {
    let keyboard = vec![
        vec![Button::new("\u{1f510} Permissions", "admin:permissions")],
        vec![Button::new("\u{1f4cb} Logs", "admin:logs")],
        vec![Button::new("\u{1f50d} Audit Logs", "admin:audit_logs")],
        vec![Button::new("\u{2795} Add Project", "admin:add_project")],
        vec![Button::new("\u{1f4e5} Clone Repo", "admin:clone")],
        vec![Button::new("\u{1f4ca} Status", "admin:status")],
    ];

    sender
        .send_with_keyboard(chat_id, "Admin Panel \u{2014} choose an action:", keyboard)
        .await?;

    Ok(())
}

pub async fn handle_admin_action(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    _user_id: &str,
    action_data: &str,
    state: Arc<AppState>,
) -> Result<()> {
    match action_data {
        "admin:permissions" => handle_permissions(Arc::clone(&sender), chat_id, state).await,
        "admin:logs" => handle_logs(Arc::clone(&sender), chat_id, state, String::new()).await,
        "admin:audit_logs" => {
            handle_audit_logs(Arc::clone(&sender), chat_id, state, String::new()).await
        }
        "admin:status" => handle_status(Arc::clone(&sender), chat_id, state).await,
        "admin:clone" => {
            state
                .chat_states
                .entry(chat_id.to_string())
                .or_default()
                .pending_admin_action = Some(AdminPendingAction::Clone);
            sender
                .send(
                    chat_id,
                    "Send the SSH URL and destination path:\n\
                     <code>git@github.com:org/repo.git /home/user/projects</code>",
                )
                .await?;
            Ok(())
        }
        "admin:add_project" => {
            state
                .chat_states
                .entry(chat_id.to_string())
                .or_default()
                .pending_admin_action = Some(AdminPendingAction::AddProject);
            sender
                .send(
                    chat_id,
                    "Send the local path and project name:\n\
                     <code>/home/user/my-app MY_APP</code>",
                )
                .await?;
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Legacy callback handler kept for polling.rs compatibility.
pub async fn handle_admin_callback(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action_data: &str,
    state: Arc<AppState>,
) -> Result<()> {
    handle_admin_action(sender, chat_id, user_id, action_data, state).await
}

pub async fn handle_admin_input(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    _user_id: &str,
    text: &str,
    state: Arc<AppState>,
    action: AdminPendingAction,
) -> Result<()> {
    state
        .chat_states
        .entry(chat_id.to_string())
        .or_default()
        .pending_admin_action = None;

    match action {
        AdminPendingAction::Clone => {
            handle_clone(Arc::clone(&sender), chat_id, state, text.to_string()).await
        }
        AdminPendingAction::AddProject => {
            handle_add_project(Arc::clone(&sender), chat_id, state, text.to_string()).await
        }
    }
}
