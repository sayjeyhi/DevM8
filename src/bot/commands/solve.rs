use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use teloxide::prelude::*;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};

use crate::bot::state::{AskSession, ChatState, PendingGrill, PendingSolve, PendingSolveAction};
use crate::bot::utils::{escape_html, keep_typing, split_message};
use crate::bot::AppState;
use crate::claude::types::AskOptions;

const GRILL_FIRST_Q_PROMPT: &str = "\
You are a senior software engineer stress-testing a ticket before implementation.

{issue_context}

Your job: surface the single most important gap before writing any code. Look for:
- Vague or overloaded terms that need a precise definition
- Decisions that are hard to reverse (schema changes, API contracts, data migrations)
- Missing edge cases or failure modes not addressed by the ticket
- Unstated constraints or external dependencies

Ask exactly ONE focused question. Output only the question — no preamble, no numbering.";

const GRILL_NEXT_Q_PROMPT: &str = "\
You are a senior software engineer stress-testing a ticket before implementation.

{issue_context}

Q&A so far:
{qa_history}

Based on the answers above, determine whether you have enough information to implement safely.
If yes, respond with exactly: DONE
If not, ask the single next most important clarifying question. Focus on gaps exposed by the previous answers.

Output only the question or DONE.";

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
    user_id: i64,
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

    let Some(jira) = state.jira_for_user(user_id) else {
        bot.edit_message_text(
            chat_id,
            status_msg_id,
            "Please set up your Jira account first. Use /jira → My Jira.",
        )
        .await?;
        return Ok(());
    };
    let issue = match jira.get_issue_by_key(issue_key).await {
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
    if let Some(jira) = state.jira_for_user(user_id) {
        match jira.add_comment(issue_key, &analysis).await {
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
    }

    Ok(())
}

pub async fn show_solve_action_picker(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    issue_key: &str,
    cwd: Option<String>,
    git: Option<Arc<crate::git::GitClient>>,
) -> Result<()> {
    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        entry.pending_solve_action = Some(PendingSolveAction { cwd, git });
    }

    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback(
            format!("🔍 Analyze {}", issue_key),
            format!("solve:action:analyze:{}", issue_key),
        )],
        vec![InlineKeyboardButton::callback(
            "🎯 Grill me".to_string(),
            format!("solve:action:grill:{}", issue_key),
        )],
        vec![InlineKeyboardButton::callback(
            "🚀 Analyze & implement".to_string(),
            format!("solve:action:implement:{}", issue_key),
        )],
    ]);

    bot.send_message(chat_id, "What would you like to do?")
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

const MAX_GRILL_QUESTIONS: usize = 5;

fn build_issue_context(key: &str, summary: &str, status: &str, description: &str) -> String {
    format!(
        "Issue Key: {}\nSummary: {}\nStatus: {}\nDescription:\n{}",
        key, summary, status, description
    )
}

fn build_qa_history(qa: &[(String, String)]) -> String {
    qa.iter()
        .enumerate()
        .map(|(i, (q, a))| format!("Q{}: {}\nA{}: {}", i + 1, q, i + 1, a))
        .collect::<Vec<_>>()
        .join("\n\n")
}

async fn grill_by_key(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
    issue_key: &str,
    cwd: Option<String>,
    git: Option<Arc<crate::git::GitClient>>,
) -> Result<()> {
    state.logger.info(
        "solve: grilling — fetching issue",
        Some(&json!({ "key": issue_key })),
    );

    let status_msg = bot
        .send_message(
            chat_id,
            format!(
                "Analyzing <b>{}</b> before asking questions...",
                escape_html(issue_key)
            ),
        )
        .parse_mode(ParseMode::Html)
        .await?;
    let status_msg_id = status_msg.id;
    let _typing = keep_typing(bot.clone(), chat_id);

    let Some(jira) = state.jira_for_user(user_id) else {
        bot.edit_message_text(
            chat_id,
            status_msg_id,
            "Please set up your Jira account first. Use /jira → My Jira.",
        )
        .await?;
        return Ok(());
    };
    let issue = match jira.get_issue_by_key(issue_key).await {
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

    let issue_context = build_issue_context(
        &issue.key,
        &issue.summary,
        &issue.status,
        &issue.description,
    );

    let prompt = GRILL_FIRST_Q_PROMPT.replace("{issue_context}", &issue_context);
    let opts = AskOptions {
        cwd: cwd.clone(),
        ..AskOptions::default()
    };

    let first_q = match state.claude.ask(&prompt, opts).await {
        Ok(text) => text.trim().to_string(),
        Err(e) => {
            bot.edit_message_text(chat_id, status_msg_id, format!("Claude error: {}", e))
                .await?;
            return Ok(());
        }
    };

    if first_q.is_empty() {
        bot.edit_message_text(
            chat_id,
            status_msg_id,
            "Could not generate question. Try again.",
        )
        .await?;
        return Ok(());
    }

    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        entry.pending_grill = Some(PendingGrill {
            issue_key: issue_key.to_string(),
            issue_context,
            cwd,
            git,
            qa_history: Vec::new(),
            current_question: first_q.clone(),
        });
    }

    bot.edit_message_text(
        chat_id,
        status_msg_id,
        format!(
            "Let me ask a few questions before we start.\n\n<b>Q1:</b> {}",
            escape_html(&first_q)
        ),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    Ok(())
}

pub async fn handle_grill_answer(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    user_id: i64,
) -> Result<()> {
    let chat_id = msg.chat.id;
    let answer = msg.text().unwrap_or("").trim().to_string();
    if answer.is_empty() {
        return Ok(());
    }

    let grill = match state
        .chat_states
        .get(&chat_id.0)
        .and_then(|cs| cs.pending_grill.clone())
    {
        Some(g) => g,
        None => return Ok(()),
    };

    let mut updated = grill.clone();
    updated
        .qa_history
        .push((updated.current_question.clone(), answer));

    let q_count = updated.qa_history.len();
    let done = q_count >= MAX_GRILL_QUESTIONS;

    if !done {
        // Ask Claude for the next question
        let qa_history_str = build_qa_history(&updated.qa_history);
        let prompt = GRILL_NEXT_Q_PROMPT
            .replace("{issue_context}", &updated.issue_context)
            .replace("{qa_history}", &qa_history_str);

        let typing = keep_typing(bot.clone(), chat_id);
        let opts = AskOptions {
            cwd: updated.cwd.clone(),
            ..AskOptions::default()
        };
        let next = match state.claude.ask(&prompt, opts).await {
            Ok(t) => t.trim().to_string(),
            Err(e) => {
                state
                    .logger
                    .error(&format!("grill: Claude error: {e}"), None);
                String::new()
            }
        };
        typing.abort();

        if next.eq_ignore_ascii_case("done") || next.is_empty() {
            if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
                cs.pending_grill = None;
            }
            complete_grill(bot, chat_id, state, user_id, updated).await?;
            return Ok(());
        }

        updated.current_question = next.clone();
        {
            let mut entry = state.chat_states.entry(chat_id.0).or_default();
            entry.pending_grill = Some(updated);
        }

        bot.send_message(
            chat_id,
            format!("<b>Q{}:</b> {}", q_count + 1, escape_html(&next)),
        )
        .parse_mode(ParseMode::Html)
        .await?;
    } else {
        if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
            cs.pending_grill = None;
        }
        complete_grill(bot, chat_id, state, user_id, updated).await?;
    }

    Ok(())
}

async fn complete_grill(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
    grill: PendingGrill,
) -> Result<()> {
    let mut qa_context = String::from("Clarifying Q&A gathered before implementation:\n\n");
    qa_context.push_str(&build_qa_history(&grill.qa_history));

    state.logger.info(
        "solve: grill complete, starting implement session",
        Some(&json!({ "key": &grill.issue_key, "questions": grill.qa_history.len() })),
    );

    solve_by_key(
        bot.clone(),
        chat_id,
        state.clone(),
        user_id,
        &grill.issue_key,
        grill.cwd.clone(),
    )
    .await?;

    let repo_path = grill.git.as_ref().map(|g| g.repo_path.clone());
    {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        entry.ask_session = Some(AskSession::new(repo_path, grill.git).with_context(qa_context));
    }

    bot.send_message(
        chat_id,
        "All questions answered. Implementation session is ready — send your first message to start.",
    )
    .await?;

    Ok(())
}

pub async fn handle_solve_action_callback(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
    action: &str,
    issue_key: &str,
) -> Result<()> {
    let pending = state
        .chat_states
        .get(&chat_id.0)
        .and_then(|cs| cs.pending_solve_action.clone());

    let (cwd, git) = match pending {
        Some(p) => (p.cwd, p.git),
        None => (None, None),
    };

    if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
        cs.pending_solve_action = None;
    }

    match action {
        "analyze" => solve_by_key(bot, chat_id, state, user_id, issue_key, cwd).await,
        "grill" => grill_by_key(bot, chat_id, state, user_id, issue_key, cwd, git).await,
        "implement" => {
            solve_by_key(
                bot.clone(),
                chat_id,
                state.clone(),
                user_id,
                issue_key,
                cwd.clone(),
            )
            .await?;
            let repo_path = git.as_ref().map(|g| g.repo_path.clone());
            {
                let mut entry = state.chat_states.entry(chat_id.0).or_default();
                entry.ask_session = Some(AskSession::new(repo_path, git));
            }
            bot.send_message(
                chat_id,
                "Implementation session started. Send a message to continue.",
            )
            .await?;
            Ok(())
        }
        _ => Ok(()),
    }
}

pub async fn handle_repo_picker(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
    issue_key: &str,
) -> Result<()> {
    let project_key = issue_key.split('-').next().unwrap_or("").to_uppercase();

    let repos = state.git_map.get(&project_key).cloned().unwrap_or_default();

    if repos.is_empty() {
        state.logger.info(
            "solve: no repos configured, solving without git context",
            Some(&json!({ "key": issue_key })),
        );
        return show_solve_action_picker(bot, chat_id, state, issue_key, None, None).await;
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
                awaiting_branch_name: false,
            });
        } else {
            state.chat_states.insert(
                chat_id.0,
                ChatState {
                    pending_solve: Some(PendingSolve {
                        issue_key: issue_key.to_string(),
                        git: Some(Arc::clone(&repos[0])),
                        awaiting_branch_name: false,
                    }),
                    ..Default::default()
                },
            );
        }
        return handle_branch_picker(bot, chat_id, state, user_id).await;
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

pub async fn handle_branch_picker(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    _user_id: i64,
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
                "📤 Commit & push (same branch)",
                format!("solve:branch:commitpush:{}", issue_key),
            )],
        );
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
    _user_id: i64,
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
                let suggested = format!("devm8/{}", issue_key.to_lowercase().replace('/', "-"));
                state.logger.info(
                    "solve: awaiting branch name confirmation",
                    Some(&json!({ "key": issue_key, "suggested": &suggested })),
                );
                {
                    let mut entry = state.chat_states.entry(chat_id.0).or_default();
                    if let Some(ref mut ps) = entry.pending_solve {
                        ps.awaiting_branch_name = true;
                    } else {
                        entry.pending_solve = Some(PendingSolve {
                            issue_key: issue_key.to_string(),
                            git: Some(g.clone()),
                            awaiting_branch_name: true,
                        });
                    }
                }
                bot.send_message(
                    chat_id,
                    format!(
                        "Suggested branch name: <code>{}</code>\n\nSend a name to use it, or type a different one:",
                        escape_html(&suggested)
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await?;
                return Ok(());
            }
            "commitpush" => {
                state.logger.info(
                    "solve: committing and pushing local changes",
                    Some(&json!({ "key": issue_key })),
                );
                let commit_msg =
                    format!("chore: save work in progress before solving {}", issue_key);
                if let Err(e) = g.stage_all().await {
                    bot.send_message(chat_id, format!("Failed to stage changes: {e}"))
                        .await?;
                    return Ok(());
                }
                if let Err(e) = g.commit(&commit_msg).await {
                    bot.send_message(chat_id, format!("Failed to commit: {e}"))
                        .await?;
                    return Ok(());
                }
                if let Err(e) = g.push("origin").await {
                    bot.send_message(chat_id, format!("Failed to push: {e}"))
                        .await?;
                    return Ok(());
                }
                state.logger.info(
                    "solve: committed and pushed",
                    Some(&json!({ "key": issue_key })),
                );
                bot.send_message(
                    chat_id,
                    format!(
                        "Committed and pushed: <code>{}</code>",
                        escape_html(&commit_msg)
                    ),
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

    show_solve_action_picker(bot, chat_id, state, issue_key, cwd, git).await
}

pub async fn handle_solve_branch_name_input(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    _user_id: i64,
) -> Result<()> {
    let chat_id = msg.chat.id;
    let branch_name = msg.text().unwrap_or("").trim().to_string();
    if branch_name.is_empty() {
        return Ok(());
    }

    let pending = state
        .chat_states
        .get(&chat_id.0)
        .and_then(|cs| cs.pending_solve.clone());

    let (issue_key, git) = match pending {
        Some(p) => (p.issue_key, p.git),
        None => return Ok(()),
    };

    let cwd = git
        .as_ref()
        .map(|g| g.repo_path.to_string_lossy().to_string());

    state.logger.info(
        "solve: creating branch from user input",
        Some(&json!({ "key": &issue_key, "branch": &branch_name })),
    );

    if let Some(ref g) = git {
        if let Err(e) = g
            .checkout_new_branch_from_main(&branch_name, "origin", "main")
            .await
        {
            state.logger.error(
                &format!("solve: branch creation failed: {e}"),
                Some(&json!({ "key": &issue_key, "branch": &branch_name })),
            );
            bot.send_message(chat_id, format!("Failed to create branch: {e}"))
                .await?;
            return Ok(());
        }
    }

    bot.send_message(
        chat_id,
        format!("Created branch <b>{}</b>.", escape_html(&branch_name)),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    if let Some(mut cs) = state.chat_states.get_mut(&chat_id.0) {
        cs.pending_solve = None;
    }

    show_solve_action_picker(bot, chat_id, state, &issue_key, cwd, git).await
}

pub async fn handle_solve(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
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
        handle_repo_picker(bot, chat_id, state, user_id, &issue_key).await
    } else {
        show_solve_action_picker(bot, chat_id, state, &issue_key, None, None).await
    }
}

pub async fn handle_solve_repo_callback(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<AppState>,
) -> Result<()> {
    let _ = bot.answer_callback_query(q.id.clone()).await;
    let user_id = q.from.id.0 as i64;

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
            awaiting_branch_name: false,
        });
    }

    handle_branch_picker(bot, chat_id, state, user_id).await
}
