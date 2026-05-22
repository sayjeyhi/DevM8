mod bot;
mod claude;
mod commands;
mod config;
mod daemon;
mod git;
mod jira;
mod logger;
mod shared;
mod slack;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "devm8", about = "DevM8 — Jira + Claude + Telegram assistant")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the daemon process (internal — invoked by launchd)
    Daemon,

    /// Start the daemon via launchd (macOS)
    Start,

    /// Stop the daemon
    Stop,

    /// Show daemon status
    Status,

    /// Show or follow daemon logs
    Logs {
        /// Number of lines to show
        #[arg(short = 'n', long, default_value_t = 100)]
        tail: u32,

        /// Follow log output (like tail -f)
        #[arg(short = 'f', long, default_value_t = false)]
        follow: bool,
    },

    /// Run the configuration wizard
    Config,

    /// Check for and apply binary updates
    Update,

    /// Configure Slack integration
    Slackmap,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result: Result<(), Box<dyn std::error::Error>> = async {
        match cli.command {
            Cmd::Daemon      => commands::daemon_command().await?,
            Cmd::Start       => commands::start_command().await?,
            Cmd::Stop        => commands::stop_command().await?,
            Cmd::Status      => commands::status_command().await?,
            Cmd::Logs { tail, follow } => commands::logs_command(tail, follow).await?,
            Cmd::Config      => commands::config_command().await?,
            Cmd::Update      => commands::update_command().await?,
            Cmd::Slackmap    => commands::slackmap_command().await?,
        }
        Ok(())
    }
    .await;

    if let Err(e) = result {
        eprintln!("devm8: {e}");
        std::process::exit(1);
    }
}
