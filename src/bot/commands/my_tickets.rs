use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};

use crate::bot::state::{AskSession, PageCache};
use crate::bot::utils::escape_html;
use crate::bot::AppState;
use crate::jira::types::JiraIssue;

const PAGE_SIZE: u32 = 8;

// ---------------------------------------------------------------------------
// Emoji helpers
// ---------------------------------------------------------------------------

fn status_emoji(status: &str) -> &'static str {
    match status.to_lowercase().as_str() {
        s if s.contains("done") || s.contains("closed") || s.contains("resolved") => "",
        s if s.contains("progress") || s.contains("review") || s.contains("testing") => "",
        s if s.contains("block") || s.contains("impede") => "",
        s if s.contains("todo") || s.contains("backlog") || s.contains("open") => "",
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

fn format_tickets_page(issues: &[JiraIssue], bot_username: &str) -> String {
    if issues.is_empty() {
        return "No tickets found.".to_string();
    }
    issues
        .iter()
        .map(|i| {
            let details_link = if bot_username.is_empty() {
                String::new()
            } else {
                format!(
                    "  <a href=\"https://t.me/{}?start={}\">[details]</a>",
                    bot_username, i.key,
                )
            };
            format!(
                "{} <a href=\"{}\">{}</a> — {}{}\n  <i>{}</i>",
                status_emoji(&i.status),
                i.url,
                escape_html(&i.key),
                escape_html(&i.summary),
                details_link,
                escape_html(&i.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn build_list_keyboard(page: usize, has_next: bool) -> InlineKeyboardMarkup {
    let mut nav_row: Vec<InlineKeyboardButton> = Vec::new();
    if page > 0 {
        nav_row.push(InlineKeyboardButton::callback(
            "◀️ Prev",
            format!("tickets:page:{}", page - 1),
        ));
    }
    nav_row.push(InlineKeyboardButton::callback(
        "🔄 Refresh",
        format!("tickets:refresh:{}", page),
    ));
    if has_next {
        nav_row.push(InlineKeyboardButton::callback(
            "Next ▶️",
            format!("tickets:page:{}", page + 1),
        ));
    }
    InlineKeyboardMarkup::new(vec![nav_row])
}

fn build_details_action_keyboard(issue_key: &str, back_page: usize) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("🤖 Ask", format!("tickets:ask:{}", issue_key)),
            InlineKeyboardButton::callback("🔧 Solve", format!("tickets:solve:{}", issue_key)),
        ],
        vec![
            InlineKeyboardButton::callback("🔄 Move", format!("tickets:move_start:{}", issue_key)),
            InlineKeyboardButton::callback(
                "💬 Comment",
                format!("tickets:comment_start:{}", issue_key),
            ),
        ],
        vec![InlineKeyboardButton::callback(
            "◀️ Back to list",
            format!("tickets:page:{}", back_page),
        )],
    ])
}

// ---------------------------------------------------------------------------
// Project access helpers
// ---------------------------------------------------------------------------

/// Returns the subset of Jira project keys the user is allowed to see.
/// If `project_access` is empty or a key has no entry, all allowed users can see it.
pub fn accessible_project_keys(user_id: i64, state: &AppState) -> Vec<String> {
    let is_admin = state.config.telegram.admin_user_id == Some(user_id);
    let access = state.project_access.read().unwrap();

    state
        .jira
        .project_keys()
        .iter()
        .filter(|key| {
            if is_admin || access.is_empty() {
                return true;
            }
            match access.get(key.as_str()) {
                None => true,
                Some(ids) => ids.contains(&user_id),
            }
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Main command entry
// ---------------------------------------------------------------------------

pub async fn handle_my_tickets(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    user_id: i64,
) -> Result<()> {
    let project_keys = accessible_project_keys(user_id, &state);

    if project_keys.is_empty() {
        bot.send_message(msg.chat.id, "No project keys configured.")
            .await?;
        return Ok(());
    }

    if project_keys.len() == 1 {
        let key = project_keys[0].clone();
        return handle_my_tickets_project(bot, msg.chat.id, state, &key).await;
    }

    // Multiple project keys — show picker
    let buttons: Vec<Vec<InlineKeyboardButton>> = project_keys
        .iter()
        .map(|k| {
            vec![InlineKeyboardButton::callback(
                k.clone(),
                format!("tickets:project:{}", k),
            )]
        })
        .collect();

    let keyboard = InlineKeyboardMarkup::new(buttons);
    bot.send_message(msg.chat.id, "Select a project:")
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Project selected — show status picker
// ---------------------------------------------------------------------------

pub async fn handle_my_tickets_project(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    project_key: &str,
) -> Result<()> {
    state.logger.info(
        "tickets: fetching statuses",
        Some(&json!({ "project": project_key })),
    );
    let statuses = match state.jira.get_statuses().await {
        Ok(s) => s,
        Err(e) => {
            state.logger.error(
                &format!("tickets: failed to fetch statuses: {e}"),
                Some(&json!({ "project": project_key })),
            );
            bot.send_message(chat_id, format!("Error fetching statuses: {e}"))
                .await?;
            return Ok(());
        }
    };

    let mut buttons: Vec<Vec<InlineKeyboardButton>> = vec![vec![InlineKeyboardButton::callback(
        "📋 All statuses",
        format!("tickets:status:{}:ALL", project_key),
    )]];
    for status in &statuses {
        buttons.push(vec![InlineKeyboardButton::callback(
            format!("{} {}", status_emoji(&status.name), &status.name),
            format!("tickets:status:{}:{}", project_key, status.name),
        )]);
    }

    let keyboard = InlineKeyboardMarkup::new(buttons);
    bot.send_message(chat_id, "Filter by status:")
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Status selected — show first page
// ---------------------------------------------------------------------------

pub async fn handle_my_tickets_status(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    project_key: &str,
    status_filter: &str,
) -> Result<()> {
    let bot_username = state.bot_username.clone();
    let filter = if status_filter == "ALL" {
        None
    } else {
        Some(status_filter)
    };

    state.logger.info(
        "tickets: querying issues",
        Some(&json!({ "project": project_key, "status": status_filter })),
    );
    let result = match state
        .jira
        .get_my_issues(PAGE_SIZE, None, filter, Some(project_key))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.logger.error(
                &format!("tickets: query failed: {e}"),
                Some(&json!({ "project": project_key })),
            );
            bot.send_message(chat_id, format!("Error: {e}")).await?;
            return Ok(());
        }
    };
    state.logger.info(
        "tickets: query complete",
        Some(&json!({ "project": project_key, "count": result.issues.len(), "has_next": result.next_page_token.is_some() })),
    );

    // Initialize page cache
    let mut cache = PageCache::new(project_key, filter.map(String::from));
    if result.next_page_token.is_some() {
        cache.tokens.push(result.next_page_token.clone());
    }
    cache.current_page = 0;

    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        entry.page_cache = Some(cache);
    }

    let has_next = result.next_page_token.is_some();
    let text = format_tickets_page(&result.issues, &bot_username);
    let keyboard = build_list_keyboard(0, has_next);

    bot.send_message(chat_id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

pub async fn handle_my_tickets_page(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    target_page: usize,
) -> Result<()> {
    let (project_key, status_filter, tokens, current_page) = {
        let cs = state.chat_states.get(&chat_id.0);
        match cs.as_ref().and_then(|c| c.page_cache.as_ref()) {
            Some(cache) => (
                cache.project_key.clone(),
                cache.status_filter.clone(),
                cache.tokens.clone(),
                cache.current_page,
            ),
            None => {
                bot.send_message(chat_id, "No page context found. Use /my_tickets.")
                    .await?;
                return Ok(());
            }
        }
    };

    if target_page >= tokens.len() && target_page > current_page {
        bot.send_message(chat_id, "No more pages.").await?;
        return Ok(());
    }

    let page_token = tokens.get(target_page).and_then(|t| t.as_deref());

    let result = match state
        .jira
        .get_my_issues(
            PAGE_SIZE,
            page_token,
            status_filter.as_deref(),
            Some(&project_key),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            bot.send_message(chat_id, format!("Error: {e}")).await?;
            return Ok(());
        }
    };

    // Update cache
    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        if let Some(cache) = entry.page_cache.as_mut() {
            cache.current_page = target_page;
            if let Some(next_token) = result.next_page_token.clone() {
                let next_page_idx = target_page + 1;
                if next_page_idx >= cache.tokens.len() {
                    cache.tokens.push(Some(next_token));
                }
            }
        }
    }

    let has_next = result.next_page_token.is_some();
    let text = format_tickets_page(&result.issues, &state.bot_username);
    let keyboard = build_list_keyboard(target_page, has_next);

    bot.send_message(chat_id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Ticket details
// ---------------------------------------------------------------------------

pub async fn handle_ticket_details(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    issue_key: &str,
) -> Result<()> {
    let back_page = state
        .chat_states
        .get(&chat_id.0)
        .and_then(|cs| cs.page_cache.as_ref().map(|c| c.current_page))
        .unwrap_or(0);

    state.logger.info(
        "tickets: fetching issue details",
        Some(&json!({ "key": issue_key })),
    );
    let issue = match state.jira.get_issue_by_key(issue_key).await {
        Ok(i) => i,
        Err(e) => {
            state.logger.error(
                &format!("tickets: failed to fetch issue: {e}"),
                Some(&json!({ "key": issue_key })),
            );
            bot.send_message(chat_id, format!("Error: {e}")).await?;
            return Ok(());
        }
    };

    let desc_preview: String = issue.description.chars().take(400).collect();

    let text = format!(
        "<b><a href=\"{}\">{}</a></b> — {}\nStatus: {}\n\n{}{}",
        issue.url,
        escape_html(&issue.key),
        escape_html(&issue.summary),
        escape_html(&issue.status),
        escape_html(&desc_preview),
        if issue.description.len() > 400 {
            "..."
        } else {
            ""
        }
    );

    let keyboard = build_details_action_keyboard(issue_key, back_page);

    bot.send_message(chat_id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Move — step 1: show transitions
// ---------------------------------------------------------------------------

pub async fn handle_move_start(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    issue_key: &str,
) -> Result<()> {
    let transitions = match state.jira.get_transitions(issue_key).await {
        Ok(t) => t,
        Err(e) => {
            bot.send_message(chat_id, format!("Error fetching transitions: {e}"))
                .await?;
            return Ok(());
        }
    };

    if transitions.is_empty() {
        bot.send_message(chat_id, "No available transitions.")
            .await?;
        return Ok(());
    }

    let buttons: Vec<Vec<InlineKeyboardButton>> = transitions
        .iter()
        .map(|(_, name)| {
            vec![InlineKeyboardButton::callback(
                name.clone(),
                format!("tickets:move_exec:{}:{}", issue_key, name),
            )]
        })
        .collect();

    let keyboard = InlineKeyboardMarkup::new(buttons);
    bot.send_message(
        chat_id,
        format!("Select new status for <b>{}</b>:", escape_html(issue_key)),
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(keyboard)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Move — step 2: execute
// ---------------------------------------------------------------------------

pub async fn handle_move_execute(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    issue_key: &str,
    status: &str,
) -> Result<()> {
    state.logger.info(
        "tickets: transitioning issue",
        Some(&json!({ "key": issue_key, "target_status": status })),
    );
    match state.jira.transition_issue(issue_key, status).await {
        Ok(()) => {
            state.logger.info(
                "tickets: transition complete",
                Some(&json!({ "key": issue_key, "status": status })),
            );
            bot.send_message(
                chat_id,
                format!(
                    "Moved <b>{}</b> \u{2192} {}",
                    escape_html(issue_key),
                    escape_html(status)
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }
        Err(e) => {
            state.logger.error(
                &format!("tickets: transition failed: {e}"),
                Some(&json!({ "key": issue_key, "target_status": status })),
            );
            bot.send_message(chat_id, format!("Error: {e}")).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Comment start: set pending comment state
// ---------------------------------------------------------------------------

pub async fn handle_comment_start(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    issue_key: &str,
) -> Result<()> {
    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        entry.pending_comment = Some((issue_key.to_string(),));
    }

    bot.send_message(
        chat_id,
        format!("Type a comment for <b>{}</b>:", escape_html(issue_key)),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Ask: start an ask session with ticket context
// ---------------------------------------------------------------------------

pub async fn handle_ticket_ask(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    issue_key: &str,
) -> Result<()> {
    state.logger.info(
        "tickets: starting ask session for ticket",
        Some(&json!({ "key": issue_key })),
    );

    let issue = match state.jira.get_issue_by_key(issue_key).await {
        Ok(i) => i,
        Err(e) => {
            state.logger.error(
                &format!("tickets: ask — failed to fetch issue: {e}"),
                Some(&json!({ "key": issue_key })),
            );
            bot.send_message(chat_id, format!("Error fetching ticket: {e}"))
                .await?;
            return Ok(());
        }
    };

    let project_key = issue_key.split('-').next().unwrap_or("").to_uppercase();

    // Build context string that Claude will receive with every message
    let context = format!(
        "You are helping with Jira ticket {key}: \"{summary}\".\nStatus: {status}\n\nDescription:\n{description}",
        key = issue.key,
        summary = issue.summary,
        status = issue.status,
        description = if issue.description.is_empty() { "(no description)".into() } else { issue.description.clone() },
    );

    // Find repos for this project
    let repos = state.git_map.get(&project_key).cloned().unwrap_or_default();

    if repos.is_empty() {
        // No git context — start session directly
        {
            let mut entry = state.chat_states.entry(chat_id.0).or_default();
            entry.ask_session = Some(AskSession::new(None, None).with_context(context));
        }
        bot.send_message(
            chat_id,
            format!(
                "📋 <b><a href=\"{}\">{}</a></b> — {}\nStatus: {}\n\nWhat would you like to ask?",
                issue.url,
                escape_html(&issue.key),
                escape_html(&issue.summary),
                escape_html(&issue.status),
            ),
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    if repos.len() == 1 {
        let git = repos.into_iter().next().unwrap();
        let branch = git
            .current_branch()
            .await
            .unwrap_or_else(|_| "unknown".into());
        let clean = git.is_clean().await.unwrap_or(true);

        {
            let mut entry = state.chat_states.entry(chat_id.0).or_default();
            entry.ask_session = Some(
                AskSession::new(Some(git.repo_path.clone()), Some(git.clone()))
                    .with_context(context),
            );
        }

        let repo_name = git
            .repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo");

        bot.send_message(
            chat_id,
            format!(
                "📋 <b><a href=\"{}\">{}</a></b> — {}\nStatus: {}\n\n📂 <b>{}</b> | Branch: <code>{}</code>{}\n\nWhat would you like to ask?",
                issue.url,
                escape_html(&issue.key),
                escape_html(&issue.summary),
                escape_html(&issue.status),
                escape_html(repo_name),
                escape_html(&branch),
                if clean { "" } else { " ⚠️ dirty" },
            ),
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    // Multiple repos — show picker, preserving context in pending ask
    use crate::bot::state::PendingAsk;
    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        // Store context temporarily; session will be created after repo selection
        entry.ask_session = Some(AskSession::new(None, None).with_context(context));
        entry.pending_ask = Some(PendingAsk {
            repo_path: None,
            git: None,
            inline_question: None,
            mode: None,
        });
    }

    let buttons: Vec<Vec<InlineKeyboardButton>> = repos
        .iter()
        .enumerate()
        .map(|(i, git)| {
            let label = format!(
                "{} / {}",
                project_key,
                git.repo_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("repo")
            );
            vec![InlineKeyboardButton::callback(
                label,
                format!("ask:repo:{}", i),
            )]
        })
        .collect();

    bot.send_message(
        chat_id,
        format!(
            "📋 <b><a href=\"{}\">{}</a></b> — {}\n\nSelect a repository:",
            issue.url,
            escape_html(&issue.key),
            escape_html(&issue.summary),
        ),
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(InlineKeyboardMarkup::new(buttons))
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Callback router for all tickets:* callbacks
// ---------------------------------------------------------------------------

pub async fn handle_my_tickets_callback(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<AppState>,
) -> Result<()> {
    let _ = bot.answer_callback_query(q.id.clone()).await;

    let data = q.data.as_deref().unwrap_or("");
    let chat_id = match q.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };

    // tickets:project:<key>
    if let Some(key) = data.strip_prefix("tickets:project:") {
        return handle_my_tickets_project(bot, chat_id, state, key).await;
    }

    // tickets:status:<project_key>:<status>
    if let Some(rest) = data.strip_prefix("tickets:status:") {
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if parts.len() == 2 {
            return handle_my_tickets_status(bot, chat_id, state, parts[0], parts[1]).await;
        }
        return Ok(());
    }

    // tickets:page:<page_index>
    if let Some(page_str) = data.strip_prefix("tickets:page:") {
        let page: usize = page_str.parse().unwrap_or(0);
        return handle_my_tickets_page(bot, chat_id, state, page).await;
    }

    // tickets:refresh:<page_index> — re-fetch from Jira (clears token cache, goes to page 0)
    if data.starts_with("tickets:refresh:") {
        let (project_key, status_filter) = {
            let cs = state.chat_states.get(&chat_id.0);
            match cs.as_ref().and_then(|c| c.page_cache.as_ref()) {
                Some(cache) => (cache.project_key.clone(), cache.status_filter.clone()),
                None => {
                    bot.send_message(chat_id, "No list context. Use /my_tickets.")
                        .await?;
                    return Ok(());
                }
            }
        };
        let filter = status_filter.as_deref().unwrap_or("ALL");
        return handle_my_tickets_status(bot, chat_id, state, &project_key, filter).await;
    }

    // tickets:details:<issue_key>
    if let Some(key) = data.strip_prefix("tickets:details:") {
        return handle_ticket_details(bot, chat_id, state, key).await;
    }

    // tickets:ask:<issue_key>
    if let Some(key) = data.strip_prefix("tickets:ask:") {
        return handle_ticket_ask(bot, chat_id, state, key).await;
    }

    // tickets:solve:<issue_key>
    if let Some(key) = data.strip_prefix("tickets:solve:") {
        return crate::bot::commands::solve::handle_repo_picker(bot, chat_id, state, key).await;
    }

    // tickets:move_start:<issue_key>
    if let Some(key) = data.strip_prefix("tickets:move_start:") {
        return handle_move_start(bot, chat_id, state, key).await;
    }

    // tickets:move_exec:<issue_key>:<status>
    if let Some(rest) = data.strip_prefix("tickets:move_exec:") {
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if parts.len() == 2 {
            return handle_move_execute(bot, chat_id, state, parts[0], parts[1]).await;
        }
        return Ok(());
    }

    // tickets:comment_start:<issue_key>
    if let Some(key) = data.strip_prefix("tickets:comment_start:") {
        return handle_comment_start(bot, chat_id, state, key).await;
    }

    Ok(())
}
