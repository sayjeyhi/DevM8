use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use teloxide::prelude::*;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};

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

pub async fn solve_by_key(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    issue_key: &str,
    cwd: Option<String>,
) -> Result<()> {
    state.logger.info(
        "solve: fetching issue",
        Some(&json!({ "key": issue_key, "cwd": cwd.as_deref().unwrap_or("(none)") })),
    );

    let status_msg = bot
        .send_message(
            chat_id,
            format!("Analyzing <b>{}</b> with Claude...", escape_html(issue_key)),
        )
        .parse_mode(ParseMode::Html)
        .await?;

    let status_msg_id = status_msg.id;
    let _typing = keep_typing(bot.clone(), chat_id);

    let issue = match state.jira.get_issue_by_key(issue_key).await {
        Ok(i) => i,
        Err(e) => {
            state.logger.error(
                &format!("solve: failed to fetch issue: {e}"),
                Some(&json!({ "key": issue_key })),
            );
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

    state.logger.info(
        "solve: issue fetched, asking Claude",
        Some(&json!({ "key": &issue.key, "status": &issue.status })),
    );

    let prompt = SOLVE_PROMPT_TEMPLATE
        .replace("{key}", &issue.key)
        .replace("{summary}", &issue.summary)
        .replace("{status}", &issue.status)
        .replace("{description}", &issue.description);

    let bot_progress = bot.clone();
    let chat_id_progress = chat_id;
    let msg_id_progress = status_msg_id;
    let key_progress = issue_key.to_string();

    let on_progress: crate::claude::types::ProgressCallback =
        Box::new(move |lines: Vec<String>| {
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
            state.logger.error(
                &format!("solve: Claude error: {e}"),
                Some(&json!({ "key": issue_key })),
            );
            bot.edit_message_text(chat_id, status_msg_id, format!("Claude error: {}", e))
                .await?;
            return Ok(());
        }
    };

    bot.edit_message_text(
        chat_id,
        status_msg_id,
        format!("Analysis complete for <b>{}</b>", escape_html(issue_key)),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    let chunks = split_message(&analysis, 4096);
    for chunk in &chunks {
        bot.send_message(chat_id, chunk).await?;
    }

    state.logger.info(
        "solve: posting analysis as Jira comment",
        Some(&json!({ "key": issue_key })),
    );
    match state.jira.add_comment(issue_key, &analysis).await {
        Ok(()) => {
            state
                .logger
                .info("solve: comment posted", Some(&json!({ "key": issue_key })));
        }
        Err(e) => {
            state.logger.warn(
                &format!("solve: failed to post comment: {e}"),
                Some(&json!({ "key": issue_key })),
            );
        }
    }

    Ok(())
}

pub async fn handle_repo_picker(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    issue_key: &str,
) -> Result<()> {
    let project_key = issue_key.split('-').next().unwrap_or("").to_uppercase();

    let repos = state.git_map.get(&project_key).cloned().unwrap_or_default();

    if repos.is_empty() {
        state.logger.info(
            "solve: no repos configured, solving without git context",
            Some(&json!({ "key": issue_key })),
        );
        return solve_by_key(bot, chat_id, state, issue_key, None).await;
    }

    if repos.len() == 1 {
        state.logger.info(
            "solve: single repo, proceeding to branch picker",
            Some(&json!({ "key": issue_key, "repo": repos[0].repo_path.display().to_string() })),
        );
        if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
            cs.pending_solve = Some(PendingSolve {
                issue_key: issue_key.to_string(),
                git: Some(Arc::clone(&repos[0])),
            });
        } else {
            state.chat_states.insert(
                chat_id.0,
                ChatState {
                    pending_solve: Some(PendingSolve {
                        issue_key: issue_key.to_string(),
                        git: Some(Arc::clone(&repos[0])),
                    }),
                    ..Default::default()
                },
            );
        }
        return handle_branch_picker(bot, chat_id, state).await;
    }

    state.logger.info(
        "solve: multiple repos, showing picker",
        Some(&json!({ "key": issue_key, "repo_count": repos.len() })),
    );

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

pub async fn handle_branch_picker(bot: Bot, chat_id: ChatId, state: Arc<AppState>) -> Result<()> {
    let pending = {
        state
            .chat_states
            .get(&chat_id.0)
            .and_then(|cs| cs.pending_solve.clone())
    };

    let (issue_key, git) = match pending {
        Some(p) => (p.issue_key, p.git),
        None => {
            bot.send_message(chat_id, "No pending solve action.")
                .await?;
            return Ok(());
        }
    };

    let (current_branch, is_clean) = if let Some(ref g) = git {
        let branch = g
            .current_branch()
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        let clean = g.is_clean().await.unwrap_or(false);
        (branch, clean)
    } else {
        ("(none)".to_string(), true)
    };

    state.logger.info(
        "solve: branch picker",
        Some(&json!({
            "key": &issue_key,
            "branch": &current_branch,
            "clean": is_clean,
        })),
    );

    let clean_label = if is_clean { "" } else { " (dirty)" };
    let text = format!(
        "Repository is on branch: <b>{}{}</b>\n\nHow would you like to proceed?",
        escape_html(&current_branch),
        clean_label
    );

    let mut buttons: Vec<Vec<InlineKeyboardButton>> = vec![
        vec![InlineKeyboardButton::callback(
            "🌿 New branch (from main)".to_string(),
            format!("solve:branch:new:{}", issue_key),
        )],
        vec![InlineKeyboardButton::callback(
            "📌 Stay on current branch",
            format!("solve:branch:curr:{}", issue_key),
        )],
    ];

    if !is_clean {
        buttons.insert(
            0,
            vec![InlineKeyboardButton::callback(
                "📦 Stash changes & new branch",
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

    state.logger.info(
        "solve: branch choice",
        Some(&json!({ "key": issue_key, "choice": choice })),
    );

    if let Some(ref g) = git {
        match choice {
            "stash" => {
                state.logger.info(
                    "solve: stashing changes",
                    Some(&json!({ "key": issue_key })),
                );
                if let Err(e) = g
                    .stash(Some(&format!("devm8: before solving {}", issue_key)))
                    .await
                {
                    state.logger.error(
                        &format!("solve: stash failed: {e}"),
                        Some(&json!({ "key": issue_key })),
                    );
                    bot.send_message(chat_id, format!("Failed to stash: {e}"))
                        .await?;
                    return Ok(());
                }
                let branch_name = format!("devm8/{}", issue_key.to_lowercase().replace('/', "-"));
                state.logger.info(
                    "solve: creating branch",
                    Some(&json!({ "key": issue_key, "branch": &branch_name })),
                );
                if let Err(e) = g
                    .checkout_new_branch_from_main(&branch_name, "origin", "main")
                    .await
                {
                    state.logger.error(
                        &format!("solve: branch creation failed after stash: {e}"),
                        Some(&json!({ "key": issue_key, "branch": &branch_name })),
                    );
                    bot.send_message(
                        chat_id,
                        format!("Stashed, but failed to create branch: {e}"),
                    )
                    .await?;
                    return Ok(());
                }
                state.logger.info(
                    "solve: stashed and created branch",
                    Some(&json!({ "key": issue_key, "branch": &branch_name })),
                );
                bot.send_message(
                    chat_id,
                    format!(
                        "Changes stashed. Created branch <b>{}</b>.",
                        escape_html(&branch_name)
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await?;
            }
            "new" => {
                let branch_name = format!("devm8/{}", issue_key.to_lowercase().replace('/', "-"));
                state.logger.info(
                    "solve: creating new branch",
                    Some(&json!({ "key": issue_key, "branch": &branch_name })),
                );
                if let Err(e) = g
                    .checkout_new_branch_from_main(&branch_name, "origin", "main")
                    .await
                {
                    state.logger.error(
                        &format!("solve: branch creation failed: {e}"),
                        Some(&json!({ "key": issue_key, "branch": &branch_name })),
                    );
                    bot.send_message(chat_id, format!("Failed to create branch: {e}"))
                        .await?;
                    return Ok(());
                }
                state.logger.info(
                    "solve: branch created",
                    Some(&json!({ "key": issue_key, "branch": &branch_name })),
                );
                bot.send_message(
                    chat_id,
                    format!("Created branch <b>{}</b>.", escape_html(&branch_name)),
                )
                .parse_mode(ParseMode::Html)
                .await?;
            }
            _ => {
                state.logger.info(
                    "solve: staying on current branch",
                    Some(&json!({ "key": issue_key })),
                );
            }
        }
    }

    if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
        cs.pending_solve = None;
    }

    solve_by_key(bot, chat_id, state, issue_key, cwd).await
}

pub async fn handle_solve(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let issue_key = args.trim().to_string();
    if issue_key.is_empty() {
        bot.send_message(chat_id, "Send the issue key:\n<code>MYAPP-123</code>")
            .parse_mode(ParseMode::Html)
            .await?;
        return Ok(());
    }

    let project_key = issue_key.split('-').next().unwrap_or("").to_uppercase();

    let has_repos = state.git_map.contains_key(&project_key);

    state.logger.info(
        "solve: command received",
        Some(&json!({ "key": &issue_key, "has_repos": has_repos })),
    );

    if has_repos {
        handle_repo_picker(bot, chat_id, state, &issue_key).await
    } else {
        solve_by_key(bot, chat_id, state, &issue_key, None).await
    }
}

pub async fn handle_solve_repo_callback(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<AppState>,
) -> Result<()> {
    let _ = bot.answer_callback_query(q.id.clone()).await;

    let data = q.data.as_deref().unwrap_or("");
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

    let project_key = issue_key.split('-').next().unwrap_or("").to_uppercase();

    let repos = state.git_map.get(&project_key).cloned().unwrap_or_default();
    let git = repos.get(repo_idx).cloned();

    state.logger.info(
        "solve: repo selected",
        Some(&json!({ "key": issue_key, "repo_idx": repo_idx })),
    );

    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        entry.pending_solve = Some(PendingSolve {
            issue_key: issue_key.to_string(),
            git,
        });
    }

    handle_branch_picker(bot, chat_id, state).await
}
