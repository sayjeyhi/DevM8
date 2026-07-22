use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::extract::{Extension, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::Json;
use axum::routing::{get, post};
use axum::{middleware, Router};
use futures_util::Stream;
use tokio::sync::mpsc;

use crate::bot::commands::solve::{
    handle_branch_choice, handle_post_analysis_implement, handle_solve_action_callback,
    handle_solve_repo_callback, solve_by_key,
};
use crate::bot::commands::{
    ask_with_session, handle_ask_session_callback, handle_jira, handle_jira_action,
    handle_jira_input_with_text, handle_my_tickets_callback, handle_pending_comment,
    handle_pr_review_action, start_pr_review,
};
use crate::bot::state::AskSession;
use crate::bot::AppState;
use crate::channel::ChannelSender;

use super::auth::{require_bearer_auth, AuthedUser};
use super::cli_sender::CliSender;
use super::protocol::*;
use super::ApiState;

pub fn build_router(state: ApiState) -> Router {
    let authed = Router::new()
        .route("/v1/auth/revoke", post(revoke))
        .route("/v1/me", get(me))
        .route("/v1/projects", get(projects))
        .route("/v1/ask", post(ask))
        .route("/v1/solve", post(solve))
        .route("/v1/pr-review", post(pr_review))
        .route("/v1/jira/start", post(jira_start))
        .route("/v1/action", post(action))
        .route("/v1/history", get(list_history))
        .route("/v1/history/:session_id", get(get_history))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer_auth,
        ));

    Router::new()
        .route("/v1/auth/pair", post(pair))
        .merge(authed)
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

async fn pair(
    State(state): State<ApiState>,
    Json(req): Json<PairRequest>,
) -> Result<Json<PairResponse>, StatusCode> {
    let (token, email) = state
        .app_state
        .db
        .redeem_pairing_code(&req.code, req.label.as_deref())
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(Json(PairResponse { token, email }))
}

async fn revoke(
    State(state): State<ApiState>,
    Extension(_user): Extension<AuthedUser>,
    headers: axum::http::HeaderMap,
) -> StatusCode {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if let Some(token) = token {
        let _ = state.app_state.db.revoke_token(token).await;
    }
    StatusCode::NO_CONTENT
}

async fn me(Extension(user): Extension<AuthedUser>) -> Json<MeResponse> {
    Json(MeResponse {
        email: user.email,
        is_admin: user.is_admin,
    })
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

async fn projects(
    State(state): State<ApiState>,
    Extension(user): Extension<AuthedUser>,
) -> Json<Vec<ProjectDto>> {
    let jira_project_keys: std::collections::HashSet<String> = state
        .app_state
        .jira_for_email(&user.email)
        .await
        .map(|j| j.project_keys().to_vec())
        .unwrap_or_default()
        .into_iter()
        .collect();

    let dtos = state
        .app_state
        .git_map
        .keys()
        .map(|key| ProjectDto {
            key: key.clone(),
            has_git: true,
            has_jira: jira_project_keys.contains(key),
        })
        .collect();
    Json(dtos)
}

// ---------------------------------------------------------------------------
// Ask (SSE)
// ---------------------------------------------------------------------------

/// Adapts an `UnboundedReceiver` into a `Stream` without pulling in `tokio-stream`.
struct EventReceiverStream(mpsc::UnboundedReceiver<AskEvent>);

impl Stream for EventReceiverStream {
    type Item = AskEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}

fn to_sse_stream(
    rx: mpsc::UnboundedReceiver<AskEvent>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    use futures_util::StreamExt;
    EventReceiverStream(rx).map(|event| {
        Ok(Event::default()
            .json_data(&event)
            .unwrap_or_else(|_| Event::default().data("{\"type\":\"error\"}")))
    })
}

async fn ask(
    State(state): State<ApiState>,
    Extension(user): Extension<AuthedUser>,
    Json(req): Json<AskRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let chat_id = format!("cli:{}", user.email);
    let app_state = Arc::clone(&state.app_state);

    // A Jira flow (setup wizard step, create/move/comment/solve prompt, or a
    // ticket-details "Comment" follow-up) is waiting for free text — route
    // there instead of into the ask session, mirroring the same
    // pending_jira_action/pending_comment priority checks polling.rs applies
    // to incoming Telegram messages.
    let pending_jira = app_state
        .chat_states
        .get(&chat_id)
        .and_then(|s| s.pending_jira_action.clone());
    let pending_comment = if pending_jira.is_none() {
        app_state
            .chat_states
            .get(&chat_id)
            .and_then(|s| s.pending_comment.clone())
    } else {
        None
    };

    if pending_jira.is_some() || pending_comment.is_some() {
        let (tx, rx) = mpsc::unbounded_channel::<AskEvent>();
        let sender: Arc<dyn ChannelSender> = Arc::new(CliSender::new(tx.clone()));
        let email = user.email.clone();
        let text = req.question.clone();

        tokio::spawn(async move {
            let result = if let Some(action) = pending_jira {
                handle_jira_input_with_text(
                    sender,
                    &chat_id,
                    &email,
                    action,
                    |_pk: &str| true,
                    app_state,
                    text,
                )
                .await
            } else {
                let (issue_key,) = pending_comment.unwrap();
                handle_pending_comment(sender, &chat_id, &email, &text, app_state, issue_key).await
            };
            let _ = match result {
                Ok(()) => tx.send(AskEvent::Done),
                Err(e) => tx.send(AskEvent::Error {
                    message: e.to_string(),
                }),
            };
        });

        return Sse::new(to_sse_stream(rx)).keep_alive(KeepAlive::default());
    }

    // Seed a session on first use so `AskSession.user_id` is the caller's email
    // (not left to `ask_with_session`'s empty-string default), optionally
    // scoped to a project's repo — mirrors how Telegram primes `AskSession`
    // before calling `ask_with_session`.
    if app_state.chat_states.get(&chat_id).is_none() {
        let repo = req
            .project
            .as_ref()
            .and_then(|pkey| app_state.git_map.get(pkey).and_then(|v| v.first()));
        let mut session = match repo {
            Some(g) => AskSession::new(
                user.email.clone(),
                Some(g.repo_path.clone()),
                Some(Arc::clone(g)),
            ),
            None => AskSession::new(user.email.clone(), None, None),
        };
        if let Some(project_key) = &req.project {
            session = session.with_project_key(project_key.clone());
        }
        app_state
            .chat_states
            .entry(chat_id.clone())
            .or_default()
            .ask_session = Some(session);
    }

    let (tx, rx) = mpsc::unbounded_channel::<AskEvent>();
    let sender: Arc<dyn ChannelSender> = Arc::new(CliSender::new(tx.clone()));
    let question = req.question.clone();

    tokio::spawn(async move {
        let result = ask_with_session(sender, &chat_id, app_state, question).await;
        let _ = match result {
            Ok(()) => tx.send(AskEvent::Done),
            Err(e) => tx.send(AskEvent::Error {
                message: e.to_string(),
            }),
        };
    });

    Sse::new(to_sse_stream(rx)).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// Solve (SSE)
// ---------------------------------------------------------------------------

async fn solve(
    State(state): State<ApiState>,
    Extension(user): Extension<AuthedUser>,
    Json(req): Json<SolveRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let chat_id = format!("cli:{}", user.email);
    let app_state = Arc::clone(&state.app_state);

    let (tx, rx) = mpsc::unbounded_channel::<AskEvent>();
    let sender: Arc<dyn ChannelSender> = Arc::new(CliSender::new(tx.clone()));
    let email = user.email.clone();
    let issue_key = req.issue_key.clone();

    tokio::spawn(async move {
        let cwd = issue_key
            .split('-')
            .next()
            .map(|k| k.to_uppercase())
            .and_then(|pkey| {
                app_state
                    .git_map
                    .get(&pkey)
                    .and_then(|v| v.first())
                    .cloned()
            })
            .map(|g| g.repo_path.to_string_lossy().to_string());

        let result = solve_by_key(sender, &chat_id, app_state, &email, &issue_key, cwd).await;
        let _ = match result {
            Ok(_) => tx.send(AskEvent::Done),
            Err(e) => tx.send(AskEvent::Error {
                message: e.to_string(),
            }),
        };
    });

    Sse::new(to_sse_stream(rx)).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// PR review (SSE)
// ---------------------------------------------------------------------------

async fn pr_review(
    State(state): State<ApiState>,
    Extension(user): Extension<AuthedUser>,
    Json(req): Json<PrReviewRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let chat_id = format!("cli:{}", user.email);
    let app_state = Arc::clone(&state.app_state);

    let (tx, rx) = mpsc::unbounded_channel::<AskEvent>();
    let sender: Arc<dyn ChannelSender> = Arc::new(CliSender::new(tx.clone()));
    let url = req.url.clone();

    tokio::spawn(async move {
        let result = start_pr_review(sender, &chat_id, app_state, &url).await;
        let _ = match result {
            Ok(()) => tx.send(AskEvent::Done),
            Err(e) => tx.send(AskEvent::Error {
                message: e.to_string(),
            }),
        };
    });

    Sse::new(to_sse_stream(rx)).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// Jira (SSE) — opens the same top-level menu Telegram's /jira command shows
// ---------------------------------------------------------------------------

async fn jira_start(
    State(state): State<ApiState>,
    Extension(user): Extension<AuthedUser>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let chat_id = format!("cli:{}", user.email);
    let app_state = Arc::clone(&state.app_state);

    let (tx, rx) = mpsc::unbounded_channel::<AskEvent>();
    let sender: Arc<dyn ChannelSender> = Arc::new(CliSender::new(tx.clone()));
    let email = user.email.clone();

    tokio::spawn(async move {
        let result = handle_jira(sender, &chat_id, &email, app_state).await;
        let _ = match result {
            Ok(()) => tx.send(AskEvent::Done),
            Err(e) => tx.send(AskEvent::Error {
                message: e.to_string(),
            }),
        };
    });

    Sse::new(to_sse_stream(rx)).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// Action (button-click follow-ups: cancel, repo/branch pickers, "implement", ...)
// ---------------------------------------------------------------------------

/// Routes a button's `data` string exactly as Telegram/Slack/Teams would route
/// a callback query — the CLI has no native buttons, so the client sends the
/// chosen choice's `data` back here instead of as free text. "ask:*", "solve:*",
/// "jira:*", and "tickets:*" prefixes are handled; admin/permissions button
/// flows remain Telegram/Slack/Teams-only.
async fn action(
    State(state): State<ApiState>,
    Extension(user): Extension<AuthedUser>,
    Json(req): Json<ActionRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let chat_id = format!("cli:{}", user.email);
    let app_state = Arc::clone(&state.app_state);

    let (tx, rx) = mpsc::unbounded_channel::<AskEvent>();
    let sender: Arc<dyn ChannelSender> = Arc::new(CliSender::new(tx.clone()));
    let email = user.email.clone();
    let action_data = req.action.clone();

    tokio::spawn(async move {
        let prefix = action_data.split(':').next().unwrap_or("");
        let result = match prefix {
            "ask" => {
                handle_ask_session_callback(
                    sender,
                    &chat_id,
                    &email,
                    &action_data,
                    app_state,
                    |_pk: &str| true,
                )
                .await
            }
            "solve" => route_solve_action(sender, &chat_id, &email, &action_data, app_state).await,
            "prreview" => {
                handle_pr_review_action(sender, &chat_id, &action_data, app_state).await
            }
            "jira" => {
                handle_jira_action(sender, &chat_id, &email, &action_data, None, app_state).await
            }
            "tickets" => {
                handle_my_tickets_callback(sender, &chat_id, &email, &action_data, app_state).await
            }
            other => Err(anyhow::anyhow!(
                "unsupported action prefix for devm8-client: {other}"
            )),
        };
        let _ = match result {
            Ok(()) => tx.send(AskEvent::Done),
            Err(e) => tx.send(AskEvent::Error {
                message: e.to_string(),
            }),
        };
    });

    Sse::new(to_sse_stream(rx)).keep_alive(KeepAlive::default())
}

/// Mirrors `teams_bot::webhook::dispatch_solve_action`'s routing.
async fn route_solve_action(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    user_id: &str,
    action: &str,
    state: Arc<AppState>,
) -> anyhow::Result<()> {
    if action == "solve:cancel" {
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

    if action.starts_with("solve:repo:") {
        return handle_solve_repo_callback(sender, chat_id, user_id, state, action).await;
    }
    if let Some(issue_key) = action.strip_prefix("solve:post:implement:") {
        return handle_post_analysis_implement(sender, chat_id, state, user_id, issue_key).await;
    }
    if action.starts_with("solve:action:") {
        let parts: Vec<&str> = action.splitn(4, ':').collect();
        if parts.len() == 4 {
            return handle_solve_action_callback(
                sender, chat_id, state, user_id, parts[2], parts[3],
            )
            .await;
        }
        return Ok(());
    }
    if action.starts_with("solve:branch:") {
        let parts: Vec<&str> = action.splitn(4, ':').collect();
        if parts.len() == 4 {
            return handle_branch_choice(sender, chat_id, state, user_id, parts[2], parts[3]).await;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

async fn list_history(
    State(state): State<ApiState>,
    Extension(user): Extension<AuthedUser>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<SessionSummaryDto>>, StatusCode> {
    let sessions = state
        .app_state
        .db
        .list_sessions(
            &user.email,
            q.project.as_deref(),
            q.limit.unwrap_or(20),
            q.offset.unwrap_or(0),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(
        sessions
            .into_iter()
            .map(|s| SessionSummaryDto {
                session_id: s.session_id,
                project_key: s.project_key,
                channel: s.channel,
                first_message: s.first_message,
                started_at: s.started_at,
            })
            .collect(),
    ))
}

async fn get_history(
    State(state): State<ApiState>,
    Extension(_user): Extension<AuthedUser>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Result<Json<Vec<ChatTurnDto>>, StatusCode> {
    let rows = state
        .app_state
        .db
        .get_session_history(&session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(
        rows.into_iter()
            .map(|r| ChatTurnDto {
                role: r.role,
                content: r.content,
                created_at: r.created_at,
            })
            .collect(),
    ))
}
