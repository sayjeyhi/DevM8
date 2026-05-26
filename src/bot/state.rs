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
    /// True when we're waiting for the user to confirm or type a new branch name.
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
    /// Pre-formatted "Key / Summary / Status / Description" block passed to every prompt.
    pub issue_context: String,
    pub cwd: Option<String>,
    pub git: Option<Arc<GitClient>>,
    /// (question, answer) pairs collected so far.
    pub qa_history: Vec<(String, String)>,
    /// The question currently being shown to the user.
    pub current_question: String,
}

// ---------------------------------------------------------------------------
// Post-analysis implement button state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PendingPostAnalysis {
    pub issue_key: String,
    pub cwd: Option<String>,
    pub git: Option<Arc<GitClient>>,
    /// Q&A context from grill flow, if any.
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
    /// Step 1: waiting for the issue title; holds the chosen project key
    CreateTitle(String),
    /// Step 2: waiting for user to confirm or replace the suggested description
    /// Fields: (project_key, corrected_title, claude_suggested_description)
    CreateDescription(String, String, String),
    Move,
    Comment,
    Solve,
    /// Jira account setup — step 1: waiting for base URL
    JiraSetupUrl,
    /// Jira account setup — step 2: base URL received, waiting for email
    JiraSetupEmail(String),
    /// Jira account setup — step 3: base URL + email received, waiting for API token
    JiraSetupToken(String, String),
    /// Jira account setup — step 4: credentials verified, user is selecting project keys.
    /// Fields: (base_url, email, api_token, all_projects as (key, name), selected_keys)
    JiraSetupProjects(String, String, String, Vec<(String, String)>, Vec<String>),
    /// Post-setup project management: user is toggling project keys.
    /// Fields: (all_projects as (key, name), selected_keys)
    JiraManageProjects(Vec<(String, String)>, Vec<String>),
    /// Favorite statuses picker: user is selecting preferred statuses.
    /// Fields: (all_status_names, selected_status_names)
    JiraFavoriteStatuses(Vec<String>, Vec<String>),
}

// ---------------------------------------------------------------------------
// Permissions wizard state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PendingPermissions {
    /// The user whose access is currently being edited; None when showing the user list.
    pub target_user_id: Option<i64>,
    /// Project keys currently toggled on.
    pub selected: HashSet<String>,
    /// ID of the single reused message (for in-place keyboard edits).
    pub message_id: Option<i32>,
    /// True when the admin clicked "Add new user" and we're waiting for a typed user ID.
    pub awaiting_user_id_input: bool,
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

    /// Pending solve action — branch is ready, user picking analyze/grill/implement
    pub pending_solve_action: Option<PendingSolveAction>,

    /// Active grill session — asking user clarifying questions one by one
    pub pending_grill: Option<PendingGrill>,

    /// Analysis complete — waiting for user to click "Implement"
    pub pending_post_analysis: Option<PendingPostAnalysis>,

    /// Active /permissions wizard
    pub pending_permissions: Option<PendingPermissions>,

    /// Waiting for admin to type input for an admin panel action
    pub pending_admin_action: Option<AdminPendingAction>,

    /// Waiting for user to type input for a Jira panel action
    pub pending_jira_action: Option<JiraPendingAction>,
}
