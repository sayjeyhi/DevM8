use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Local;
use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::bot::state::PrReviewComment;
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender};
use crate::claude::types::{AskOptions, ProgressCallback};
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

/// Runs Claude's review of `url`, showing live progress as it works, then
/// prints every comment's full detail inline and saves the whole report to
/// `~/.devm8/pr-reviews/`.
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

    let started = Instant::now();
    let sender_cb = Arc::clone(&sender);
    let status_ref_cb = status_ref.clone();
    let on_progress: ProgressCallback = Box::new(move |lines: Vec<String>| {
        let sender = Arc::clone(&sender_cb);
        let sref = status_ref_cb.clone();
        let kb = vec![vec![Button::new("Cancel", "prreview:cancel")]];
        let elapsed = started.elapsed().as_secs();
        let generated: usize = lines.iter().map(|l| l.len()).sum();
        Box::pin(async move {
            let text = if generated == 0 {
                format!(
                    "Reviewing pull request with Claude... ({elapsed}s elapsed — fetching PR \
                     details and diff via gh CLI)"
                )
            } else {
                format!(
                    "Reviewing pull request with Claude... ({elapsed}s elapsed, {generated} \
                     chars of findings generated so far)"
                )
            };
            let _ = sender.edit_with_keyboard(&sref, &text, kb).await;
        })
    });

    let opts = AskOptions {
        on_progress: Some(on_progress),
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
            notify_saved_report(&sender, chat_id, &state, url, &text).await;
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

    if !parsed.comments.is_empty() {
        let detail = render_comments_detail(&sender, &parsed.comments);
        sender.send_in_chunks(chat_id, &detail).await?;
    }

    let report = render_report_markdown(url, &parsed);
    notify_saved_report(&sender, chat_id, &state, url, &report).await;

    Ok(())
}

/// Formats every comment's full detail (location, severity, title, body) as
/// one message, replacing the old "pick a comment to view" flow.
fn render_comments_detail(sender: &Arc<dyn ChannelSender>, comments: &[PrReviewComment]) -> String {
    comments
        .iter()
        .map(|c| {
            let loc = match c.line {
                Some(l) => format!("{}:{}", c.file, l),
                None => c.file.clone(),
            };
            format!(
                "{}\n{}\n\n{}",
                sender.bold(&sender.escape(&format!("[{}] {}", c.severity, loc))),
                sender.escape(&c.title),
                sender.escape(&c.body)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

/// Plain-text (unescaped) version of the report, suitable for writing to disk.
fn render_report_markdown(url: &str, parsed: &PrReviewOutput) -> String {
    let mut out = format!(
        "# PR Review: {}\n\nURL: {}\nReviewed: {}\n\n## Summary\n{}\n\n",
        parsed.pr_title,
        url,
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        parsed.summary,
    );

    if parsed.comments.is_empty() {
        out.push_str("No issues found.\n");
        return out;
    }

    out.push_str(&format!("## Comments ({})\n\n", parsed.comments.len()));
    for c in &parsed.comments {
        let loc = match c.line {
            Some(l) => format!("{}:{}", c.file, l),
            None => c.file.clone(),
        };
        out.push_str(&format!(
            "### [{}] {} — {}\n{}\n\n",
            c.severity, loc, c.title, c.body
        ));
    }
    out
}

/// Saves `contents` under `~/.devm8/pr-reviews/` and tells the user where it
/// landed, or logs and reports the error if saving failed.
async fn notify_saved_report(
    sender: &Arc<dyn ChannelSender>,
    chat_id: &str,
    state: &Arc<AppState>,
    url: &str,
    contents: &str,
) {
    match save_report(url, contents) {
        Ok(path) => {
            let _ = sender
                .send(chat_id, &format!("Saved full review to {}", path.display()))
                .await;
        }
        Err(e) => {
            state.logger.error(
                &format!("pr-review: failed to save report: {e}"),
                Some(&json!({ "url": url })),
            );
            let _ = sender
                .send(chat_id, &format!("Couldn't save the review to disk: {e}"))
                .await;
        }
    }
}

fn reviews_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    let dir = home.join(".devm8").join("pr-reviews");
    std::fs::create_dir_all(&dir).context("creating ~/.devm8/pr-reviews")?;
    Ok(dir)
}

fn save_report(url: &str, contents: &str) -> Result<PathBuf> {
    let dir = reviews_dir()?;
    let filename = format!(
        "{}_{}.md",
        pr_slug(url),
        Local::now().format("%Y%m%d-%H%M%S")
    );
    let path = dir.join(filename);
    std::fs::write(&path, contents).context("writing pr review file")?;
    Ok(path)
}

/// Turns `https://github.com/owner/repo/pull/123` into `owner-repo-123`,
/// falling back to a sanitized version of the whole URL for anything else.
fn pr_slug(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    let parts: Vec<&str> = trimmed.rsplitn(4, '/').collect();
    if parts.len() == 4 && parts[1] == "pull" {
        format!("{}-{}-{}", parts[3], parts[2], parts[0])
    } else {
        trimmed
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect()
    }
}

/// Routes a "prreview:*" action button — currently only "cancel", which
/// aborts an in-flight review.
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
    }
    Ok(())
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
