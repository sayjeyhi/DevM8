use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use crate::bot::state::{AskMode, AskSession, HistoryEntry, PendingAsk, Role};
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender};
use crate::claude::types::AskOptions;

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

fn build_prompt(
    sender: &Arc<dyn ChannelSender>,
    question: &str,
    history: &[HistoryEntry],
    context: Option<&str>,
) -> String {
    let system_prefix = sender.system_context_prefix();
    let context_prefix = context
        .map(|c| format!("{}\n\n---\n\n", c))
        .unwrap_or_default();

    if history.is_empty() {
        return format!("{}{}{}", system_prefix, context_prefix, question);
    }
    let turns = history
        .iter()
        .map(|e| {
            let role = if e.role == Role::User {
                "User"
            } else {
                "Assistant"
            };
            format!("{}: {}", role, e.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "{}{}This is a continuing conversation. Previous exchanges:\n\n{}\n\nUser: {}",
        system_prefix, context_prefix, turns, question
    )
}

// ---------------------------------------------------------------------------
// Repo-ready message: branch + status + Pull button
// ---------------------------------------------------------------------------

async fn send_repo_ready_message(
    sender: &Arc<dyn ChannelSender>,
    chat_id: &str,
    project_key: &str,
    repo_name: &str,
    git: &Arc<crate::git::GitClient>,
) -> Result<()> {
    let (branch, clean, behind) =
        tokio::join!(git.current_branch(), git.is_clean(), git.commits_behind(),);
    let branch = branch.unwrap_or_else(|_| "unknown".into());
    let clean = clean.unwrap_or(true);
    let status_icon = if clean {
        "\u{2705} clean"
    } else {
        "\u{26a0}\u{fe0f} dirty"
    };

    let text = format!(
        "\u{1f4c2} <b>{}</b> (<code>{}</code>) selected.\n\nBranch: <code>{}</code>\nStatus: {}\n\nPull latest or type your question:",
        sender.escape(repo_name),
        sender.escape(project_key),
        sender.escape(&branch),
        status_icon,
    );

    let pull_label = format!("\u{2b07}\u{fe0f} Pull latest ({} behind)", behind);
    let mut rows: Vec<Vec<Button>> = vec![
        vec![Button::new(pull_label, "ask:pull_latest")],
        vec![Button::new("\u{1f33f} New branch", "ask:branch")],
    ];
    if !clean {
        rows.push(vec![Button::new(
            "\u{2705} Commit changes",
            "ask:commit",
        )]);
        rows.push(vec![Button::new(
            "\u{1f4e6} Stash changes",
            "ask:stash_only",
        )]);
    }
    rows.push(vec![Button::new("\u{1f4bb} CLI", "ask:cli")]);

    sender.send_with_keyboard(chat_id, &text, rows).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Session keyboard
// ---------------------------------------------------------------------------

async fn session_keyboard(
    pushed: bool,
    git: Option<&Arc<crate::git::GitClient>>,
) -> Vec<Vec<Button>> {
    let (commit_label, push_label, pull_label) = if let Some(g) = git {
        let (changed, ahead, behind) = tokio::join!(
            g.changed_files_count(),
            g.commits_ahead(),
            g.commits_behind()
        );
        (
            format!("\u{2705} Commit ({} changed)", changed),
            format!("\u{1f680} Push ({} ahead)", ahead),
            format!("\u{2b07}\u{fe0f} Pull ({} behind)", behind),
        )
    } else {
        (
            "\u{2705} Commit".to_string(),
            "\u{1f680} Push".to_string(),
            "\u{2b07}\u{fe0f} Pull".to_string(),
        )
    };

    let mut rows: Vec<Vec<Button>> = vec![
        vec![
            Button::new("\u{1f4ac} Follow up", "ask:followup"),
            Button::new("\u{1f4bb} CLI", "ask:cli"),
        ],
        vec![
            Button::new("\u{1f33f} Branch", "ask:branch"),
            Button::new(commit_label, "ask:commit"),
        ],
        vec![
            Button::new(push_label, "ask:push"),
            Button::new(pull_label, "ask:pull"),
        ],
        vec![Button::new("\u{1f51a} End session", "ask:end")],
    ];

    if pushed {
        rows.push(vec![Button::new("\u{1f500} Open PR", "ask:openpr")]);
    }

    rows
}

// ---------------------------------------------------------------------------
// Core: ask with session
// ---------------------------------------------------------------------------

pub async fn ask_with_session(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    question: String,
) -> Result<()> {
    let (history, repo_path_opt, git_opt, context) = {
        let cs = state.chat_states.get(chat_id);
        if let Some(ref cs) = cs {
            let session = cs.ask_session.as_ref();
            let history = session.map(|s| s.history.clone()).unwrap_or_default();
            let repo = session.and_then(|s| s.repo_path.clone());
            let git = session.and_then(|s| s.git.clone());
            let ctx = session.and_then(|s| s.context.clone());
            (history, repo, git, ctx)
        } else {
            (vec![], None, None, None)
        }
    };

    let prompt = build_prompt(&sender, &question, &history, context.as_deref());

    state.logger.info(
        "ask: invoking Claude",
        Some(&json!({
            "history_len": history.len(),
            "cwd": repo_path_opt.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(none)".into()),
        })),
    );

    let status_ref = sender.send(chat_id, "Thinking...").await?;

    let typing = sender.start_typing(chat_id);

    let sender_cb = Arc::clone(&sender);
    let status_ref_cb = status_ref.clone();

    let on_progress: crate::claude::types::ProgressCallback =
        Box::new(move |lines: Vec<String>| {
            let sender = Arc::clone(&sender_cb);
            let sref = status_ref_cb.clone();
            let preview = lines.join("").chars().take(300).collect::<String>();
            Box::pin(async move {
                if !preview.is_empty() {
                    let _ = sender
                        .edit_text(
                            &sref,
                            &format!("<pre>{}</pre>", sender.escape(&preview)),
                        )
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

    let (answer, usage) = match state.ai.ask(&prompt, opts).await {
        Ok(r) => r,
        Err(e) => {
            typing.abort();
            state.logger.error(&format!("ask: Claude error: {e}"), None);
            sender
                .edit_text(&status_ref, &format!("Error: {e}"))
                .await?;
            return Ok(());
        }
    };
    typing.abort();

    state.logger.info(
        "ask: Claude responded",
        Some(&json!({ "response_len": answer.len() })),
    );

    // Update session history
    let pushed = {
        let session_user_id = state
            .chat_states
            .get(chat_id)
            .and_then(|cs| cs.ask_session.as_ref().map(|s| s.user_id.clone()))
            .unwrap_or_default();
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        let session = entry.ask_session.get_or_insert_with(|| {
            let mut s = AskSession::new(
                session_user_id.clone(),
                repo_path_opt.clone(),
                git_opt.clone(),
            );
            s.context = context.clone();
            s
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
    sender.edit_text(&status_ref, "Done.").await?;

    // Send response in chunks
    sender.send_in_chunks(chat_id, &answer).await?;

    // Show "What next?" keyboard with optional usage footer
    let keyboard = session_keyboard(pushed, git_opt.as_ref()).await;
    let next_text = match usage.format_footer() {
        Some(f) => format!(
            "What would you like to do next?\n<i>{}</i>",
            sender.escape(&f)
        ),
        None => "What would you like to do next?".to_string(),
    };
    sender
        .send_with_keyboard(chat_id, &next_text, keyboard)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Main command handler
// ---------------------------------------------------------------------------

/// Returns the git repos accessible to `user_id`, preserving a stable ordering.
/// Each entry is `(project_key, repo_path, git_client)`.
fn accessible_repos(
    state: &AppState,
    is_authorized_for: &impl Fn(&str) -> bool,
) -> Vec<(String, std::path::PathBuf, Arc<crate::git::GitClient>)> {
    let mut repos: Vec<(String, std::path::PathBuf, Arc<crate::git::GitClient>)> = state
        .git_map
        .iter()
        .filter(|(project_key, _)| is_authorized_for(project_key))
        .flat_map(|(project_key, repos)| {
            repos
                .iter()
                .map(|g| (project_key.clone(), g.repo_path.clone(), Arc::clone(g)))
                .collect::<Vec<_>>()
        })
        .collect();
    repos.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    repos
}

pub async fn handle_ask(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    args: String,
    user_id: &str,
    is_authorized_for: impl Fn(&str) -> bool,
) -> Result<()> {
    let question = args.trim().to_string();
    

    let all_repos: Vec<(String, std::path::PathBuf)> =
        accessible_repos(&state, &is_authorized_for)
            .into_iter()
            .map(|(k, p, _)| (k, p))
            .collect();

    if all_repos.is_empty() {
        // No projects configured — ask without git context
        let pending = PendingAsk {
            repo_path: None,
            git: None,
            inline_question: if question.is_empty() {
                None
            } else {
                Some(question.clone())
            },
            mode: None,
        };
        if question.is_empty() {
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_ask = Some(pending);
            }
            sender
                .send(chat_id, "What would you like to ask Claude?")
                .await?;
            return Ok(());
        } else {
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.ask_session = Some(AskSession::new(user_id, None, None));
            }
            return ask_with_session(Arc::clone(&sender), chat_id, state, question).await;
        }
    }

    if all_repos.len() == 1 {
        let (project_key, repo_path) = &all_repos[0];
        let main_git = state
            .git_map
            .values()
            .flat_map(|v| v.iter())
            .find(|g| g.repo_path == *repo_path)
            .cloned();

        let session = if let Some(mg) = main_git {
            state.worktree_session(user_id, mg).await
        } else {
            AskSession::new(user_id, Some(repo_path.clone()), None)
        };
        let session_git = session.git.clone();

        {
            let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
            entry.ask_session = Some(session);
        }

        if question.is_empty() {
            let repo_name = repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo");
            if let Some(ref g) = session_git {
                send_repo_ready_message(&sender, chat_id, project_key, repo_name, g).await?;
            } else {
                sender
                    .send(
                        chat_id,
                        "What would you like to ask Claude about this repository?",
                    )
                    .await?;
            }
            return Ok(());
        }

        return ask_with_session(Arc::clone(&sender), chat_id, state, question).await;
    }

    // Multiple projects — show picker
    let buttons: Vec<Vec<Button>> = all_repos
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
            vec![Button::new(label, format!("ask:repo:{}", i))]
        })
        .collect();

    // Store the question for after repo selection
    let pending = PendingAsk {
        repo_path: None,
        git: None,
        inline_question: if question.is_empty() {
            None
        } else {
            Some(question.clone())
        },
        mode: None,
    };
    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.pending_ask = Some(pending);
    }

    sender
        .send_with_keyboard(chat_id, "Select a project:", buttons)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Handle free-text input for pending ask states
// ---------------------------------------------------------------------------

pub async fn handle_ask_text_input(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    text: String,
    state: Arc<AppState>,
) -> Result<()> {
    let pending = {
        state
            .chat_states
            .get(chat_id)
            .and_then(|cs| cs.pending_ask.clone())
    };

    let pending = match pending {
        Some(p) => p,
        None => return Ok(()),
    };

    

    match pending.mode {
        Some(AskMode::Branch) => {
            let git = pending.git.clone().or_else(|| {
                state
                    .chat_states
                    .get(chat_id)
                    .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()))
            });

            if let Some(git) = git {
                state
                    .logger
                    .info("ask: creating branch", Some(&json!({ "branch": &text })));
                match git
                    .checkout_new_branch_from_main(&text, "origin", "main")
                    .await
                {
                    Ok(()) => {
                        state
                            .logger
                            .info("ask: branch created", Some(&json!({ "branch": &text })));
                        sender
                            .send(
                                chat_id,
                                &format!(
                                    "Created and switched to branch <b>{}</b>",
                                    sender.escape(&text)
                                ),
                            )
                            .await?;
                    }
                    Err(e) => {
                        sender
                            .send(chat_id, &format!("Failed to create branch: {e}"))
                            .await?;
                    }
                }
            } else {
                sender
                    .send(chat_id, "\u{26a0}\u{fe0f} Cannot create branch \u{2014} this session has no linked git repository. Start a new /start session and select a configured project.")
                    .await?;
            }

            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_ask = None;
            }
        }

        Some(AskMode::Commit) => {
            // Commit with provided message
            let git = pending.git.clone().or_else(|| {
                state
                    .chat_states
                    .get(chat_id)
                    .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()))
            });
            let pushed = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().map(|s| s.pushed))
                .unwrap_or(false);

            if let Some(git) = git {
                state
                    .logger
                    .info("ask: committing", Some(&json!({ "message": &text })));
                let _ = git.stage_all().await;
                match git.commit(&text).await {
                    Ok(()) => {
                        state
                            .logger
                            .info("ask: commit complete", Some(&json!({ "message": &text })));
                        sender
                            .send(
                                chat_id,
                                &format!(
                                    "Committed with message: <b>{}</b>",
                                    sender.escape(&text)
                                ),
                            )
                            .await?;
                        let keyboard = session_keyboard(pushed, Some(&git)).await;
                        sender
                            .send_with_keyboard(
                                chat_id,
                                "What would you like to do next?",
                                keyboard,
                            )
                            .await?;
                    }
                    Err(e) => {
                        state
                            .logger
                            .error(&format!("ask: commit failed: {e}"), None);
                        sender
                            .send(
                                chat_id,
                                &format!(
                                    "Commit failed: {e}\n\nSend commit message again to retry:"
                                ),
                            )
                            .await?;
                        // Keep pending_ask so the user can retry
                        return Ok(());
                    }
                }
            } else {
                sender
                    .send(chat_id, "\u{26a0}\u{fe0f} Cannot commit \u{2014} this session has no linked git repository. Start a new /start session and select a configured project.")
                    .await?;
            }

            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_ask = None;
            }
        }

        Some(AskMode::Cli) => {
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_ask = None;
            }

            let cwd = pending
                .repo_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string());

            state.logger.info(
                "ask: running cli command",
                Some(&json!({ "cmd": &text, "cwd": cwd.as_deref().unwrap_or("(default)") })),
            );

            let status_ref = sender
                .send(
                    chat_id,
                    &format!("Running: <code>{}</code>\u{2026}", sender.escape(&text)),
                )
                .await?;

            let mut cmd = state.ai.sandboxed_sh_command(cwd.as_deref(), &text);
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());

            let output =
                tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output()).await;

            sender.delete_message(&status_ref).await;

            let reply = match output {
                Err(_) => "Command timed out after 60 seconds.".to_string(),
                Ok(Err(e)) => format!("Failed to spawn: {e}"),
                Ok(Ok(o)) => {
                    let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                    let exit_code = o.status.code().unwrap_or(-1);

                    let mut parts: Vec<String> = Vec::new();
                    parts.push(format!(
                        "<b>$</b> <code>{}</code>  (exit {})",
                        sender.escape(&text),
                        exit_code
                    ));
                    if !stdout.trim().is_empty() {
                        parts.push(format!(
                            "<pre>{}</pre>",
                            sender.escape(stdout.trim())
                        ));
                    }
                    if !stderr.trim().is_empty() {
                        parts.push(format!(
                            "<b>stderr:</b>\n<pre>{}</pre>",
                            sender.escape(stderr.trim())
                        ));
                    }
                    if stdout.trim().is_empty() && stderr.trim().is_empty() {
                        parts.push("(no output)".to_string());
                    }
                    parts.join("\n")
                }
            };

            sender.send_in_chunks(chat_id, &reply).await?;

            // Show session keyboard so user can continue
            let (pushed, git) = {
                let cs = state.chat_states.get(chat_id);
                let pushed = cs
                    .as_ref()
                    .and_then(|cs| cs.ask_session.as_ref().map(|s| s.pushed))
                    .unwrap_or(false);
                let git = cs.and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));
                (pushed, git)
            };
            sender
                .send_with_keyboard(
                    chat_id,
                    "What would you like to do next?",
                    session_keyboard(pushed, git.as_ref()).await,
                )
                .await?;
        }

        Some(AskMode::Followup) | None => {
            // Treat as a follow-up ask
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_ask = None;

                // Ensure session exists with repo context
                if entry.ask_session.is_none() {
                    entry.ask_session = Some(AskSession::new(
                        user_id,
                        pending.repo_path.clone(),
                        pending.git.clone(),
                    ));
                }
            }

            ask_with_session(Arc::clone(&sender), chat_id, state, text).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Callback handler for ask:* actions
// ---------------------------------------------------------------------------

pub async fn handle_ask_session_callback(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action_data: &str,
    state: Arc<AppState>,
    is_authorized_for: impl Fn(&str) -> bool,
) -> Result<()> {
    

    // Handle repo selection: ask:repo:<index>
    if action_data.starts_with("ask:repo:") {
        let idx: usize = action_data
            .trim_start_matches("ask:repo:")
            .parse()
            .unwrap_or(0);

        let all_repos = accessible_repos(&state, &is_authorized_for);

        let (project_key, repo_path, main_git) = match all_repos.into_iter().nth(idx) {
            Some(item) => (item.0, item.1, item.2),
            None => {
                sender.send(chat_id, "Invalid selection.").await?;
                return Ok(());
            }
        };

        let question = {
            state.chat_states.get(chat_id).and_then(|cs| {
                cs.pending_ask
                    .as_ref()
                    .and_then(|p| p.inline_question.clone())
            })
        };

        let session = state.worktree_session(user_id, main_git).await;
        let session_git = session.git.clone();

        {
            let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
            entry.pending_ask = None;
            entry.ask_session = Some(session);
        }

        if let Some(q_text) = question {
            ask_with_session(Arc::clone(&sender), chat_id, state, q_text).await?;
        } else {
            let repo_name = repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo");
            if let Some(ref g) = session_git {
                send_repo_ready_message(&sender, chat_id, &project_key, repo_name, g).await?;
            } else {
                sender
                    .send(chat_id, "What would you like to ask?")
                    .await?;
            }
        }

        return Ok(());
    }

    let action = action_data.trim_start_matches("ask:");

    match action {
        "pull_latest" => {
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(git) = git {
                match git.pull("origin").await {
                    Ok(_) => {
                        let branch = git.current_branch().await.unwrap_or_default();
                        let clean = git.is_clean().await.unwrap_or(true);
                        let status_icon = if clean {
                            "\u{2705} clean"
                        } else {
                            "\u{26a0}\u{fe0f} dirty"
                        };
                        let text = format!(
                            "Pulled <b>{}</b>.\nStatus: {}\n\nType your question:",
                            sender.escape(&branch),
                            status_icon,
                        );
                        sender.send(chat_id, &text).await?;
                    }
                    Err(e) => {
                        sender
                            .send(
                                chat_id,
                                &format!("Pull failed: {e}\n\nType your question:"),
                            )
                            .await?;
                    }
                }
            } else {
                sender
                    .send(chat_id, "\u{26a0}\u{fe0f} No git repository linked to this session \u{2014} pull is unavailable. Type your question:")
                    .await?;
            }
        }

        "followup" => {
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));
            let repo_path = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));

            let pending = PendingAsk {
                repo_path,
                git,
                inline_question: None,
                mode: Some(AskMode::Followup),
            };
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_ask = Some(pending);
            }
            sender
                .send(chat_id, "Type your follow-up question:")
                .await?;
        }

        "stash_only" => {
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(ref g) = git {
                match g.stash(Some("devm8: ask session stash")).await {
                    Ok(()) => {
                        sender
                            .send(chat_id, "Changes stashed. Type your question:")
                            .await?;
                    }
                    Err(e) => {
                        sender
                            .send(chat_id, &format!("Stash failed: {e}"))
                            .await?;
                    }
                }
            } else {
                sender
                    .send(chat_id, "\u{26a0}\u{fe0f} Cannot stash \u{2014} this session has no linked git repository.")
                    .await?;
            }
        }

        "branch" => {
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(ref g) = git {
                let is_clean = g.is_clean().await.unwrap_or(true);
                if !is_clean {
                    // Ask to stash or keep
                    let keyboard = vec![vec![
                        Button::new(
                            "\u{1f4e6} Stash first",
                            "ask:branch_stash",
                        ),
                        Button::new("\u{1f4cc} Keep changes", "ask:branch_keep"),
                    ]];
                    sender
                        .send_with_keyboard(
                            chat_id,
                            "Working tree is dirty. Stash or keep changes?",
                            keyboard,
                        )
                        .await?;
                    return Ok(());
                }
            }

            let repo_path = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));

            let pending = PendingAsk {
                repo_path,
                git,
                inline_question: None,
                mode: Some(AskMode::Branch),
            };
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_ask = Some(pending);
            }
            sender.send(chat_id, "Enter the new branch name:").await?;
        }

        "branch_stash" => {
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(ref g) = git {
                if let Err(e) = g.stash(Some("devm8: ask session stash")).await {
                    sender
                        .send(chat_id, &format!("Stash failed: {e}"))
                        .await?;
                    return Ok(());
                }
            }

            let repo_path = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));

            let pending = PendingAsk {
                repo_path,
                git,
                inline_question: None,
                mode: Some(AskMode::Branch),
            };
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_ask = Some(pending);
            }
            sender
                .send(chat_id, "Stashed. Enter the new branch name:")
                .await?;
        }

        "branch_keep" => {
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));
            let repo_path = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));

            let pending = PendingAsk {
                repo_path,
                git,
                inline_question: None,
                mode: Some(AskMode::Branch),
            };
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_ask = Some(pending);
            }
            sender.send(chat_id, "Enter the new branch name:").await?;
        }

        "commit" => {
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            // Suggest a commit message via Claude
            if let Some(ref g) = git {
                let diff = g.get_diff_stat().await.unwrap_or_default();
                if diff.is_empty() {
                    sender
                        .send(chat_id, "No staged changes to commit.")
                        .await?;
                    return Ok(());
                }

                let prompt = format!(
                    "Generate a concise conventional commit message for these changes:\n\n{}\n\nOutput only the commit message, nothing else.",
                    diff
                );

                let commit_cwd = g.repo_path.to_string_lossy().to_string();
                let typing_commit = sender.start_typing(chat_id);
                let suggestion = state
                    .ai
                    .ask(
                        &prompt,
                        AskOptions {
                            cwd: Some(commit_cwd),
                            ..AskOptions::default()
                        },
                    )
                    .await
                    .map(|(t, _)| t)
                    .unwrap_or_default();
                typing_commit.abort();

                // Strip markdown code fences Claude sometimes wraps around the message
                let suggestion = suggestion
                    .trim()
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim()
                    .to_string();

                let repo_path = state
                    .chat_states
                    .get(chat_id)
                    .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));

                let pending = PendingAsk {
                    repo_path,
                    git: git.clone(),
                    inline_question: None,
                    mode: Some(AskMode::Commit),
                };
                {
                    let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                    entry.pending_ask = Some(pending);
                }

                let text = if suggestion.is_empty() {
                    "Enter a commit message:".to_string()
                } else {
                    format!(
                        "Suggested commit message:\n<pre>{}</pre>\n\nType a commit message (or send the suggestion above):",
                        sender.escape(&suggestion)
                    )
                };
                sender.send(chat_id, &text).await?;
            } else {
                sender
                    .send(chat_id, "\u{26a0}\u{fe0f} Cannot commit \u{2014} this session has no linked git repository.")
                    .await?;
            }
        }

        "push" => {
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(git) = git {
                state.logger.info("ask: pushing to origin", None);
                match git.push("origin").await {
                    Ok(()) => {
                        // Mark as pushed
                        {
                            let mut entry =
                                state.chat_states.entry(chat_id.to_string()).or_default();
                            if let Some(session) = entry.ask_session.as_mut() {
                                session.pushed = true;
                            }
                        }

                        let branch = git.current_branch().await.unwrap_or_default();
                        let is_main = branch == "main" || branch == "master";

                        state.logger.info(
                            "ask: push complete",
                            Some(&json!({ "branch": &branch })),
                        );
                        let text = format!(
                            "Pushed branch <b>{}</b>.",
                            sender.escape(&branch)
                        );
                        if !is_main {
                            let keyboard =
                                vec![vec![Button::new("\u{1f500} Open PR", "ask:openpr")]];
                            sender
                                .send_with_keyboard(chat_id, &text, keyboard)
                                .await?;
                        } else {
                            sender.send(chat_id, &text).await?;
                        }
                    }
                    Err(e) => {
                        sender
                            .send(chat_id, &format!("Push failed: {e}"))
                            .await?;
                    }
                }
            } else {
                sender
                    .send(chat_id, "\u{26a0}\u{fe0f} Cannot push \u{2014} this session has no linked git repository.")
                    .await?;
            }
        }

        "pull" => {
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(git) = git {
                match git.pull("origin").await {
                    Ok(output) => {
                        let branch = git.current_branch().await.unwrap_or_default();
                        let text = format!(
                            "Pulled <b>{}</b>.\n<pre>{}</pre>",
                            sender.escape(&branch),
                            sender.escape(&output)
                        );
                        sender.send(chat_id, &text).await?;
                    }
                    Err(e) => {
                        sender
                            .send(chat_id, &format!("Pull failed: {e}"))
                            .await?;
                    }
                }
            } else {
                sender
                    .send(chat_id, "\u{26a0}\u{fe0f} Cannot pull \u{2014} this session has no linked git repository.")
                    .await?;
            }
        }

        "cli" => {
            let repo_path = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.repo_path.clone()));
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            let pending = PendingAsk {
                repo_path: repo_path.clone(),
                git,
                inline_question: None,
                mode: Some(AskMode::Cli),
            };
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_ask = Some(pending);
            }

            let cwd_hint = repo_path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|name| {
                    format!(
                        " (in <code>{}</code>)",
                        sender.escape(&name.to_string_lossy())
                    )
                })
                .unwrap_or_default();
            sender
                .send(
                    chat_id,
                    &format!("Enter the command to run{}:", cwd_hint),
                )
                .await?;
        }

        "end" => {
            let cleanup = state.chat_states.get(chat_id).and_then(|cs| {
                cs.ask_session
                    .as_ref()
                    .and_then(|s| s.main_git.as_ref().map(|mg| (Arc::clone(mg), s.user_id.clone())))
            });
            if let Some((main_git, uid)) = cleanup {
                let _ = main_git.remove_worktree(&uid).await;
            }
            state.logger.info("ask: session ended", None);
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.ask_session = None;
                entry.pending_ask = None;
            }
            sender.send(chat_id, "Session ended.").await?;
        }

        "openpr" => {
            let git = state
                .chat_states
                .get(chat_id)
                .and_then(|cs| cs.ask_session.as_ref().and_then(|s| s.git.clone()));

            if let Some(git) = git {
                let pushed = state
                    .chat_states
                    .get(chat_id)
                    .and_then(|cs| cs.ask_session.as_ref().map(|s| s.pushed))
                    .unwrap_or(false);
                match git.create_pr().await {
                    Ok(url) => {
                        sender
                            .send(chat_id, &format!("PR created: {}", url))
                            .await?;
                        let keyboard = session_keyboard(pushed, Some(&git)).await;
                        sender
                            .send_with_keyboard(
                                chat_id,
                                "What would you like to do next?",
                                keyboard,
                            )
                            .await?;
                    }
                    Err(e) => {
                        sender
                            .send(chat_id, &format!("Failed to create PR: {e}"))
                            .await?;
                    }
                }
            } else {
                sender
                    .send(chat_id, "\u{26a0}\u{fe0f} Cannot open PR \u{2014} this session has no linked git repository.")
                    .await?;
            }
        }

        _ => {}
    }

    Ok(())
}
