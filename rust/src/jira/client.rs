#![allow(dead_code)]

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Client, Response, StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::shared::errors::{AppError, InvalidTransitionError, JiraError};

use super::{
    adf::{adf_to_text, to_adf, AdfNode},
    types::{JiraClientConfig, JiraIssue},
};

// ---------------------------------------------------------------------------
// Internal Jira REST API response shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct JiraIssueResponse {
    key: String,
    fields: JiraIssueFields,
    #[serde(rename = "self")]
    self_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JiraIssueFields {
    summary: String,
    status: JiraStatusField,
    description: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct JiraStatusField {
    name: String,
}

#[derive(Debug, Deserialize)]
struct JiraTransition {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct JiraTransitionsResponse {
    transitions: Vec<JiraTransition>,
}

#[derive(Debug, Deserialize)]
struct JiraSearchResponse {
    issues: Vec<JiraIssueResponse>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JiraMyselfResponse {
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "emailAddress")]
    email_address: String,
}

#[derive(Debug, Deserialize)]
struct JiraProject {
    key: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct JiraStatus {
    id: String,
    name: String,
    #[serde(rename = "statusCategory")]
    status_category: Option<JiraStatusCategory>,
}

#[derive(Debug, Deserialize)]
struct JiraStatusCategory {
    name: String,
}

// ---------------------------------------------------------------------------
// Public return types for `get_my_issues`
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuesPage {
    pub issues: Vec<JiraIssue>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    pub id: String,
    pub name: String,
    pub category: String,
}

// ---------------------------------------------------------------------------
// JiraClient
// ---------------------------------------------------------------------------

pub struct JiraClient {
    config: JiraClientConfig,
    http: Client,
    base_url: String,
    auth_header: String,
}

impl JiraClient {
    /// Build a new client from the given configuration.
    pub fn new(config: JiraClientConfig) -> anyhow::Result<Self> {
        let timeout_ms = config.request_timeout_ms.unwrap_or(10_000);
        let http = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()?;

        let credentials = format!("{}:{}", config.email, config.api_token);
        let encoded = BASE64.encode(credentials.as_bytes());
        let auth_header = format!("Basic {}", encoded);

        let base_url = format!("https://{}/rest/api/3", config.host);

        Ok(Self { config, http, base_url, auth_header })
    }

    /// Expose the configured project keys.
    pub fn project_keys(&self) -> &[String] {
        &self.config.project_keys
    }

    // -----------------------------------------------------------------------
    // Internal HTTP helpers
    // -----------------------------------------------------------------------

    fn default_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&self.auth_header).expect("auth header is ASCII"),
        );
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/json"),
        );
        headers
    }

    async fn handle_response_error(
        &self,
        resp: Response,
        issue_key: Option<&str>,
    ) -> AppError {
        let status = resp.status();
        match status {
            StatusCode::UNAUTHORIZED => AppError::Jira(JiraError::Auth),
            StatusCode::FORBIDDEN => AppError::Jira(JiraError::Permission),
            StatusCode::NOT_FOUND => {
                if let Some(key) = issue_key {
                    AppError::Jira(JiraError::NotFound { issue_key: key.to_string() })
                } else {
                    AppError::Jira(JiraError::Server { status: 404 })
                }
            }
            StatusCode::TOO_MANY_REQUESTS => {
                let retry_after = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());
                AppError::Jira(JiraError::RateLimit { retry_after })
            }
            s if s.is_server_error() => AppError::Jira(JiraError::Server { status: s.as_u16() }),
            s => AppError::Jira(JiraError::Server { status: s.as_u16() }),
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, AppError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .get(&url)
            .headers(self.default_headers())
            .query(query)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AppError::Jira(JiraError::Timeout)
                } else {
                    AppError::Other(e.into())
                }
            })?;

        if resp.status().is_success() {
            let body = resp.json::<T>().await.map_err(|e| AppError::Other(e.into()))?;
            Ok(body)
        } else {
            Err(self.handle_response_error(resp, None).await)
        }
    }

    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<T, AppError> {
        self.post_with_issue_key(path, body, None).await
    }

    async fn post_with_issue_key<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &Value,
        issue_key: Option<&str>,
    ) -> Result<T, AppError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .headers(self.default_headers())
            .json(body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AppError::Jira(JiraError::Timeout)
                } else {
                    AppError::Other(e.into())
                }
            })?;

        if resp.status().is_success() {
            let body = resp.json::<T>().await.map_err(|e| AppError::Other(e.into()))?;
            Ok(body)
        } else {
            Err(self.handle_response_error(resp, issue_key).await)
        }
    }

    async fn post_no_body(&self, path: &str, body: &Value) -> Result<(), AppError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .headers(self.default_headers())
            .json(body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AppError::Jira(JiraError::Timeout)
                } else {
                    AppError::Other(e.into())
                }
            })?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(self.handle_response_error(resp, None).await)
        }
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Verify credentials and return basic account info.
    pub async fn ping(&self) -> Result<(String, String), AppError> {
        let resp: JiraMyselfResponse = self.get("/myself", &[]).await?;
        Ok((resp.display_name, resp.email_address))
    }

    /// Create a new issue and return it.
    pub async fn create_issue(
        &self,
        title: &str,
        description: &str,
    ) -> Result<JiraIssue, AppError> {
        // Use the first configured project key.
        let project_key = self
            .config
            .project_keys
            .first()
            .cloned()
            .unwrap_or_default();
        let issue_type = self
            .config
            .issue_type
            .clone()
            .unwrap_or_else(|| "Task".to_string());

        let adf_desc = to_adf(description);
        let body = json!({
            "fields": {
                "project": { "key": project_key },
                "summary": title,
                "description": adf_desc,
                "issuetype": { "name": issue_type }
            }
        });

        let raw: JiraIssueResponse = self.post("/issue", &body).await?;
        Ok(self.map_issue(raw))
    }

    /// Fetch a single issue by key.
    pub async fn get_issue(&self, issue_key: &str) -> Result<JiraIssue, AppError> {
        self.get_issue_by_key(issue_key).await
    }

    /// Fetch a single issue by key using GET.
    pub async fn get_issue_by_key(&self, issue_key: &str) -> Result<JiraIssue, AppError> {
        let path = format!("/issue/{}", issue_key);
        let raw: JiraIssueResponse = self.get(&path, &[]).await?;
        Ok(self.map_issue(raw))
    }

    /// Return available transitions for an issue.
    pub async fn get_transitions(
        &self,
        issue_key: &str,
    ) -> Result<Vec<(String, String)>, AppError> {
        let path = format!("/issue/{}/transitions", issue_key);
        let resp: JiraTransitionsResponse = self.get(&path, &[]).await?;
        Ok(resp.transitions.into_iter().map(|t| (t.id, t.name)).collect())
    }

    /// Transition an issue to a new status.
    pub async fn transition_issue(
        &self,
        issue_key: &str,
        target_status: &str,
    ) -> Result<(), AppError> {
        let transitions = self.get_transitions(issue_key).await?;

        let transition_id = transitions
            .iter()
            .find(|(_, name)| name.eq_ignore_ascii_case(target_status))
            .map(|(id, _)| id.clone());

        let id = match transition_id {
            Some(id) => id,
            None => {
                let available: Vec<String> =
                    transitions.iter().map(|(_, n)| n.clone()).collect();
                return Err(AppError::InvalidTransition(
                    InvalidTransitionError::new(target_status, available),
                ));
            }
        };

        let path = format!("/issue/{}/transitions", issue_key);
        let body = json!({ "transition": { "id": id } });
        self.post_no_body(&path, &body).await
    }

    /// Add a plain-text comment to an issue (converted to ADF).
    pub async fn add_comment(&self, issue_key: &str, body: &str) -> Result<(), AppError> {
        let path = format!("/issue/{}/comment", issue_key);
        let adf_body = to_adf(body);
        let payload = json!({ "body": adf_body });
        // Jira returns the created comment; we discard it.
        let _: Value = self.post(&path, &payload).await?;
        Ok(())
    }

    /// Return all projects accessible to the configured account.
    pub async fn get_projects(&self) -> Result<Vec<ProjectInfo>, AppError> {
        let projects: Vec<JiraProject> = self.get("/project", &[]).await?;
        Ok(projects
            .into_iter()
            .map(|p| ProjectInfo { key: p.key, name: p.name })
            .collect())
    }

    /// Return all statuses configured on this Jira instance.
    pub async fn get_statuses(&self) -> Result<Vec<StatusInfo>, AppError> {
        let statuses: Vec<JiraStatus> = self.get("/status", &[]).await?;
        Ok(statuses
            .into_iter()
            .map(|s| StatusInfo {
                id: s.id,
                name: s.name,
                category: s
                    .status_category
                    .map(|c| c.name)
                    .unwrap_or_default(),
            })
            .collect())
    }

    /// Paginated list of issues assigned to the current user.
    pub async fn get_my_issues(
        &self,
        limit: u32,
        next_page_token: Option<&str>,
        status: Option<&str>,
        project_key: Option<&str>,
    ) -> Result<IssuesPage, AppError> {
        let mut jql = "assignee = currentUser()".to_string();
        if let Some(pk) = project_key {
            jql.push_str(&format!(" AND project = \"{}\"", pk));
        }
        if let Some(s) = status {
            jql.push_str(&format!(" AND status = \"{}\"", s));
        }
        jql.push_str(" ORDER BY updated DESC");

        let mut body = json!({
            "jql": jql,
            "maxResults": limit,
            "fields": ["summary", "status"]
        });

        if let Some(token) = next_page_token {
            body["nextPageToken"] = json!(token);
        }

        let resp: JiraSearchResponse = self.post("/search/jql", &body).await?;

        let issues = resp.issues.into_iter().map(|r| self.map_issue(r)).collect();
        Ok(IssuesPage {
            issues,
            next_page_token: resp.next_page_token,
        })
    }

    // -----------------------------------------------------------------------
    // Mapping helpers
    // -----------------------------------------------------------------------

    fn map_issue(&self, raw: JiraIssueResponse) -> JiraIssue {
        let description = raw
            .fields
            .description
            .as_ref()
            .and_then(|v| serde_json::from_value::<AdfNode>(v.clone()).ok())
            .as_ref()
            .map(|node| adf_to_text(Some(node)))
            .unwrap_or_default();

        let url = raw
            .self_url
            .as_deref()
            .map(|u| {
                // Convert REST URL to browser URL:
                // https://host/rest/api/3/issue/KEY -> https://host/browse/KEY
                let base = format!("https://{}/browse/{}", self.config.host, raw.key);
                let _ = u; // suppress unused warning
                base
            })
            .unwrap_or_else(|| {
                format!("https://{}/browse/{}", self.config.host, raw.key)
            });

        JiraIssue {
            key: raw.key,
            summary: raw.fields.summary,
            status: raw.fields.status.name,
            description,
            url,
        }
    }
}
