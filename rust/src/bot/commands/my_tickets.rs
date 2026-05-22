use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};

use crate::bot::state::PageCache;
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

fn format_tickets_page(issues: &[JiraIssue]) -> String {
    if issues.is_empty() {
        return "No tickets found.".to_string();
    }
    issues
        .iter()
        .map(|i| {
            format!(
                "{} <a href=\"{}\">{}</a> — {}\n  <i>{}</i>",
                status_emoji(&i.status),
                i.url,
                escape_html(&i.key),
                escape_html(&i.summary),
                escape_html(&i.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn build_list_keyboard(issues: &[JiraIssue], page: usize, has_next: bool) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = issues
        .iter()
        .map(|i| {
            let summary_short: String = i.summary.chars().take(32).collect();
            let ellipsis = if i.summary.len() > 32 { "…" } else { "" };
            let label = format!("{} {} — {}{}", status_emoji(&i.status), i.key, summary_short, ellipsis);
            vec![InlineKeyboardButton::callback(label, format!("tickets:details:{}", i.key))]
        })
        .collect();

    let mut nav_row: Vec<InlineKeyboardButton> = Vec::new();
    if page > 0 {
        nav_row.push(InlineKeyboardButton::callback("\u{2190} Prev", format!("tickets:page:{}", page - 1)));
    }
    nav_row.push(InlineKeyboardButton::callback("\u{21ba} Refresh", format!("tickets:refresh:{}", page)));
    if has_next {
        nav_row.push(InlineKeyboardButton::callback("Next \u{2192}", format!("tickets:page:{}", page + 1)));
    }
    rows.push(nav_row);

    InlineKeyboardMarkup::new(rows)
}

fn build_details_action_keyboard(issue_key: &str, back_page: usize) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("Solve", format!("tickets:solve:{}", issue_key)),
            InlineKeyboardButton::callback("Move", format!("tickets:move_start:{}", issue_key)),
            InlineKeyboardButton::callback("Comment", format!("tickets:comment_start:{}", issue_key)),
        ],
        vec![
            InlineKeyboardButton::callback("\u{2190} Back to list", format!("tickets:page:{}", back_page)),
        ],
    ])
}

// ---------------------------------------------------------------------------
// Main command entry
// ---------------------------------------------------------------------------

pub async fn handle_my_tickets(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
) -> Result<()> {
    let project_keys = state.jira.project_keys();

    if project_keys.is_empty() {
        bot.send_message(msg.chat.id, "No project keys configured.").await?;
        return Ok(());
    }

    if project_keys.len() == 1 {
        let key = project_keys[0].clone();
        return handle_my_tickets_project(bot, msg.chat.id, state, &key).await;
    }

    // Multiple project keys — show picker
    let buttons: Vec<Vec<InlineKeyboardButton>> = project_keys
        .iter()
        .map(|k| vec![InlineKeyboardButton::callback(k.clone(), format!("tickets:project:{}", k))])
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
    let statuses = match state.jira.get_statuses().await {
        Ok(s) => s,
        Err(e) => {
            bot.send_message(chat_id, format!("Error fetching statuses: {e}")).await?;
            return Ok(());
        }
    };

    let mut buttons: Vec<Vec<InlineKeyboardButton>> = vec![
        vec![InlineKeyboardButton::callback(
            "All statuses",
            format!("tickets:status:{}:ALL", project_key),
        )],
    ];
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
    let filter = if status_filter == "ALL" {
        None
    } else {
        Some(status_filter)
    };

    let result = match state
        .jira
        .get_my_issues(PAGE_SIZE, None, filter, Some(project_key))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            bot.send_message(chat_id, format!("Error: {e}")).await?;
            return Ok(());
        }
    };

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
    let text = format_tickets_page(&result.issues);
    let keyboard = build_list_keyboard(&result.issues, 0, has_next);

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
    let text = format_tickets_page(&result.issues);
    let keyboard = build_list_keyboard(&result.issues, target_page, has_next);

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

    let issue = match state.jira.get_issue_by_key(issue_key).await {
        Ok(i) => i,
        Err(e) => {
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
        if issue.description.len() > 400 { "..." } else { "" }
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
            bot.send_message(chat_id, format!("Error fetching transitions: {e}")).await?;
            return Ok(());
        }
    };

    if transitions.is_empty() {
        bot.send_message(chat_id, "No available transitions.").await?;
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
    match state.jira.transition_issue(issue_key, status).await {
        Ok(()) => {
            bot.send_message(
                chat_id,
                format!("Moved <b>{}</b> \u{2192} {}", escape_html(issue_key), escape_html(status)),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }
        Err(e) => {
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
                    bot.send_message(chat_id, "No list context. Use /my_tickets.").await?;
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

    // tickets:solve:<issue_key>
    if let Some(key) = data.strip_prefix("tickets:solve:") {
        return crate::bot::commands::solve::solve_by_key(
            bot, chat_id, state, key, None,
        )
        .await;
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
