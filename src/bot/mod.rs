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

use crate::claude::client::{AiClient, ClaudeClient};
use crate::claude::types::ClaudeClientConfig;
use crate::config::schema::{AiTool, AppConfig, UserJiraConfig};
use crate::db::Db;
use crate::git::GitClient;
use crate::jira::client::JiraClient;
use crate::jira::types::JiraClientConfig;
use crate::kiro::client::KiroClient;
use crate::kiro::types::KiroClientConfig;
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

    /// AI CLI client (Claude or Kiro, depending on config).
    pub ai: Arc<dyn AiClient>,

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

    /// Teams project access map (project key → allowed Teams AAD user IDs).
    pub teams_project_access: RwLock<HashMap<String, Vec<String>>>,

    /// Cache of Teams user_id → display name.
    pub teams_user_names: DashMap<String, String>,

    /// Email-identity + chat-history store (SQLite).
    pub db: Arc<Db>,
}

impl AppState {
    pub fn is_admin(&self, user_id: i64) -> bool {
        match self.config.telegram.admin_user_id {
            Some(admin_id) => user_id == admin_id,
            None => true,
        }
    }

    #[allow(dead_code)]
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

    pub fn slack_is_admin(&self, user_id: &str) -> bool {
        match self
            .config
            .slack
            .as_ref()
            .and_then(|s| s.admin_user_id.as_deref())
        {
            Some(admin_id) => user_id == admin_id,
            None => true,
        }
    }

    pub fn slack_is_authorized(&self, user_id: &str) -> bool {
        let allowed = self
            .config
            .slack
            .as_ref()
            .map(|s| s.allowed_user_ids.as_slice())
            .unwrap_or_default();
        allowed.is_empty() || allowed.iter().any(|id| id == user_id)
    }

    pub fn teams_is_admin(&self, user_id: &str) -> bool {
        match self
            .config
            .teams
            .as_ref()
            .and_then(|t| t.admin_user_id.as_deref())
        {
            Some(admin_id) => user_id == admin_id,
            None => true,
        }
    }

    pub fn teams_is_authorized(&self, user_id: &str) -> bool {
        let allowed = self
            .config
            .teams
            .as_ref()
            .map(|t| t.allowed_user_ids.as_slice())
            .unwrap_or_default();
        allowed.is_empty() || allowed.iter().any(|id| id == user_id)
    }

    pub fn teams_is_authorized_for_project(&self, user_id: &str, project_key: &str) -> bool {
        if self.teams_is_admin(user_id) {
            return true;
        }
        let access = self.teams_project_access.read().unwrap();
        if access.is_empty() {
            return true;
        }
        let is_restricted = access
            .values()
            .any(|ids| ids.iter().any(|id| id == user_id));
        match access.get(project_key) {
            None => !is_restricted,
            Some(ids) => ids.iter().any(|id| id == user_id),
        }
    }

    pub fn slack_is_authorized_for_project(&self, user_id: &str, project_key: &str) -> bool {
        if self.slack_is_admin(user_id) {
            return true;
        }
        let access = self.slack_project_access.read().unwrap();
        if access.is_empty() {
            return true;
        }
        let is_restricted = access
            .values()
            .any(|ids| ids.iter().any(|id| id == user_id));
        match access.get(project_key) {
            None => !is_restricted,
            Some(ids) => ids.iter().any(|id| id == user_id),
        }
    }

    /// Returns the per-user Jira client, or the global fallback if configured.
    ///
    /// Falls back through any other channel identity linked to the same
    /// email — e.g. Jira configured via Telegram becomes usable from the
    /// CLI, whose `user_id` is the verified email rather than a channel-native
    /// ID. This only affects *using* an already-configured client; ownership
    /// checks for the setup/reconnect/disconnect wizard (`has_user_jira`)
    /// intentionally stay scoped to the caller's own identity, so disconnect
    /// only ever touches the entry the caller actually owns.
    pub async fn jira_for_user(&self, user_id: &str) -> Option<Arc<JiraClient>> {
        if let Some(c) = self.user_jira_clients.get(user_id) {
            return Some(Arc::clone(&*c));
        }
        if let Ok(identities) = self.db.list_channel_identities_for_email(user_id).await {
            if let Some(c) = identities
                .iter()
                .find_map(|(_, external_id)| self.user_jira_clients.get(external_id))
            {
                return Some(Arc::clone(&*c));
            }
        }
        self.jira.as_ref().map(Arc::clone)
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

    /// Resolve `channel`/`external_id` (e.g. "telegram"/"123456") to the devm8 user's
    /// email, auto-provisioning a placeholder user on first sight so chat history is
    /// never dropped before an admin runs `devm8 migrate-users`.
    ///
    /// The "cli" channel is special-cased: unlike Telegram/Slack/Teams, its
    /// `external_id` is already the bearer-authenticated user's real, verified
    /// email (see `AskSession::user_id` set from `AuthedUser.email` in
    /// `src/api/routes.rs`), not an opaque platform ID needing a lookup. Routing
    /// it through `get_or_create_user_for_channel` would mint a bogus
    /// `cli-<email>@unmapped.devm8.local` placeholder and split CLI chat history
    /// away from the user's real account.
    pub async fn email_for_channel_user(&self, channel: &str, external_id: &str) -> String {
        if channel == "cli" {
            return external_id.to_string();
        }
        match self
            .db
            .get_or_create_user_for_channel(channel, external_id, None)
            .await
        {
            Ok(email) => email,
            Err(e) => {
                self.logger.warn(
                    &format!("db: get_or_create_user_for_channel failed: {e}"),
                    None,
                );
                format!("{channel}-{external_id}@unmapped.devm8.local")
            }
        }
    }

    /// The Jira client for `email` — checks for a CLI-native account keyed directly
    /// by email first (set up via devm8-client's own Jira setup wizard), then falls
    /// back to any Telegram/Slack/Teams identity mapped to this email that has one
    /// configured.
    #[allow(dead_code)]
    pub async fn jira_for_email(&self, email: &str) -> Option<Arc<JiraClient>> {
        if let Some(c) = self.user_jira_clients.get(email) {
            return Some(Arc::clone(&*c));
        }
        let identities = self
            .db
            .list_channel_identities_for_email(email)
            .await
            .ok()?;
        identities
            .iter()
            .find_map(|(_, external_id)| self.user_jira_clients.get(external_id))
            .map(|c| Arc::clone(&*c))
    }

    /// Build an `AskSession` backed by a per-user git worktree.
    /// Falls back to using the main repo path directly if worktree creation fails.
    pub async fn worktree_session(
        &self,
        user_id: &str,
        main_git: Arc<GitClient>,
        branch: &str,
    ) -> state::AskSession {
        match main_git.create_worktree(user_id, branch).await {
            Ok(wt_path) => {
                let wt_git = Arc::new(GitClient::new(wt_path.clone()));
                let mut session = state::AskSession::new(user_id, Some(wt_path), Some(wt_git));
                session.main_git = Some(main_git);
                session
            }
            Err(e) => {
                self.logger.error(
                    &format!("worktree setup failed, using direct repo: {e}"),
                    None,
                );
                let repo_path = main_git.repo_path.clone();
                state::AskSession::new(user_id, Some(repo_path), Some(main_git))
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

        let ai: Arc<dyn AiClient> = match config.ai_tool {
            AiTool::Claude => {
                let claude_cfg = config
                    .claude
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("claude config missing but ai_tool = claude"))?;
                Arc::new(ClaudeClient::new(
                    ClaudeClientConfig {
                        binary_path: claude_cfg.binary_path.clone(),
                        timeout_ms: claude_cfg.timeout_ms,
                        model: None,
                        api_key: claude_cfg.api_key.clone(),
                        sandbox_enabled: claude_cfg.sandbox,
                        sandbox_extra_paths: claude_cfg.sandbox_extra_paths.clone(),
                    },
                    Arc::clone(&logger),
                ))
            }
            AiTool::Kiro => {
                let kiro_cfg = config
                    .kiro
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("kiro config missing but ai_tool = kiro"))?;
                Arc::new(KiroClient::new(
                    KiroClientConfig {
                        binary_path: kiro_cfg.binary_path.clone(),
                        timeout_ms: kiro_cfg.timeout_ms,
                    },
                    Arc::clone(&logger),
                ))
            }
        };

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
        let slack_project_access = RwLock::new(
            config
                .slack
                .as_ref()
                .map(|s| s.project_access.clone())
                .unwrap_or_default(),
        );
        let teams_project_access = RwLock::new(
            config
                .teams
                .as_ref()
                .map(|t| t.project_access.clone())
                .unwrap_or_default(),
        );

        let audit_logger = Arc::new(AuditLogger::new(&PATHS.audit_log_file));

        let db = Arc::new(Db::open(&PATHS.db_file)?);

        Ok(Self {
            jira,
            user_jira_clients,
            ai,
            chat_states: DashMap::new(),
            config,
            logger,
            audit_logger,
            git_map,
            slack,
            bot_username,
            project_access,
            user_names: DashMap::new(),
            slack_project_access,
            slack_user_names: DashMap::new(),
            teams_project_access,
            teams_user_names: DashMap::new(),
            db,
        })
    }
}
