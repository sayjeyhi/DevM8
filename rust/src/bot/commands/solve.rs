use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};

use crate::bot::state::{ChatState, PendingSolve};
use crate::bot::utils::{escape_html, keep_typing, split_message};
use crate::bot::AppState;
use crate::claude::types::AskOptions;

const SOLVE_PROMPT_TEMPLATE: &str = "\
You are a senior software engineer analyzing a Jira issue.

Issue Key: {key}
Summary: {summary}
Status: {status}
Description:
{description}

Please provide:
1. **Assessment** — a brief analysis of what needs to be done and why.
2. **Implementation Steps** — a numbered list of concrete steps to resolve this issue.
3. **Risks & Considerations** — any edge cases, potential pitfalls, or dependencies to be aware of.

Be specific, technical, and actionable. Format your response clearly.";

// ---------------------------------------------------------------------------
// Core solve logic
// ---------------------------------------------------------------------------

/// Fetch issue, ask Claude, stream progress into an editable message,
/// then post the full solution as chunked messages.
pub async fn solve_by_key(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    issue_key: &str,
    cwd: Option<String>,
) -> Result<()> {
    // Send an initial "working" message
    let status_msg = bot
        .send_message(
            chat_id,
            format!("Analyzing <b>{}</b> with Claude...", escape_html(issue_key)),
        )
        .parse_mode(ParseMode::Html)
        .await?;

    let status_msg_id = status_msg.id;

    let _typing = keep_typing(bot.clone(), chat_id);

    // Fetch the issue
    let issue = match state.jira.get_issue_by_key(issue_key).await {
        Ok(i) => i,
        Err(e) => {
            bot.edit_message_text(
                chat_id,
                status_msg_id,
                format!("Could not fetch <b>{}</b>: {}", escape_html(issue_key), e),
            )
            .parse_mode(ParseMode::Html)
            .await?;
            return Ok(());
        }
    };

    let prompt = SOLVE_PROMPT_TEMPLATE
        .replace("{key}", &issue.key)
        .replace("{summary}", &issue.summary)
        .replace("{status}", &issue.status)
        .replace("{description}", &issue.description);

    // Progress closure — edit message with partial output
    let bot_progress = bot.clone();
    let chat_id_progress = chat_id;
    let msg_id_progress = status_msg_id;
    let key_progress = issue_key.to_string();

    let on_progress: crate::claude::types::ProgressCallback = Box::new(move |lines: Vec<String>| {
        let bot = bot_progress.clone();
        let chat_id = chat_id_progress;
        let msg_id = msg_id_progress;
        let key = key_progress.clone();
        let preview = lines.join("").chars().take(200).collect::<String>();
        Box::pin(async move {
            let text = if preview.is_empty() {
                format!("Analyzing <b>{}</b> with Claude...", escape_html(&key))
            } else {
                format!(
                    "Analyzing <b>{}</b>...\n\n<pre>{}</pre>",
                    escape_html(&key),
                    escape_html(&preview)
                )
            };
            let _ = bot
                .edit_message_text(chat_id, msg_id, text)
                .parse_mode(ParseMode::Html)
                .await;
        })
    });

    let opts = AskOptions {
        on_progress: Some(on_progress),
        cwd,
        ..AskOptions::default()
    };

    let analysis = match state.claude.ask(&prompt, opts).await {
        Ok(text) => text,
        Err(e) => {
            bot.edit_message_text(
                chat_id,
                status_msg_id,
                format!("Claude error: {}", e),
            )
            .await?;
            return Ok(());
        }
    };

    // Final status edit
    bot.edit_message_text(
        chat_id,
        status_msg_id,
        format!(
            "Analysis complete for <b>{}</b>",
            escape_html(issue_key)
        ),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    // Post solution in chunks
    let chunks = split_message(&analysis, 4096);
    for chunk in &chunks {
        bot.send_message(chat_id, chunk).await?;
    }

    // Also add as a Jira comment
    let _ = state.jira.add_comment(issue_key, &analysis).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// Repo picker
// ---------------------------------------------------------------------------

/// Show inline keyboard for repo selection (when multiple repos exist for the project key).
pub async fn handle_repo_picker(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    issue_key: &str,
) -> Result<()> {
    // Determine project key from issue key (e.g., "PROJ-123" -> "PROJ")
    let project_key = issue_key
        .split('-')
        .next()
        .unwrap_or("")
        .to_uppercase();

    let repos = state.git_map.get(&project_key).cloned().unwrap_or_default();

    if repos.is_empty() {
        // No git context — solve directly
        return solve_by_key(bot, chat_id, state, issue_key, None).await;
    }

    if repos.len() == 1 {
        // Only one repo — go straight to branch picker
        if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
            cs.pending_solve = Some(PendingSolve {
                issue_key: issue_key.to_string(),
                git: Some(Arc::clone(&repos[0])),
            });
        } else {
            let mut cs = ChatState::default();
            cs.pending_solve = Some(PendingSolve {
                issue_key: issue_key.to_string(),
                git: Some(Arc::clone(&repos[0])),
            });
            state.chat_states.insert(chat_id.0, cs);
        }
        return handle_branch_picker(bot, chat_id, state).await;
    }

    // Multiple repos — show picker
    let buttons: Vec<Vec<InlineKeyboardButton>> = repos
        .iter()
        .enumerate()
        .map(|(i, git)| {
            let label = git
                .repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo")
                .to_string();
            vec![InlineKeyboardButton::callback(
                label,
                format!("solve:repo:{}:{}", issue_key, i),
            )]
        })
        .collect();

    let keyboard = InlineKeyboardMarkup::new(buttons);
    bot.send_message(
        chat_id,
        format!(
            "Select the repository to use for <b>{}</b>:",
            escape_html(issue_key)
        ),
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(keyboard)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Branch picker
// ---------------------------------------------------------------------------

/// Show inline keyboard: stash+new branch, new branch (keep work), or stay on current.
pub async fn handle_branch_picker(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
) -> Result<()> {
    let pending = {
        state
            .chat_states
            .get(&chat_id.0)
            .and_then(|cs| cs.pending_solve.clone())
    };

    let (issue_key, git) = match pending {
        Some(p) => (p.issue_key, p.git),
        None => {
            bot.send_message(chat_id, "No pending solve action.").await?;
            return Ok(());
        }
    };

    let (current_branch, is_clean) = if let Some(ref g) = git {
        let branch = g.current_branch().await.unwrap_or_else(|_| "unknown".to_string());
        let clean = g.is_clean().await.unwrap_or(false);
        (branch, clean)
    } else {
        ("(none)".to_string(), true)
    };

    let clean_label = if is_clean { "" } else { " (dirty)" };
    let text = format!(
        "Repository is on branch: <b>{}{}</b>\n\nHow would you like to proceed?",
        escape_html(&current_branch),
        clean_label
    );

    let mut buttons: Vec<Vec<InlineKeyboardButton>> = vec![
        vec![InlineKeyboardButton::callback(
            format!("New branch (from main)"),
            format!("solve:branch:new:{}", issue_key),
        )],
        vec![InlineKeyboardButton::callback(
            "Stay on current branch",
            format!("solve:branch:curr:{}", issue_key),
        )],
    ];

    if !is_clean {
        buttons.insert(
            0,
            vec![InlineKeyboardButton::callback(
                "Stash changes & new branch",
                format!("solve:branch:stash:{}", issue_key),
            )],
        );
    }

    let keyboard = InlineKeyboardMarkup::new(buttons);
    bot.send_message(chat_id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Branch choice handler (callback)
// ---------------------------------------------------------------------------

/// Called when user picks "new", "curr", or "stash" from the branch picker.
pub async fn handle_branch_choice(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    choice: &str,
    issue_key: &str,
) -> Result<()> {
    let pending = {
        state
            .chat_states
            .get(&chat_id.0)
            .and_then(|cs| cs.pending_solve.clone())
    };

    let git = pending.and_then(|p| p.git);

    let cwd = git
        .as_ref()
        .map(|g| g.repo_path.to_string_lossy().to_string());

    if let Some(ref g) = git {
        match choice {
            "stash" => {
                if let Err(e) = g.stash(Some(&format!("devm8: before solving {}", issue_key))).await {
                    bot.send_message(chat_id, format!("Failed to stash: {e}")).await?;
                    return Ok(());
                }
                // Create new branch after stash
                let branch_name = format!("devm8/{}", issue_key.to_lowercase().replace('/', "-"));
                if let Err(e) = g.checkout_new_branch_from_main(&branch_name, "origin", "main").await {
                    bot.send_message(
                        chat_id,
                        format!("Stashed, but failed to create branch: {e}"),
                    )
                    .await?;
                    return Ok(());
                }
                bot.send_message(
                    chat_id,
                    format!("Changes stashed. Created branch <b>{}</b>.", escape_html(&branch_name)),
                )
                .parse_mode(ParseMode::Html)
                .await?;
            }
            "new" => {
                let branch_name = format!("devm8/{}", issue_key.to_lowercase().replace('/', "-"));
                if let Err(e) = g.checkout_new_branch_from_main(&branch_name, "origin", "main").await {
                    bot.send_message(chat_id, format!("Failed to create branch: {e}")).await?;
                    return Ok(());
                }
                bot.send_message(
                    chat_id,
                    format!("Created branch <b>{}</b>.", escape_html(&branch_name)),
                )
                .parse_mode(ParseMode::Html)
                .await?;
            }
            "curr" | _ => {
                // Stay on current branch — do nothing
            }
        }
    }

    // Clear pending solve
    if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
        cs.pending_solve = None;
    }

    solve_by_key(bot, chat_id, state, issue_key, cwd).await
}

// ---------------------------------------------------------------------------
// Main command handler
// ---------------------------------------------------------------------------

pub async fn handle_solve(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let issue_key = args.trim().to_string();
    if issue_key.is_empty() {
        bot.send_message(
            msg.chat.id,
            "Usage: /solve &lt;issue-key&gt;",
        )
        .parse_mode(ParseMode::Html)
        .await?;
        return Ok(());
    }

    let project_key = issue_key
        .split('-')
        .next()
        .unwrap_or("")
        .to_uppercase();

    let has_repos = state.git_map.contains_key(&project_key);

    if has_repos {
        handle_repo_picker(bot, msg.chat.id, state, &issue_key).await
    } else {
        solve_by_key(bot, msg.chat.id, state, &issue_key, None).await
    }
}

// ---------------------------------------------------------------------------
// Callback: solve:repo:<key>:<index>
// ---------------------------------------------------------------------------

pub async fn handle_solve_repo_callback(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<AppState>,
) -> Result<()> {
    let _ = bot.answer_callback_query(q.id.clone()).await;

    let data = q.data.as_deref().unwrap_or("");
    // format: solve:repo:<issue_key>:<repo_index>
    let parts: Vec<&str> = data.splitn(4, ':').collect();
    if parts.len() < 4 {
        return Ok(());
    }
    let issue_key = parts[2];
    let repo_idx: usize = parts[3].parse().unwrap_or(0);

    let chat_id = match q.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };

    let project_key = issue_key
        .split('-')
        .next()
        .unwrap_or("")
        .to_uppercase();

    let repos = state.git_map.get(&project_key).cloned().unwrap_or_default();
    let git = repos.get(repo_idx).cloned();

    // Store pending solve with chosen git
    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        entry.pending_solve = Some(PendingSolve {
            issue_key: issue_key.to_string(),
            git,
        });
    }

    handle_branch_picker(bot, chat_id, state).await
}
