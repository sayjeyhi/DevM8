use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};

use crate::bot::state::{AskMode, AskSession, ChatState, HistoryEntry, PendingAsk, Role};
use crate::bot::utils::{escape_html, keep_typing, split_message};
use crate::bot::AppState;
use crate::claude::types::AskOptions;

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

fn build_prompt(question: &str, history: &[HistoryEntry]) -> String {
    if history.is_empty() {
        return question.to_string();
    }
    let turns = history
        .iter()
        .map(|e| {
            let role = if e.role == Role::User { "User" } else { "Assistant" };
            format!("{}: {}", role, e.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "This is a continuing conversation. Previous exchanges:\n\n{}\n\nUser: {}",
        turns, question
    )
}

// ---------------------------------------------------------------------------
// Repo-ready message: branch + status + Pull button
// ---------------------------------------------------------------------------

async fn send_repo_ready_message(
    bot: &Bot,
    chat_id: ChatId,
    project_key: &str,
    repo_name: &str,
    git: &Arc<crate::git::GitClient>,
) -> Result<()> {
    let branch = git.current_branch().await.unwrap_or_else(|_| "unknown".into());
    let clean = git.is_clean().await.unwrap_or(true);
    let status_icon = if clean { "✅ clean" } else { "⚠️ dirty" };

    let text = format!(
        "📂 <b>{}</b> (<code>{}</code>) selected.\n\nBranch: <code>{}</code>\nStatus: {}\n\nPull latest or type your question:",
        escape_html(repo_name),
        escape_html(project_key),
        escape_html(&branch),
        status_icon,
    );

    let keyboard = InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback("Pull latest", "ask:pull_latest"),
    ]]);

    bot.send_message(chat_id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Session keyboard
// ---------------------------------------------------------------------------

fn session_keyboard(pushed: bool) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = vec![
        vec![
            InlineKeyboardButton::callback("Follow up", "ask:followup"),
            InlineKeyboardButton::callback("Branch", "ask:branch"),
        ],
        vec![
            InlineKeyboardButton::callback("Commit", "ask:commit"),
            InlineKeyboardButton::callback("Push", "ask:push"),
        ],
        vec![
            InlineKeyboardButton::callback("Pull", "ask:pull"),
            InlineKeyboardButton::callback("End session", "ask:end"),
        ],
    ];

    if pushed {
        rows.push(vec![InlineKeyboardButton::callback(
            "Open PR",
            "ask:openpr",
        )]);
    }

    InlineKeyboardMarkup::new(rows)
}

// ---------------------------------------------------------------------------
// Core: ask with session
// ---------------------------------------------------------------------------

pub async fn ask_with_session(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    question: String,
) -> Result<()> {
    let (history, repo_path_opt, git_opt) = {
        let cs = state.chat_states.get(&chat_id.0);
        if let Some(ref cs) = cs {
            let session = cs.ask_session.as_ref();
            let history = session.map(|s| s.history.clone()).unwrap_or_default();
            let repo = session.and_then(|s| s.repo_path.clone());
            let git = session.and_then(|s| s.git.clone());
            (history, repo, git)
        } else {
            (vec![], None, None)
        }
    };

    let prompt = build_prompt(&question, &history);

    let status_msg = bot
        .send_message(chat_id, "Thinking...")
        .await?;
    let status_msg_id = status_msg.id;

    let _typing = keep_typing(bot.clone(), chat_id);

    let bot_cb = bot.clone();
    let chat_id_cb = chat_id;
    let msg_id_cb = status_msg_id;

    let on_progress: crate::claude::types::ProgressCallback = Box::new(move |lines: Vec<String>| {
        let bot = bot_cb.clone();
        let chat_id = chat_id_cb;
        let msg_id = msg_id_cb;
        let preview = lines.join("").chars().take(300).collect::<String>();
        Box::pin(async move {
            if !preview.is_empty() {
                let _ = bot
                    .edit_message_text(
                        chat_id,
                        msg_id,
                        format!("<pre>{}</pre>", escape_html(&preview)),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
            }
        })
    });

    let cwd = repo_path_opt
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());

    let opts = AskOptions {
        on_progress: Some(on_progress),
        cwd,
        ..AskOptions::default()
    };

    let answer = match state.claude.ask(&prompt, opts).await {
        Ok(a) => a,
        Err(e) => {
            bot.edit_message_text(chat_id, status_msg_id, format!("Error: {e}"))
                .await?;
            return Ok(());
        }
    };

    // Update session history
    let pushed = {
        let mut entry = state.chat_states.entry(chat_id.0).or_default();
        let session = entry.ask_session.get_or_insert_with(|| {
            AskSession::new(repo_path_opt.clone(), git_opt.clone())
        });
        session.history.push(HistoryEntry {
            role: Role::User,
            content: question.clone(),
        });
        session.history.push(HistoryEntry {
            role: Role::Assistant,
            content: answer.clone(),
        });
        session.pushed
    };

    // Edit the status message away
    bot.edit_message_text(chat_id, status_msg_id, "Done.")
        .await?;

    // Send response in chunks
    let chunks = split_message(&answer, 4096);
    for chunk in &chunks {
        bot.send_message(chat_id, chunk).await?;
    }

    // Show "What next?" keyboard
    let keyboard = session_keyboard(pushed);
    bot.send_message(chat_id, "What would you like to do next?")
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Main command handler
// ---------------------------------------------------------------------------

pub async fn handle_ask(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let question = args.trim().to_string();

    // Collect available repos
    let all_repos: Vec<(String, std::path::PathBuf)> = state
        .git_map
        .iter()
        .flat_map(|(project_key, repos)| {
            repos
                .iter()
                .map(|g| (project_key.clone(), g.repo_path.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    if all_repos.is_empty() {
        // No repos — ask without git context
        let pending = PendingAsk {
            repo_path: None,
            git: None,
            inline_question: if question.is_empty() { None } else { Some(question.clone()) },
            mode: None,
        };
        if question.is_empty() {
            // Prompt user to type a question
            {
                let mut entry = state.chat_states.entry(msg.chat.id.0).or_default();
                entry.pending_ask = Some(pending);
            }
            bot.send_message(msg.chat.id, "What would you like to ask Claude?")
                .await?;
            return Ok(());
        } else {
            // Initialize session without repo context
            {
                let mut entry = state.chat_states.entry(msg.chat.id.0).or_default();
                entry.ask_session = Some(AskSession::new(None, None));
            }
            return ask_with_session(bot, msg.chat.id, state, question).await;
        }
    }

    if all_repos.len() == 1 {
        let (project_key, repo_path) = &all_repos[0];
        let git = state
            .git_map
            .values()
            .flat_map(|v| v.iter())
            .find(|g| g.repo_path == *repo_path)
            .cloned();

        {
            let mut entry = state.chat_states.entry(msg.chat.id.0).or_default();
            entry.ask_session = Some(AskSession::new(Some(repo_path.clone()), git.clone()));
        }

        if question.is_empty() {
            let repo_name = repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo");
            if let Some(ref g) = git {
                send_repo_ready_message(&bot, msg.chat.id, project_key, repo_name, g).await?;
            } else {
                bot.send_message(msg.chat.id, "What would you like to ask Claude about this repository?")
                    .await?;
            }
            return Ok(());
        }

        return ask_with_session(bot, msg.chat.id, state, question).await;
    }

    // Multiple repos — show picker
    let buttons: Vec<Vec<InlineKeyboardButton>> = all_repos
        .iter()
        .enumerate()
        .map(|(i, (project_key, repo_path))| {
            let label = format!(
                "{} / {}",
                project_key,
                repo_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("repo")
            );
            vec![InlineKeyboardButton::callback(
                label,
                format!("ask:repo:{}", i),
            )]
        })
        .collect();

    // Store the question for after repo selection
    let pending = PendingAsk {
        repo_path: None,
        git: None,
        inline_question: if question.is_empty() { None } else { Some(question.clone()) },
        mode: None,
    };
    {
        let mut entry = state.chat_states.entry(msg.chat.id.0).or_default();
        entry.pending_ask = Some(pending);
    }

    let keyboard = InlineKeyboardMarkup::new(buttons);
    bot.send_message(msg.chat.id, "Select a repository:")
        .reply_markup(keyboard)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Handle free-text input for pending ask states
// ---------------------------------------------------------------------------

pub async fn handle_ask_text_input(
    bot: Bot,
    msg: Message,
    state: Arc<AppState>,
) -> Result<()> {
    let text = msg.text().unwrap_or("").trim().to_string();

    let pending = {
        state
            .chat_states
            .get(&msg.chat.id.0)
            .and_then(|cs| cs.pending_ask.clone())
    };

    let pending = match pending {
        Some(p) => p,
        None => return Ok(()),
    };

    match pending.mode {
        Some(AskMode::Branch) => {
            // Create a new git branch
            let git = pending.git.clone().or_else(|| {
                state
                    .chat_states
                    .get(&msg.chat.id.0)
                    .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()))
            });

            if let Some(git) = git {
                match git.checkout_new_branch_from_main(&text, "origin", "main").await {
                    Ok(()) => {
                        bot.send_message(
                            msg.chat.id,
                            format!("Created and switched to branch <b>{}</b>", escape_html(&text)),
                        )
                        .parse_mode(ParseMode::Html)
                        .await?;
                    }
                    Err(e) => {
                        bot.send_message(msg.chat.id, format!("Failed to create branch: {e}"))
                            .await?;
                    }
                }
            } else {
                bot.send_message(msg.chat.id, "No git context available.")
                    .await?;
            }

            {
                let mut entry = state.chat_states.entry(msg.chat.id.0).or_default();
                entry.pending_ask = None;
            }
        }

        Some(AskMode::Commit) => {
            // Commit with provided message
            let git = pending.git.clone().or_else(|| {
                state
                    .chat_states
                    .get(&msg.chat.id.0)
                    .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()))
            });

            if let Some(git) = git {
                let _ = git.stage_all().await;
                match git.commit(&text).await {
                    Ok(()) => {
                        bot.send_message(
                            msg.chat.id,
                            format!("Committed with message: <b>{}</b>", escape_html(&text)),
                        )
                        .parse_mode(ParseMode::Html)
                        .await?;
                    }
                    Err(e) => {
                        bot.send_message(msg.chat.id, format!("Commit failed: {e}"))
                            .await?;
                    }
                }
            } else {
                bot.send_message(msg.chat.id, "No git context available.")
                    .await?;
            }

            {
                let mut entry = state.chat_states.entry(msg.chat.id.0).or_default();
                entry.pending_ask = None;
            }
        }

        Some(AskMode::Followup) | None => {
            // Treat as a follow-up ask
            {
                let mut entry = state.chat_states.entry(msg.chat.id.0).or_default();
                entry.pending_ask = None;

                // Ensure session exists with repo context
                if entry.ask_session.is_none() {
                    entry.ask_session =
                        Some(AskSession::new(pending.repo_path.clone(), pending.git.clone()));
                }
            }

            ask_with_session(bot, msg.chat.id, state, text).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Callback handler for ask:* actions
// ---------------------------------------------------------------------------

pub async fn handle_ask_session_callback(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<AppState>,
) -> Result<()> {
    let _ = bot.answer_callback_query(q.id.clone()).await;

    let data = q.data.as_deref().unwrap_or("");
    let chat_id = match q.message.as_ref().map(|m| m.chat().id) {
        Some(id) => id,
        None => return Ok(()),
    };

    // Handle repo selection: ask:repo:<index>
    if data.starts_with("ask:repo:") {
        let idx: usize = data
            .trim_start_matches("ask:repo:")
            .parse()
            .unwrap_or(0);

        let all_repos: Vec<(String, std::path::PathBuf, Option<Arc<crate::git::GitClient>>)> = state
            .git_map
            .iter()
            .flat_map(|(project_key, repos)| {
                repos
                    .iter()
                    .map(|g| (project_key.clone(), g.repo_path.clone(), Some(Arc::clone(g))))
                    .collect::<Vec<_>>()
            })
            .collect();

        let (project_key, repo_path, git) = match all_repos.get(idx) {
            Some(item) => item.clone(),
            None => {
                bot.send_message(chat_id, "Invalid selection.").await?;
                return Ok(());
            }
        };

        let question = {
            state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.pending_ask.as_ref().and_then(|p| p.inline_question.clone()))
        };

        {
            let mut entry = state.chat_states.entry(chat_id.0).or_default();
            entry.pending_ask = None;
            entry.ask_session = Some(AskSession::new(Some(repo_path.clone()), git.clone()));
        }

        if let Some(q_text) = question {
            ask_with_session(bot, chat_id, state, q_text).await?;
        } else {
            let repo_name = repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo");
            if let Some(ref g) = git {
                send_repo_ready_message(&bot, chat_id, &project_key, repo_name, g).await?;
            } else {
                bot.send_message(chat_id, "What would you like to ask?").await?;
            }
        }

        return Ok(());
    }

    let action = data.trim_start_matches("ask:");

    match action {
        "pull_latest" => {
            let git = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(git) = git {
                match git.pull("origin").await {
                    Ok(_) => {
                        let branch = git.current_branch().await.unwrap_or_default();
                        let clean = git.is_clean().await.unwrap_or(true);
                        let status_icon = if clean { "✅ clean" } else { "⚠️ dirty" };
                        let text = format!(
                            "Pulled <b>{}</b>.\nStatus: {}\n\nType your question:",
                            escape_html(&branch),
                            status_icon,
                        );
                        bot.send_message(chat_id, text)
                            .parse_mode(ParseMode::Html)
                            .await?;
                    }
                    Err(e) => {
                        bot.send_message(chat_id, format!("Pull failed: {e}\n\nType your question:"))
                            .await?;
                    }
                }
            } else {
                bot.send_message(chat_id, "No git context. Type your question:").await?;
            }
        }

        "followup" => {
            let git = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));
            let repo_path = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));

            let pending = PendingAsk {
                repo_path,
                git,
                inline_question: None,
                mode: Some(AskMode::Followup),
            };
            {
                let mut entry = state.chat_states.entry(chat_id.0).or_default();
                entry.pending_ask = Some(pending);
            }
            bot.send_message(chat_id, "Type your follow-up question:")
                .await?;
        }

        "branch" => {
            let git = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(ref g) = git {
                let is_clean = g.is_clean().await.unwrap_or(true);
                if !is_clean {
                    // Ask to stash or keep
                    let keyboard = InlineKeyboardMarkup::new(vec![
                        vec![
                            InlineKeyboardButton::callback("Stash first", "ask:branch_stash"),
                            InlineKeyboardButton::callback("Keep changes", "ask:branch_keep"),
                        ],
                    ]);
                    bot.send_message(chat_id, "Working tree is dirty. Stash or keep changes?")
                        .reply_markup(keyboard)
                        .await?;
                    return Ok(());
                }
            }

            let repo_path = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));

            let pending = PendingAsk {
                repo_path,
                git,
                inline_question: None,
                mode: Some(AskMode::Branch),
            };
            {
                let mut entry = state.chat_states.entry(chat_id.0).or_default();
                entry.pending_ask = Some(pending);
            }
            bot.send_message(chat_id, "Enter the new branch name:").await?;
        }

        "branch_stash" => {
            let git = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(ref g) = git {
                if let Err(e) = g.stash(Some("devm8: ask session stash")).await {
                    bot.send_message(chat_id, format!("Stash failed: {e}"))
                        .await?;
                    return Ok(());
                }
            }

            let repo_path = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));

            let pending = PendingAsk {
                repo_path,
                git,
                inline_question: None,
                mode: Some(AskMode::Branch),
            };
            {
                let mut entry = state.chat_states.entry(chat_id.0).or_default();
                entry.pending_ask = Some(pending);
            }
            bot.send_message(chat_id, "Stashed. Enter the new branch name:")
                .await?;
        }

        "branch_keep" => {
            let git = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));
            let repo_path = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));

            let pending = PendingAsk {
                repo_path,
                git,
                inline_question: None,
                mode: Some(AskMode::Branch),
            };
            {
                let mut entry = state.chat_states.entry(chat_id.0).or_default();
                entry.pending_ask = Some(pending);
            }
            bot.send_message(chat_id, "Enter the new branch name:").await?;
        }

        "commit" => {
            let git = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            // Suggest a commit message via Claude
            if let Some(ref g) = git {
                let diff = g.get_diff_stat().await.unwrap_or_default();
                if diff.is_empty() {
                    bot.send_message(chat_id, "No staged changes to commit.").await?;
                    return Ok(());
                }

                let prompt = format!(
                    "Generate a concise conventional commit message for these changes:\n\n{}\n\nOutput only the commit message, nothing else.",
                    diff
                );

                let suggestion = state.claude.ask(&prompt, AskOptions::default()).await.unwrap_or_default();

                let repo_path = state
                    .chat_states
                    .get(&chat_id.0)
                    .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));

                let pending = PendingAsk {
                    repo_path,
                    git: git.clone(),
                    inline_question: None,
                    mode: Some(AskMode::Commit),
                };
                {
                    let mut entry = state.chat_states.entry(chat_id.0).or_default();
                    entry.pending_ask = Some(pending);
                }

                let text = if suggestion.is_empty() {
                    "Enter a commit message:".to_string()
                } else {
                    format!(
                        "Suggested commit message:\n<pre>{}</pre>\n\nType a commit message (or send the suggestion above):",
                        escape_html(&suggestion)
                    )
                };
                bot.send_message(chat_id, text)
                    .parse_mode(ParseMode::Html)
                    .await?;
            } else {
                bot.send_message(chat_id, "No git context available.").await?;
            }
        }

        "push" => {
            let git = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(git) = git {
                match git.push("origin").await {
                    Ok(()) => {
                        // Mark as pushed
                        {
                            let mut entry = state.chat_states.entry(chat_id.0).or_default();
                            if let Some(session) = entry.ask_session.as_mut() {
                                session.pushed = true;
                            }
                        }

                        let branch = git.current_branch().await.unwrap_or_default();
                        let is_main = branch == "main" || branch == "master";

                        let text = format!("Pushed branch <b>{}</b>.", escape_html(&branch));
                        if !is_main {
                            let keyboard = InlineKeyboardMarkup::new(vec![vec![
                                InlineKeyboardButton::callback("Open PR", "ask:openpr"),
                            ]]);
                            bot.send_message(chat_id, text)
                                .parse_mode(ParseMode::Html)
                                .reply_markup(keyboard)
                                .await?;
                        } else {
                            bot.send_message(chat_id, text)
                                .parse_mode(ParseMode::Html)
                                .await?;
                        }
                    }
                    Err(e) => {
                        bot.send_message(chat_id, format!("Push failed: {e}")).await?;
                    }
                }
            } else {
                bot.send_message(chat_id, "No git context available.").await?;
            }
        }

        "pull" => {
            let git = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(git) = git {
                match git.pull("origin").await {
                    Ok(output) => {
                        let branch = git.current_branch().await.unwrap_or_default();
                        let text = format!(
                            "Pulled <b>{}</b>.\n<pre>{}</pre>",
                            escape_html(&branch),
                            escape_html(&output)
                        );
                        bot.send_message(chat_id, text)
                            .parse_mode(ParseMode::Html)
                            .await?;
                    }
                    Err(e) => {
                        bot.send_message(chat_id, format!("Pull failed: {e}")).await?;
                    }
                }
            } else {
                bot.send_message(chat_id, "No git context available.").await?;
            }
        }

        "end" => {
            {
                let mut entry = state.chat_states.entry(chat_id.0).or_default();
                entry.ask_session = None;
                entry.pending_ask = None;
            }
            bot.send_message(chat_id, "Session ended.").await?;
        }

        "openpr" => {
            let git = state
                .chat_states
                .get(&chat_id.0)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(git) = git {
                match git.create_pr().await {
                    Ok(url) => {
                        bot.send_message(
                            chat_id,
                            format!("PR created: {}", url),
                        )
                        .await?;
                    }
                    Err(e) => {
                        bot.send_message(chat_id, format!("Failed to create PR: {e}")).await?;
                    }
                }
            } else {
                bot.send_message(chat_id, "No git context available.").await?;
            }
        }

        _ => {}
    }

    Ok(())
}
