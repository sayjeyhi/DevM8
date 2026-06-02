use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use crate::bot::state::{
    AskSession, ChatState, PendingGrill, PendingPostAnalysis, PendingSolve, PendingSolveAction,
};
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender};
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
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
    issue_key: &str,
    cwd: Option<String>,
) -> Result<()> {
    state.logger.info(
        "solve: fetching issue",
        Some(&json!({ "key": issue_key, "cwd": cwd.as_deref().unwrap_or("(none)") })),
    );

    let status_ref = sender
        .send(
            chat_id,
            &format!(
                "Analyzing <b>{}</b> with Claude...",
                sender.escape(issue_key)
            ),
        )
        .await?;
    let typing = sender.start_typing(chat_id);

    let Some(jira) = state.jira_for_user(user_id) else {
        typing.abort();
        sender
            .edit_text(
                &status_ref,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    let issue = match jira.get_issue_by_key(issue_key).await {
        Ok(i) => i,
        Err(e) => {
            typing.abort();
            state.logger.error(
                &format!("solve: failed to fetch issue: {e}"),
                Some(&json!({ "key": issue_key })),
            );
            sender
                .edit_text(
                    &status_ref,
                    &format!(
                        "Could not fetch <b>{}</b>: {}",
                        sender.escape(issue_key),
                        e
                    ),
                )
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

    let sender_cb = Arc::clone(&sender);
    let status_ref_cb = status_ref.clone();
    let key_progress = issue_key.to_string();

    let on_progress: crate::claude::types::ProgressCallback =
        Box::new(move |lines: Vec<String>| {
            let sender = Arc::clone(&sender_cb);
            let sref = status_ref_cb.clone();
            let key = key_progress.clone();
            let preview = lines.join("").chars().take(200).collect::<String>();
            Box::pin(async move {
                let text = if preview.is_empty() {
                    format!(
                        "Analyzing <b>{}</b> with Claude...",
                        sender.escape(&key)
                    )
                } else {
                    format!(
                        "Analyzing <b>{}</b>...\n\n<pre>{}</pre>",
                        sender.escape(&key),
                        sender.escape(&preview)
                    )
                };
                let _ = sender.edit_text(&sref, &text).await;
            })
        });

    let opts = AskOptions {
        on_progress: Some(on_progress),
        cwd,
        ..AskOptions::default()
    };

    let analysis = match state.claude.ask(&prompt, opts).await {
        Ok((text, _)) => text,
        Err(e) => {
            state.logger.error(
                &format!("solve: Claude error: {e}"),
                Some(&json!({ "key": issue_key })),
            );
            sender
                .edit_text(&status_ref, &format!("Claude error: {}", e))
                .await?;
            return Ok(());
        }
    };

    sender
        .edit_text(
            &status_ref,
            &format!(
                "Analysis complete for <b>{}</b>",
                sender.escape(issue_key)
            ),
        )
        .await?;

    sender.send_in_chunks(chat_id, &analysis).await?;

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
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    issue_key: &str,
    cwd: Option<String>,
    git: Option<Arc<crate::git::GitClient>>,
) -> Result<()> {
    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.pending_solve_action = Some(PendingSolveAction { cwd, git });
    }

    let keyboard = vec![
        vec![Button::new(
            format!("\u{1f50d} Analyze {}", issue_key),
            format!("solve:action:analyze:{}", issue_key),
        )],
        vec![Button::new(
            "\u{1f3af} Grill me".to_string(),
            format!("solve:action:grill:{}", issue_key),
        )],
        vec![Button::new(
            "\u{1f680} Analyze & implement".to_string(),
            format!("solve:action:implement:{}", issue_key),
        )],
    ];

    sender
        .send_with_keyboard(chat_id, "What would you like to do?", keyboard)
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
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
    issue_key: &str,
    cwd: Option<String>,
    git: Option<Arc<crate::git::GitClient>>,
) -> Result<()> {
    state.logger.info(
        "solve: grilling — fetching issue",
        Some(&json!({ "key": issue_key })),
    );

    let status_ref = sender
        .send(
            chat_id,
            &format!(
                "Analyzing <b>{}</b> before asking questions...",
                sender.escape(issue_key)
            ),
        )
        .await?;
    let _typing = sender.start_typing(chat_id);

    let Some(jira) = state.jira_for_user(user_id) else {
        sender
            .edit_text(
                &status_ref,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    let issue = match jira.get_issue_by_key(issue_key).await {
        Ok(i) => i,
        Err(e) => {
            sender
                .edit_text(
                    &status_ref,
                    &format!(
                        "Could not fetch <b>{}</b>: {}",
                        sender.escape(issue_key),
                        e
                    ),
                )
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
        Ok((text, _)) => text.trim().to_string(),
        Err(e) => {
            sender
                .edit_text(&status_ref, &format!("Claude error: {}", e))
                .await?;
            return Ok(());
        }
    };

    if first_q.is_empty() {
        sender
            .edit_text(&status_ref, "Could not generate question. Try again.")
            .await?;
        return Ok(());
    }

    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.pending_grill = Some(PendingGrill {
            issue_key: issue_key.to_string(),
            issue_context,
            cwd,
            git,
            qa_history: Vec::new(),
            current_question: first_q.clone(),
        });
    }

    sender
        .edit_text(
            &status_ref,
            &format!(
                "Let me ask a few questions before we start.\n\n<b>Q1:</b> {}",
                sender.escape(&first_q)
            ),
        )
        .await?;

    Ok(())
}

pub async fn handle_grill_answer(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
    answer: String,
) -> Result<()> {
    if answer.is_empty() {
        return Ok(());
    }

    let grill = match state
        .chat_states
        .get(chat_id)
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

        let _typing = sender.start_typing(chat_id);
        let opts = AskOptions {
            cwd: updated.cwd.clone(),
            ..AskOptions::default()
        };
        let next = match state.claude.ask(&prompt, opts).await {
            Ok((t, _)) => t.trim().to_string(),
            Err(e) => {
                state
                    .logger
                    .error(&format!("grill: Claude error: {e}"), None);
                String::new()
            }
        };

        if next.eq_ignore_ascii_case("done") || next.is_empty() {
            if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
                cs.pending_grill = None;
            }
            complete_grill(Arc::clone(&sender), chat_id, state, user_id, updated).await?;
            return Ok(());
        }

        updated.current_question = next.clone();
        {
            let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
            entry.pending_grill = Some(updated);
        }

        sender
            .send(
                chat_id,
                &format!(
                    "<b>Q{}:</b> {}",
                    q_count + 1,
                    sender.escape(&next)
                ),
            )
            .await?;
    } else {
        if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
            cs.pending_grill = None;
        }
        complete_grill(Arc::clone(&sender), chat_id, state, user_id, updated).await?;
    }

    Ok(())
}

async fn complete_grill(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
    grill: PendingGrill,
) -> Result<()> {
    let mut qa_context = String::from("Clarifying Q&A gathered before implementation:\n\n");
    qa_context.push_str(&build_qa_history(&grill.qa_history));

    state.logger.info(
        "solve: grill complete, running analysis",
        Some(&json!({ "key": &grill.issue_key, "questions": grill.qa_history.len() })),
    );

    solve_by_key(
        Arc::clone(&sender),
        chat_id,
        state.clone(),
        user_id,
        &grill.issue_key,
        grill.cwd.clone(),
    )
    .await?;

    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.pending_post_analysis = Some(PendingPostAnalysis {
            issue_key: grill.issue_key.clone(),
            git: grill.git.clone(),
            qa_context: Some(qa_context),
        });
    }

    let keyboard = vec![vec![Button::new(
        "\u{26a1} Implement".to_string(),
        format!("solve:post:implement:{}", grill.issue_key),
    )]];
    sender
        .send_with_keyboard(
            chat_id,
            "All questions answered. Ready to implement?",
            keyboard,
        )
        .await?;

    Ok(())
}

pub async fn handle_solve_action_callback(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
    action: &str,
    issue_key: &str,
) -> Result<()> {
    let pending = state
        .chat_states
        .get(chat_id)
        .and_then(|cs| cs.pending_solve_action.clone());

    let (cwd, git) = match pending {
        Some(p) => (p.cwd, p.git),
        None => (None, None),
    };

    if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
        cs.pending_solve_action = None;
    }

    match action {
        "analyze" => {
            solve_by_key(
                Arc::clone(&sender),
                chat_id,
                state.clone(),
                user_id,
                issue_key,
                cwd.clone(),
            )
            .await?;
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.pending_post_analysis = Some(PendingPostAnalysis {
                    issue_key: issue_key.to_string(),
                    git,
                    qa_context: None,
                });
            }
            let keyboard = vec![vec![Button::new(
                "\u{26a1} Implement".to_string(),
                format!("solve:post:implement:{}", issue_key),
            )]];
            sender
                .send_with_keyboard(chat_id, "Ready to implement?", keyboard)
                .await?;
            Ok(())
        }
        "grill" => grill_by_key(sender, chat_id, state, user_id, issue_key, cwd, git).await,
        "implement" => {
            solve_by_key(
                Arc::clone(&sender),
                chat_id,
                state.clone(),
                user_id,
                issue_key,
                cwd.clone(),
            )
            .await?;
            let session = if let Some(mg) = git {
                state.worktree_session(user_id, mg).await
            } else {
                let uid_i64 = user_id.parse::<i64>().unwrap_or(0);
                AskSession::new(uid_i64, None, None)
            };
            {
                let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
                entry.ask_session = Some(session);
            }
            sender
                .send(
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
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
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
        return show_solve_action_picker(
            Arc::clone(&sender),
            chat_id,
            state,
            issue_key,
            None,
            None,
        )
        .await;
    }

    if repos.len() == 1 {
        state.logger.info(
            "solve: single repo, proceeding to branch picker",
            Some(&json!({ "key": issue_key, "repo": repos[0].repo_path.display().to_string() })),
        );
        if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
            cs.pending_solve = Some(PendingSolve {
                issue_key: issue_key.to_string(),
                git: Some(Arc::clone(&repos[0])),
                awaiting_branch_name: false,
            });
        } else {
            state.chat_states.insert(
                chat_id.to_string(),
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
        return handle_branch_picker(Arc::clone(&sender), chat_id, state, user_id).await;
    }

    state.logger.info(
        "solve: multiple repos, showing picker",
        Some(&json!({ "key": issue_key, "repo_count": repos.len() })),
    );

    let buttons: Vec<Vec<Button>> = repos
        .iter()
        .enumerate()
        .map(|(i, git)| {
            let label = git
                .repo_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo")
                .to_string();
            vec![Button::new(
                label,
                format!("solve:repo:{}:{}", issue_key, i),
            )]
        })
        .collect();

    sender
        .send_with_keyboard(
            chat_id,
            &format!(
                "Select the repository to use for <b>{}</b>:",
                sender.escape(issue_key)
            ),
            buttons,
        )
        .await?;

    Ok(())
}

pub async fn handle_branch_picker(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    _user_id: &str,
) -> Result<()> {
    let pending = {
        state
            .chat_states
            .get(chat_id)
            .and_then(|cs| cs.pending_solve.clone())
    };

    let (issue_key, git) = match pending {
        Some(p) => (p.issue_key, p.git),
        None => {
            sender
                .send(chat_id, "No pending solve action.")
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
        sender.escape(&current_branch),
        clean_label
    );

    let mut buttons: Vec<Vec<Button>> = vec![
        vec![Button::new(
            "\u{1f33f} New branch (from main)".to_string(),
            format!("solve:branch:new:{}", issue_key),
        )],
        vec![Button::new(
            "\u{1f4cc} Stay on current branch",
            format!("solve:branch:curr:{}", issue_key),
        )],
    ];

    if !is_clean {
        buttons.insert(
            0,
            vec![Button::new(
                "\u{1f4e4} Commit & push (same branch)",
                format!("solve:branch:commitpush:{}", issue_key),
            )],
        );
        buttons.insert(
            0,
            vec![Button::new(
                "\u{1f4e6} Stash changes & new branch",
                format!("solve:branch:stash:{}", issue_key),
            )],
        );
    }

    sender.send_with_keyboard(chat_id, &text, buttons).await?;

    Ok(())
}

pub async fn handle_branch_choice(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    _user_id: &str,
    choice: &str,
    issue_key: &str,
) -> Result<()> {
    let pending = {
        state
            .chat_states
            .get(chat_id)
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
                    sender
                        .send(chat_id, &format!("Failed to stash: {e}"))
                        .await?;
                    return Ok(());
                }
                let branch_name =
                    format!("devm8/{}", issue_key.to_lowercase().replace('/', "-"));
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
                    sender
                        .send(
                            chat_id,
                            &format!("Stashed, but failed to create branch: {e}"),
                        )
                        .await?;
                    return Ok(());
                }
                state.logger.info(
                    "solve: stashed and created branch",
                    Some(&json!({ "key": issue_key, "branch": &branch_name })),
                );
                sender
                    .send(
                        chat_id,
                        &format!(
                            "Changes stashed. Created branch <b>{}</b>.",
                            sender.escape(&branch_name)
                        ),
                    )
                    .await?;
            }
            "new" => {
                let suggested =
                    format!("devm8/{}", issue_key.to_lowercase().replace('/', "-"));
                state.logger.info(
                    "solve: awaiting branch name confirmation",
                    Some(&json!({ "key": issue_key, "suggested": &suggested })),
                );
                {
                    let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
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
                sender
                    .send(
                        chat_id,
                        &format!(
                            "Suggested branch name: <code>{}</code>\n\nSend a name to use it, or type a different one:",
                            sender.escape(&suggested)
                        ),
                    )
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
                    sender
                        .send(chat_id, &format!("Failed to stage changes: {e}"))
                        .await?;
                    return Ok(());
                }
                if let Err(e) = g.commit(&commit_msg).await {
                    sender
                        .send(chat_id, &format!("Failed to commit: {e}"))
                        .await?;
                    return Ok(());
                }
                if let Err(e) = g.push("origin").await {
                    sender
                        .send(chat_id, &format!("Failed to push: {e}"))
                        .await?;
                    return Ok(());
                }
                state.logger.info(
                    "solve: committed and pushed",
                    Some(&json!({ "key": issue_key })),
                );
                sender
                    .send(
                        chat_id,
                        &format!(
                            "Committed and pushed: <code>{}</code>",
                            sender.escape(&commit_msg)
                        ),
                    )
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

    if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
        cs.pending_solve = None;
    }

    show_solve_action_picker(Arc::clone(&sender), chat_id, state, issue_key, cwd, git).await
}

pub async fn handle_solve_branch_name_input(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    _user_id: &str,
    branch_name: String,
) -> Result<()> {
    if branch_name.is_empty() {
        return Ok(());
    }

    let pending = state
        .chat_states
        .get(chat_id)
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
            sender
                .send(chat_id, &format!("Failed to create branch: {e}"))
                .await?;
            return Ok(());
        }
    }

    sender
        .send(
            chat_id,
            &format!(
                "Created branch <b>{}</b>.",
                sender.escape(&branch_name)
            ),
        )
        .await?;

    if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
        cs.pending_solve = None;
    }

    show_solve_action_picker(Arc::clone(&sender), chat_id, state, &issue_key, cwd, git).await
}

pub async fn handle_post_analysis_implement(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
    issue_key: &str,
) -> Result<()> {
    let pending = state
        .chat_states
        .get(chat_id)
        .and_then(|cs| cs.pending_post_analysis.clone());

    let Some(p) = pending else {
        sender
            .send(chat_id, "No pending analysis. Run /solve again.")
            .await?;
        return Ok(());
    };

    if p.issue_key != issue_key {
        sender
            .send(chat_id, "No pending analysis. Run /solve again.")
            .await?;
        return Ok(());
    }

    if let Some(mut cs) = state.chat_states.get_mut(chat_id) {
        cs.pending_post_analysis = None;
    }

    let session = if let Some(mg) = p.git {
        state.worktree_session(user_id, mg).await
    } else {
        let uid_i64 = user_id.parse::<i64>().unwrap_or(0);
        AskSession::new(uid_i64, None, None)
    };
    let session = match p.qa_context {
        Some(ctx) => session.with_context(ctx),
        None => session,
    };

    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.ask_session = Some(session);
    }

    sender
        .send(
            chat_id,
            "Implementation session started. Send a message to begin.",
        )
        .await?;

    Ok(())
}

pub async fn handle_solve(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
    args: String,
) -> Result<()> {
    let issue_key = args.trim().to_string();
    if issue_key.is_empty() {
        sender
            .send(chat_id, "Send the issue key:\n<code>MYAPP-123</code>")
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
        handle_repo_picker(
            Arc::clone(&sender),
            chat_id,
            user_id,
            state,
            &issue_key,
        )
        .await
    } else {
        show_solve_action_picker(
            Arc::clone(&sender),
            chat_id,
            state,
            &issue_key,
            None,
            None,
        )
        .await
    }
}

pub async fn handle_solve_repo_callback(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    state: Arc<AppState>,
    action_data: &str,
) -> Result<()> {
    let parts: Vec<&str> = action_data.splitn(4, ':').collect();
    if parts.len() < 4 {
        return Ok(());
    }
    let issue_key = parts[2];
    let repo_idx: usize = parts[3].parse().unwrap_or(0);

    let project_key = issue_key.split('-').next().unwrap_or("").to_uppercase();

    let repos = state.git_map.get(&project_key).cloned().unwrap_or_default();
    let git = repos.get(repo_idx).cloned();

    state.logger.info(
        "solve: repo selected",
        Some(&json!({ "key": issue_key, "repo_idx": repo_idx })),
    );

    {
        let mut entry = state.chat_states.entry(chat_id.to_string()).or_default();
        entry.pending_solve = Some(PendingSolve {
            issue_key: issue_key.to_string(),
            git,
            awaiting_branch_name: false,
        });
    }

    handle_branch_picker(Arc::clone(&sender), chat_id, state, user_id).await
}
