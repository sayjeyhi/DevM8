use std::collections::HashSet;
use std::sync::Arc;

use teloxide::dispatching::{DpHandlerDescription, UpdateFilterExt};
use teloxide::dptree::Handler;
use teloxide::prelude::*;
use teloxide::types::{BotCommandScope, Recipient};
use tokio_util::sync::CancellationToken;

use crate::config::schema::AppConfig;
use crate::logger::Logger;

use super::commands::{
    ask_with_session, handle_admin, handle_admin_callback, handle_admin_input, handle_ask,
    handle_ask_session_callback, handle_ask_text_input, handle_grill_answer, handle_help,
    handle_jira, handle_jira_action, handle_jira_input_with_text, handle_my_tickets_callback,
    handle_pending_comment, handle_permissions_add, handle_permissions_back,
    handle_permissions_done, handle_permissions_revoke, handle_permissions_toggle,
    handle_permissions_user_input, handle_permissions_user_select, handle_post_analysis_implement,
    handle_solve_action_callback, handle_solve_branch_name_input, handle_solve_repo_callback,
    handle_worktree_branch_name_input,
};
use super::handlers::{handle_pending_slack_reply, handle_slack_callback};
use super::sender::TelegramSender;
use super::AppState;

// ---------------------------------------------------------------------------
// Command enum
// ---------------------------------------------------------------------------

#[derive(teloxide::utils::command::BotCommands, Clone, Debug)]
#[command(rename_rule = "snake_case", description = "DevM8 commands")]
pub enum BotCommand {
    #[command(description = "Show help")]
    Help,
    #[command(description = "Ask Claude a question (or view ticket details via deep link)")]
    Start(String),
    #[command(description = "Jira — manage tickets, create issues, and more")]
    Jira,
    #[command(description = "Admin panel — permissions, projects, logs, and repos")]
    Admin,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

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
            "jira_projects": config.jira.as_ref().map(|j| j.project_keys.as_slice()).unwrap_or_default(),
            "git_projects": config.projects
                .as_ref()
                .map(|m| m.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
        })),
    );

    // /admin is admin-only; all other commands are visible to everyone.
    let all_commands = BotCommand::bot_commands();
    const ADMIN_ONLY: &[&str] = &["admin"];
    let non_admin_commands: Vec<_> = all_commands
        .iter()
        .filter(|c| !ADMIN_ONLY.contains(&c.command.as_str()))
        .cloned()
        .collect();

    if let Err(e) = bot.set_my_commands(non_admin_commands).await {
        logger.warn(
            &format!("Failed to register default bot commands: {e}"),
            None,
        );
    }
    if let Some(admin_id) = config.telegram.admin_user_id {
        if let Err(e) = bot
            .set_my_commands(all_commands)
            .scope(BotCommandScope::Chat {
                chat_id: Recipient::Id(ChatId(admin_id)),
            })
            .await
        {
            logger.warn(&format!("Failed to register admin bot commands: {e}"), None);
        }
    }

    let allowed_ids: Arc<HashSet<i64>> =
        Arc::new(config.telegram.allowed_user_ids.iter().copied().collect());

    // Start Slack poller if configured
    let slack_cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let _slack_poller_handle =
        if let (Some(slack_cfg), Some(slack_client)) = (&config.slack, state.slack.clone()) {
            let bot_clone = bot.clone();
            let admin_id = config.telegram.admin_user_id;
            let interval_ms = slack_cfg.poll_interval_ms;
            let cancelled_clone = Arc::clone(&slack_cancel_flag);
            let logger_clone = Arc::clone(logger);

            // Known Jira project keys, used to recognize ticket mentions in forwarded
            // Slack messages (global config + every configured per-user Jira account).
            let mut jira_project_keys: Vec<String> = config
                .jira
                .as_ref()
                .map(|j| j.project_keys.clone())
                .unwrap_or_default();
            for user_cfg in config.user_jira.values() {
                jira_project_keys.extend(user_cfg.project_keys.iter().cloned());
            }

            let handle = tokio::spawn(async move {
                use crate::slack::poller::{MessageHandler, SlackPoller};

                let bot_inner = bot_clone.clone();

                let on_message: MessageHandler = Box::new(move |new_msg| {
                    let bot = bot_inner.clone();
                    let admin_id = admin_id;
                    let jira_project_keys = jira_project_keys.clone();
                    Box::pin(async move {
                        let sender: Arc<dyn crate::channel::ChannelSender> =
                            Arc::new(TelegramSender::new(bot));
                        let chat_ids: Vec<String> =
                            admin_id.iter().map(|id| id.to_string()).collect();
                        crate::bot::handlers::create_slack_forward_handler(
                            sender,
                            chat_ids,
                            &new_msg,
                            &jira_project_keys,
                        )
                        .await
                    })
                });

                let poller = SlackPoller::new(
                    slack_client,
                    interval_ms,
                    on_message,
                    None,
                    Arc::clone(&logger_clone),
                );
                poller.start(cancelled_clone).await;
            });
            Some(handle)
        } else {
            None
        };

    let ct_slack = ct.clone();
    let cancel_flag_clone = Arc::clone(&slack_cancel_flag);
    tokio::spawn(async move {
        ct_slack.cancelled().await;
        cancel_flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    // Start Slack Socket Mode bot if app_token + bot_token are configured.
    if config
        .slack
        .as_ref()
        .map(|s| s.bot_enabled())
        .unwrap_or(false)
    {
        let ct_socket = ct.clone();
        let state_socket = Arc::clone(&state);
        let logger_socket = Arc::clone(logger);
        tokio::spawn(async move {
            let _ = crate::slack_bot::bot::start_slack_bot(ct_socket, state_socket, &logger_socket)
                .await;
        });
    }

    // Start Teams webhook server if configured.
    if config.teams.is_some() {
        let ct_teams = ct.clone();
        let state_teams = Arc::clone(&state);
        let logger_teams = Arc::clone(logger);
        tokio::spawn(async move {
            let _ =
                crate::teams_bot::bot::start_teams_bot(ct_teams, state_teams, &logger_teams).await;
        });
    }

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
        .branch(
            Update::filter_message()
                .filter_command::<BotCommand>()
                .endpoint(dispatch_command),
        )
        .branch(Update::filter_callback_query().endpoint(dispatch_callback))
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
    let user_id_i64 = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    let user_id = user_id_i64.to_string();
    let chat_id = msg.chat.id.0.to_string();

    if let Some(u) = &msg.from {
        state.user_names.insert(user_id_i64, format_user_name(u));
    }

    if !is_authorized(&msg, &allowed_ids, &state) {
        state.logger.warn(
            "unauthorized command attempt",
            Some(&serde_json::json!({ "user_id": user_id_i64, "chat_id": msg.chat.id.0 })),
        );
        let uname = msg.from.as_ref().map(format_user_name).unwrap_or_default();
        let raw_text = msg.text().unwrap_or("").to_string();
        state.audit_logger.log_action(
            user_id_i64,
            &uname,
            "unauthorized_command",
            "",
            Some(serde_json::json!({ "text": raw_text })),
        );
        if let Some(u) = &msg.from {
            notify_admin_unauthorized(&bot, &state, u, "command").await;
        }
        return Ok(());
    }

    let sender: Arc<dyn crate::channel::ChannelSender> = Arc::new(TelegramSender::new(bot.clone()));

    let (cmd_name, cmd_args) = match &cmd {
        BotCommand::Help => ("help", String::new()),
        BotCommand::Start(a) => ("start", a.clone()),
        BotCommand::Jira => ("jira", String::new()),
        BotCommand::Admin => ("admin", String::new()),
    };
    state.logger.info(
        "command received",
        Some(
            &serde_json::json!({ "cmd": cmd_name, "user_id": user_id_i64, "chat_id": msg.chat.id.0 }),
        ),
    );
    let uname = state
        .user_names
        .get(&user_id_i64)
        .map(|n| n.clone())
        .unwrap_or_default();
    state.audit_logger.log_action(
        user_id_i64,
        &uname,
        "command",
        cmd_name,
        Some(serde_json::json!({ "args": cmd_args })),
    );

    // Any new slash command cancels whatever the user was in the middle of.
    clear_pending_states(&state, &chat_id);

    match cmd {
        BotCommand::Help => handle_help(Arc::clone(&sender), &chat_id, state).await,
        BotCommand::Start(args) => {
            let trimmed = args.trim().to_string();
            // Deep-link ticket lookup: /start PROJ-123 (Jira key pattern)
            let is_jira_key = trimmed
                .split_whitespace()
                .next()
                .map(|w| {
                    w.len() <= 20
                        && w.contains('-')
                        && w.split('-')
                            .next()
                            .map(|p| p.chars().all(|c| c.is_ascii_uppercase()))
                            .unwrap_or(false)
                        && w.split('-')
                            .nth(1)
                            .map(|n| n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty())
                            .unwrap_or(false)
                })
                .unwrap_or(false);
            if is_jira_key {
                super::commands::my_tickets::handle_ticket_details(
                    Arc::clone(&sender),
                    &chat_id,
                    &user_id,
                    state,
                    &trimmed,
                )
                .await
            } else {
                let auth_fn = {
                    let state_ref = Arc::clone(&state);
                    move |pk: &str| is_authorized_for_project(user_id_i64, pk, &state_ref)
                };
                handle_ask(
                    Arc::clone(&sender),
                    &chat_id,
                    state,
                    trimmed,
                    &user_id,
                    auth_fn,
                )
                .await
            }
        }
        BotCommand::Jira => handle_jira(Arc::clone(&sender), &chat_id, &user_id, state).await,
        BotCommand::Admin => {
            if !is_admin(user_id_i64, &state) {
                sender
                    .send(&chat_id, "Access denied. This command is admin-only.")
                    .await?;
                return Ok(());
            }
            handle_admin(Arc::clone(&sender), &chat_id, state).await
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
    let user_id_i64 = query.from.id.0 as i64;
    let user_id = user_id_i64.to_string();

    if !is_authorized_id(user_id_i64, &allowed_ids, &state) {
        state.logger.warn(
            "unauthorized callback attempt",
            Some(&serde_json::json!({ "user_id": user_id_i64 })),
        );
        let uname = format_user_name(&query.from);
        let cb_data = query.data.as_deref().unwrap_or("").to_string();
        state.audit_logger.log_action(
            user_id_i64,
            &uname,
            "unauthorized_callback",
            "",
            Some(serde_json::json!({ "data": cb_data })),
        );
        notify_admin_unauthorized(&bot, &state, &query.from, "callback").await;
        let _ = bot.answer_callback_query(query.id.clone()).await;
        return Ok(());
    }

    // Extract chat_id and message_ref before answering
    let chat_id = match query.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id.0.to_string(),
        None => {
            let _ = bot.answer_callback_query(query.id).await;
            return Ok(());
        }
    };
    let msg_ref_opt = query
        .message
        .as_ref()
        .map(|m| crate::channel::SentMessageRef {
            chat_id: chat_id.clone(),
            message_id: m.id().0.to_string(),
        });

    // Answer the callback query first
    let _ = bot.answer_callback_query(query.id.clone()).await;

    let data = query.data.clone().unwrap_or_default();
    state.logger.debug(
        "callback received",
        Some(&serde_json::json!({ "data": data, "user_id": user_id_i64 })),
    );
    let cb_prefix = data.split(':').next().unwrap_or(&data);
    let uname = state
        .user_names
        .get(&user_id_i64)
        .map(|n| n.clone())
        .unwrap_or_else(|| format_user_name(&query.from));
    state.audit_logger.log_action(
        user_id_i64,
        &uname,
        "callback",
        cb_prefix,
        Some(serde_json::json!({ "data": data })),
    );

    let sender: Arc<dyn crate::channel::ChannelSender> = Arc::new(TelegramSender::new(bot.clone()));

    if data.starts_with("admin:") {
        if !is_admin(user_id_i64, &state) {
            return Ok(());
        }
        return handle_admin_callback(Arc::clone(&sender), &chat_id, &user_id, &data, state).await;
    }

    if data.starts_with("jira:") {
        return handle_jira_action(
            Arc::clone(&sender),
            &chat_id,
            &user_id,
            &data,
            msg_ref_opt,
            state,
        )
        .await;
    }

    if data.starts_with("tickets:") {
        let denied_project = if let Some(key) = data.strip_prefix("tickets:project:") {
            (!is_authorized_for_project(user_id_i64, key, &state)).then_some(key.to_string())
        } else if let Some(rest) = data.strip_prefix("tickets:status:") {
            let project_key = rest.split(':').next().unwrap_or("");
            (!is_authorized_for_project(user_id_i64, project_key, &state))
                .then_some(project_key.to_string())
        } else {
            None
        };

        if denied_project.is_some() {
            sender
                .send(&chat_id, "Access denied for that project.")
                .await?;
            return Ok(());
        }

        return handle_my_tickets_callback(Arc::clone(&sender), &chat_id, &user_id, &data, state)
            .await;
    }

    if data == "solve:cancel" {
        let cancelled = {
            let mut entry = state.chat_states.entry(chat_id.clone()).or_default();
            match entry.cancel_token.take() {
                Some(ct) => {
                    ct.cancel();
                    true
                }
                None => false,
            }
        };
        if !cancelled {
            sender
                .send(&chat_id, "No active request to cancel.")
                .await?;
        }
        return Ok(());
    }

    if data.starts_with("solve:repo:") {
        return handle_solve_repo_callback(Arc::clone(&sender), &chat_id, &user_id, state, &data)
            .await;
    }

    if data.starts_with("solve:branch:") {
        let parts: Vec<&str> = data.splitn(4, ':').collect();
        if parts.len() == 4 {
            let choice = parts[2].to_string();
            let issue_key = parts[3].to_string();
            return super::commands::solve::handle_branch_choice(
                Arc::clone(&sender),
                &chat_id,
                state,
                &user_id,
                &choice,
                &issue_key,
            )
            .await;
        }
        return Ok(());
    }

    if let Some(issue_key) = data.strip_prefix("solve:post:implement:") {
        let issue_key = issue_key.to_string();
        return handle_post_analysis_implement(
            Arc::clone(&sender),
            &chat_id,
            state,
            &user_id,
            &issue_key,
        )
        .await;
    }

    if data.starts_with("solve:action:") {
        let parts: Vec<&str> = data.splitn(4, ':').collect();
        if parts.len() == 4 {
            let action = parts[2].to_string();
            let issue_key = parts[3].to_string();
            return handle_solve_action_callback(
                Arc::clone(&sender),
                &chat_id,
                state,
                &user_id,
                &action,
                &issue_key,
            )
            .await;
        }
        return Ok(());
    }

    if data.starts_with("ask:") {
        let auth_fn = {
            let state_ref = Arc::clone(&state);
            move |pk: &str| is_authorized_for_project(user_id_i64, pk, &state_ref)
        };
        return handle_ask_session_callback(
            Arc::clone(&sender),
            &chat_id,
            &user_id,
            &data,
            state,
            auth_fn,
        )
        .await;
    }

    if data.starts_with("slack:") {
        return handle_slack_callback(Arc::clone(&sender), &chat_id, &user_id, &data, state).await;
    }

    if data.starts_with("perms:") {
        if data == "perms:done" {
            return handle_permissions_done(Arc::clone(&sender), &chat_id, state).await;
        }
        if data == "perms:back" {
            return handle_permissions_back(Arc::clone(&sender), &chat_id, state, msg_ref_opt)
                .await;
        }
        if data == "perms:add" {
            return handle_permissions_add(Arc::clone(&sender), &chat_id, state).await;
        }
        if let Some(key) = data.strip_prefix("perms:toggle:") {
            return handle_permissions_toggle(
                Arc::clone(&sender),
                &chat_id,
                state,
                key.to_string(),
            )
            .await;
        }
        if let Some(rest) = data.strip_prefix("perms:user:") {
            if let Ok(target_id) = rest.parse::<i64>() {
                return handle_permissions_user_select(
                    Arc::clone(&sender),
                    &chat_id,
                    state,
                    target_id,
                )
                .await;
            }
        }
        if let Some(rest) = data.strip_prefix("perms:revoke:") {
            if let Ok(target_id) = rest.parse::<i64>() {
                return handle_permissions_revoke(Arc::clone(&sender), &chat_id, state, target_id)
                    .await;
            }
        }
        return Ok(());
    }

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
    if !is_authorized(&msg, &allowed_ids, &state) {
        if let Some(u) = &msg.from {
            state.logger.warn(
                "unauthorized message attempt",
                Some(&serde_json::json!({ "user_id": u.id.0, "chat_id": msg.chat.id.0 })),
            );
            let uname = format_user_name(u);
            let preview = truncate_for_audit(msg.text().unwrap_or(""));
            state.audit_logger.log_action(
                u.id.0 as i64,
                &uname,
                "unauthorized_message",
                "",
                Some(serde_json::json!({ "text": preview })),
            );
            notify_admin_unauthorized(&bot, &state, u, "message").await;
        }
        return Ok(());
    }

    let user_id_i64 = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    let user_id = user_id_i64.to_string();
    let chat_id = msg.chat.id.0.to_string();

    if let Some(u) = &msg.from {
        state.user_names.insert(user_id_i64, format_user_name(u));
    }

    let msg_context = {
        let s = state.chat_states.get(&chat_id);
        if s.as_deref()
            .and_then(|s| s.pending_admin_action.as_ref())
            .is_some()
        {
            "pending_admin_input"
        } else if s
            .as_deref()
            .and_then(|s| s.pending_jira_action.as_ref())
            .is_some()
        {
            "pending_jira_input"
        } else if s
            .as_deref()
            .map(|s| {
                s.pending_permissions
                    .as_ref()
                    .map(|p| p.awaiting_user_id_input)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
        {
            "pending_permissions_input"
        } else if s
            .as_deref()
            .and_then(|s| s.pending_comment.as_ref())
            .is_some()
        {
            "pending_comment"
        } else if s
            .as_deref()
            .map(|s| {
                s.pending_solve
                    .as_ref()
                    .map(|p| p.awaiting_branch_name)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
        {
            "pending_solve_branch_name"
        } else if s
            .as_deref()
            .map(|s| s.pending_worktree_branch.is_some())
            .unwrap_or(false)
        {
            "pending_worktree_branch_name"
        } else if s
            .as_deref()
            .map(|s| s.pending_grill.is_some())
            .unwrap_or(false)
        {
            "pending_grill_answer"
        } else if s
            .as_deref()
            .map(|s| s.pending_ask.is_some())
            .unwrap_or(false)
        {
            "pending_ask_input"
        } else if s
            .as_deref()
            .map(|s| s.pending_slack_reply.is_some())
            .unwrap_or(false)
        {
            "pending_slack_reply"
        } else {
            "freeform_message"
        }
    };
    let uname = state
        .user_names
        .get(&user_id_i64)
        .map(|n| n.clone())
        .unwrap_or_default();
    let text_preview = truncate_for_audit(msg.text().unwrap_or(""));
    state.audit_logger.log_action(
        user_id_i64,
        &uname,
        "message",
        msg_context,
        Some(serde_json::json!({ "text": text_preview })),
    );

    let sender: Arc<dyn crate::channel::ChannelSender> = Arc::new(TelegramSender::new(bot.clone()));

    // Check pending admin panel input (clone / add_project)
    let pending_admin = state
        .chat_states
        .get(&chat_id)
        .and_then(|s| s.pending_admin_action.clone());

    if let Some(action) = pending_admin {
        let text = msg.text().unwrap_or("").to_string();
        return handle_admin_input(
            Arc::clone(&sender),
            &chat_id,
            &user_id,
            &text,
            state,
            action,
        )
        .await;
    }

    // Check pending Jira panel input
    let pending_jira = state
        .chat_states
        .get(&chat_id)
        .and_then(|s| s.pending_jira_action.clone());

    if let Some(action) = pending_jira {
        let text = msg.text().unwrap_or("").trim().to_string();
        let auth_check = {
            let state_ref = Arc::clone(&state);
            move |pk: &str| is_authorized_for_project(user_id_i64, pk, &state_ref)
        };
        return handle_jira_input_with_text(
            Arc::clone(&sender),
            &chat_id,
            &user_id,
            action,
            auth_check,
            state,
            text,
        )
        .await;
    }

    // Check pending permissions: waiting for admin to type a target user ID.
    let waiting_for_user_id = state
        .chat_states
        .get(&chat_id)
        .map(|s| {
            s.pending_permissions
                .as_ref()
                .map(|p| p.awaiting_user_id_input)
                .unwrap_or(false)
        })
        .unwrap_or(false);

    if waiting_for_user_id {
        let text = msg.text().unwrap_or("").trim().to_string();
        return handle_permissions_user_input(Arc::clone(&sender), &chat_id, &text, state).await;
    }

    // Check pending comment
    let pending_comment = state
        .chat_states
        .get(&chat_id)
        .and_then(|s| s.pending_comment.clone());

    if let Some((issue_key,)) = pending_comment {
        let text = msg.text().unwrap_or("").to_string();
        return handle_pending_comment(
            Arc::clone(&sender),
            &chat_id,
            &user_id,
            &text,
            state,
            issue_key,
        )
        .await;
    }

    // Check pending solve branch name confirmation
    let awaiting_branch_name = state
        .chat_states
        .get(&chat_id)
        .map(|s| {
            s.pending_solve
                .as_ref()
                .map(|p| p.awaiting_branch_name)
                .unwrap_or(false)
        })
        .unwrap_or(false);

    if awaiting_branch_name {
        let branch_name = msg.text().unwrap_or("").trim().to_string();
        return handle_solve_branch_name_input(
            Arc::clone(&sender),
            &chat_id,
            state,
            &user_id,
            branch_name,
        )
        .await;
    }

    // Check pending worktree branch name confirmation (ask + solve implement)
    let awaiting_worktree_branch = state
        .chat_states
        .get(&chat_id)
        .map(|s| s.pending_worktree_branch.is_some())
        .unwrap_or(false);

    if awaiting_worktree_branch {
        let branch_name = msg.text().unwrap_or("").trim().to_string();
        return handle_worktree_branch_name_input(
            Arc::clone(&sender),
            &chat_id,
            state,
            branch_name,
        )
        .await;
    }

    // Check active grill session
    let has_pending_grill = state
        .chat_states
        .get(&chat_id)
        .map(|s| s.pending_grill.is_some())
        .unwrap_or(false);

    if has_pending_grill {
        let answer = msg.text().unwrap_or("").trim().to_string();
        return handle_grill_answer(Arc::clone(&sender), &chat_id, &user_id, state, answer).await;
    }

    // Check pending ask
    let has_pending_ask = state
        .chat_states
        .get(&chat_id)
        .map(|s| s.pending_ask.is_some())
        .unwrap_or(false);

    if has_pending_ask {
        let text = msg.text().unwrap_or("").trim().to_string();
        return handle_ask_text_input(Arc::clone(&sender), &chat_id, &user_id, text, state).await;
    }

    // Check pending Slack reply
    let has_pending_slack = state
        .chat_states
        .get(&chat_id)
        .map(|s| s.pending_slack_reply.is_some())
        .unwrap_or(false);

    if has_pending_slack {
        let text = msg.text().unwrap_or("").trim().to_string();
        return handle_pending_slack_reply(Arc::clone(&sender), &chat_id, &text, state).await;
    }

    let text = msg.text().unwrap_or("").trim().to_string();
    if text.is_empty() {
        return Ok(());
    }

    if text.starts_with('/') {
        sender.send(&chat_id, "Unknown command. Try /help").await?;
        return Ok(());
    }

    ask_with_session(Arc::clone(&sender), &chat_id, state, text).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Authorization helpers
// ---------------------------------------------------------------------------

fn is_authorized(msg: &Message, allowed: &HashSet<i64>, state: &AppState) -> bool {
    let user_id = match msg.from.as_ref() {
        Some(u) => u.id.0 as i64,
        None => return false,
    };
    is_authorized_id(user_id, allowed, state)
}

fn is_authorized_id(user_id: i64, allowed: &HashSet<i64>, state: &AppState) -> bool {
    if allowed.is_empty() {
        return true;
    }
    if allowed.contains(&user_id) {
        return true;
    }
    let access = state.project_access.read().unwrap();
    access.values().any(|ids| ids.contains(&user_id))
}

fn is_admin(user_id: i64, state: &AppState) -> bool {
    state.is_admin(user_id)
}

fn format_user_name(u: &teloxide::types::User) -> String {
    let mut name = u.first_name.clone();
    if let Some(last) = &u.last_name {
        name.push(' ');
        name.push_str(last);
    }
    if let Some(un) = &u.username {
        name.push_str(&format!(" (@{})", un));
    }
    name
}

async fn notify_admin_unauthorized(
    bot: &Bot,
    state: &AppState,
    user: &teloxide::types::User,
    attempt: &str,
) {
    let admin_id = match state.config.telegram.admin_user_id {
        Some(id) => id,
        None => return,
    };
    let name = format_user_name(user);
    let text = format!(
        "\u{26a0}\u{fe0f} Unauthorized {attempt} attempt\nUser: {name}\nID: {}",
        user.id.0
    );
    let _ = bot.send_message(ChatId(admin_id), text).await;
}

/// Caps message text at 300 chars for audit records so logs stay manageable.
fn truncate_for_audit(text: &str) -> String {
    let mut s: String = text.chars().take(300).collect();
    if text.chars().count() > 300 {
        s.push('\u{2026}');
    }
    s
}

fn clear_pending_states(state: &Arc<AppState>, chat_id: &str) {
    if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
        cs.pending_comment = None;
        cs.pending_ask = None;
        cs.pending_jira_action = None;
        cs.pending_admin_action = None;
        cs.pending_slack_reply = None;
        cs.pending_solve = None;
        cs.pending_worktree_branch = None;
        cs.pending_permissions = None;
    }
}

fn is_authorized_for_project(user_id: i64, project_key: &str, state: &AppState) -> bool {
    if is_admin(user_id, state) {
        return true;
    }
    let access = state.project_access.read().unwrap();
    if access.is_empty() {
        return true;
    }
    let is_restricted = access.values().any(|ids| ids.contains(&user_id));
    match access.get(project_key) {
        None => !is_restricted,
        Some(ids) => ids.contains(&user_id),
    }
}
