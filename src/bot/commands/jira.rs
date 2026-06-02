use std::sync::Arc;

use anyhow::Result;

use crate::bot::state::JiraPendingAction;
use crate::bot::utils::project_key_from_args;
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender, SentMessageRef};

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

pub async fn handle_jira(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
) -> Result<()> {
    let has_user_jira = state.has_user_jira(user_id);
    let jira_account_label = if has_user_jira {
        "\u{2699}\u{fe0f} Settings \u{2713}"
    } else {
        "\u{2699}\u{fe0f} Settings"
    };

    let rows = vec![
        vec![
            Button::new("\u{1f3ab} My Tickets", "jira:my_tickets"),
            Button::new("\u{270f}\u{fe0f} Create Ticket", "jira:create"),
        ],
        vec![
            Button::new("\u{1f500} Move Ticket", "jira:move"),
            Button::new("\u{1f4ac} Add Comment", "jira:comment"),
        ],
        vec![Button::new("\u{2705} Solve Ticket", "jira:solve")],
        vec![Button::new(jira_account_label, "jira:setup")],
    ];

    sender
        .send_with_keyboard(chat_id, "Jira \u{2014} choose an action:", rows)
        .await?;

    Ok(())
}

async fn prompt_for_title(
    sender: &Arc<dyn ChannelSender>,
    chat_id: &str,
    project_key: &str,
) -> Result<()> {
    sender
        .send(
            chat_id,
            &format!(
                "Project: <code>{}</code>\n\nSend the issue title:",
                sender.escape(project_key)
            ),
        )
        .await?;
    Ok(())
}

pub async fn handle_jira_action(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action_data: &str,
    msg_ref: Option<SentMessageRef>,
    state: Arc<AppState>,
) -> Result<()> {
    // Setup callbacks
    if action_data == "jira:setup" {
        return handle_jira_setup_start(Arc::clone(&sender), chat_id, state, user_id).await;
    }
    if action_data == "jira:setup_reconnect" {
        return start_url_step(&sender, chat_id, &state).await;
    }
    if action_data == "jira:setup_clear" {
        return handle_jira_clear(Arc::clone(&sender), chat_id, state, user_id).await;
    }

    // Project picker callbacks (step 4 of setup)
    if let Some(key) = action_data.strip_prefix("jira:setup_proj_toggle:") {
        let mref = msg_ref.unwrap_or_else(|| SentMessageRef {
            chat_id: chat_id.to_string(),
            message_id: "0".to_string(),
        });
        return handle_jira_setup_project_toggle(
            Arc::clone(&sender),
            chat_id,
            mref,
            state,
            user_id,
            key,
        )
        .await;
    }
    if action_data == "jira:setup_proj_done" {
        let mref = msg_ref.unwrap_or_else(|| SentMessageRef {
            chat_id: chat_id.to_string(),
            message_id: "0".to_string(),
        });
        return handle_jira_setup_project_done(Arc::clone(&sender), chat_id, mref, state, user_id)
            .await;
    }

    // Manage-projects callbacks (post-setup project picker)
    if action_data == "jira:projects" {
        return handle_jira_projects_start(Arc::clone(&sender), chat_id, state, user_id).await;
    }
    if let Some(key) = action_data.strip_prefix("jira:manage_proj_toggle:") {
        let mref = msg_ref.unwrap_or_else(|| SentMessageRef {
            chat_id: chat_id.to_string(),
            message_id: "0".to_string(),
        });
        return handle_jira_manage_project_toggle(Arc::clone(&sender), chat_id, mref, state, key)
            .await;
    }
    if action_data == "jira:manage_proj_done" {
        let mref = msg_ref.unwrap_or_else(|| SentMessageRef {
            chat_id: chat_id.to_string(),
            message_id: "0".to_string(),
        });
        return handle_jira_manage_project_done(Arc::clone(&sender), chat_id, mref, state, user_id)
            .await;
    }

    // Favorite-statuses callbacks
    if action_data == "jira:fav_statuses" {
        return handle_jira_fav_statuses_start(Arc::clone(&sender), chat_id, state, user_id).await;
    }
    if let Some(name) = action_data.strip_prefix("jira:fav_status_toggle:") {
        let mref = msg_ref.unwrap_or_else(|| SentMessageRef {
            chat_id: chat_id.to_string(),
            message_id: "0".to_string(),
        });
        return handle_jira_fav_status_toggle(Arc::clone(&sender), chat_id, mref, state, name)
            .await;
    }
    if action_data == "jira:fav_status_done" {
        let mref = msg_ref.unwrap_or_else(|| SentMessageRef {
            chat_id: chat_id.to_string(),
            message_id: "0".to_string(),
        });
        return handle_jira_fav_status_done(Arc::clone(&sender), chat_id, mref, state, user_id)
            .await;
    }

    // Step 3a: user confirmed Claude's description
    if action_data == "jira:create_confirm" {
        let pending = state
            .chat_states
            .get(chat_id)
            .and_then(|s| s.pending_jira_action.clone());

        if let Some(JiraPendingAction::CreateDescription(pk, title, suggested)) = pending {
            state
                .chat_states
                .entry(chat_id.to_string())
                .or_default()
                .pending_jira_action = None;
            return handle_create_confirm(
                Arc::clone(&sender),
                chat_id,
                state,
                user_id,
                &pk,
                &title,
                &suggested,
            )
            .await;
        }
        return Ok(());
    }

    // Step 1b: project selected from picker
    if let Some(pk) = action_data.strip_prefix("jira:create_project:") {
        state
            .chat_states
            .entry(chat_id.to_string())
            .or_default()
            .pending_jira_action = Some(JiraPendingAction::CreateTitle(pk.to_string()));
        return prompt_for_title(&sender, chat_id, pk).await;
    }

    match action_data {
        "jira:my_tickets" => handle_my_tickets(Arc::clone(&sender), chat_id, user_id, state).await,

        // Step 1a: show project picker (or skip if single project)
        "jira:create" => {
            let uid_i64 = user_id.parse::<i64>().unwrap_or(0);
            let projects = accessible_project_keys(uid_i64, &state);

            if projects.is_empty() {
                sender
                    .send(chat_id, "No Jira projects configured.")
                    .await?;
                return Ok(());
            }

            if projects.len() == 1 {
                let pk = projects.into_iter().next().unwrap();
                state
                    .chat_states
                    .entry(chat_id.to_string())
                    .or_default()
                    .pending_jira_action = Some(JiraPendingAction::CreateTitle(pk.clone()));
                return prompt_for_title(&sender, chat_id, &pk).await;
            }

            let buttons: Vec<Vec<Button>> = projects
                .iter()
                .map(|k| vec![Button::new(k.clone(), format!("jira:create_project:{k}"))])
                .collect();

            sender
                .send_with_keyboard(chat_id, "Select a project:", buttons)
                .await?;
            Ok(())
        }

        "jira:move" => {
            state
                .chat_states
                .entry(chat_id.to_string())
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::Move);
            sender
                .send(
                    chat_id,
                    "Send the issue key and target status:\n\
                     <code>MYAPP-123 In Progress</code>",
                )
                .await?;
            Ok(())
        }

        "jira:comment" => {
            state
                .chat_states
                .entry(chat_id.to_string())
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::Comment);
            sender
                .send(
                    chat_id,
                    "Send the issue key and comment text:\n\
                     <code>MYAPP-123 Fixed in PR #42</code>",
                )
                .await?;
            Ok(())
        }

        "jira:solve" => {
            state
                .chat_states
                .entry(chat_id.to_string())
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::Solve);
            sender
                .send(chat_id, "Send the issue key:\n<code>MYAPP-123</code>")
                .await?;
            Ok(())
        }

        _ => Ok(()),
    }
}

pub async fn handle_jira_input_with_text(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: JiraPendingAction,
    is_authorized_for_project: impl Fn(&str) -> bool,
    state: Arc<AppState>,
    text: String,
) -> Result<()> {
    // Setup steps don't clear pending action — they re-set it for the next step
    match &action {
        JiraPendingAction::JiraSetupUrl
        | JiraPendingAction::JiraSetupEmail(_)
        | JiraPendingAction::JiraSetupToken(_, _) => {
            state
                .chat_states
                .entry(chat_id.to_string())
                .or_default()
                .pending_jira_action = None;
            return handle_jira_setup_input(
                Arc::clone(&sender),
                chat_id,
                state,
                user_id,
                action,
                text,
            )
            .await;
        }
        JiraPendingAction::JiraSetupProjects(_, _, _, _, _)
        | JiraPendingAction::JiraManageProjects(_, _)
        | JiraPendingAction::JiraFavoriteStatuses(_, _) => {
            // Selection is via inline buttons; restore state and guide user.
            state
                .chat_states
                .entry(chat_id.to_string())
                .or_default()
                .pending_jira_action = Some(action.clone());
            sender
                .send(
                    chat_id,
                    "Use the buttons above to make your selection, then tap \u{2713} Done.",
                )
                .await?;
            return Ok(());
        }
        _ => {}
    }

    // Clear current pending state (suggest step will re-set it to CreateDescription)
    state
        .chat_states
        .entry(chat_id.to_string())
        .or_default()
        .pending_jira_action = None;

    match action {
        // Step 2: title received → suggest description
        JiraPendingAction::CreateTitle(project_key) => {
            handle_create_suggest(
                Arc::clone(&sender),
                chat_id,
                state,
                user_id,
                project_key,
                text,
            )
            .await
        }

        // Step 3b: user sent their own description instead of using Claude's
        JiraPendingAction::CreateDescription(pk, title, _suggested) => {
            handle_create_confirm(Arc::clone(&sender), chat_id, state, user_id, &pk, &title, &text)
                .await
        }

        JiraPendingAction::Move => {
            if let Some(pk) = project_key_from_args(&text) {
                if !is_authorized_for_project(&pk) {
                    sender
                        .send(chat_id, "Access denied for that project.")
                        .await?;
                    return Ok(());
                }
            }
            handle_move(Arc::clone(&sender), chat_id, state, user_id, text).await
        }

        JiraPendingAction::Comment => {
            if let Some(pk) = project_key_from_args(&text) {
                if !is_authorized_for_project(&pk) {
                    sender
                        .send(chat_id, "Access denied for that project.")
                        .await?;
                    return Ok(());
                }
            }
            handle_comment(Arc::clone(&sender), chat_id, state, user_id, text).await
        }

        JiraPendingAction::Solve => {
            if let Some(pk) = project_key_from_args(&text) {
                if !is_authorized_for_project(&pk) {
                    sender
                        .send(chat_id, "Access denied for that project.")
                        .await?;
                    return Ok(());
                }
            }
            handle_solve(Arc::clone(&sender), chat_id, state, user_id, text).await
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
