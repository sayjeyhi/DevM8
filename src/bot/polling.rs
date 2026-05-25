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
    handle_ask_session_callback, handle_ask_text_input, handle_help, handle_jira,
    handle_jira_callback, handle_jira_input, handle_my_tickets_callback, handle_pending_comment,
    handle_permissions_add, handle_permissions_back, handle_permissions_done,
    handle_permissions_revoke, handle_permissions_toggle, handle_permissions_user_input,
    handle_permissions_user_select, handle_solve_repo_callback,
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
    #[command(description = "Ask Claude a question")]
    Ask(String),
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
            "jira_projects": config.jira.project_keys,
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

    let ct_slack = ct.clone();
    let cancel_flag_clone = Arc::clone(&slack_cancel_flag);
    tokio::spawn(async move {
        ct_slack.cancelled().await;
        cancel_flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    });

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
    let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);

    if let Some(u) = &msg.from {
        state.user_names.insert(user_id, format_user_name(u));
    }

    if !is_authorized(&msg, &allowed_ids, &state) {
        state.logger.warn(
            "unauthorized command attempt",
            Some(&serde_json::json!({ "user_id": user_id, "chat_id": msg.chat.id.0 })),
        );
        return Ok(());
    }

    let cmd_name = match &cmd {
        BotCommand::Help => "help",
        BotCommand::Start(_) => "start",
        BotCommand::Ask(_) => "ask",
        BotCommand::Jira => "jira",
        BotCommand::Admin => "admin",
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
        BotCommand::Ask(args) => handle_ask(bot, msg, state, args, user_id).await,
        BotCommand::Jira => handle_jira(bot, msg, state).await,
        BotCommand::Admin => {
            if !is_admin(user_id, &state) {
                bot.send_message(msg.chat.id, "Access denied. This command is admin-only.")
                    .await?;
                return Ok(());
            }
            handle_admin(bot, msg, state).await
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
    if !is_authorized_id(user_id, &allowed_ids, &state) {
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

    if data.starts_with("admin:") {
        if !is_admin(user_id, &state) {
            let _ = bot.answer_callback_query(query.id).await;
            return Ok(());
        }
        return handle_admin_callback(bot, query, state).await;
    }

    if data.starts_with("jira:") {
        return handle_jira_callback(bot, query, state).await;
    }

    if data.starts_with("tickets:") {
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
        if data == "perms:back" {
            return handle_permissions_back(bot, query, state).await;
        }
        if data == "perms:add" {
            return handle_permissions_add(bot, query, state).await;
        }
        if let Some(key) = data.strip_prefix("perms:toggle:") {
            return handle_permissions_toggle(bot, query, state, key.to_string()).await;
        }
        if let Some(rest) = data.strip_prefix("perms:user:") {
            if let Ok(target_id) = rest.parse::<i64>() {
                return handle_permissions_user_select(bot, query, state, target_id).await;
            }
        }
        if let Some(rest) = data.strip_prefix("perms:revoke:") {
            if let Ok(target_id) = rest.parse::<i64>() {
                return handle_permissions_revoke(bot, query, state, target_id).await;
            }
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
    if !is_authorized(&msg, &allowed_ids, &state) {
        return Ok(());
    }

    if let Some(u) = &msg.from {
        let uid = u.id.0 as i64;
        state.user_names.insert(uid, format_user_name(u));
    }

    let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    let chat_id = msg.chat.id.0;

    // Check pending admin panel input (clone / add_project)
    let pending_admin = state
        .chat_states
        .get(&chat_id)
        .and_then(|s| s.pending_admin_action.clone());

    if let Some(action) = pending_admin {
        return handle_admin_input(bot, msg, state, action).await;
    }

    // Check pending Jira panel input
    let pending_jira = state
        .chat_states
        .get(&chat_id)
        .and_then(|s| s.pending_jira_action.clone());

    if let Some(action) = pending_jira {
        let auth_check = {
            let state_ref = Arc::clone(&state);
            move |pk: &str| is_authorized_for_project(user_id, pk, &state_ref)
        };
        return handle_jira_input(bot, msg, state, action, auth_check).await;
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

    let text = msg.text().unwrap_or("").trim().to_string();
    if text.is_empty() {
        return Ok(());
    }

    if text.starts_with('/') {
        bot.send_message(msg.chat.id, "Unknown command. Try /help")
            .await?;
        return Ok(());
    }

    ask_with_session(bot, msg.chat.id, state, text).await?;

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
