use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode};

use crate::bot::utils::keep_typing;
use crate::bot::AppState;
use crate::claude::types::AskOptions;

const ENRICH_PROMPT_TEMPLATE: &str = "\
You are a technical project manager writing Jira ticket descriptions.

Title: {title}

Raw description provided by the developer:
{description}

Please rewrite the description to be clear, concise, and well-structured for a Jira ticket. \
Include: a brief overview, acceptance criteria (as a bullet list), and any relevant technical notes. \
Keep the tone professional. Output only the final description text with no preamble.";

const EXPAND_PROMPT_TEMPLATE: &str = "\
You are a technical project manager writing Jira ticket descriptions.

Title: {title}

Based only on the title, write a clear, concise Jira ticket description. \
Include: a brief overview, acceptance criteria (as a bullet list), and any relevant technical notes. \
Keep the tone professional. Output only the final description text with no preamble.";

pub async fn handle_create(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let args = args.trim().to_string();
    if args.is_empty() {
        bot.send_message(
            chat_id,
            "Send the issue title (optionally with description after <code>--</code>):\n\
             <code>New login page -- Add OAuth2 support</code>",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    let (title, raw_description, use_enrich) = if let Some(idx) = args.find(" -- ") {
        let t = args[..idx].trim().to_string();
        let d = args[idx + 4..].trim().to_string();
        (t, d, true)
    } else {
        (args.clone(), String::new(), false)
    };

    state.logger.info(
        "create: generating description with Claude",
        Some(&json!({ "title": &title, "has_description": use_enrich })),
    );

    let thinking = bot.send_message(chat_id, "Thinking...").await?;
    let _typing = keep_typing(bot.clone(), chat_id);

    let prompt = if use_enrich {
        ENRICH_PROMPT_TEMPLATE
            .replace("{title}", &title)
            .replace("{description}", &raw_description)
    } else {
        EXPAND_PROMPT_TEMPLATE.replace("{title}", &title)
    };

    let description = match state.claude.ask(&prompt, AskOptions::default()).await {
        Ok(text) => text,
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

    state.logger.info(
        "create: creating Jira issue",
        Some(&json!({ "title": &title })),
    );

    let issue = match state.jira.create_issue(&title, &description).await {
        Ok(issue) => issue,
        Err(e) => {
            state.logger.error(
                &format!("create: Jira error: {e}"),
                Some(&json!({ "title": &title })),
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
