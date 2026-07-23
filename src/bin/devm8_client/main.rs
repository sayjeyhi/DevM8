mod config;
mod sse;

use std::io::Write;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use devm8::api::protocol::{
    AskEvent, AskRequest, AskStartRequest, MeResponse, PairRequest, PairResponse, PrReviewRequest,
    ProjectDto, SolveRequest,
};
use url::Url;

use config::Credentials;

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
    },

    /// Check for and apply devm8-client binary updates
    Update,
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
        eprintln!("devm8-client: {e:#}");
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
        Cmd::Ask { question } => ask(question).await,
        Cmd::Solve { issue_key } => solve(issue_key).await,
        Cmd::PrReview { url } => pr_review(url).await,
        Cmd::Jira => jira().await,
        Cmd::History { action, limit } => history(action, limit).await,
        Cmd::Update => update().await,
    }
}

async fn update() -> Result<()> {
    devm8::commands::client_update_command().await?;
    Ok(())
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
        server,
        email: pair.email.clone(),
        token: pair.token,
    })?;

    println!("Logged in as {}.", pair.email);
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
    println!("Logged out.");
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
    println!(
        "{} ({}){}",
        me.email,
        creds.server,
        if me.is_admin { " [admin]" } else { "" }
    );
    Ok(())
}

async fn projects() -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let projects = fetch_projects(&client, &creds).await?;
    for p in projects {
        println!(
            "{}{}{}",
            p.key,
            if p.has_git { " [git]" } else { "" },
            if p.has_jira { " [jira]" } else { "" }
        );
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

/// Prompt the user to pick one of their accessible projects, returning its key.
/// Used by `ask` and `history` so every session works within an explicit project.
async fn select_project(client: &reqwest::Client, creds: &Credentials) -> Result<String> {
    let projects = fetch_projects(client, creds).await?;
    if projects.is_empty() {
        bail!("no accessible projects — ask an admin to configure one");
    }

    println!("Select a project:");
    for (i, p) in projects.iter().enumerate() {
        println!(
            "  [{}] {}{}{}",
            i + 1,
            p.key,
            if p.has_git { " [git]" } else { "" },
            if p.has_jira { " [jira]" } else { "" }
        );
    }

    loop {
        print!("> ");
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

/// Rendered choices from the last event that carried a keyboard, so a
/// follow-up numeric selection can be posted to `/v1/action`.
struct PendingChoices(Vec<String>);

fn render_event(event: &AskEvent) -> Option<PendingChoices> {
    match event {
        AskEvent::Text { text, .. } | AskEvent::EditText { text, .. } => {
            println!("{text}");
            None
        }
        AskEvent::Keyboard { text, choices, .. }
        | AskEvent::EditWithKeyboard { text, choices, .. } => {
            println!("{text}");
            Some(print_choices(choices))
        }
        AskEvent::EditKeyboard { choices, .. } => Some(print_choices(choices)),
        AskEvent::Delete { .. } | AskEvent::Done => None,
        AskEvent::Error { message } => {
            eprintln!("Error: {message}");
            None
        }
    }
}

fn print_choices(choices: &devm8::api::protocol::KeyboardDto) -> PendingChoices {
    let flat: Vec<_> = choices.iter().flatten().cloned().collect();
    for (i, c) in flat.iter().enumerate() {
        println!("  [{}] {}", i + 1, c.label);
    }
    PendingChoices(flat.into_iter().map(|c| c.data).collect())
}

async fn stream_and_render(
    client: &reqwest::Client,
    url: String,
    body: impl serde::Serialize,
) -> Result<Option<PendingChoices>> {
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    // Only overwrite on events that actually carry choices — a trailing plain
    // `Text`/`Done` event must not clear the last keyboard we saw.
    let mut last_choices = None;
    sse::for_each_event(resp, |event| {
        if let Some(choices) = render_event(event) {
            last_choices = Some(choices);
        }
    })
    .await?;
    Ok(last_choices)
}

async fn ask(question: Vec<String>) -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let project = select_project(&client, &creds).await?;
    let question = question.join(" ");

    if !question.is_empty() {
        stream_and_render(
            &client,
            format!("{}/v1/ask", creds.server),
            AskRequest {
                question,
                project: Some(project),
            },
        )
        .await?;
        return Ok(());
    }

    println!("Interactive session — Ctrl-D to exit.");
    let pending = stream_and_render(
        &client,
        format!("{}/v1/ask/start", creds.server),
        AskStartRequest {
            project: project.clone(),
        },
    )
    .await?;
    interactive_loop(&client, &creds, Some(project), pending).await
}

/// Reads lines from stdin until EOF, dispatching each one either as a numeric
/// choice (posted to `/v1/action`, when `pending` holds choices from the last
/// response) or as free text (posted to `/v1/ask`). Shared by `ask`'s
/// interactive session and `jira`'s menu — both are driven by the same
/// SSE + numbered-choice protocol.
async fn interactive_loop(
    client: &reqwest::Client,
    creds: &Credentials,
    project: Option<String>,
    mut pending: Option<PendingChoices>,
) -> Result<()> {
    loop {
        print!("> ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        if let Some(PendingChoices(ref data)) = pending {
            if let Ok(idx) = line.parse::<usize>() {
                if idx >= 1 && idx <= data.len() {
                    let action = data[idx - 1].clone();
                    pending = stream_and_render(
                        client,
                        format!("{}/v1/action", creds.server),
                        devm8::api::protocol::ActionRequest { action },
                    )
                    .await?;
                    continue;
                }
            }
        }

        pending = stream_and_render(
            client,
            format!("{}/v1/ask", creds.server),
            AskRequest {
                question: line,
                project: project.clone(),
            },
        )
        .await?;
    }
    Ok(())
}

async fn solve(issue_key: String) -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    stream_and_render(
        &client,
        format!("{}/v1/solve", creds.server),
        SolveRequest { issue_key },
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// PR review — streams Claude's review of a GitHub PR, then lets the user pick
// numbered comments to view in full. Selection only ever prints locally;
// nothing is posted back to GitHub.
// ---------------------------------------------------------------------------

async fn pr_review(url: String) -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let mut pending = stream_and_render(
        &client,
        format!("{}/v1/pr-review", creds.server),
        PrReviewRequest { url },
    )
    .await?;

    while let Some(PendingChoices(ref data)) = pending {
        if data.is_empty() {
            break;
        }

        print!("> ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(idx) = line.parse::<usize>() else {
            println!("Enter a number to view a comment, or Ctrl-D to exit.");
            continue;
        };
        if idx < 1 || idx > data.len() {
            println!(
                "Invalid selection — enter a number between 1 and {}.",
                data.len()
            );
            continue;
        }

        let action = data[idx - 1].clone();
        pending = stream_and_render(
            &client,
            format!("{}/v1/action", creds.server),
            devm8::api::protocol::ActionRequest { action },
        )
        .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Jira (My Tickets, Create, Move, Comment, Solve, account setup) — opens the
// same menu Telegram's /jira command shows, driven by the same numbered-choice
// action loop as `ask`. No upfront project selection: multi-project accounts
// get a picker from the menu itself, exactly as they would in Telegram.
// ---------------------------------------------------------------------------

async fn jira() -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let pending = stream_and_render(
        &client,
        format!("{}/v1/jira/start", creds.server),
        serde_json::json!({}),
    )
    .await?;
    interactive_loop(&client, &creds, None, pending).await
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

async fn history(action: Option<HistoryAction>, limit: i64) -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;

    match action {
        None => {
            let project = select_project(&client, &creds).await?;
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
            for s in sessions {
                println!(
                    "{}  [{}]{}  {}",
                    s.started_at,
                    s.channel,
                    s.project_key.map(|p| format!(" {p}")).unwrap_or_default(),
                    s.session_id
                );
                println!("    {}", truncate(&s.first_message, 100));
            }
        }
        Some(HistoryAction::Show { session_id }) => {
            let turns: Vec<devm8::api::protocol::ChatTurnDto> = client
                .get(format!("{}/v1/history/{session_id}", creds.server))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            for t in turns {
                println!("--- {} ({}) ---", t.role, t.created_at);
                println!("{}", t.content);
                println!();
            }
        }
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max).collect::<String>())
    }
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
}
