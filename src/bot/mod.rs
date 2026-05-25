#[allow(clippy::module_inception)]
pub mod bot;
pub mod commands;
pub mod handlers;
pub mod polling;
pub mod state;
pub mod utils;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use dashmap::DashMap;

use crate::claude::client::ClaudeClient;
use crate::claude::types::ClaudeClientConfig;
use crate::config::schema::AppConfig;
use crate::git::GitClient;
use crate::jira::client::JiraClient;
use crate::jira::types::JiraClientConfig;
use crate::logger::Logger;
use crate::slack::SlackClient;

// ---------------------------------------------------------------------------
// Shared application state passed to every Telegram handler
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct AppState {
    /// Jira client (shared, cheaply cloned via Arc).
    pub jira: Arc<JiraClient>,

    /// Claude CLI client.
    pub claude: Arc<ClaudeClient>,

    /// Per-chat mutable state.
    pub chat_states: DashMap<i64, state::ChatState>,

    /// Resolved application configuration.
    pub config: AppConfig,

    /// Logger instance.
    pub logger: Arc<dyn Logger>,

    /// Maps project key (e.g. "MYAPP") to one or more Git repositories.
    pub git_map: HashMap<String, Vec<Arc<GitClient>>>,

    /// Optional Slack client.
    pub slack: Option<Arc<SlackClient>>,

    /// Telegram bot username (e.g. "MyBot"), used to generate deep links.
    pub bot_username: String,

    /// Live project-access map (project key → allowed user IDs).
    /// Wraps a copy of config.telegram.project_access and is updated at
    /// runtime by /permissions without requiring a daemon restart.
    pub project_access: RwLock<HashMap<String, Vec<i64>>>,
}

impl AppState {
    pub fn new(
        config: AppConfig,
        logger: Arc<dyn Logger>,
        bot_username: String,
    ) -> anyhow::Result<Self> {
        // Extract the host from the base_url (strip "https://" prefix and trailing slash).
        let host = config
            .jira
            .base_url
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .to_string();

        let jira_cfg = JiraClientConfig {
            host,
            email: config.jira.email.clone(),
            api_token: config.jira.api_token.clone(),
            project_keys: config.jira.project_keys.clone(),
            issue_type: None,
            request_timeout_ms: None,
        };

        let jira = Arc::new(JiraClient::new(jira_cfg)?);

        let claude_cfg = ClaudeClientConfig {
            binary_path: config.claude.binary_path.clone(),
            timeout_ms: config.claude.timeout_ms,
            model: None,
        };

        let claude = Arc::new(ClaudeClient::new(claude_cfg, Arc::clone(&logger)));

        // Build git_map from config.projects
        let mut git_map: HashMap<String, Vec<Arc<GitClient>>> = HashMap::new();
        if let Some(repos) = &config.projects {
            for (project_key, paths) in repos {
                let clients: Vec<Arc<GitClient>> =
                    paths.iter().map(|p| Arc::new(GitClient::new(p))).collect();
                git_map.insert(project_key.clone(), clients);
            }
        }

        // Build Slack client if configured
        let slack = config
            .slack
            .as_ref()
            .map(|sc| Arc::new(SlackClient::new(sc.user_token.clone())));

        let project_access = RwLock::new(config.telegram.project_access.clone());

        Ok(Self {
            jira,
            claude,
            chat_states: DashMap::new(),
            config,
            logger,
            git_map,
            slack,
            bot_username,
            project_access,
        })
    }
}
