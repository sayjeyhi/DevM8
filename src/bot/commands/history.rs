use std::sync::Arc;

use anyhow::Result;

use crate::bot::AppState;
use crate::channel::{Button, ChannelSender};

const PAGE_SIZE: i64 = 8;
const ALL_PROJECTS: &str = "_ALL_";

/// Entry point for `/history` and the "History" button on `/status`. Always
/// scoped to the calling user's own resolved email — no cross-user browsing.
pub async fn handle_history(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
) -> Result<()> {
    let email = state.email_for_channel_user("telegram", user_id).await;
    let mut project_keys = state
        .db
        .list_project_keys_for_email(&email)
        .await
        .unwrap_or_default();
    project_keys.sort();

    if project_keys.is_empty() {
        show_session_page(sender, chat_id, &state, &email, None, 0).await?;
        return Ok(());
    }

    let mut keyboard: Vec<Vec<Button>> = project_keys
        .iter()
        .map(|key| vec![Button::new(key.clone(), format!("history:project:{key}"))])
        .collect();
    keyboard.push(vec![Button::new(
        "All projects",
        format!("history:project:{ALL_PROJECTS}"),
    )]);

    sender
        .send_with_keyboard(chat_id, "Which project's history?", keyboard)
        .await?;
    Ok(())
}

/// Routes `history:*` callback data — mirrors the shape of `handle_my_tickets_callback`.
pub async fn handle_history_callback(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action_data: &str,
    state: Arc<AppState>,
) -> Result<()> {
    if action_data == "history:root" {
        return handle_history(sender, chat_id, user_id, state).await;
    }

    let email = state.email_for_channel_user("telegram", user_id).await;

    if let Some(project) = action_data.strip_prefix("history:project:") {
        let project = normalize_project(project);
        show_session_page(sender, chat_id, &state, &email, project.as_deref(), 0).await?;
        return Ok(());
    }

    if let Some(rest) = action_data.strip_prefix("history:page:") {
        if let Some((project, offset)) = rest.rsplit_once(':') {
            let project = normalize_project(project);
            let offset: i64 = offset.parse().unwrap_or(0);
            show_session_page(sender, chat_id, &state, &email, project.as_deref(), offset).await?;
        }
        return Ok(());
    }

    if let Some(session_id) = action_data.strip_prefix("history:session:") {
        let turns = state.db.get_session_history(session_id).await?;
        if turns.is_empty() {
            sender.send(chat_id, "Session not found.").await?;
            return Ok(());
        }
        let text = turns
            .iter()
            .map(|t| format!("--- {} ({}) ---\n{}", t.role, t.created_at, t.content))
            .collect::<Vec<_>>()
            .join("\n\n");
        sender.send_in_chunks(chat_id, &text).await?;
        return Ok(());
    }

    Ok(())
}

fn normalize_project(raw: &str) -> Option<String> {
    if raw == ALL_PROJECTS {
        None
    } else {
        Some(raw.to_string())
    }
}

async fn show_session_page(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: &Arc<AppState>,
    email: &str,
    project: Option<&str>,
    offset: i64,
) -> Result<()> {
    // Fetch one extra row to know whether a "Next" page exists.
    let mut sessions = state
        .db
        .list_sessions(email, project, PAGE_SIZE + 1, offset)
        .await?;
    let has_next = sessions.len() as i64 > PAGE_SIZE;
    sessions.truncate(PAGE_SIZE as usize);

    if sessions.is_empty() && offset == 0 {
        sender.send(chat_id, "No chat history found.").await?;
        return Ok(());
    }

    let project_slug = project.unwrap_or(ALL_PROJECTS).to_string();

    let mut keyboard: Vec<Vec<Button>> = sessions
        .iter()
        .map(|s| {
            let label = format!(
                "{} {}",
                s.started_at.split('T').next().unwrap_or(&s.started_at),
                truncate(&s.first_message, 40),
            );
            vec![Button::new(
                label,
                format!("history:session:{}", s.session_id),
            )]
        })
        .collect();

    let mut nav_row = Vec::new();
    if offset > 0 {
        nav_row.push(Button::new(
            "\u{25c0}\u{fe0f} Prev",
            format!(
                "history:page:{project_slug}:{}",
                (offset - PAGE_SIZE).max(0)
            ),
        ));
    }
    if has_next {
        nav_row.push(Button::new(
            "Next \u{25b6}\u{fe0f}",
            format!("history:page:{project_slug}:{}", offset + PAGE_SIZE),
        ));
    }
    if !nav_row.is_empty() {
        keyboard.push(nav_row);
    }

    let title = match project {
        Some(p) => format!("History \u{2014} {p}"),
        None => "History \u{2014} all projects".to_string(),
    };
    sender.send_with_keyboard(chat_id, &title, keyboard).await?;
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max).collect::<String>())
    }
}
