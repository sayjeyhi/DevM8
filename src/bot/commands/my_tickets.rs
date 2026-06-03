use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use crate::bot::state::{AskSession, PageCache};
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender};
use crate::config::loader::load_config;
use crate::jira::types::JiraIssue;

const PAGE_SIZE: u32 = 8;

// ---------------------------------------------------------------------------
// Emoji helpers
// ---------------------------------------------------------------------------

fn status_emoji(status: &str) -> &'static str {
    match status.to_lowercase().as_str() {
        s if s.contains("done") || s.contains("closed") || s.contains("resolved") => "\u{2705}",
        s if s.contains("progress") || s.contains("review") || s.contains("testing") => "\u{1f504}",
        s if s.contains("block") || s.contains("impede") => "\u{1f6d1}",
        s if s.contains("todo") || s.contains("backlog") || s.contains("open") => "\u{1f4cb}",
        _ => "\u{25aa}\u{fe0f}",
    }
}

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

fn format_tickets_page(
    sender: &Arc<dyn ChannelSender>,
    issues: &[JiraIssue],
    bot_username: Option<&str>,
) -> String {
    if issues.is_empty() {
        return "No tickets found.".to_string();
    }
    issues
        .iter()
        .map(|i| {
            let details_link = match bot_username {
                Some(uname) if !uname.is_empty() => format!(
                    "  <a href=\"https://t.me/{}?start={}\">[details]</a>",
                    uname, i.key,
                ),
                _ => String::new(),
            };
            format!(
                "{} <a href=\"{}\">{}</a> \u{2014} {}{}\n  <i>{}</i>",
                status_emoji(&i.status),
                i.url,
                sender.escape(&i.key),
                sender.escape(&i.summary),
                details_link,
                sender.escape(&i.status),
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn build_list_keyboard(page: usize, has_next: bool) -> Vec<Vec<Button>> {
    let mut nav_row: Vec<Button> = Vec::new();
    if page > 0 {
        nav_row.push(Button::new(
            "\u{25c0}\u{fe0f} Prev",
            format!("tickets:page:{}", page - 1),
        ));
    }
    nav_row.push(Button::new(
        "\u{1f504} Refresh",
        format!("tickets:refresh:{}", page),
    ));
    if has_next {
        nav_row.push(Button::new(
            "Next \u{25b6}\u{fe0f}",
            format!("tickets:page:{}", page + 1),
        ));
    }
    vec![nav_row]
}

fn build_details_action_keyboard(issue_key: &str, back_page: usize) -> Vec<Vec<Button>> {
    vec![
        vec![
            Button::new("\u{1f916} Ask", format!("tickets:ask:{}", issue_key)),
            Button::new("\u{1f527} Solve", format!("tickets:solve:{}", issue_key)),
        ],
        vec![
            Button::new(
                "\u{1f504} Move",
                format!("tickets:move_start:{}", issue_key),
            ),
            Button::new(
                "\u{1f4ac} Comment",
                format!("tickets:comment_start:{}", issue_key),
            ),
        ],
        vec![Button::new(
            "\u{25c0}\u{fe0f} Back to list",
            format!("tickets:page:{}", back_page),
        )],
    ]
}

// ---------------------------------------------------------------------------
// Project access helpers
// ---------------------------------------------------------------------------

/// Returns the subset of Jira project keys the user is allowed to see.
/// If `project_access` is empty or a key has no entry, all allowed users can see it.
pub fn accessible_project_keys(user_id: i64, state: &AppState) -> Vec<String> {
    let is_admin = state.is_admin(user_id);
    let access = state.project_access.read().unwrap();
    let is_restricted = !is_admin && access.values().any(|ids| ids.contains(&user_id));

    let jira = match state.jira_for_user(&user_id.to_string()) {
        Some(j) => j,
        None => return vec![],
    };
    jira.project_keys()
        .iter()
        .filter(|key| {
            if is_admin || access.is_empty() {
                return true;
            }
            match access.get(key.as_str()) {
                None => !is_restricted,
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
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
) -> Result<()> {
    let uid_i64 = user_id.parse::<i64>().unwrap_or(0);
    let project_keys = accessible_project_keys(uid_i64, &state);

    if project_keys.is_empty() {
        sender.send(chat_id, "No project keys configured.").await?;
        return Ok(());
    }

    if project_keys.len() == 1 {
        let key = project_keys[0].clone();
        return handle_my_tickets_project(Arc::clone(&sender), chat_id, user_id, state, &key).await;
    }

    // Multiple project keys — show picker
    let buttons: Vec<Vec<Button>> = project_keys
        .iter()
        .map(|k| vec![Button::new(k.clone(), format!("tickets:project:{}", k))])
        .collect();

    sender
        .send_with_keyboard(chat_id, "Select a project:", buttons)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Project selected — show status picker
// ---------------------------------------------------------------------------

pub async fn handle_my_tickets_project(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
    project_key: &str,
) -> Result<()> {
    state.logger.info(
        "tickets: fetching statuses",
        Some(&json!({ "project": project_key })),
    );
    let favorite_statuses: Vec<String> = load_config(None)
        .ok()
        .and_then(|c| c.user_jira.get(user_id).cloned())
        .map(|c| c.favorite_statuses)
        .unwrap_or_default();

    let status_names: Vec<String> = if !favorite_statuses.is_empty() {
        favorite_statuses
    } else {
        let Some(jira) = state.jira_for_user(user_id) else {
            sender
                .send(
                    chat_id,
                    "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
                )
                .await?;
            return Ok(());
        };
        match jira.get_statuses().await {
            Ok(s) => s.into_iter().map(|s| s.name).collect(),
            Err(e) => {
                state.logger.error(
                    &format!("tickets: failed to fetch statuses: {e}"),
                    Some(&json!({ "project": project_key })),
                );
                sender
                    .send(chat_id, &format!("Error fetching statuses: {e}"))
                    .await?;
                return Ok(());
            }
        }
    };

    let mut buttons: Vec<Vec<Button>> = vec![vec![Button::new(
        "\u{1f4cb} All statuses",
        format!("tickets:status:{}:ALL", project_key),
    )]];
    for name in &status_names {
        buttons.push(vec![Button::new(
            format!("{} {}", status_emoji(name), name),
            format!("tickets:status:{}:{}", project_key, name),
        )]);
    }

    sender
        .send_with_keyboard(chat_id, "Filter by status:", buttons)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Status selected — show first page
// ---------------------------------------------------------------------------

pub async fn handle_my_tickets_status(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
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
    let Some(jira) = state.jira_for_user(user_id) else {
        sender
            .send(
                chat_id,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    let result = match jira
        .get_my_issues(PAGE_SIZE, None, filter, Some(project_key))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.logger.error(
                &format!("tickets: query failed: {e}"),
                Some(&json!({ "project": project_key })),
            );
            sender.send(chat_id, &format!("Error: {e}")).await?;
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
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.page_cache = Some(cache);
    }

    let has_next = result.next_page_token.is_some();
    let text = format_tickets_page(&sender, &result.issues, Some(&bot_username));
    let keyboard = build_list_keyboard(0, has_next);

    sender.send_with_keyboard(chat_id, &text, keyboard).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

pub async fn handle_my_tickets_page(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
    target_page: usize,
) -> Result<()> {
    let (project_key, status_filter, tokens, current_page) = {
        let cs = state.chat_states.get(chat_id);
        match cs.as_ref().and_then(|c| c.page_cache.as_ref()) {
            Some(cache) => (
                cache.project_key.clone(),
                cache.status_filter.clone(),
                cache.tokens.clone(),
                cache.current_page,
            ),
            None => {
                sender
                    .send(chat_id, "No page context found. Use /my_tickets.")
                    .await?;
                return Ok(());
            }
        }
    };

    if target_page >= tokens.len() && target_page > current_page {
        sender.send(chat_id, "No more pages.").await?;
        return Ok(());
    }

    let page_token = tokens.get(target_page).and_then(|t| t.as_deref());

    let Some(jira) = state.jira_for_user(user_id) else {
        sender
            .send(
                chat_id,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    let result = match jira
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
            sender.send(chat_id, &format!("Error: {e}")).await?;
            return Ok(());
        }
    };

    // Update cache
    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
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
    let text = format_tickets_page(&sender, &result.issues, Some(&state.bot_username));
    let keyboard = build_list_keyboard(target_page, has_next);

    sender.send_with_keyboard(chat_id, &text, keyboard).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Ticket details
// ---------------------------------------------------------------------------

pub async fn handle_ticket_details(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
    issue_key: &str,
) -> Result<()> {
    let back_page = state
        .chat_states
        .get(chat_id)
        .and_then(|cs| cs.page_cache.as_ref().map(|c| c.current_page))
        .unwrap_or(0);

    state.logger.info(
        "tickets: fetching issue details",
        Some(&json!({ "key": issue_key })),
    );
    let Some(jira) = state.jira_for_user(user_id) else {
        sender
            .send(
                chat_id,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    let issue = match jira.get_issue_by_key(issue_key).await {
        Ok(i) => i,
        Err(e) => {
            state.logger.error(
                &format!("tickets: failed to fetch issue: {e}"),
                Some(&json!({ "key": issue_key })),
            );
            sender.send(chat_id, &format!("Error: {e}")).await?;
            return Ok(());
        }
    };

    let desc_preview: String = issue.description.chars().take(400).collect();

    let text = format!(
        "<b><a href=\"{}\">{}</a></b> \u{2014} {}\nStatus: {}\n\n{}{}",
        issue.url,
        sender.escape(&issue.key),
        sender.escape(&issue.summary),
        sender.escape(&issue.status),
        sender.escape(&desc_preview),
        if issue.description.len() > 400 {
            "..."
        } else {
            ""
        }
    );

    let keyboard = build_details_action_keyboard(issue_key, back_page);

    sender.send_with_keyboard(chat_id, &text, keyboard).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Move — step 1: show transitions
// ---------------------------------------------------------------------------

pub async fn handle_move_start(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
    issue_key: &str,
) -> Result<()> {
    let Some(jira) = state.jira_for_user(user_id) else {
        sender
            .send(
                chat_id,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    let transitions = match jira.get_transitions(issue_key).await {
        Ok(t) => t,
        Err(e) => {
            sender
                .send(chat_id, &format!("Error fetching transitions: {e}"))
                .await?;
            return Ok(());
        }
    };

    if transitions.is_empty() {
        sender.send(chat_id, "No available transitions.").await?;
        return Ok(());
    }

    let buttons: Vec<Vec<Button>> = transitions
        .iter()
        .map(|(_, name)| {
            vec![Button::new(
                name.clone(),
                format!("tickets:move_exec:{}:{}", issue_key, name),
            )]
        })
        .collect();

    sender
        .send_with_keyboard(
            chat_id,
            &format!("Select new status for <b>{}</b>:", sender.escape(issue_key)),
            buttons,
        )
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Move — step 2: execute
// ---------------------------------------------------------------------------

pub async fn handle_move_execute(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
    issue_key: &str,
    status: &str,
) -> Result<()> {
    state.logger.info(
        "tickets: transitioning issue",
        Some(&json!({ "key": issue_key, "target_status": status })),
    );
    let Some(jira) = state.jira_for_user(user_id) else {
        sender
            .send(
                chat_id,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    match jira.transition_issue(issue_key, status).await {
        Ok(()) => {
            state.logger.info(
                "tickets: transition complete",
                Some(&json!({ "key": issue_key, "status": status })),
            );
            sender
                .send(
                    chat_id,
                    &format!(
                        "Moved <b>{}</b> \u{2192} {}",
                        sender.escape(issue_key),
                        sender.escape(status)
                    ),
                )
                .await?;
        }
        Err(e) => {
            state.logger.error(
                &format!("tickets: transition failed: {e}"),
                Some(&json!({ "key": issue_key, "target_status": status })),
            );
            sender.send(chat_id, &format!("Error: {e}")).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Comment start: set pending comment state
// ---------------------------------------------------------------------------

pub async fn handle_comment_start(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    issue_key: &str,
) -> Result<()> {
    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.pending_comment = Some((issue_key.to_string(),));
    }

    sender
        .send(
            chat_id,
            &format!("Type a comment for <b>{}</b>:", sender.escape(issue_key)),
        )
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Ask: start an ask session with ticket context
// ---------------------------------------------------------------------------

pub async fn handle_ticket_ask(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
    issue_key: &str,
) -> Result<()> {
    state.logger.info(
        "tickets: starting ask session for ticket",
        Some(&json!({ "key": issue_key })),
    );

    let Some(jira) = state.jira_for_user(user_id) else {
        sender
            .send(
                chat_id,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    let issue = match jira.get_issue_by_key(issue_key).await {
        Ok(i) => i,
        Err(e) => {
            state.logger.error(
                &format!("tickets: ask — failed to fetch issue: {e}"),
                Some(&json!({ "key": issue_key })),
            );
            sender
                .send(chat_id, &format!("Error fetching ticket: {e}"))
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
            let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
            entry.ask_session = Some(AskSession::new(user_id, None, None).with_context(context));
        }
        sender
            .send(
                chat_id,
                &format!(
                    "\u{1f4cb} <b><a href=\"{}\">{}</a></b> \u{2014} {}\nStatus: {}\n\nWhat would you like to ask?",
                    issue.url,
                    sender.escape(&issue.key),
                    sender.escape(&issue.summary),
                    sender.escape(&issue.status),
                ),
            )
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
            let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
            entry.ask_session = Some(
                AskSession::new(user_id, Some(git.repo_path.clone()), Some(git.clone()))
                    .with_context(context),
            );
        }

        let repo_name = git
            .repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo");

        sender
            .send(
                chat_id,
                &format!(
                    "\u{1f4cb} <b><a href=\"{}\">{}</a></b> \u{2014} {}\nStatus: {}\n\n\u{1f4c2} <b>{}</b> | Branch: <code>{}</code>{}\n\nWhat would you like to ask?",
                    issue.url,
                    sender.escape(&issue.key),
                    sender.escape(&issue.summary),
                    sender.escape(&issue.status),
                    sender.escape(repo_name),
                    sender.escape(&branch),
                    if clean { "" } else { " \u{26a0}\u{fe0f} dirty" },
                ),
            )
            .await?;
        return Ok(());
    }

    // Multiple repos — show picker, preserving context in pending ask
    use crate::bot::state::PendingAsk;
    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        // Store context temporarily; session will be created after repo selection
        entry.ask_session = Some(AskSession::new(user_id, None, None).with_context(context));
        entry.pending_ask = Some(PendingAsk {
            repo_path: None,
            git: None,
            inline_question: None,
            mode: None,
        });
    }

    let buttons: Vec<Vec<Button>> = repos
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
            vec![Button::new(label, format!("ask:repo:{}", i))]
        })
        .collect();

    sender
        .send_with_keyboard(
            chat_id,
            &format!(
                "\u{1f4cb} <b><a href=\"{}\">{}</a></b> \u{2014} {}\n\nSelect a repository:",
                issue.url,
                sender.escape(&issue.key),
                sender.escape(&issue.summary),
            ),
            buttons,
        )
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Callback router for all tickets:* callbacks
// ---------------------------------------------------------------------------

pub async fn handle_my_tickets_callback(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action_data: &str,
    state: Arc<AppState>,
) -> Result<()> {
    // tickets:project:<key>
    if let Some(key) = action_data.strip_prefix("tickets:project:") {
        return handle_my_tickets_project(Arc::clone(&sender), chat_id, user_id, state, key).await;
    }

    // tickets:status:<project_key>:<status>
    if let Some(rest) = action_data.strip_prefix("tickets:status:") {
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if parts.len() == 2 {
            return handle_my_tickets_status(
                Arc::clone(&sender),
                chat_id,
                user_id,
                state,
                parts[0],
                parts[1],
            )
            .await;
        }
        return Ok(());
    }

    // tickets:page:<page_index>
    if let Some(page_str) = action_data.strip_prefix("tickets:page:") {
        let page: usize = page_str.parse().unwrap_or(0);
        return handle_my_tickets_page(Arc::clone(&sender), chat_id, user_id, state, page).await;
    }

    // tickets:refresh:<page_index> — re-fetch from Jira (clears token cache, goes to page 0)
    if action_data.starts_with("tickets:refresh:") {
        let (project_key, status_filter) = {
            let cs = state.chat_states.get(chat_id);
            match cs.as_ref().and_then(|c| c.page_cache.as_ref()) {
                Some(cache) => (cache.project_key.clone(), cache.status_filter.clone()),
                None => {
                    sender
                        .send(chat_id, "No list context. Use /my_tickets.")
                        .await?;
                    return Ok(());
                }
            }
        };
        let filter = status_filter.as_deref().unwrap_or("ALL");
        return handle_my_tickets_status(
            Arc::clone(&sender),
            chat_id,
            user_id,
            state,
            &project_key,
            filter,
        )
        .await;
    }

    // tickets:details:<issue_key>
    if let Some(key) = action_data.strip_prefix("tickets:details:") {
        return handle_ticket_details(Arc::clone(&sender), chat_id, user_id, state, key).await;
    }

    // tickets:ask:<issue_key>
    if let Some(key) = action_data.strip_prefix("tickets:ask:") {
        return handle_ticket_ask(Arc::clone(&sender), chat_id, user_id, state, key).await;
    }

    // tickets:solve:<issue_key>
    if let Some(key) = action_data.strip_prefix("tickets:solve:") {
        return crate::bot::commands::solve::handle_repo_picker(
            Arc::clone(&sender),
            chat_id,
            user_id,
            state,
            key,
        )
        .await;
    }

    // tickets:move_start:<issue_key>
    if let Some(key) = action_data.strip_prefix("tickets:move_start:") {
        return handle_move_start(Arc::clone(&sender), chat_id, user_id, state, key).await;
    }

    // tickets:move_exec:<issue_key>:<status>
    if let Some(rest) = action_data.strip_prefix("tickets:move_exec:") {
        let parts: Vec<&str> = rest.splitn(2, ':').collect();
        if parts.len() == 2 {
            return handle_move_execute(
                Arc::clone(&sender),
                chat_id,
                user_id,
                state,
                parts[0],
                parts[1],
            )
            .await;
        }
        return Ok(());
    }

    // tickets:comment_start:<issue_key>
    if let Some(key) = action_data.strip_prefix("tickets:comment_start:") {
        return handle_comment_start(Arc::clone(&sender), chat_id, state, key).await;
    }

    Ok(())
}
