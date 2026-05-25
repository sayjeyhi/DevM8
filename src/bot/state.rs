use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use crate::git::GitClient;

// ---------------------------------------------------------------------------
// Ask session
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskMode {
    Followup,
    Branch,
    Commit,
    Cli,
}

#[derive(Debug, Clone)]
pub struct AskSession {
    pub repo_path: Option<PathBuf>,
    pub git: Option<Arc<GitClient>>,
    pub history: Vec<HistoryEntry>,
    /// Whether the branch has been pushed (enables "Open PR" button)
    pub pushed: bool,
    /// Optional system context prepended to every Claude prompt (e.g. ticket details)
    pub context: Option<String>,
}

impl AskSession {
    pub fn new(repo_path: Option<PathBuf>, git: Option<Arc<GitClient>>) -> Self {
        Self {
            repo_path,
            git,
            history: Vec::new(),
            pushed: false,
            context: None,
        }
    }

    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }
}

#[derive(Debug, Clone)]
pub struct PendingAsk {
    pub repo_path: Option<PathBuf>,
    pub git: Option<Arc<GitClient>>,
    pub inline_question: Option<String>,
    pub mode: Option<AskMode>,
}

// ---------------------------------------------------------------------------
// Pending Slack reply
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PendingSlackAction {
    pub channel_id: String,
    pub thread_ts: Option<String>,
    /// When Some, we already have a draft from AI that we may want to send
    pub ai_draft: Option<String>,
}

// ---------------------------------------------------------------------------
// Page cache for my_tickets pagination
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PageCache {
    pub project_key: String,
    pub status_filter: Option<String>,
    /// Ordered list of page tokens; index 0 = first page (token = None)
    pub tokens: Vec<Option<String>>,
    pub current_page: usize,
}

impl PageCache {
    pub fn new(project_key: impl Into<String>, status_filter: Option<String>) -> Self {
        Self {
            project_key: project_key.into(),
            status_filter,
            tokens: vec![None],
            current_page: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Solve: pending git selections
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PendingSolve {
    pub issue_key: String,
    pub git: Option<Arc<GitClient>>,
}

// ---------------------------------------------------------------------------
// Permissions wizard state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PendingPermissions {
    /// None while waiting for the admin to type a target user ID.
    pub target_user_id: Option<i64>,
    /// Project keys currently toggled on.
    pub selected: HashSet<String>,
    /// ID of the project-picker message (for in-place keyboard edits). Stored
    /// as the raw i32 so state.rs stays free of teloxide imports.
    pub message_id: Option<i32>,
}

// ---------------------------------------------------------------------------
// Per-chat state — stored in AppState.chat_states DashMap
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct ChatState {
    /// Waiting for user to type a comment body for a ticket
    pub pending_comment: Option<(String,)>, // (issue_key,)

    /// Waiting for user to type a freeform ask input
    pub pending_ask: Option<PendingAsk>,

    /// Active ask session
    pub ask_session: Option<AskSession>,

    /// Pending Slack reply (waiting for user to type the reply text)
    pub pending_slack_reply: Option<PendingSlackAction>,

    /// Cached page state for my_tickets navigation
    pub page_cache: Option<PageCache>,

    /// Pending solve — user chose a repo, now picking branch action
    pub pending_solve: Option<PendingSolve>,

    /// Active /permissions wizard
    pub pending_permissions: Option<PendingPermissions>,
}
