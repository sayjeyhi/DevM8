mod config;
mod sse;

use std::io::Write;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use devm8::api::protocol::{
    AskEvent, AskRequest, MeResponse, PairRequest, PairResponse, ProjectDto, SolveRequest,
};

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
        #[arg(long)]
        project: Option<String>,
    },

    /// Run the /solve analysis flow for a Jira issue
    Solve { issue_key: String },

    /// Browse persisted chat history
    History {
        #[command(subcommand)]
        action: Option<HistoryAction>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: i64,
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
        eprintln!("devm8-client: {e}");
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
        Cmd::History {
            action,
            project,
            limit,
        } => history(action, project, limit).await,
    }
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

async fn login(server: String, code: String, label: Option<String>) -> Result<()> {
    let label = label.or_else(sysinfo::System::host_name);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{server}/v1/auth/pair"))
        .json(&PairRequest { code, label })
        .send()
        .await
        .context("failed to reach server")?;

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
    let projects: Vec<ProjectDto> = client
        .get(format!("{}/v1/projects", creds.server))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
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

async fn ask(question: Vec<String>, project: Option<String>) -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;
    let question = question.join(" ");

    if !question.is_empty() {
        stream_and_render(
            &client,
            format!("{}/v1/ask", creds.server),
            AskRequest { question, project },
        )
        .await?;
        return Ok(());
    }

    println!("Interactive session — Ctrl-D to exit.");
    let mut pending: Option<PendingChoices> = None;
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
                        &client,
                        format!("{}/v1/action", creds.server),
                        devm8::api::protocol::ActionRequest { action },
                    )
                    .await?;
                    continue;
                }
            }
        }

        pending = stream_and_render(
            &client,
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
// History
// ---------------------------------------------------------------------------

async fn history(action: Option<HistoryAction>, project: Option<String>, limit: i64) -> Result<()> {
    let creds = config::load()?;
    let client = authed_client(&creds)?;

    match action {
        None => {
            let mut url = format!("{}/v1/history?limit={limit}", creds.server);
            if let Some(p) = &project {
                url.push_str(&format!("&project={p}"));
            }
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
