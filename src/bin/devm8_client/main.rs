mod config;
mod render;
mod sse;
mod ui;

use std::io::Write;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use devm8::api::protocol::{
    AskRequest, AskStartRequest, MeResponse, PairRequest, PairResponse, PrReviewRequest,
    ProjectDto, SolveRequest,
};
use url::Url;

use config::Credentials;
use render::{PendingChoices, TerminalSink};

#[derive(Parser)]
#[command(
    name = "devm8-client",
    about = "DevM8 terminal client",
    version = env!("DEVM8_VERSION"),
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Pair this machine with a devm8 server using a one-time code
    /// (issued by an admin via `devm8 migrate-users`)
    Login {
        /// Server base URL, e.g. https://myserver.tailnet-name.ts.net:7887
        #[arg(long)]
        server: String,
        #[arg(long)]
        code: String,
        /// Label for this device, stored alongside the issued token (defaults to hostname)
        #[arg(long)]
        label: Option<String>,
    },

    /// Forget stored credentials
    Logout,

    /// Show the currently logged-in identity
    Whoami,

    /// List accessible projects
    Projects,

    /// Ask Claude a question (interactive if no question is given)
    Ask {
        /// The question to ask. If omitted, starts an interactive session.
        question: Vec<String>,
        /// Project key, to skip the project picker
        #[arg(long)]
        project: Option<String>,
    },

    /// Run the /solve analysis flow for a Jira issue
    Solve { issue_key: String },

    /// Review a GitHub pull request with Claude and list its comments
    PrReview {
        /// The pull request URL, e.g. https://github.com/org/repo/pull/123
        url: String,
    },

    /// Open the Jira menu (My Tickets, Create, Move, Comment, Solve, account setup)
    Jira,

    /// Browse persisted chat history
    History {
        #[command(subcommand)]
        action: Option<HistoryAction>,
        #[arg(long, default_value_t = 20)]
        limit: i64,
        /// Project key, to skip the project picker
        #[arg(long)]
        project: Option<String>,
    },

    /// Check for and apply devm8-client binary updates
    Update,

    /// Attach an interactive `opencode` session to your active /ask worktree
    /// on the server, over Tailscale SSH.
    #[command(name = "opencode")]
    OpenCode {
        /// OS account name the server's opencode-login binary is installed as,
        /// as set up by an admin (see the devm8 README's opencode-over-SSH section).
        #[arg(long, default_value = "devm8-opencode")]
        account: String,
    },
}

#[derive(Subcommand)]
enum HistoryAction {
    /// Show the full transcript of one session
    Show { session_id: String },
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        // `{:#}` walks the full error chain (context + underlying reqwest/io
        // error), not just the top-level message — e.g. "failed to reach
        // ...: error sending request ...: connection refused".
        eprintln!("{} devm8-client: {e:#}", ui::err_red(ui::CROSS));
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Login {
            server,
            code,
            label,
        } => login(server, code, label).await,
        Cmd::Logout => logout().await,
        Cmd::Whoami => whoami().await,
        Cmd::Projects => projects().await,
        Cmd::Ask { question, project } => ask(question, project).await,
        Cmd::Solve { issue_key } => solve(issue_key).await,
        Cmd::PrReview { url } => pr_review(url).await,
        Cmd::Jira => jira().await,
        Cmd::History {
            action,
            limit,
            project,
        } => history(action, limit, project).await,
        Cmd::Update => update().await,
        Cmd::OpenCode { account } => opencode(account).await,
    }
}

async fn update() -> Result<()> {
    devm8::commands::client_update_command().await?;
    Ok(())
}

/// Shells out to the local `ssh` binary against the shared opencode account —
/// no local opencode install needed, since the whole session runs server-side
/// (see `opencode-login`, the account's login shell).
async fn opencode(account: String) -> Result<()> {
    let creds = config::load()?;
    let host = Url::parse(&creds.server)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .context("could not determine a host from the stored --server URL")?;

    let target = format!("{account}@{host}");
    println!(
        "{}",
        ui::dim(&format!("Connecting to {target} (Tailscale SSH)…"))
    );

    let status = std::process::Command::new("ssh")
        .arg("-t")
        .arg(&target)
        .status()
        .context("failed to launch local `ssh` — is it installed and on PATH?")?;

    std::process::exit(status.code().unwrap_or(1));
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

async fn login(server: String, code: String, label: Option<String>) -> Result<()> {
    let server = validate_server_url(&server)?;
    let label = label.or_else(sysinfo::System::host_name);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{server}/v1/auth/pair"))
        .json(&PairRequest { code, label })
        .send()
        .await
        .with_context(|| {
            format!(
                "failed to reach {server} — is the devm8 API server running \
                 and reachable from this machine (e.g. over your Tailscale tailnet)?"
            )
        })?;

    if !resp.status().is_success() {
        bail!(
            "pairing failed ({}): {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }

    let pair: PairResponse = resp.json().await.context("unexpected server response")?;
    config::save(&Credentials {
        server: server.clone(),
        email: pair.email.clone(),
        token: pair.token,
    })?;

    println!(
        "{} Logged in as {}.",
        ui::green(ui::CHECK),
        ui::bold(&pair.email)
    );
    println!("{}", ui::dim(&format!("server: {server}")));
    Ok(())
}

/// Validate and normalize a `--server` URL, catching the common mistakes
/// (missing scheme, missing port) with a specific message before ever
/// touching the network, rather than surfacing a bare connection error.
fn validate_server_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        bail!("--server must not be empty");
    }

    let url = Url::parse(trimmed).with_context(|| {
        format!(
            "'{trimmed}' is not a valid URL — expected something like \
             https://myserver.tailnet-name.ts.net:7887"
        )
    })?;

    if url.scheme() != "http" && url.scheme() != "https" {
        bail!(
            "server URL must start with http:// or https:// — got '{trimmed}' \
             (parsed scheme: '{}')",
            url.scheme()
        );
    }

    match url.host_str() {
        Some(h) if !h.is_empty() => {}
        _ => bail!("server URL is missing a host — got '{trimmed}'"),
    }

    if !has_explicit_port(trimmed) {
        bail!(
            "server URL must include an explicit port — got '{trimmed}'.\n\
             The devm8 API server has no default port (commonly 7887 — check \
             the [api] port in the server's config.toml), e.g.:\n\
             \n  https://myserver.tailnet-name.ts.net:7887"
        );
    }

    Ok(trimmed.to_string())
}

/// Whether the URL's authority section has an explicit `:port` suffix.
/// Checked against the raw string rather than the parsed `Url`, which
/// silently drops a port that matches the scheme's default (e.g. an
/// explicit `:443` on `https://` normalizes away and `Url::port()` would
/// return `None` for it too).
fn has_explicit_port(raw: &str) -> bool {
    let after_scheme = raw.split_once("://").map(|(_, rest)| rest).unwrap_or(raw);
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, hp)| hp)
        .unwrap_or(authority);
    match host_port.rsplit_once(':') {
        Some((_, port)) => !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

async fn logout() -> Result<()> {
    if let Ok(creds) = config::load() {
        let client = reqwest::Client::new();
        let _ = client
            .post(format!("{}/v1/auth/revoke", creds.server))
            .bearer_auth(&creds.token)
            .send()
            .await;
    }
    config::clear()?;
    println!("{} Logged out.", ui::green(ui::CHECK));
    Ok(())
}

async fn whoami() -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let me: MeResponse = client
        .get(format!("{}/v1/me", creds.server))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let admin = if me.is_admin {
        format!(" {}", ui::magenta("[admin]"))
    } else {
        String::new()
    };
    println!(
        "{} {}{}  {}",
        ui::green(ui::CHECK),
        ui::bold(&me.email),
        admin,
        ui::dim(&creds.server)
    );
    Ok(())
}

async fn projects() -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let projects = fetch_projects(&client, &creds).await?;
    if projects.is_empty() {
        println!(
            "{}",
            ui::dim("No accessible projects — ask an admin to configure one.")
        );
        return Ok(());
    }

    let width = projects
        .iter()
        .map(|p| p.key.chars().count())
        .max()
        .unwrap_or(0);
    for p in projects {
        let key = format!("{:<width$}", p.key);
        let mut badges = Vec::new();
        if p.has_git {
            badges.push(ui::cyan("git"));
        }
        if p.has_jira {
            badges.push(ui::magenta("jira"));
        }
        println!("{}  {}", ui::bold(&key), badges.join(&ui::dim(" · ")));
    }
    Ok(())
}

async fn fetch_projects(client: &reqwest::Client, creds: &Credentials) -> Result<Vec<ProjectDto>> {
    let projects: Vec<ProjectDto> = client
        .get(format!("{}/v1/projects", creds.server))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(projects)
}

/// One-line description of a project for pickers and lists, e.g.
/// `myapp  · git · jira`.
fn project_label(p: &ProjectDto) -> String {
    let mut s = p.key.clone();
    if p.has_git {
        s.push_str("  · git");
    }
    if p.has_jira {
        s.push_str("  · jira");
    }
    s
}

/// Prompt the user to pick one of their accessible projects, returning its
/// key. Used by `ask` and `history` so every session works within an explicit
/// project. `override_key` (from `--project`) skips the picker entirely.
async fn select_project(
    client: &reqwest::Client,
    creds: &Credentials,
    override_key: Option<String>,
) -> Result<String> {
    if let Some(key) = override_key {
        return Ok(key);
    }

    let projects = fetch_projects(client, creds).await?;
    if projects.is_empty() {
        bail!("no accessible projects — ask an admin to configure one");
    }
    if projects.len() == 1 {
        return Ok(projects[0].key.clone());
    }

    if ui::interactive() {
        let labels: Vec<String> = projects.iter().map(project_label).collect();
        match inquire::Select::new("Select a project", labels)
            .with_help_message("↑/↓ to move · Enter to select")
            .raw_prompt()
        {
            Ok(picked) => return Ok(projects[picked.index].key.clone()),
            Err(inquire::InquireError::OperationInterrupted) => bail!("no project selected"),
            Err(_) => {} // terminal can't run the select — numbered prompt below
        }
    }

    println!("Select a project:");
    for (i, p) in projects.iter().enumerate() {
        println!("  [{}] {}", i + 1, project_label(p));
    }

    loop {
        print!("{} ", ui::bold(">"));
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            bail!("no project selected");
        }
        if let Ok(idx) = line.trim().parse::<usize>() {
            if idx >= 1 && idx <= projects.len() {
                return Ok(projects[idx - 1].key.clone());
            }
        }
        println!(
            "Invalid selection — enter a number between 1 and {}.",
            projects.len()
        );
    }
}

fn authed_client(creds: &Credentials) -> Result<reqwest::Client> {
    use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", creds.token))?,
    );
    Ok(reqwest::Client::builder()
        .default_headers(headers)
        .build()?)
}

// ---------------------------------------------------------------------------
// Ask (streams, then optionally loops interactively)
// ---------------------------------------------------------------------------

/// Streams one request/response exchange through the event renderer.
///
/// `list_choices` controls whether a final choice keyboard is printed as a
/// numbered list: one-shot runs want it (there is no follow-up prompt), the
/// interactive REPL does not (an arrow-key select replaces the list).
async fn stream_and_render(
    client: &reqwest::Client,
    url: String,
    body: impl serde::Serialize,
    sink: &mut TerminalSink,
    list_choices: bool,
) -> Result<Option<PendingChoices>> {
    sink.set_list_choices(list_choices);
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    let mut renderer = render::EventRenderer::new();
    // Only overwrite on events that actually carry choices — a trailing plain
    // `Text`/`Done` event must not clear the last keyboard we saw.
    let mut last_choices = None;
    let interrupted = {
        let mut consume = std::pin::pin!(sse::for_each_event(resp, |event| {
            if let Some(choices) = renderer.render(sink, event) {
                last_choices = Some(choices);
            }
        }));
        tokio::select! {
            result = &mut consume => { result?; false }
            _ = tokio::signal::ctrl_c() => true,
        }
        // `consume` (and its borrow of renderer/sink) ends here.
    };
    if interrupted {
        renderer.finish(sink); // clear the spinner line before exiting
        println!();
        std::process::exit(130);
    }
    renderer.finish(sink);
    Ok(last_choices)
}

async fn ask(question: Vec<String>, project_override: Option<String>) -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let mut sink = TerminalSink::new(!ui::interactive());
    let project = select_project(&client, &creds, project_override).await?;
    let question = question.join(" ");

    if !question.is_empty() {
        println!("{}", ui::dim(&format!("▸ {question}")));
        stream_and_render(
            &client,
            format!("{}/v1/ask", creds.server),
            AskRequest {
                question,
                project: Some(project),
            },
            &mut sink,
            true,
        )
        .await?;
        return Ok(());
    }

    println!(
        "{}",
        ui::dim("Interactive session — Ctrl-C or Ctrl-D to exit · Esc skips the choice menu")
    );
    let pending = stream_and_render(
        &client,
        format!("{}/v1/ask/start", creds.server),
        AskStartRequest {
            project: project.clone(),
        },
        &mut sink,
        false,
    )
    .await?;
    interactive_loop(&client, &creds, Some(project), pending, &mut sink).await
}

/// What the user chose at the REPL prompt.
enum UserInput {
    Choice(usize),
    FreeText(String),
    Exit,
}

const ASK_INSTEAD_LABEL: &str = "✎  Ask something else…";

/// Prompts for the next REPL action: pick one of the pending choices (arrow
/// keys when interactive) or type free text. Falls back to the plain
/// `> ` + numbered-choice loop when the terminal can't run full-screen
/// prompts (piped stdin/out).
fn prompt_input(pending: Option<&PendingChoices>) -> UserInput {
    if ui::interactive() {
        if let Some(input) = prompt_interactive(pending) {
            return input;
        }
    }
    prompt_raw(pending)
}

fn prompt_interactive(pending: Option<&PendingChoices>) -> Option<UserInput> {
    use inquire::InquireError;

    if let Some(pc) = pending.filter(|p| !p.is_empty()) {
        let mut labels: Vec<String> = pc.items.iter().map(|c| c.label.clone()).collect();
        labels.push(ASK_INSTEAD_LABEL.to_string());
        match inquire::Select::new("Choose an action", labels)
            .with_help_message("↑/↓ to move · Enter to select · Esc to type instead")
            .raw_prompt()
        {
            Ok(picked) if picked.index < pc.items.len() => {
                return Some(UserInput::Choice(picked.index))
            }
            Ok(_) => return prompt_text_interactive(), // "Ask something else…"
            Err(InquireError::OperationInterrupted) => return Some(UserInput::Exit),
            Err(InquireError::NotTTY) => return None,
            Err(_) => return Some(UserInput::Exit),
        }
    }
    prompt_text_interactive()
}

fn prompt_text_interactive() -> Option<UserInput> {
    use inquire::InquireError;

    match inquire::Text::new("Message").prompt() {
        Ok(text) => Some(UserInput::FreeText(text)),
        Err(InquireError::OperationInterrupted) => Some(UserInput::Exit),
        // Esc on the text prompt: return to the previous prompt rather than
        // discarding the session — an empty line just re-prompts.
        Err(InquireError::OperationCanceled) => Some(UserInput::FreeText(String::new())),
        Err(InquireError::NotTTY) => None,
        Err(_) => Some(UserInput::Exit), // EOF (Ctrl-D) and friends
    }
}

fn prompt_raw(pending: Option<&PendingChoices>) -> UserInput {
    let choices = pending.filter(|p| !p.is_empty());
    loop {
        print!("{} ", ui::bold(">"));
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
            return UserInput::Exit;
        }
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if let Some(pc) = choices {
            if let Ok(idx) = line.parse::<usize>() {
                if idx >= 1 && idx <= pc.items.len() {
                    return UserInput::Choice(idx - 1);
                }
            }
        }
        return UserInput::FreeText(line);
    }
}

/// Reads the next user action, dispatching each one either as a choice
/// (posted to `/v1/action`, when `pending` holds choices from the last
/// response) or as free text (posted to `/v1/ask`). Shared by `ask`'s
/// interactive session and `jira`'s menu — both are driven by the same
/// SSE + choice protocol.
async fn interactive_loop(
    client: &reqwest::Client,
    creds: &Credentials,
    project: Option<String>,
    mut pending: Option<PendingChoices>,
    sink: &mut TerminalSink,
) -> Result<()> {
    loop {
        let next = match prompt_input(pending.as_ref()) {
            UserInput::Exit => break,
            UserInput::FreeText(q) if q.trim().is_empty() => continue,
            UserInput::FreeText(q) => {
                stream_and_render(
                    client,
                    format!("{}/v1/ask", creds.server),
                    AskRequest {
                        question: q,
                        project: project.clone(),
                    },
                    sink,
                    false,
                )
                .await?
            }
            UserInput::Choice(i) => {
                let Some(action) = pending
                    .as_ref()
                    .and_then(|p| p.items.get(i))
                    .map(|c| c.data.clone())
                else {
                    continue;
                };
                stream_and_render(
                    client,
                    format!("{}/v1/action", creds.server),
                    devm8::api::protocol::ActionRequest { action },
                    sink,
                    false,
                )
                .await?
            }
        };
        pending = next;
        println!();
        println!("{}", ui::separator());
    }
    println!("{}", ui::dim("Bye!"));
    Ok(())
}

async fn solve(issue_key: String) -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let mut sink = TerminalSink::new(!ui::interactive());
    stream_and_render(
        &client,
        format!("{}/v1/solve", creds.server),
        SolveRequest { issue_key },
        &mut sink,
        true,
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// PR review — streams Claude's review of a GitHub PR with live progress,
// prints every comment's full detail, and saves the report server-side under
// ~/.devm8/pr-reviews/. Nothing is posted back to GitHub.
// ---------------------------------------------------------------------------

async fn pr_review(url: String) -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let mut sink = TerminalSink::new(!ui::interactive());
    stream_and_render(
        &client,
        format!("{}/v1/pr-review", creds.server),
        PrReviewRequest { url },
        &mut sink,
        true,
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Jira (My Tickets, Create, Move, Comment, Solve, account setup) — opens the
// same menu Telegram's /jira command shows, driven by the same choice-action
// loop as `ask`. No upfront project selection: multi-project accounts get a
// picker from the menu itself, exactly as they would in Telegram.
// ---------------------------------------------------------------------------

async fn jira() -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let mut sink = TerminalSink::new(!ui::interactive());
    let pending = stream_and_render(
        &client,
        format!("{}/v1/jira/start", creds.server),
        serde_json::json!({}),
        &mut sink,
        false,
    )
    .await?;
    interactive_loop(&client, &creds, None, pending, &mut sink).await
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

/// `2026-10-01T14:32:05Z` → local `2026-10-01 14:32`; unparseable values pass
/// through untouched.
fn short_time(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| iso.to_string())
}

async fn history(
    action: Option<HistoryAction>,
    limit: i64,
    project_override: Option<String>,
) -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;

    match action {
        None => {
            let project = select_project(&client, &creds, project_override).await?;
            let url = format!(
                "{}/v1/history?limit={limit}&project={project}",
                creds.server
            );
            let sessions: Vec<devm8::api::protocol::SessionSummaryDto> = client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            if sessions.is_empty() {
                println!("{}", ui::dim("No past sessions for this project yet."));
                return Ok(());
            }

            for s in &sessions {
                let project_key = s
                    .project_key
                    .as_deref()
                    .map(|p| format!(" {}", ui::bold(p)))
                    .unwrap_or_default();
                println!(
                    "{} {} {}  {}",
                    ui::dim(&short_time(&s.started_at)),
                    ui::cyan(&format!("[{}]", s.channel)),
                    project_key,
                    ui::gray(&s.session_id)
                );
                println!("  {}", ui::gray(&ui::truncate(&s.first_message, 80)));
            }

            if ui::interactive() {
                let labels: Vec<String> = sessions
                    .iter()
                    .map(|s| {
                        format!(
                            "{}  {}",
                            short_time(&s.started_at),
                            ui::truncate(&s.first_message, 50)
                        )
                    })
                    .collect();
                if let Ok(picked) =
                    inquire::Select::new("Open a transcript (Esc to quit)", labels).raw_prompt()
                {
                    show_transcript(&client, &creds, &sessions[picked.index].session_id).await?;
                }
            }
        }
        Some(HistoryAction::Show { session_id }) => {
            show_transcript(&client, &creds, &session_id).await?
        }
    }
    Ok(())
}

async fn show_transcript(
    client: &reqwest::Client,
    creds: &Credentials,
    session_id: &str,
) -> Result<()> {
    let turns: Vec<devm8::api::protocol::ChatTurnDto> = client
        .get(format!("{}/v1/history/{session_id}", creds.server))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if turns.is_empty() {
        println!("{}", ui::dim("(empty session)"));
        return Ok(());
    }
    for t in &turns {
        let who = match t.role.as_str() {
            "user" => ui::yellow("You"),
            "assistant" => ui::cyan("Claude"),
            other => ui::gray(other),
        };
        println!("{} {}", who, ui::gray(&short_time(&t.created_at)));
        ui::print_markdown(&ui::strip_html(&t.content));
        println!();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_url_with_port() {
        let result = validate_server_url("https://myserver.tailnet-name.ts.net:7887").unwrap();
        assert_eq!(result, "https://myserver.tailnet-name.ts.net:7887");
    }

    #[test]
    fn strips_trailing_slash() {
        let result = validate_server_url("https://myserver.tailnet-name.ts.net:7887/").unwrap();
        assert_eq!(result, "https://myserver.tailnet-name.ts.net:7887");
    }

    #[test]
    fn accepts_ipv6_with_port() {
        let result = validate_server_url("http://[::1]:7887").unwrap();
        assert_eq!(result, "http://[::1]:7887");
    }

    #[test]
    fn rejects_missing_scheme() {
        let err = validate_server_url("myserver.tailnet-name.ts.net:7887").unwrap_err();
        assert!(err.to_string().contains("not a valid URL") || err.to_string().contains("http"));
    }

    #[test]
    fn rejects_non_http_scheme() {
        let err = validate_server_url("ftp://myserver:7887").unwrap_err();
        assert!(err.to_string().contains("http:// or https://"));
    }

    #[test]
    fn rejects_missing_port() {
        let err = validate_server_url("https://myserver.tailnet-name.ts.net").unwrap_err();
        assert!(err.to_string().contains("explicit port"));
    }

    #[test]
    fn rejects_ipv6_without_port() {
        let err = validate_server_url("http://[::1]").unwrap_err();
        assert!(err.to_string().contains("explicit port"));
    }

    #[test]
    fn rejects_empty() {
        assert!(validate_server_url("").is_err());
        assert!(validate_server_url("   ").is_err());
    }

    #[test]
    fn project_label_shows_badges() {
        let p = |git, jira| ProjectDto {
            key: "myapp".into(),
            has_git: git,
            has_jira: jira,
        };
        assert_eq!(project_label(&p(false, false)), "myapp");
        assert_eq!(project_label(&p(true, false)), "myapp  · git");
        assert_eq!(project_label(&p(true, true)), "myapp  · git  · jira");
    }
}
