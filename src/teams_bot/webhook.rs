use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::bot::AppState;
use crate::channel::state::{AdminPendingAction, JiraPendingAction};
use crate::logger::Logger;

use super::sender::TeamsSender;
use super::types::TeamsActivity;

// ---------------------------------------------------------------------------
// Axum shared state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct WebhookState {
    app_state: Arc<AppState>,
    /// Shared sender — keeps the OAuth token and message-text cache alive across requests.
    sender: Arc<TeamsSender>,
    logger: Arc<dyn Logger>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn run_webhook(
    ct: CancellationToken,
    state: Arc<AppState>,
    logger: &Arc<dyn Logger>,
    sender: Arc<TeamsSender>,
    port: u16,
) -> anyhow::Result<()> {
    let ws = WebhookState {
        app_state: state,
        sender,
        logger: Arc::clone(logger),
    };

    let app = Router::new()
        .route("/api/messages", post(handle_activity))
        .with_state(ws);

    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    logger.info(
        "teams webhook listening",
        Some(&json!({ "addr": addr })),
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(async move { ct.cancelled().await })
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP handler — ACK immediately, dispatch in background
// ---------------------------------------------------------------------------

async fn handle_activity(
    State(ws): State<WebhookState>,
    Json(activity): Json<TeamsActivity>,
) -> impl IntoResponse {
    let activity_type = activity.activity_type.as_str();

    ws.logger.debug(
        "teams activity received",
        Some(&json!({
            "type": activity_type,
            "user": activity.user_id(),
            "conv": activity.conversation_id(),
        })),
    );

    match activity_type {
        "message" => {
            tokio::spawn(dispatch_message(ws, activity));
        }
        "conversationUpdate" => {
            // Optionally greet new members — ignored for now.
        }
        other => {
            ws.logger
                .debug(&format!("teams: unhandled activity type: {other}"), None);
        }
    }

    StatusCode::OK
}

// ---------------------------------------------------------------------------
// Message dispatcher — routes button clicks vs plain text
// ---------------------------------------------------------------------------

async fn dispatch_message(ws: WebhookState, activity: TeamsActivity) {
    let user_id = activity.user_id().to_string();
    let chat_id = activity.chat_id();

    // Cache the user's display name.
    if !activity.user_name().is_empty() {
        ws.app_state
            .teams_user_names
            .insert(user_id.clone(), activity.user_name().to_string());
    }

    if !ws.app_state.teams_is_authorized(&user_id) {
        ws.logger.warn(
            "teams: unauthorized message",
            Some(&json!({ "user_id": user_id })),
        );
        return;
    }

    let sender: Arc<dyn crate::channel::ChannelSender> = ws.sender.clone();

    // Button click — Adaptive Card Action.Submit.
    if let Some(data) = activity.button_data() {
        let data = data.to_string();
        dispatch_action(&ws.app_state, sender, &chat_id, &user_id, &data, &ws.logger).await;
        return;
    }

    // Plain text message.
    let text = activity
        .text
        .as_deref()
        .map(|t| t.trim())
        .unwrap_or("")
        .to_string();

    if text.is_empty() {
        return;
    }

    dispatch_text(
        &ws.app_state,
        sender,
        &chat_id,
        &user_id,
        &text,
        &ws.logger,
    )
    .await;
}

// ---------------------------------------------------------------------------
// Text router — handles slash commands and pending state logic
// ---------------------------------------------------------------------------

async fn dispatch_text(
    state: &Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    logger: &Arc<dyn Logger>,
) {
    // Slash commands are regular text messages that start with '/'.
    if text.starts_with('/') {
        let (cmd, args) = text
            .splitn(2, ' ')
            .collect::<Vec<_>>()
            .split_first()
            .map(|(c, rest)| (*c, rest.first().copied().unwrap_or("").trim()))
            .unwrap_or((text, ""));

        // Clear pending states on any new slash command.
        clear_pending_states(state, chat_id);

        match cmd {
            "/ask" | "/start" => {
                dispatch_ask_command(
                    Arc::clone(state),
                    sender,
                    chat_id,
                    user_id,
                    args,
                    logger,
                )
                .await;
            }
            "/jira" => {
                dispatch_jira_command(Arc::clone(state), sender, chat_id, user_id, logger).await;
            }
            "/help" => {
                dispatch_help_command(Arc::clone(state), sender, chat_id, logger).await;
            }
            "/admin" => {
                if !state.teams_is_admin(user_id) {
                    let _ = sender.send(chat_id, "Access denied. This command is admin-only.").await;
                    return;
                }
                dispatch_admin_command(Arc::clone(state), sender, chat_id, logger).await;
            }
            other => {
                let _ = sender
                    .send(chat_id, &format!("Unknown command: `{other}`. Try /help"))
                    .await;
            }
        }
        return;
    }

    // Pending state routing (mirrors Telegram/Slack dispatch_message logic).
    let pending_admin = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_admin_action.clone());
    if let Some(action) = pending_admin {
        dispatch_admin_input(
            Arc::clone(state),
            sender,
            chat_id,
            user_id,
            text,
            action,
            logger,
        )
        .await;
        return;
    }

    let pending_jira = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_jira_action.clone());
    if let Some(action) = pending_jira {
        dispatch_jira_input(
            Arc::clone(state),
            sender,
            chat_id,
            user_id,
            text,
            action,
            logger,
        )
        .await;
        return;
    }

    let waiting_for_user_id = state
        .chat_states
        .get(chat_id)
        .map(|s| {
            s.pending_permissions
                .as_ref()
                .map(|p| p.awaiting_user_id_input)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if waiting_for_user_id {
        dispatch_permissions_user_input(Arc::clone(state), sender, chat_id, text, logger).await;
        return;
    }

    let pending_comment = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_comment.clone());
    if let Some((issue_key,)) = pending_comment {
        dispatch_pending_comment(
            Arc::clone(state),
            sender,
            chat_id,
            user_id,
            text,
            &issue_key,
            logger,
        )
        .await;
        return;
    }

    let awaiting_branch_name = state
        .chat_states
        .get(chat_id)
        .map(|s| {
            s.pending_solve
                .as_ref()
                .map(|p| p.awaiting_branch_name)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    if awaiting_branch_name {
        dispatch_solve_branch_name_input(Arc::clone(state), sender, chat_id, user_id, text, logger)
            .await;
        return;
    }

    let has_pending_grill = state
        .chat_states
        .get(chat_id)
        .map(|s| s.pending_grill.is_some())
        .unwrap_or(false);
    if has_pending_grill {
        dispatch_grill_answer(Arc::clone(state), sender, chat_id, user_id, text, logger).await;
        return;
    }

    let has_pending_ask = state
        .chat_states
        .get(chat_id)
        .map(|s| s.pending_ask.is_some())
        .unwrap_or(false);
    if has_pending_ask {
        dispatch_ask_text_input(Arc::clone(state), sender, chat_id, user_id, text, logger).await;
        return;
    }

    // Default — treat free text as an ask session message.
    dispatch_ask_session(Arc::clone(state), sender, chat_id, text, logger).await;
}

// ---------------------------------------------------------------------------
// Action (button click) router
// ---------------------------------------------------------------------------

async fn dispatch_action(
    state: &Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    logger: &Arc<dyn Logger>,
) {
    let prefix = action.split(':').next().unwrap_or(action);

    match prefix {
        "admin" => {
            if !state.teams_is_admin(user_id) {
                let _ = sender.send(chat_id, "Access denied.").await;
                return;
            }
            dispatch_admin_action(Arc::clone(state), sender, chat_id, user_id, action, logger)
                .await;
        }
        "jira" => {
            let auth_ok = {
                let parts: Vec<&str> = action.splitn(3, ':').collect();
                if parts.len() == 3 && parts[1] == "project" {
                    state.teams_is_authorized_for_project(user_id, parts[2])
                } else {
                    true
                }
            };
            if !auth_ok {
                let _ = sender
                    .send(chat_id, "Access denied for that project.")
                    .await;
                return;
            }
            dispatch_jira_action(Arc::clone(state), sender, chat_id, user_id, action, logger)
                .await;
        }
        "tickets" => {
            dispatch_my_tickets_action(
                Arc::clone(state),
                sender,
                chat_id,
                user_id,
                action,
                logger,
            )
            .await;
        }
        "ask" => {
            dispatch_ask_action(Arc::clone(state), sender, chat_id, user_id, action, logger).await;
        }
        "solve" => {
            dispatch_solve_action(Arc::clone(state), sender, chat_id, user_id, action, logger)
                .await;
        }
        other => {
            logger.debug(
                &format!("teams: unhandled action prefix: {other}"),
                None,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Platform-agnostic command dispatchers (mirrors slack_bot/socket.rs)
// ---------------------------------------------------------------------------

use crate::bot::commands::{
    ask_with_session, handle_admin, handle_admin_callback, handle_admin_input, handle_ask,
    handle_ask_session_callback, handle_ask_text_input, handle_grill_answer, handle_help,
    handle_jira, handle_jira_action, handle_jira_input_with_text, handle_my_tickets_callback,
    handle_pending_comment, handle_permissions_add, handle_permissions_back,
    handle_permissions_done, handle_permissions_revoke, handle_permissions_toggle,
    handle_permissions_user_input, handle_permissions_user_select, handle_post_analysis_implement,
    handle_solve_action_callback, handle_solve_branch_name_input, handle_solve_repo_callback,
};

fn teams_auth_fn(state: Arc<AppState>, user_id: String) -> impl Fn(&str) -> bool {
    move |pk: &str| state.teams_is_authorized_for_project(&user_id, pk)
}

async fn dispatch_ask_command(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let auth = teams_auth_fn(Arc::clone(&state), user_id.to_string());
    let _ = handle_ask(sender, chat_id, state, text.to_string(), user_id, auth).await;
}

async fn dispatch_jira_command(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_jira(sender, chat_id, user_id, state).await;
}

async fn dispatch_help_command(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_help(sender, chat_id, state).await;
}

async fn dispatch_admin_command(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_admin(sender, chat_id, state).await;
}

async fn dispatch_admin_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_admin_callback(sender, chat_id, user_id, action, state).await;
}

async fn dispatch_jira_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_jira_action(sender, chat_id, user_id, action, None, state).await;
}

async fn dispatch_my_tickets_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_my_tickets_callback(sender, chat_id, user_id, action, state).await;
}

async fn dispatch_ask_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    _logger: &Arc<dyn Logger>,
) {
    let auth = teams_auth_fn(Arc::clone(&state), user_id.to_string());
    let _ = handle_ask_session_callback(sender, chat_id, user_id, action, state, auth).await;
}

async fn dispatch_solve_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    _logger: &Arc<dyn Logger>,
) {
    if action == "solve:cancel" {
        let cancelled = {
            let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
            match entry.cancel_token.take() {
                Some(ct) => {
                    ct.cancel();
                    true
                }
                None => false,
            }
        };
        if !cancelled {
            let _ = sender.send(chat_id, "No active request to cancel.").await;
        }
        return;
    }

    if action.starts_with("solve:repo:") {
        let _ = handle_solve_repo_callback(Arc::clone(&sender), chat_id, user_id, state, action)
            .await;
        return;
    }
    if let Some(issue_key) = action.strip_prefix("solve:post:implement:") {
        let _ =
            handle_post_analysis_implement(Arc::clone(&sender), chat_id, state, user_id, issue_key)
                .await;
        return;
    }
    if action.starts_with("solve:action:") {
        let parts: Vec<&str> = action.splitn(4, ':').collect();
        if parts.len() == 4 {
            let _ = handle_solve_action_callback(
                Arc::clone(&sender),
                chat_id,
                state,
                user_id,
                parts[2],
                parts[3],
            )
            .await;
        }
        return;
    }
    if action.starts_with("solve:branch:") {
        let parts: Vec<&str> = action.splitn(4, ':').collect();
        if parts.len() == 4 {
            let _ = crate::bot::commands::solve::handle_branch_choice(
                sender, chat_id, state, user_id, parts[2], parts[3],
            )
            .await;
        }
    }
}

async fn dispatch_admin_input(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    action: AdminPendingAction,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_admin_input(sender, chat_id, user_id, text, state, action).await;
}

async fn dispatch_jira_input(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    action: JiraPendingAction,
    _logger: &Arc<dyn Logger>,
) {
    let auth = teams_auth_fn(Arc::clone(&state), user_id.to_string());
    let _ = handle_jira_input_with_text(
        sender,
        chat_id,
        user_id,
        action,
        auth,
        state,
        text.to_string(),
    )
    .await;
}

async fn dispatch_permissions_user_input(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_permissions_user_input(sender, chat_id, text, state).await;
}

async fn dispatch_pending_comment(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    issue_key: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ =
        handle_pending_comment(sender, chat_id, user_id, text, state, issue_key.to_string()).await;
}

async fn dispatch_solve_branch_name_input(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ =
        handle_solve_branch_name_input(sender, chat_id, state, user_id, text.to_string()).await;
}

async fn dispatch_grill_answer(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_grill_answer(sender, chat_id, user_id, state, text.to_string()).await;
}

async fn dispatch_ask_text_input(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_ask_text_input(sender, chat_id, user_id, text.to_string(), state).await;
}

async fn dispatch_ask_session(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = ask_with_session(sender, chat_id, state, text.to_string()).await;
}

// Perms actions — handled via admin callbacks (Telegram user-ID-based).
// Teams user IDs are AAD strings; the perms panel targets Telegram user IDs,
// so these are forwarded to handle_admin_callback which routes them further.
#[allow(dead_code)]
async fn dispatch_permissions_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::ChannelSender>,
    chat_id: &str,
    _user_id: &str,
    action: &str,
    _logger: &Arc<dyn Logger>,
) {
    if action == "perms:done" {
        let _ = handle_permissions_done(sender, chat_id, state).await;
        return;
    }
    if action == "perms:back" {
        let _ = handle_permissions_back(sender, chat_id, state, None).await;
        return;
    }
    if action == "perms:add" {
        let _ = handle_permissions_add(sender, chat_id, state).await;
        return;
    }
    if let Some(key) = action.strip_prefix("perms:toggle:") {
        let _ = handle_permissions_toggle(sender, chat_id, state, key.to_string()).await;
        return;
    }
    if let Some(rest) = action.strip_prefix("perms:user:") {
        if let Ok(target_id) = rest.parse::<i64>() {
            let _ = handle_permissions_user_select(sender, chat_id, state, target_id).await;
        }
        return;
    }
    if let Some(rest) = action.strip_prefix("perms:revoke:") {
        if let Ok(target_id) = rest.parse::<i64>() {
            let _ = handle_permissions_revoke(sender, chat_id, state, target_id).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn clear_pending_states(state: &Arc<AppState>, chat_id: &str) {
    if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
        cs.pending_comment = None;
        cs.pending_ask = None;
        cs.pending_jira_action = None;
        cs.pending_admin_action = None;
        cs.pending_slack_reply = None;
        cs.pending_solve = None;
        cs.pending_permissions = None;
    }
}
