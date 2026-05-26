/// Interactive configuration wizard.
///
/// Mirrors the TypeScript `runWizard` that uses `@clack/prompts`.
/// Uses the `inquire` crate for prompts.
use std::collections::HashMap;

use inquire::{Confirm, MultiSelect, Select, Text};

use crate::config::schema::{
    AppConfig, AppSettings, ClaudeConfig, JiraConfig, LogLevel, SlackConfig, TelegramConfig,
};
use crate::config::validators;
use crate::shared::errors::AppError;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the interactive configuration wizard.
///
/// If `existing` is `Some`, pre-fill prompts with the current values so the
/// user can do an incremental edit (press Enter to keep the current value).
pub fn run_wizard(existing: Option<&AppConfig>) -> Result<AppConfig, AppError> {
    println!("\n  devm8 configuration wizard\n");

    let telegram = collect_telegram(existing.map(|c| &c.telegram))?;
    let jira = collect_jira(existing.map(|c| &c.jira))?;
    let claude = collect_claude(existing.map(|c| &c.claude))?;
    let projects = collect_projects(
        &jira.project_keys,
        existing.and_then(|c| c.projects.as_ref()),
    )?;
    let app = collect_app_settings(existing.map(|c| &c.app))?;
    let slack = collect_slack(existing.and_then(|c| c.slack.as_ref()))?;

    println!("\n  Configuration complete.\n");

    Ok(AppConfig {
        telegram,
        jira,
        claude,
        projects: if projects.is_empty() {
            None
        } else {
            Some(projects)
        },
        app,
        slack,
        user_jira: std::collections::HashMap::new(),
    })
}

// ---------------------------------------------------------------------------
// Section collectors
// ---------------------------------------------------------------------------

fn collect_telegram(existing: Option<&TelegramConfig>) -> Result<TelegramConfig, AppError> {
    println!("--- Telegram ---");

    let bot_token = Text::new("Bot token (format: 123456:ABCdef…):")
        .with_initial_value(existing.map(|c| c.bot_token.as_str()).unwrap_or(""))
        .with_validator(|v: &str| {
            Ok(validators::validate_bot_token(v)
                .map(|e| inquire::validator::Validation::Invalid(e.into()))
                .unwrap_or(inquire::validator::Validation::Valid))
        })
        .prompt()
        .map_err(|e| prompt_err("telegram.bot_token", e))?;

    let ids_default = existing
        .map(|c| {
            c.allowed_user_ids
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    let allowed_user_ids_raw =
        Text::new("Allowed Telegram user IDs (comma-separated, e.g. 12345, 67890):")
            .with_initial_value(&ids_default)
            .with_validator(|v: &str| {
                if v.trim().is_empty() {
                    return Ok(inquire::validator::Validation::Valid);
                }
                Ok(validators::validate_allowed_user_ids(v)
                    .map(|e| inquire::validator::Validation::Invalid(e.into()))
                    .unwrap_or(inquire::validator::Validation::Valid))
            })
            .prompt()
            .map_err(|e| prompt_err("telegram.allowed_user_ids", e))?;

    let allowed_user_ids = parse_ids(&allowed_user_ids_raw);

    // Keep existing admin if re-running wizard; otherwise default to first allowed user.
    let admin_user_id = existing
        .and_then(|c| c.admin_user_id)
        .or_else(|| allowed_user_ids.first().copied());

    // Preserve per-project access rules; configure manually in the TOML file.
    let project_access = existing
        .map(|c| c.project_access.clone())
        .unwrap_or_default();

    Ok(TelegramConfig {
        bot_token,
        allowed_user_ids,
        admin_user_id,
        project_access,
    })
}

fn collect_jira(existing: Option<&JiraConfig>) -> Result<JiraConfig, AppError> {
    println!("--- Jira ---");

    let base_url = Text::new("Jira base URL (e.g. https://yourcompany.atlassian.net):")
        .with_initial_value(existing.map(|c| c.base_url.as_str()).unwrap_or(""))
        .with_validator(|v: &str| {
            Ok(validators::validate_jira_base_url(v)
                .map(|e| inquire::validator::Validation::Invalid(e.into()))
                .unwrap_or(inquire::validator::Validation::Valid))
        })
        .prompt()
        .map_err(|e| prompt_err("jira.base_url", e))?;

    let api_token = Text::new("Jira API token:")
        .with_initial_value(existing.map(|c| c.api_token.as_str()).unwrap_or(""))
        .with_validator(|v: &str| {
            Ok(validators::validate_api_token(v)
                .map(|e| inquire::validator::Validation::Invalid(e.into()))
                .unwrap_or(inquire::validator::Validation::Valid))
        })
        .prompt()
        .map_err(|e| prompt_err("jira.api_token", e))?;

    let email = Text::new("Jira account email:")
        .with_initial_value(existing.map(|c| c.email.as_str()).unwrap_or(""))
        .with_validator(|v: &str| {
            Ok(validators::validate_email(v)
                .map(|e| inquire::validator::Validation::Invalid(e.into()))
                .unwrap_or(inquire::validator::Validation::Valid))
        })
        .prompt()
        .map_err(|e| prompt_err("jira.email", e))?;

    // Try to fetch project keys from Jira — fall back to manual entry.
    let project_keys = collect_project_keys(&base_url, &api_token, &email, existing)?;

    Ok(JiraConfig {
        base_url,
        api_token,
        email,
        project_keys,
    })
}

fn collect_project_keys(
    base_url: &str,
    api_token: &str,
    email: &str,
    existing: Option<&JiraConfig>,
) -> Result<Vec<String>, AppError> {
    // Attempt to list projects via the Jira REST API.
    // We do a best-effort blocking HTTP call; if it fails we fall back to
    // manual text entry.
    let fetched = fetch_jira_projects(base_url, api_token, email);

    match fetched {
        Ok(keys) if !keys.is_empty() => {
            println!("  Fetched {} project(s) from Jira.", keys.len());

            let defaults: Vec<usize> = existing
                .map(|c| {
                    c.project_keys
                        .iter()
                        .filter_map(|k| keys.iter().position(|fk| fk == k))
                        .collect()
                })
                .unwrap_or_default();

            let selected = MultiSelect::new("Select project keys to watch:", keys.clone())
                .with_default(&defaults)
                .prompt()
                .map_err(|e| prompt_err("jira.project_keys", e))?;

            Ok(selected)
        }
        _ => {
            // Could not fetch — ask user to type them.
            let default_keys = existing
                .map(|c| c.project_keys.join(", "))
                .unwrap_or_default();

            let raw = Text::new("Jira project keys (comma-separated, e.g. MYPROJ, OTHERPROJ):")
                .with_initial_value(&default_keys)
                .with_validator(|v: &str| {
                    Ok(validators::validate_project_keys(v)
                        .map(|e| inquire::validator::Validation::Invalid(e.into()))
                        .unwrap_or(inquire::validator::Validation::Valid))
                })
                .prompt()
                .map_err(|e| prompt_err("jira.project_keys", e))?;

            let keys = raw
                .split(',')
                .map(|s| s.trim().to_uppercase())
                .filter(|s| !s.is_empty())
                .collect();
            Ok(keys)
        }
    }
}

/// Best-effort synchronous fetch of Jira project keys using `reqwest::blocking`.
fn fetch_jira_projects(
    base_url: &str,
    api_token: &str,
    email: &str,
) -> anyhow::Result<Vec<String>> {
    let url = format!("{}/rest/api/3/project", base_url.trim_end_matches('/'));

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let resp = client
        .get(&url)
        .basic_auth(email, Some(api_token))
        .send()?
        .error_for_status()?;

    let json: serde_json::Value = resp.json()?;
    let keys = json
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p["key"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(keys)
}

fn collect_claude(existing: Option<&ClaudeConfig>) -> Result<ClaudeConfig, AppError> {
    println!("--- Claude ---");

    let binary_path = Text::new("Path to claude binary (e.g. /usr/local/bin/claude):")
        .with_initial_value(existing.map(|c| c.binary_path.as_str()).unwrap_or(""))
        .with_validator(|v: &str| {
            Ok(validators::validate_binary_path(v)
                .map(|e| inquire::validator::Validation::Invalid(e.into()))
                .unwrap_or(inquire::validator::Validation::Valid))
        })
        .prompt()
        .map_err(|e| prompt_err("claude.binary_path", e))?;

    let api_key_default = existing.and_then(|c| c.api_key.as_deref()).unwrap_or("");
    let api_key_raw = Text::new("Anthropic API key (optional, press Enter to skip):")
        .with_initial_value(api_key_default)
        .prompt()
        .map_err(|e| prompt_err("claude.api_key", e))?;

    let api_key = if api_key_raw.trim().is_empty() {
        None
    } else {
        Some(api_key_raw.trim().to_string())
    };

    Ok(ClaudeConfig {
        binary_path,
        api_key,
        timeout_ms: None,
        sandbox: cfg!(target_os = "linux"),
    })
}

fn collect_projects(
    project_keys: &[String],
    existing: Option<&HashMap<String, Vec<String>>>,
) -> Result<HashMap<String, Vec<String>>, AppError> {
    if project_keys.is_empty() {
        return Ok(HashMap::new());
    }

    println!("--- Project paths ---");

    let configure = Confirm::new("Configure local repository paths for project keys?")
        .with_default(existing.is_some())
        .prompt()
        .map_err(|e| prompt_err("projects", e))?;

    if !configure {
        return Ok(existing.cloned().unwrap_or_default());
    }

    let mut projects = HashMap::new();

    for key in project_keys {
        let default_paths = existing
            .and_then(|m| m.get(key.as_str()))
            .map(|v| v.join(", "))
            .unwrap_or_default();

        let raw = Text::new(&format!(
            "Repo paths for {key} (comma-separated absolute dirs):"
        ))
        .with_initial_value(&default_paths)
        .with_validator(|v: &str| {
            if v.trim().is_empty() {
                return Ok(inquire::validator::Validation::Valid);
            }
            Ok(validators::validate_repo_paths(v)
                .map(|e| inquire::validator::Validation::Invalid(e.into()))
                .unwrap_or(inquire::validator::Validation::Valid))
        })
        .prompt()
        .map_err(|e| prompt_err("projects", e))?;

        if !raw.trim().is_empty() {
            let paths: Vec<String> = raw
                .split([',', '\n'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            projects.insert(key.clone(), paths);
        }
    }

    Ok(projects)
}

fn collect_app_settings(existing: Option<&AppSettings>) -> Result<AppSettings, AppError> {
    println!("--- App settings ---");

    let options = vec!["info", "debug", "error"];
    let default_idx = existing
        .map(|s| match s.log_level {
            LogLevel::Info => 0,
            LogLevel::Debug => 1,
            LogLevel::Error => 2,
        })
        .unwrap_or(0);

    let choice = Select::new("Log level:", options)
        .with_starting_cursor(default_idx)
        .prompt()
        .map_err(|e| prompt_err("app.log_level", e))?;

    let log_level = choice.parse::<LogLevel>().unwrap_or(LogLevel::Info);

    Ok(AppSettings { log_level })
}

fn collect_slack(existing: Option<&SlackConfig>) -> Result<Option<SlackConfig>, AppError> {
    println!("--- Slack (optional) ---");

    let configure = Confirm::new("Configure Slack integration?")
        .with_default(existing.is_some())
        .prompt()
        .map_err(|e| prompt_err("slack", e))?;

    if !configure {
        return Ok(None);
    }

    let user_token = Text::new("Slack user token (starts with xoxp-):")
        .with_initial_value(existing.map(|c| c.user_token.as_str()).unwrap_or(""))
        .with_validator(|v: &str| {
            Ok(validators::validate_slack_user_token(v)
                .map(|e| inquire::validator::Validation::Invalid(e.into()))
                .unwrap_or(inquire::validator::Validation::Valid))
        })
        .prompt()
        .map_err(|e| prompt_err("slack.user_token", e))?;

    let poll_interval_default = existing
        .map(|c| c.poll_interval_ms.to_string())
        .unwrap_or_else(|| "30000".to_string());

    let poll_raw = Text::new("Poll interval in milliseconds:")
        .with_initial_value(&poll_interval_default)
        .with_validator(|v: &str| match v.trim().parse::<u64>() {
            Ok(n) if n > 0 => Ok(inquire::validator::Validation::Valid),
            _ => Ok(inquire::validator::Validation::Invalid(
                "Must be a positive integer".into(),
            )),
        })
        .prompt()
        .map_err(|e| prompt_err("slack.poll_interval_ms", e))?;

    let poll_interval_ms = poll_raw.trim().parse::<u64>().unwrap_or(30_000);

    Ok(Some(SlackConfig {
        user_token,
        poll_interval_ms,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_ids(raw: &str) -> Vec<i64> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<i64>().ok())
        .filter(|&n| n > 0)
        .collect()
}

fn prompt_err(field: &str, e: inquire::InquireError) -> AppError {
    AppError::Friendly(crate::shared::errors::FriendlyError::new(format!(
        "Prompt for '{field}' failed: {e}"
    )))
}
