use serde::{Deserialize, Serialize};

/// A single button choice, mirroring `crate::channel::types::Button` over the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceDto {
    pub label: String,
    pub data: String,
}

/// One row of buttons.
pub type KeyboardDto = Vec<Vec<ChoiceDto>>;

/// Streamed over SSE from `/v1/ask` and `/v1/solve`. Mirrors `ChannelSender`'s
/// methods 1:1 — the CLI renders each variant the way a chat client would
/// render a message/edit/keyboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AskEvent {
    Text {
        id: String,
        text: String,
    },
    Keyboard {
        id: String,
        text: String,
        choices: KeyboardDto,
    },
    EditText {
        id: String,
        text: String,
    },
    EditWithKeyboard {
        id: String,
        text: String,
        choices: KeyboardDto,
    },
    EditKeyboard {
        id: String,
        choices: KeyboardDto,
    },
    Delete {
        id: String,
    },
    /// The background handler finished successfully — no more events follow.
    Done,
    /// The background handler returned an error.
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairRequest {
    pub code: String,
    /// Client-supplied label (e.g. hostname) stored alongside the issued token.
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairResponse {
    pub token: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeResponse {
    pub email: String,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDto {
    pub key: String,
    pub has_git: bool,
    pub has_jira: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskRequest {
    pub question: String,
    #[serde(default)]
    pub project: Option<String>,
}

/// Explicitly begins an ask session for a project, mirroring Telegram's
/// `/start` — for a git-backed project this triggers the same worktree
/// branch-name prompt / repo-ready message before any question can be asked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskStartRequest {
    pub project: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveRequest {
    pub issue_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrReviewRequest {
    pub url: String,
}

/// A button's `data` string, sent back exactly as a Telegram callback_data
/// payload would be (e.g. "ask:cancel", "solve:repo:PROJ-1:0").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRequest {
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryQuery {
    pub project: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummaryDto {
    pub session_id: String,
    pub project_key: Option<String>,
    pub channel: String,
    pub first_message: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatTurnDto {
    pub role: String,
    pub content: String,
    pub created_at: String,
}
