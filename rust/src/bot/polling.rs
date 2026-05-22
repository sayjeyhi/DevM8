use std::collections::HashSet;
use std::sync::Arc;

use teloxide::dispatching::{DpHandlerDescription, UpdateFilterExt};
use teloxide::dptree::Handler;
use teloxide::prelude::*;
use tokio_util::sync::CancellationToken;

use crate::config::schema::AppConfig;
use crate::logger::Logger;

use super::AppState;
use super::commands::{
    handle_ask, handle_ask_session_callback, handle_ask_text_input, handle_comment,
    handle_create, handle_help, handle_logs, handle_move, handle_my_tickets,
    handle_my_tickets_callback, handle_pending_comment, handle_solve,
    handle_solve_repo_callback,
};
use super::handlers::{handle_pending_slack_reply, handle_slack_callback};

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

    let state = Arc::new(AppState::new(config.clone(), Arc::clone(logger), bot_username)?);

    logger.info(
        "telegram bot starting",
        Some(&serde_json::json!({ "projects": config.jira.project_keys })),
    );

    // Register commands with Telegram's menu
    if let Err(e) = bot.set_my_commands(BotCommand::bot_commands()).await {
        logger.warn(&format!("Failed to register bot commands: {}", e), None);
    }

    // Build allowed IDs set
    let allowed_ids: Arc<HashSet<i64>> = Arc::new(
        config
            .telegram
            .allowed_user_ids
            .iter()
            .copied()
            .collect(),
    );

    // Start Slack poller if configured
    let slack_cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let _slack_poller_handle = if let (Some(slack_cfg), Some(slack_client)) =
        (&config.slack, state.slack.clone())
    {
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

    let listener = teloxide::update_listeners::polling_default(
        Bot::new(&config.telegram.bot_token),
    )
    .await;

    let err_handler = LoggingErrorHandler::with_custom_text(
        "Dispatcher error in update handler",
    );
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
        BotCommand::Move(args) => handle_move(bot, msg, state, args).await,
        BotCommand::Comment(args) => handle_comment(bot, msg, state, args).await,
        BotCommand::Solve(args) => handle_solve(bot, msg, state, args).await,
        BotCommand::MyTickets => handle_my_tickets(bot, msg, state).await,
        BotCommand::Ask(args) => handle_ask(bot, msg, state, args).await,
        BotCommand::Logs(args) => handle_logs(bot, msg, state, args).await,
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

    let data = query.data.as_deref().unwrap_or("");
    state.logger.debug(
        "callback received",
        Some(&serde_json::json!({ "data": data, "user_id": user_id })),
    );

    if data.starts_with("tickets:") {
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

    // Free text → ask Claude
    use crate::claude::types::AskOptions;
    let answer = match state.claude.ask(&text, AskOptions::default()).await {
        Ok(a) => a,
        Err(e) => format!("Error: {e}"),
    };

    bot.send_message(msg.chat.id, answer).await?;

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
