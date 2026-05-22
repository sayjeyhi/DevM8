use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::ParseMode;

use crate::bot::AppState;
use crate::bot::utils::keep_typing;
use crate::claude::types::AskOptions;

/// Prompt used when both title and description are supplied (-- separator).
const ENRICH_PROMPT_TEMPLATE: &str = "\
You are a technical project manager writing Jira ticket descriptions.

Title: {title}

Raw description provided by the developer:
{description}

Please rewrite the description to be clear, concise, and well-structured for a Jira ticket. \
Include: a brief overview, acceptance criteria (as a bullet list), and any relevant technical notes. \
Keep the tone professional. Output only the final description text with no preamble.";

/// Prompt used when only a title is provided (expand it into a description).
const EXPAND_PROMPT_TEMPLATE: &str = "\
You are a technical project manager writing Jira ticket descriptions.

Title: {title}

Based only on the title, write a clear, concise Jira ticket description. \
Include: a brief overview, acceptance criteria (as a bullet list), and any relevant technical notes. \
Keep the tone professional. Output only the final description text with no preamble.";

pub async fn handle_create(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let args = args.trim().to_string();
    if args.is_empty() {
        bot.send_message(
            msg.chat.id,
            "Usage: /create &lt;title&gt; [-- &lt;description&gt;]",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    // Check for " -- " separator
    let (title, raw_description, use_enrich) = if let Some(idx) = args.find(" -- ") {
        let t = args[..idx].trim().to_string();
        let d = args[idx + 4..].trim().to_string();
        (t, d, true)
    } else {
        (args.clone(), String::new(), false)
    };

    let thinking = bot
        .send_message(msg.chat.id, "Thinking...")
        .await?;
    let _typing = keep_typing(bot.clone(), msg.chat.id);

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
            bot.edit_message_text(msg.chat.id, thinking.id, format!("Claude error: {e}"))
                .await?;
            return Ok(());
        }
    };

    let issue = match state.jira.create_issue(&title, &description).await {
        Ok(issue) => issue,
        Err(e) => {
            bot.edit_message_text(msg.chat.id, thinking.id, format!("Jira error: {e}"))
                .await?;
            return Ok(());
        }
    };

    bot.edit_message_text(
        msg.chat.id,
        thinking.id,
        format!(
            "Created: <a href=\"{}\">{}</a> — {}",
            issue.url,
            issue.key,
            issue.summary
        ),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    Ok(())
}
