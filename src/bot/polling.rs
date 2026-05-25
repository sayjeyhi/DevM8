use std::collections::HashSet;
use std::sync::Arc;

use teloxide::dispatching::{DpHandlerDescription, UpdateFilterExt};
use teloxide::dptree::Handler;
use teloxide::prelude::*;
use tokio_util::sync::CancellationToken;

use crate::config::schema::AppConfig;
use crate::logger::Logger;

use super::commands::{
    ask_with_session, handle_add_project, handle_ask, handle_ask_session_callback,
    handle_ask_text_input, handle_clone, handle_comment, handle_create, handle_help, handle_logs,
    handle_move, handle_my_tickets, handle_my_tickets_callback, handle_pending_comment,
    handle_permissions, handle_permissions_done, handle_permissions_toggle,
    handle_permissions_user_input, handle_solve, handle_solve_repo_callback, handle_status,
};
use super::handlers::{handle_pending_slack_reply, handle_slack_callback};
use super::AppState;

// ---------------------------------------------------------------------------
// Command enum
// ---------------------------------------------------------------------------

#[derive(teloxide::utils::command::BotCommands, Clone, Debug)]
#[command(rename_rule = "snake_case", description = "DevM8 commands")]
pub enum BotCommand {
    #[command(description = "Show help")]
    Help,
    #[command(description = "Start bot or view ticket details")]
    Start(String),
    #[command(description = "Create a Jira issue")]
    Create(String),
    #[command(description = "Move issue to a new status")]
    Move(String),
    #[command(description = "Add a comment to an issue")]
    Comment(String),
    #[command(description = "Solve an issue with Claude")]
    Solve(String),
    #[command(description = "List my Jira tickets")]
    MyTickets,
    #[command(description = "Ask Claude a question")]
    Ask(String),
    #[command(description = "Show recent daemon logs")]
    Logs(String),
    #[command(description = "Clone a repo via SSH and register it")]
    Clone(String),
    #[command(description = "Show daemon and config status")]
    Status,
    #[command(description = "Register a local git repo as a project")]
    AddProject(String),
    #[command(description = "Manage user project permissions (admin only)")]
    Permissions,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Start the Telegram polling loop.
///
/// Runs until `ct` is cancelled. Errors from individual message handlers
/// are logged and silently swallowed so that one bad update cannot crash the
/// daemon.
pub async fn start_polling(
    ct: CancellationToken,
    logger: &Arc<dyn Logger>,
    config: &AppConfig,
) -> anyhow::Result<()> {
    use teloxide::utils::command::BotCommands as _;

    let bot = Bot::new(&config.telegram.bot_token);

    let bot_username = bot
        .get_me()
        .await
        .map(|me| me.username().to_string())
        .unwrap_or_default();

    let state = Arc::new(AppState::new(
        config.clone(),
        Arc::clone(logger),
        bot_username,
    )?);

    logger.info(
        "telegram bot starting",
        Some(&serde_json::json!({
            "jira_projects": config.jira.project_keys,
            "git_projects": config.projects
                .as_ref()
                .map(|m| m.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
        })),
    );

    // Register commands with Telegram's menu
    if let Err(e) = bot.set_my_commands(BotCommand::bot_commands()).await {
        logger.warn(&format!("Failed to register bot commands: {}", e), None);
    }

    // Build allowed IDs set
    let allowed_ids: Arc<HashSet<i64>> =
        Arc::new(config.telegram.allowed_user_ids.iter().copied().collect());

    // Start Slack poller if configured
    let slack_cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let _slack_poller_handle =
        if let (Some(slack_cfg), Some(slack_client)) = (&config.slack, state.slack.clone()) {
            let bot_clone = bot.clone();
            let allowed_ids_clone = allowed_ids.clone();
            let interval_ms = slack_cfg.poll_interval_ms;
            let cancelled_clone = Arc::clone(&slack_cancel_flag);

            let handle = tokio::spawn(async move {
                use crate::slack::poller::{MessageHandler, SlackPoller};

                let bot_inner = bot_clone.clone();
                let ids: Vec<i64> = allowed_ids_clone.iter().copied().collect();

                let on_message: MessageHandler = Box::new(move |new_msg| {
                    let bot = bot_inner.clone();
                    let ids = ids.clone();
                    Box::pin(async move {
                        crate::bot::handlers::create_slack_forward_handler(bot, ids, &new_msg).await
                    })
                });

                let poller = SlackPoller::new(slack_client, interval_ms, on_message, None);
                poller.start(cancelled_clone).await;
            });
            Some(handle)
        } else {
            None
        };

    // Cancel Slack poller when the main token fires
    let ct_slack = ct.clone();
    let cancel_flag_clone = Arc::clone(&slack_cancel_flag);
    tokio::spawn(async move {
        ct_slack.cancelled().await;
        cancel_flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    // Build the dispatcher
    let handler = build_handler();

    let listener =
        teloxide::update_listeners::polling_default(Bot::new(&config.telegram.bot_token)).await;

    let err_handler = LoggingErrorHandler::with_custom_text("Dispatcher error in update handler");
    let listener_err_handler = LoggingErrorHandler::with_custom_text("Polling listener error");

    let mut dispatcher = Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![state.clone(), allowed_ids.clone()])
        .default_handler(|_upd| async move {})
        .error_handler(err_handler)
        .enable_ctrlc_handler()
        .build();

    tokio::select! {
        _ = dispatcher.dispatch_with_listener(listener, listener_err_handler) => {
            logger.info("Bot dispatcher stopped.", None);
        }
        _ = ct.cancelled() => {
            logger.info("Bot received cancellation signal.", None);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Handler tree
// ---------------------------------------------------------------------------

fn build_handler() -> Handler<'static, DependencyMap, anyhow::Result<()>, DpHandlerDescription> {
    dptree::entry()
        // Slash commands
        .branch(
            Update::filter_message()
                .filter_command::<BotCommand>()
                .endpoint(dispatch_command),
        )
        // Callback queries
        .branch(Update::filter_callback_query().endpoint(dispatch_callback))
        // Plain text / pending state
        .branch(Update::filter_message().endpoint(dispatch_message))
}

// ---------------------------------------------------------------------------
// Dispatch: commands
// ---------------------------------------------------------------------------

async fn dispatch_command(
    bot: Bot,
    msg: Message,
    cmd: BotCommand,
    state: Arc<AppState>,
    allowed_ids: Arc<HashSet<i64>>,
) -> anyhow::Result<()> {
    let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    if !is_authorized(&msg, &allowed_ids) {
        state.logger.warn(
            "unauthorized command attempt",
            Some(&serde_json::json!({ "user_id": user_id, "chat_id": msg.chat.id.0 })),
        );
        return Ok(());
    }

    let cmd_name = match &cmd {
        BotCommand::Help => "help",
        BotCommand::Start(_) => "start",
        BotCommand::Create(_) => "create",
        BotCommand::Move(_) => "move",
        BotCommand::Comment(_) => "comment",
        BotCommand::Solve(_) => "solve",
        BotCommand::MyTickets => "my_tickets",
        BotCommand::Ask(_) => "ask",
        BotCommand::Logs(_) => "logs",
        BotCommand::Clone(_) => "clone",
        BotCommand::Status => "status",
        BotCommand::AddProject(_) => "add_project",
        BotCommand::Permissions => "permissions",
    };
    state.logger.info(
        "command received",
        Some(&serde_json::json!({ "cmd": cmd_name, "user_id": user_id, "chat_id": msg.chat.id.0 })),
    );

    match cmd {
        BotCommand::Help => handle_help(bot, msg, state).await,
        BotCommand::Start(args) => {
            if args.trim().is_empty() {
                handle_help(bot, msg, state).await
            } else {
                super::commands::my_tickets::handle_ticket_details(
                    bot,
                    msg.chat.id,
                    state,
                    args.trim(),
                )
                .await
            }
        }
        BotCommand::Create(args) => handle_create(bot, msg, state, args).await,
        BotCommand::Move(args) => {
            if let Some(pk) = project_key_from_issue_args(&args) {
                if !is_authorized_for_project(user_id, &pk, &state) {
                    bot.send_message(msg.chat.id, "Access denied for that project.")
                        .await?;
                    return Ok(());
                }
            }
            handle_move(bot, msg, state, args).await
        }
        BotCommand::Comment(args) => {
            if let Some(pk) = project_key_from_issue_args(&args) {
                if !is_authorized_for_project(user_id, &pk, &state) {
                    bot.send_message(msg.chat.id, "Access denied for that project.")
                        .await?;
                    return Ok(());
                }
            }
            handle_comment(bot, msg, state, args).await
        }
        BotCommand::Solve(args) => {
            if let Some(pk) = project_key_from_issue_args(&args) {
                if !is_authorized_for_project(user_id, &pk, &state) {
                    bot.send_message(msg.chat.id, "Access denied for that project.")
                        .await?;
                    return Ok(());
                }
            }
            handle_solve(bot, msg, state, args).await
        }
        BotCommand::MyTickets => handle_my_tickets(bot, msg, state, user_id).await,
        BotCommand::Ask(args) => handle_ask(bot, msg, state, args).await,
        BotCommand::Logs(args) => {
            if !is_admin(user_id, &state) {
                bot.send_message(msg.chat.id, "Access denied. This command is admin-only.")
                    .await?;
                return Ok(());
            }
            handle_logs(bot, msg, state, args).await
        }
        BotCommand::Clone(args) => {
            if !is_admin(user_id, &state) {
                bot.send_message(msg.chat.id, "Access denied. This command is admin-only.")
                    .await?;
                return Ok(());
            }
            handle_clone(bot, msg, state, args).await
        }
        BotCommand::Status => handle_status(bot, msg, state).await,
        BotCommand::AddProject(args) => {
            if !is_admin(user_id, &state) {
                bot.send_message(msg.chat.id, "Access denied. This command is admin-only.")
                    .await?;
                return Ok(());
            }
            handle_add_project(bot, msg, state, args).await
        }
        BotCommand::Permissions => {
            if !is_admin(user_id, &state) {
                bot.send_message(msg.chat.id, "Access denied. This command is admin-only.")
                    .await?;
                return Ok(());
            }
            handle_permissions(bot, msg, state).await
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatch: callbacks
// ---------------------------------------------------------------------------

async fn dispatch_callback(
    bot: Bot,
    query: CallbackQuery,
    state: Arc<AppState>,
    allowed_ids: Arc<HashSet<i64>>,
) -> anyhow::Result<()> {
    let user_id = query.from.id.0 as i64;
    if !is_authorized_id(user_id, &allowed_ids) {
        state.logger.warn(
            "unauthorized callback attempt",
            Some(&serde_json::json!({ "user_id": user_id })),
        );
        let _ = bot.answer_callback_query(query.id.clone()).await;
        return Ok(());
    }

    let data = query.data.clone().unwrap_or_default();
    state.logger.debug(
        "callback received",
        Some(&serde_json::json!({ "data": data, "user_id": user_id })),
    );

    if data.starts_with("tickets:") {
        // Gate project-scoped callbacks before routing into the handler.
        let denied_project = if let Some(key) = data.strip_prefix("tickets:project:") {
            (!is_authorized_for_project(user_id, key, &state)).then_some(key.to_string())
        } else if let Some(rest) = data.strip_prefix("tickets:status:") {
            let project_key = rest.split(':').next().unwrap_or("");
            (!is_authorized_for_project(user_id, project_key, &state))
                .then_some(project_key.to_string())
        } else {
            None
        };

        if denied_project.is_some() {
            if let Some(msg) = query.message.as_ref() {
                bot.send_message(msg.chat().id, "Access denied for that project.")
                    .await?;
            }
            return Ok(());
        }

        return handle_my_tickets_callback(bot, query, state).await;
    }

    if data.starts_with("solve:repo:") {
        return handle_solve_repo_callback(bot, query, state).await;
    }

    if data.starts_with("solve:branch:") {
        // solve:branch:<choice>:<issue_key>
        let parts: Vec<&str> = data.splitn(4, ':').collect();
        if parts.len() == 4 {
            let choice = parts[2].to_string();
            let issue_key = parts[3].to_string();
            let chat_id = match query.message.as_ref().map(|m| m.chat().id) {
                Some(id) => id,
                None => return Ok(()),
            };
            let _ = bot.answer_callback_query(query.id.clone()).await;
            return super::commands::solve::handle_branch_choice(
                bot, chat_id, state, &choice, &issue_key,
            )
            .await;
        }
        return Ok(());
    }

    if data.starts_with("ask:") {
        return handle_ask_session_callback(bot, query, state).await;
    }

    if data.starts_with("slack:") {
        return handle_slack_callback(bot, query, state).await;
    }

    if data.starts_with("perms:") {
        if data == "perms:done" {
            return handle_permissions_done(bot, query, state).await;
        }
        if let Some(key) = data.strip_prefix("perms:toggle:") {
            return handle_permissions_toggle(bot, query, state, key.to_string()).await;
        }
        let _ = bot.answer_callback_query(query.id).await;
        return Ok(());
    }

    let _ = bot.answer_callback_query(query.id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Dispatch: plain text messages
// ---------------------------------------------------------------------------

async fn dispatch_message(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    allowed_ids: Arc<HashSet<i64>>,
) -> anyhow::Result<()> {
    if !is_authorized(&msg, &allowed_ids) {
        return Ok(());
    }

    let chat_id = msg.chat.id.0;

    // Check pending permissions: waiting for admin to type a target user ID.
    let waiting_for_user_id = state
        .chat_states
        .get(&chat_id)
        .map(|s| {
            s.pending_permissions
                .as_ref()
                .map(|p| p.target_user_id.is_none())
                .unwrap_or(false)
        })
        .unwrap_or(false);

    if waiting_for_user_id {
        return handle_permissions_user_input(bot, msg, state).await;
    }

    // Check pending comment
    let pending_comment = state
        .chat_states
        .get(&chat_id)
        .and_then(|s| s.pending_comment.clone());

    if let Some((issue_key,)) = pending_comment {
        return handle_pending_comment(bot, msg, state, issue_key).await;
    }

    // Check pending ask
    let has_pending_ask = state
        .chat_states
        .get(&chat_id)
        .map(|s| s.pending_ask.is_some())
        .unwrap_or(false);

    if has_pending_ask {
        return handle_ask_text_input(bot, msg, state).await;
    }

    // Check pending Slack reply
    let has_pending_slack = state
        .chat_states
        .get(&chat_id)
        .map(|s| s.pending_slack_reply.is_some())
        .unwrap_or(false);

    if has_pending_slack {
        return handle_pending_slack_reply(bot, msg, state).await;
    }

    // Unknown command or plain text — forward to Claude
    let text = msg.text().unwrap_or("").trim().to_string();
    if text.is_empty() {
        return Ok(());
    }

    if text.starts_with('/') {
        bot.send_message(msg.chat.id, "Unknown command. Try /help")
            .await?;
        return Ok(());
    }

    // Free text → route through ask session so the active repo cwd is used.
    ask_with_session(bot, msg.chat.id, state, text).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Authorization helpers
// ---------------------------------------------------------------------------

fn is_authorized(msg: &Message, allowed: &HashSet<i64>) -> bool {
    if allowed.is_empty() {
        return true;
    }
    msg.from
        .as_ref()
        .map(|u| allowed.contains(&(u.id.0 as i64)))
        .unwrap_or(false)
}

fn is_authorized_id(user_id: i64, allowed: &HashSet<i64>) -> bool {
    allowed.is_empty() || allowed.contains(&user_id)
}

fn is_admin(user_id: i64, state: &AppState) -> bool {
    match state.config.telegram.admin_user_id {
        Some(admin_id) => user_id == admin_id,
        None => true,
    }
}

/// Returns the uppercase project key from the first token of an issue-key argument string.
/// E.g. "MYAPP-123 some text" → Some("MYAPP"), "notanissue" → None.
fn project_key_from_issue_args(args: &str) -> Option<String> {
    let first = args.trim().split_whitespace().next()?;
    let (prefix, _) = first.split_once('-')?;
    if prefix.is_empty() {
        return None;
    }
    Some(prefix.to_uppercase())
}

fn is_authorized_for_project(user_id: i64, project_key: &str, state: &AppState) -> bool {
    if is_admin(user_id, state) {
        return true;
    }
    let access = state.project_access.read().unwrap();
    if access.is_empty() {
        return true;
    }
    match access.get(project_key) {
        None => true,
        Some(ids) => ids.contains(&user_id),
    }
}
