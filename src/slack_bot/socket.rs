use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use crate::bot::AppState;
use crate::channel::state::{AdminPendingAction, JiraPendingAction};
use crate::logger::Logger;

use super::sender::SlackSender;
use super::types::{AckFrame, ConnectionsOpenResponse, SocketEnvelope};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Connects to Slack via Socket Mode and dispatches events until cancelled.
/// Reconnects on disconnect with exponential backoff (start 2s, max 60s).
pub async fn run_socket_loop(
    ct: CancellationToken,
    state: Arc<AppState>,
    logger: &Arc<dyn Logger>,
) -> anyhow::Result<()> {
    let (bot_token, app_token) = {
        let sc = state
            .config
            .slack
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no slack config"))?;
        (
            sc.bot_token
                .clone()
                .ok_or_else(|| anyhow::anyhow!("slack.bot_token not set"))?,
            sc.app_token
                .clone()
                .ok_or_else(|| anyhow::anyhow!("slack.app_token not set"))?,
        )
    };

    let mut backoff = Duration::from_secs(2);
    const MAX_BACKOFF: Duration = Duration::from_secs(60);

    loop {
        if ct.is_cancelled() {
            return Ok(());
        }

        logger.info("slack socket mode: connecting", None);

        match connect_and_run(
            ct.clone(),
            Arc::clone(&state),
            logger,
            &bot_token,
            &app_token,
        )
        .await
        {
            Ok(()) => {
                // Returned because ct was cancelled.
                return Ok(());
            }
            Err(e) => {
                if ct.is_cancelled() {
                    return Ok(());
                }
                logger.warn(
                    &format!("slack socket mode disconnected: {e}, reconnecting in {backoff:?}"),
                    None,
                );
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = ct.cancelled() => { return Ok(()); }
                }
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Single connection lifetime
// ---------------------------------------------------------------------------

async fn connect_and_run(
    ct: CancellationToken,
    state: Arc<AppState>,
    logger: &Arc<dyn Logger>,
    bot_token: &str,
    app_token: &str,
) -> anyhow::Result<()> {
    let wss_url = get_wss_url(app_token).await?;
    logger.info(
        "slack socket mode: websocket url obtained",
        Some(&json!({ "url_prefix": wss_url.get(..40).unwrap_or(&wss_url) })),
    );

    let (ws_stream, _) = tokio_tungstenite::connect_async(&wss_url).await?;
    let (mut write, mut read) = ws_stream.split();

    // Reset backoff on successful connection.
    logger.info("slack socket mode: connected", None);

    loop {
        tokio::select! {
            _ = ct.cancelled() => {
                logger.info("slack socket mode: cancellation received, closing", None);
                let _ = write.close().await;
                return Ok(());
            }
            msg = read.next() => {
                match msg {
                    None => {
                        return Err(anyhow::anyhow!("websocket stream closed"));
                    }
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!("websocket error: {e}"));
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_frame(
                            &text,
                            &mut write,
                            Arc::clone(&state),
                            logger,
                            bot_token,
                        )
                        .await
                        {
                            logger.warn(
                                &format!("slack frame handler error: {e}"),
                                None,
                            );
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = write.send(Message::Pong(data)).await;
                    }
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Frame handler
// ---------------------------------------------------------------------------

async fn handle_frame(
    text: &str,
    write: &mut (impl SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin),
    state: Arc<AppState>,
    logger: &Arc<dyn Logger>,
    bot_token: &str,
) -> anyhow::Result<()> {
    let envelope: SocketEnvelope = match serde_json::from_str(text) {
        Ok(e) => e,
        Err(e) => {
            logger.warn(&format!("failed to parse socket envelope: {e}"), None);
            return Ok(());
        }
    };

    // ACK immediately.
    let ack = serde_json::to_string(&AckFrame {
        envelope_id: envelope.envelope_id.clone(),
        payload: String::new(),
    })?;
    write.send(Message::Text(ack.into())).await.ok();

    logger.debug(
        "slack socket event",
        Some(&json!({ "type": envelope.r#type })),
    );

    match envelope.r#type.as_str() {
        "hello" => {
            logger.info("slack socket mode: hello received", None);
        }
        "slash_commands" => {
            dispatch_slash_command(state, logger, bot_token, &envelope.payload).await;
        }
        "interactive" => {
            dispatch_interactive(state, logger, bot_token, &envelope.payload).await;
        }
        "events_api" => {
            dispatch_event(state, logger, bot_token, &envelope.payload).await;
        }
        other => {
            logger.debug(
                &format!("slack socket mode: unhandled event type: {other}"),
                None,
            );
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Get WSS URL from apps.connections.open
// ---------------------------------------------------------------------------

async fn get_wss_url(app_token: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://slack.com/api/apps.connections.open")
        .bearer_auth(app_token)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body("")
        .send()
        .await?
        .json::<ConnectionsOpenResponse>()
        .await?;

    if resp.ok {
        resp.url
            .ok_or_else(|| anyhow::anyhow!("apps.connections.open returned no url"))
    } else {
        Err(anyhow::anyhow!(
            "apps.connections.open failed: {}",
            resp.error.unwrap_or_else(|| "unknown".into())
        ))
    }
}

// ---------------------------------------------------------------------------
// Slash command dispatcher
// ---------------------------------------------------------------------------

async fn dispatch_slash_command(
    state: Arc<AppState>,
    logger: &Arc<dyn Logger>,
    bot_token: &str,
    payload: &serde_json::Value,
) {
    let user_id = payload
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let channel_id = payload
        .get("channel_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let command = payload
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let text = payload
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if !state.slack_is_authorized(&user_id) {
        logger.warn(
            "slack: unauthorized slash command attempt",
            Some(&json!({ "user_id": user_id, "command": command })),
        );
        return;
    }

    logger.info(
        "slack command received",
        Some(&json!({ "cmd": command, "user_id": user_id, "channel_id": channel_id })),
    );

    let sender: Arc<dyn crate::channel::sender::ChannelSender> =
        Arc::new(SlackSender::new(bot_token.to_string()));
    let chat_id = channel_id.as_str();

    // Clear pending states on any new slash command.
    clear_slack_pending_states(&state, chat_id);

    match command.as_str() {
        "/ask" | "/start" => {
            dispatch_ask_command(state, sender, chat_id, &user_id, &text, logger).await;
        }
        "/jira" => {
            dispatch_jira_command(state, sender, chat_id, &user_id, logger).await;
        }
        "/help" => {
            dispatch_help_command(state, sender, chat_id, &user_id, logger).await;
        }
        "/admin" => {
            if !state.slack_is_admin(&user_id) {
                let _ = sender
                    .send(chat_id, "Access denied. This command is admin-only.")
                    .await;
                return;
            }
            dispatch_admin_command(state, sender, chat_id, &user_id, logger).await;
        }
        other => {
            logger.debug(&format!("slack: unhandled slash command: {other}"), None);
        }
    }
}

// ---------------------------------------------------------------------------
// Interactive (block actions) dispatcher
// ---------------------------------------------------------------------------

async fn dispatch_interactive(
    state: Arc<AppState>,
    logger: &Arc<dyn Logger>,
    bot_token: &str,
    payload: &serde_json::Value,
) {
    // Block actions payload nests under "payload" for Socket Mode interactive events.
    let inner = if payload.get("type").and_then(|v| v.as_str()) == Some("block_actions") {
        payload
    } else {
        match payload.get("payload") {
            Some(p) => p,
            None => payload,
        }
    };

    let user_id = inner
        .pointer("/user/id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let channel_id = inner
        .pointer("/channel/id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let action_value = inner
        .pointer("/actions/0/value")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if !state.slack_is_authorized(&user_id) {
        logger.warn(
            "slack: unauthorized block action",
            Some(&json!({ "user_id": user_id })),
        );
        return;
    }

    logger.debug(
        "slack block action",
        Some(&json!({ "action": action_value, "user_id": user_id })),
    );

    let sender: Arc<dyn crate::channel::sender::ChannelSender> =
        Arc::new(SlackSender::new(bot_token.to_string()));
    let chat_id = channel_id.as_str();
    let prefix = action_value.split(':').next().unwrap_or(&action_value);

    match prefix {
        "admin" => {
            if !state.slack_is_admin(&user_id) {
                let _ = sender.send(chat_id, "Access denied.").await;
                return;
            }
            dispatch_admin_action(state, sender, chat_id, &user_id, &action_value, logger).await;
        }
        "jira" => {
            let auth_ok = {
                // Check project-level auth for jira:project:KEY actions.
                let parts: Vec<&str> = action_value.splitn(3, ':').collect();
                if parts.len() == 3 && parts[1] == "project" {
                    state.slack_is_authorized_for_project(&user_id, parts[2])
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
            dispatch_jira_action(state, sender, chat_id, &user_id, &action_value, logger).await;
        }
        "tickets" => {
            dispatch_my_tickets_action(state, sender, chat_id, &user_id, &action_value, logger)
                .await;
        }
        "ask" => {
            dispatch_ask_action(state, sender, chat_id, &user_id, &action_value, logger).await;
        }
        "solve" => {
            dispatch_solve_action(state, sender, chat_id, &user_id, &action_value, logger).await;
        }
        "perms" => {
            if !state.slack_is_admin(&user_id) {
                let _ = sender.send(chat_id, "Access denied.").await;
                return;
            }
            dispatch_permissions_action(state, sender, chat_id, &user_id, &action_value, logger)
                .await;
        }
        other => {
            logger.debug(
                &format!("slack: unhandled block action prefix: {other}"),
                None,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Events API dispatcher
// ---------------------------------------------------------------------------

async fn dispatch_event(
    state: Arc<AppState>,
    logger: &Arc<dyn Logger>,
    bot_token: &str,
    payload: &serde_json::Value,
) {
    let event = match payload.get("event") {
        Some(e) => e,
        None => return,
    };

    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let channel_type = event
        .get("channel_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Only handle direct messages to the bot.
    if event_type != "message" || channel_type != "im" {
        return;
    }

    // Skip bot's own messages.
    if event.get("bot_id").is_some() || event.get("subtype").is_some() {
        return;
    }

    let user_id = event
        .get("user")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let channel_id = event
        .get("channel")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let text = event
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if !state.slack_is_authorized(&user_id) {
        logger.warn(
            "slack: unauthorized DM",
            Some(&json!({ "user_id": user_id })),
        );
        return;
    }

    if text.is_empty() {
        return;
    }

    let sender: Arc<dyn crate::channel::sender::ChannelSender> =
        Arc::new(SlackSender::new(bot_token.to_string()));
    // For DMs, the channel_id IS the DM channel (same as user's conversation).
    let chat_id = channel_id.as_str();

    dispatch_dm_message(state, sender, chat_id, &user_id, &text, logger).await;
}

// ---------------------------------------------------------------------------
// DM message router (mirrors Telegram dispatch_message pending-state logic)
// ---------------------------------------------------------------------------

async fn dispatch_dm_message(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    logger: &Arc<dyn Logger>,
) {
    // Check pending states in priority order (mirrors Telegram dispatch_message).
    let pending_admin = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_admin_action.clone());

    if let Some(action) = pending_admin {
        dispatch_admin_input(state, sender, chat_id, user_id, text, action, logger).await;
        return;
    }

    let pending_jira = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_jira_action.clone());

    if let Some(action) = pending_jira {
        dispatch_jira_input(state, sender, chat_id, user_id, text, action, logger).await;
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
        dispatch_permissions_user_input(state, sender, chat_id, user_id, text, logger).await;
        return;
    }

    let pending_comment = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_comment.clone());

    if let Some((issue_key,)) = pending_comment {
        dispatch_pending_comment(state, sender, chat_id, user_id, text, &issue_key, logger).await;
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
        dispatch_solve_branch_name_input(state, sender, chat_id, user_id, text, logger).await;
        return;
    }

    let has_pending_grill = state
        .chat_states
        .get(chat_id)
        .map(|s| s.pending_grill.is_some())
        .unwrap_or(false);

    if has_pending_grill {
        dispatch_grill_answer(state, sender, chat_id, user_id, text, logger).await;
        return;
    }

    let has_pending_ask = state
        .chat_states
        .get(chat_id)
        .map(|s| s.pending_ask.is_some())
        .unwrap_or(false);

    if has_pending_ask {
        dispatch_ask_text_input(state, sender, chat_id, user_id, text, logger).await;
        return;
    }

    // Default: treat as an ask message.
    if text.starts_with('/') {
        let _ = sender.send(chat_id, "Unknown command. Try /help").await;
        return;
    }

    dispatch_ask_session(state, sender, chat_id, user_id, text, logger).await;
}

// ---------------------------------------------------------------------------
// Command dispatchers — delegate to platform-agnostic handlers
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

fn slack_auth_fn(state: Arc<AppState>, user_id: String) -> impl Fn(&str) -> bool {
    move |pk: &str| state.slack_is_authorized_for_project(&user_id, pk)
}

async fn dispatch_ask_command(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let auth = slack_auth_fn(Arc::clone(&state), user_id.to_string());
    let _ = handle_ask(sender, chat_id, state, text.to_string(), user_id, auth).await;
}

async fn dispatch_jira_command(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_jira(sender, chat_id, user_id, state).await;
}

async fn dispatch_help_command(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    _user_id: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_help(sender, chat_id, state).await;
}

async fn dispatch_admin_command(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    _user_id: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_admin(sender, chat_id, state).await;
}

async fn dispatch_admin_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_admin_callback(sender, chat_id, user_id, action, state).await;
}

async fn dispatch_jira_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_jira_action(sender, chat_id, user_id, action, None, state).await;
}

async fn dispatch_my_tickets_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_my_tickets_callback(sender, chat_id, user_id, action, state).await;
}

async fn dispatch_ask_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    _logger: &Arc<dyn Logger>,
) {
    let auth = slack_auth_fn(Arc::clone(&state), user_id.to_string());
    let _ = handle_ask_session_callback(sender, chat_id, user_id, action, state, auth).await;
}

async fn dispatch_solve_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
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
        let _ =
            handle_solve_repo_callback(Arc::clone(&sender), chat_id, user_id, state, action).await;
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

async fn dispatch_permissions_action(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
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

async fn dispatch_admin_input(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
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
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    action: JiraPendingAction,
    _logger: &Arc<dyn Logger>,
) {
    let auth = slack_auth_fn(Arc::clone(&state), user_id.to_string());
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
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    _user_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_permissions_user_input(sender, chat_id, text, state).await;
}

async fn dispatch_pending_comment(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
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
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_solve_branch_name_input(sender, chat_id, state, user_id, text.to_string()).await;
}

async fn dispatch_grill_answer(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_grill_answer(sender, chat_id, user_id, state, text.to_string()).await;
}

async fn dispatch_ask_text_input(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = handle_ask_text_input(sender, chat_id, user_id, text.to_string(), state).await;
}

async fn dispatch_ask_session(
    state: Arc<AppState>,
    sender: Arc<dyn crate::channel::sender::ChannelSender>,
    chat_id: &str,
    _user_id: &str,
    text: &str,
    _logger: &Arc<dyn Logger>,
) {
    let _ = ask_with_session(sender, chat_id, state, text.to_string()).await;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn clear_slack_pending_states(state: &Arc<AppState>, chat_id: &str) {
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
