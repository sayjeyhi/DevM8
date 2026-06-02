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
    /// Platform user ID (string) — used to scope git worktrees.
    /// Telegram: numeric ID as string.  Slack: "U…" ID.
    pub user_id: String,
    pub repo_path: Option<PathBuf>,
    pub git: Option<Arc<GitClient>>,
    /// Present when `repo_path` is a per-user worktree; used for cleanup on session end.
    pub main_git: Option<Arc<GitClient>>,
    pub history: Vec<HistoryEntry>,
    /// Whether the branch has been pushed (enables "Open PR" button).
    pub pushed: bool,
    /// Optional system context prepended to every Claude prompt (e.g. ticket details).
    pub context: Option<String>,
}

impl AskSession {
    pub fn new(
        user_id: impl Into<String>,
        repo_path: Option<PathBuf>,
        git: Option<Arc<GitClient>>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            repo_path,
            git,
            main_git: None,
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
// Pending Slack reply (used by the Telegram-side Slack-forward handler)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PendingSlackAction {
    pub channel_id: String,
    pub thread_ts: Option<String>,
    /// When Some, we already have an AI draft that the user may confirm.
    pub ai_draft: Option<String>,
}

// ---------------------------------------------------------------------------
// Page cache for my_tickets pagination
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PageCache {
    pub project_key: String,
    pub status_filter: Option<String>,
    /// Ordered list of page tokens; index 0 = first page (token = None).
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
    pub awaiting_branch_name: bool,
}

#[derive(Debug, Clone)]
pub struct PendingSolveAction {
    pub cwd: Option<String>,
    pub git: Option<Arc<GitClient>>,
}

#[derive(Debug, Clone)]
pub struct PendingGrill {
    pub issue_key: String,
    pub issue_context: String,
    pub cwd: Option<String>,
    pub git: Option<Arc<GitClient>>,
    pub qa_history: Vec<(String, String)>,
    pub current_question: String,
}

// ---------------------------------------------------------------------------
// Post-analysis implement button state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PendingPostAnalysis {
    pub issue_key: String,
    pub git: Option<Arc<GitClient>>,
    pub qa_context: Option<String>,
}

// ---------------------------------------------------------------------------
// Admin panel pending input
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminPendingAction {
    Clone,
    AddProject,
}

// ---------------------------------------------------------------------------
// Jira panel pending input
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JiraPendingAction {
    CreateTitle(String),
    CreateDescription(String, String, String),
    Move,
    Comment,
    Solve,
    JiraSetupUrl,
    JiraSetupEmail(String),
    JiraSetupToken(String, String),
    JiraSetupProjects(String, String, String, Vec<(String, String)>, Vec<String>),
    JiraManageProjects(Vec<(String, String)>, Vec<String>),
    JiraFavoriteStatuses(Vec<String>, Vec<String>),
}

// ---------------------------------------------------------------------------
// Permissions wizard state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PendingPermissions {
    /// The user whose access is being edited; None = showing the user list.
    pub target_user_id: Option<String>,
    pub selected: HashSet<String>,
    /// ID of the single reused message (for in-place keyboard edits).
    /// Stored as a string to support both Telegram (int) and Slack (ts).
    pub message_id: Option<String>,
    pub awaiting_user_id_input: bool,
}

// ---------------------------------------------------------------------------
// Per-chat state — stored in AppState.chat_states DashMap<String, ChatState>
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct ChatState {
    pub pending_comment: Option<(String,)>,
    pub pending_ask: Option<PendingAsk>,
    pub ask_session: Option<AskSession>,
    pub pending_slack_reply: Option<PendingSlackAction>,
    pub page_cache: Option<PageCache>,
    pub pending_solve: Option<PendingSolve>,
    pub pending_solve_action: Option<PendingSolveAction>,
    pub pending_grill: Option<PendingGrill>,
    pub pending_post_analysis: Option<PendingPostAnalysis>,
    pub pending_permissions: Option<PendingPermissions>,
    pub pending_admin_action: Option<AdminPendingAction>,
    pub pending_jira_action: Option<JiraPendingAction>,
}
