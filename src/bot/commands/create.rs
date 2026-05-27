use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use teloxide::prelude::*;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};

use crate::bot::state::JiraPendingAction;
use crate::bot::utils::{escape_html, keep_typing};
use crate::bot::AppState;
use crate::claude::types::AskOptions;

const IMPROVE_PROMPT: &str = "\
You are a technical project manager improving a Jira ticket.

Project: {project_key}
Original title: {title}

Your tasks:
1. Correct and improve the title — fix grammar, spelling, and clarity; keep it concise (under 80 chars).
2. Write a professional description including: brief overview, acceptance criteria as a bullet list, \
and relevant technical notes. Generate it from the title.

Respond in this exact format with nothing else before or after:
TITLE: <corrected title>

DESCRIPTION:
<description here>";

fn parse_claude_response(response: &str) -> (String, String) {
    let title = response
        .lines()
        .find(|l| l.starts_with("TITLE:"))
        .map(|l| l["TITLE:".len()..].trim().to_string())
        .unwrap_or_default();

    let desc = response
        .find("DESCRIPTION:\n")
        .map(|idx| response[idx + "DESCRIPTION:\n".len()..].trim().to_string())
        .unwrap_or_default();

    (title, desc)
}

/// Step 1: receives the raw title, corrects it with Claude, generates a description
/// suggestion, then shows it to the user with a "Use this" button.
pub async fn handle_create_suggest(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    _user_id: i64,
    project_key: String,
    title: String,
) -> Result<()> {
    if title.is_empty() {
        bot.send_message(
            chat_id,
            "Send the issue title:\n<code>New login page</code>",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    let thinking = bot
        .send_message(chat_id, "Correcting title and generating description...")
        .await?;
    let _typing = keep_typing(bot.clone(), chat_id);

    let prompt = IMPROVE_PROMPT
        .replace("{project_key}", &project_key)
        .replace("{title}", &title);

    let claude_output = match state.claude.ask(&prompt, AskOptions::default()).await {
        Ok((text, _)) => text,
        Err(e) => {
            state.logger.error(
                &format!("create: Claude error: {e}"),
                Some(&json!({ "title": &title })),
            );
            bot.edit_message_text(chat_id, thinking.id, format!("Claude error: {e}"))
                .await?;
            return Ok(());
        }
    };

    let (corrected_title, suggested_desc) = parse_claude_response(&claude_output);
    let corrected_title = if corrected_title.is_empty() {
        title
    } else {
        corrected_title
    };

    state
        .chat_states
        .entry(chat_id.0)
        .or_default()
        .pending_jira_action = Some(JiraPendingAction::CreateDescription(
        project_key,
        corrected_title.clone(),
        suggested_desc.clone(),
    ));

    let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
        "✅ Use this description",
        "jira:create_confirm",
    )]]);

    bot.edit_message_text(
        chat_id,
        thinking.id,
        format!(
            "Corrected title: <b>{}</b>\n\nSuggested description:\n<pre>{}</pre>\n\n\
             Tap <b>Use this description</b> or send your own below:",
            escape_html(&corrected_title),
            escape_html(&suggested_desc)
        ),
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(keyboard)
    .await?;

    Ok(())
}

/// Step 2: creates the Jira issue with the final (confirmed or custom) description.
pub async fn handle_create_confirm(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
    project_key: &str,
    title: &str,
    description: &str,
) -> Result<()> {
    state.logger.info(
        "create: creating Jira issue",
        Some(&json!({ "project": project_key, "title": title })),
    );

    let thinking = bot.send_message(chat_id, "Creating issue...").await?;

    let Some(jira) = state.jira_for_user(user_id) else {
        bot.edit_message_text(
            chat_id,
            thinking.id,
            "Please set up your Jira account first. Use /jira → My Jira.",
        )
        .await?;
        return Ok(());
    };
    let issue = match jira.create_issue(project_key, title, description).await {
        Ok(issue) => issue,
        Err(e) => {
            state.logger.error(
                &format!("create: Jira error: {e}"),
                Some(&json!({ "project": project_key, "title": title })),
            );
            bot.edit_message_text(chat_id, thinking.id, format!("Jira error: {e}"))
                .await?;
            return Ok(());
        }
    };

    state.logger.info(
        "create: issue created",
        Some(&json!({ "key": &issue.key, "url": &issue.url })),
    );

    bot.edit_message_text(
        chat_id,
        thinking.id,
        format!(
            "Created: <a href=\"{}\">{}</a> — {}",
            issue.url, issue.key, issue.summary
        ),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    Ok(())
}
