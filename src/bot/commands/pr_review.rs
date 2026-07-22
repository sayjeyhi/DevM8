use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::bot::state::PrReviewComment;
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender};
use crate::claude::types::AskOptions;
use crate::shared::errors::{AppError, ClaudeError};

const PR_REVIEW_PROMPT_TEMPLATE: &str = "\
You are a senior software engineer performing a code review of a GitHub pull request.

Pull request URL: {url}

Use the `gh` CLI (already authenticated) to gather context, for example:
  gh pr view {url} --json number,title,body,url
  gh pr diff {url}

Review the diff for bugs, security issues, style problems, missing tests, and unclear \
code. Then output your findings as a SINGLE JSON object and nothing else — no markdown \
fences, no commentary before or after it. Shape:

{{
  \"pr_title\": \"...\",
  \"summary\": \"one or two sentence overview of the change\",
  \"comments\": [
    {{\"file\": \"path/to/file\", \"line\": 123, \"severity\": \"bug|suggestion|nit|question\", \"title\": \"short title\", \"body\": \"full explanation and suggested fix\"}}
  ]
}}

If you find nothing worth flagging, return an empty \"comments\" array.";

#[derive(Debug, Deserialize)]
struct PrReviewOutput {
    #[serde(default)]
    pr_title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    comments: Vec<PrReviewComment>,
}

/// Runs Claude's review of `url` and, on success, stores the resulting comments on
/// `chat_state.pending_pr_review` and shows a picker to view each one's detail.
pub async fn start_pr_review(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    url: &str,
) -> Result<()> {
    let cancel_kb = vec![vec![Button::new("Cancel", "prreview:cancel")]];
    let status_ref = sender
        .send_with_keyboard(chat_id, "Reviewing pull request with Claude...", cancel_kb)
        .await?;
    let typing = sender.start_typing(chat_id);

    let ct = CancellationToken::new();
    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.cancel_token = Some(ct.clone());
    }

    let prompt = PR_REVIEW_PROMPT_TEMPLATE.replace("{url}", url);
    let opts = AskOptions {
        cancel_token: Some(ct),
        ..AskOptions::default()
    };

    let ask_result = state.ai.ask(&prompt, opts).await;
    typing.abort();
    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.cancel_token = None;
    }

    let text = match ask_result {
        Ok((text, _)) => text,
        Err(AppError::Claude(ClaudeError::Cancelled)) => {
            sender
                .edit_with_keyboard(&status_ref, "Cancelled.", vec![])
                .await?;
            return Ok(());
        }
        Err(e) => {
            state.logger.error(
                &format!("pr-review: Claude error: {e}"),
                Some(&json!({ "url": url })),
            );
            sender
                .edit_with_keyboard(&status_ref, &format!("Claude error: {}", e), vec![])
                .await?;
            return Ok(());
        }
    };

    let parsed: PrReviewOutput = match extract_json(&text)
        .and_then(|j| serde_json::from_str(&j).context("parsing pr-review JSON"))
    {
        Ok(p) => p,
        Err(e) => {
            state.logger.error(
                &format!("pr-review: failed to parse Claude output: {e}"),
                Some(&json!({ "url": url })),
            );
            sender
                .edit_with_keyboard(
                    &status_ref,
                    "Review complete, but the response wasn't structured as expected — showing raw output:",
                    vec![],
                )
                .await?;
            sender.send_in_chunks(chat_id, &text).await?;
            return Ok(());
        }
    };

    let header = if parsed.comments.is_empty() {
        format!(
            "No issues found for {}.",
            sender.bold(&sender.escape(&parsed.pr_title))
        )
    } else {
        format!(
            "Review of {} — {} comment(s):\n{}",
            sender.bold(&sender.escape(&parsed.pr_title)),
            parsed.comments.len(),
            sender.escape(&parsed.summary)
        )
    };
    sender.edit_with_keyboard(&status_ref, &header, vec![]).await?;

    if parsed.comments.is_empty() {
        return Ok(());
    }

    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.pending_pr_review = Some(parsed.comments.clone());
    }

    send_comment_picker(&sender, chat_id, &parsed.comments).await
}

async fn send_comment_picker(
    sender: &Arc<dyn ChannelSender>,
    chat_id: &str,
    comments: &[PrReviewComment],
) -> Result<()> {
    let keyboard: Vec<Vec<Button>> = comments
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let loc = match c.line {
                Some(l) => format!("{}:{}", c.file, l),
                None => c.file.clone(),
            };
            vec![Button::new(
                format!("[{}] {} — {}", c.severity, loc, c.title),
                format!("prreview:show:{i}"),
            )]
        })
        .collect();
    sender
        .send_with_keyboard(chat_id, "Select a comment to view details:", keyboard)
        .await?;
    Ok(())
}

/// Routes a "prreview:*" action button — "cancel" aborts an in-flight review,
/// "show:<idx>" prints one comment's full detail and re-shows the picker.
pub async fn handle_pr_review_action(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    action: &str,
    state: Arc<AppState>,
) -> Result<()> {
    if action == "prreview:cancel" {
        let cancelled = {
            let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
            match entry.cancel_token.take() {
                Some(ct) => {
                    ct.cancel();
                    true
                }
                None => false,
            }
        };
        if !cancelled {
            sender.send(chat_id, "No active request to cancel.").await?;
        }
        return Ok(());
    }

    let Some(idx_str) = action.strip_prefix("prreview:show:") else {
        return Ok(());
    };
    let Ok(idx) = idx_str.parse::<usize>() else {
        return Ok(());
    };

    let comments = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_pr_review.clone());
    let Some(comments) = comments else {
        sender
            .send(chat_id, "This review session has expired — run pr-review again.")
            .await?;
        return Ok(());
    };
    let Some(c) = comments.get(idx) else {
        return Ok(());
    };

    let loc = match c.line {
        Some(l) => format!("{}:{}", c.file, l),
        None => c.file.clone(),
    };
    let detail = format!(
        "{}\n{}\n\n{}",
        sender.bold(&sender.escape(&format!("[{}] {}", c.severity, loc))),
        sender.escape(&c.title),
        sender.escape(&c.body)
    );
    sender.send(chat_id, &detail).await?;

    send_comment_picker(&sender, chat_id, &comments).await
}

/// Pulls the first top-level `{ ... }` JSON object out of `text`, tolerating an
/// optional surrounding ```json fence — Claude is instructed to emit bare JSON
/// but sometimes wraps it anyway.
fn extract_json(text: &str) -> Result<String> {
    let trimmed = text.trim();
    let stripped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim_start())
        .unwrap_or(trimmed);
    let stripped = stripped.strip_suffix("```").unwrap_or(stripped).trim();

    let start = stripped
        .find('{')
        .context("no JSON object found in Claude output")?;
    let end = stripped
        .rfind('}')
        .context("no JSON object found in Claude output")?;
    Ok(stripped[start..=end].to_string())
}
