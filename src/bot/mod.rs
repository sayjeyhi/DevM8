#[allow(clippy::module_inception)]
pub mod bot;
pub mod commands;
pub mod handlers;
pub mod polling;
pub mod sender;
pub mod state;
pub mod utils;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use dashmap::DashMap;

use crate::claude::client::ClaudeClient;
use crate::claude::types::ClaudeClientConfig;
use crate::config::schema::{AppConfig, UserJiraConfig};
use crate::git::GitClient;
use crate::jira::client::JiraClient;
use crate::jira::types::JiraClientConfig;
use crate::logger::audit::AuditLogger;
use crate::logger::Logger;
use crate::shared::paths::PATHS;
use crate::slack::SlackClient;

// ---------------------------------------------------------------------------
// Shared application state passed to every handler
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct AppState {
    /// Global Jira client (optional fallback when no per-user override exists).
    pub jira: Option<Arc<JiraClient>>,

    /// Per-user Jira clients — keyed by user_id string.
    pub user_jira_clients: DashMap<String, Arc<JiraClient>>,

    /// Claude CLI client.
    pub claude: Arc<ClaudeClient>,

    /// Per-chat mutable state — keyed by chat_id string.
    pub chat_states: DashMap<String, state::ChatState>,

    /// Resolved application configuration.
    pub config: AppConfig,

    /// Logger instance.
    pub logger: Arc<dyn Logger>,

    /// Audit logger — records every user action to audit.log.
    pub audit_logger: Arc<AuditLogger>,

    /// Maps project key (e.g. "MYAPP") to one or more Git repositories.
    pub git_map: HashMap<String, Vec<Arc<GitClient>>>,

    /// Optional Slack client.
    pub slack: Option<Arc<SlackClient>>,

    /// Telegram bot username (e.g. "MyBot"), used to generate deep links.
    pub bot_username: String,

    /// Live project-access map (project key → allowed user IDs).
    pub project_access: RwLock<HashMap<String, Vec<i64>>>,

    /// Cache of user_id → display name.
    pub user_names: DashMap<i64, String>,

    /// Slack project access map (project key → allowed Slack user IDs).
    pub slack_project_access: RwLock<HashMap<String, Vec<String>>>,

    /// Cache of Slack user_id → display name.
    pub slack_user_names: DashMap<String, String>,
}

impl AppState {
    pub fn is_admin(&self, user_id: i64) -> bool {
        match self.config.telegram.admin_user_id {
            Some(admin_id) => user_id == admin_id,
            None => true,
        }
    }

    /// Authorization check for Telegram project access.
    pub fn telegram_is_authorized_for_project(&self, user_id: i64, project_key: &str) -> bool {
        if self.is_admin(user_id) {
            return true;
        }
        let access = self.project_access.read().unwrap();
        if access.is_empty() {
            return true;
        }
        let is_restricted = access.values().any(|ids| ids.contains(&user_id));
        match access.get(project_key) {
            None => !is_restricted,
            Some(ids) => ids.contains(&user_id),
        }
    }

    /// Returns the per-user Jira client, or the global fallback if configured.
    pub fn jira_for_user(&self, user_id: &str) -> Option<Arc<JiraClient>> {
        self.user_jira_clients
            .get(user_id)
            .map(|c| Arc::clone(&*c))
            .or_else(|| self.jira.as_ref().map(Arc::clone))
    }

    pub fn has_user_jira(&self, user_id: &str) -> bool {
        self.user_jira_clients.contains_key(user_id)
    }

    /// Build a JiraClient from a `UserJiraConfig` and cache it.
    pub fn set_user_jira(
        &self,
        user_id: &str,
        cfg: &UserJiraConfig,
    ) -> anyhow::Result<Arc<JiraClient>> {
        let host = cfg
            .base_url
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .to_string();
        let client = JiraClient::new(JiraClientConfig {
            host,
            email: cfg.email.clone(),
            api_token: cfg.api_token.clone(),
            project_keys: cfg.project_keys.clone(),
            issue_type: None,
            request_timeout_ms: None,
        })?;
        let arc = Arc::new(client);
        self.user_jira_clients
            .insert(user_id.to_string(), Arc::clone(&arc));
        Ok(arc)
    }

    pub fn remove_user_jira(&self, user_id: &str) {
        self.user_jira_clients.remove(user_id);
    }

    /// Build an `AskSession` backed by a per-user git worktree.
    /// Falls back to using the main repo path directly if worktree creation fails.
    pub async fn worktree_session(
        &self,
        user_id: &str,
        main_git: Arc<GitClient>,
    ) -> state::AskSession {
        let uid_i64 = user_id.parse::<i64>().unwrap_or(0);
        match main_git.create_worktree(uid_i64).await {
            Ok(wt_path) => {
                let wt_git = Arc::new(GitClient::new(wt_path.clone()));
                let mut session = state::AskSession::new(uid_i64, Some(wt_path), Some(wt_git));
                session.main_git = Some(main_git);
                session
            }
            Err(e) => {
                self.logger.error(
                    &format!("worktree setup failed, using direct repo: {e}"),
                    None,
                );
                let repo_path = main_git.repo_path.clone();
                state::AskSession::new(uid_i64, Some(repo_path), Some(main_git))
            }
        }
    }

    pub fn new(
        config: AppConfig,
        logger: Arc<dyn Logger>,
        bot_username: String,
    ) -> anyhow::Result<Self> {
        let jira: Option<Arc<JiraClient>> = config
            .jira
            .as_ref()
            .map(|jira_cfg| {
                let host = jira_cfg
                    .base_url
                    .trim_start_matches("https://")
                    .trim_end_matches('/')
                    .to_string();
                JiraClient::new(JiraClientConfig {
                    host,
                    email: jira_cfg.email.clone(),
                    api_token: jira_cfg.api_token.clone(),
                    project_keys: jira_cfg.project_keys.clone(),
                    issue_type: None,
                    request_timeout_ms: None,
                })
                .map(Arc::new)
            })
            .transpose()?;

        let user_jira_clients: DashMap<String, Arc<JiraClient>> = DashMap::new();
        for (uid_str, user_cfg) in &config.user_jira {
            let user_host = user_cfg
                .base_url
                .trim_start_matches("https://")
                .trim_end_matches('/')
                .to_string();
            if let Ok(client) = JiraClient::new(JiraClientConfig {
                host: user_host,
                email: user_cfg.email.clone(),
                api_token: user_cfg.api_token.clone(),
                project_keys: user_cfg.project_keys.clone(),
                issue_type: None,
                request_timeout_ms: None,
            }) {
                user_jira_clients.insert(uid_str.clone(), Arc::new(client));
            }
        }

        let claude_cfg = ClaudeClientConfig {
            binary_path: config.claude.binary_path.clone(),
            timeout_ms: config.claude.timeout_ms,
            model: None,
            api_key: config.claude.api_key.clone(),
            sandbox_enabled: config.claude.sandbox,
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

        let audit_logger = Arc::new(AuditLogger::new(&PATHS.audit_log_file));

        Ok(Self {
            jira,
            user_jira_clients,
            claude,
            chat_states: DashMap::new(),
            config,
            logger,
            audit_logger,
            git_map,
            slack,
            bot_username,
            project_access,
            user_names: DashMap::new(),
            slack_project_access: RwLock::new(HashMap::new()),
            slack_user_names: DashMap::new(),
        })
    }
}
