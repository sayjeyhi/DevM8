use inquire::Text;

use crate::config::loader::{load_config, write_config};
use crate::config::schema::SlackConfig;
use crate::shared::errors::{AppError, FriendlyError};

pub async fn slackmap_command() -> Result<(), AppError> {
    // ------------------------------------------------------------------
    // Load config — exit with a clear error if missing.
    // ------------------------------------------------------------------
    let mut config = load_config(None).map_err(|e| {
        if let AppError::ConfigMissing(_) = &e {
            AppError::Friendly(FriendlyError::with_hint(
                "No config found.".to_string(),
                "Run `devm8 config` first to set up your configuration.".to_string(),
            ))
        } else {
            e
        }
    })?;

    // ------------------------------------------------------------------
    // If Slack is already configured, display the current state.
    // ------------------------------------------------------------------
    if let Some(ref existing) = config.slack {
        println!("Slack is already configured.");
        let preview_len = 8.min(existing.user_token.len());
        println!(
            "  User token: {}…",
            &existing.user_token[..preview_len]
        );
        println!("  Poll interval: {}ms", existing.poll_interval_ms);
        println!();

        // Verify the existing token.
        match slack_auth_test(&existing.user_token).await {
            Ok(identity) => println!("  Connected as: {identity}"),
            Err(e)       => println!("  Warning: token validation failed: {e}"),
        }
        println!();
    }

    // ------------------------------------------------------------------
    // Prompt for user token.
    // ------------------------------------------------------------------
    let default_token = config
        .slack
        .as_ref()
        .map(|s| s.user_token.as_str())
        .unwrap_or("")
        .to_string();

    let user_token = Text::new("Slack user token (starts with xoxp-):")
        .with_initial_value(&default_token)
        .with_validator(|v: &str| {
            if v.starts_with("xoxp-") {
                Ok(inquire::validator::Validation::Valid)
            } else {
                Ok(inquire::validator::Validation::Invalid(
                    "Token must start with 'xoxp-'".into(),
                ))
            }
        })
        .prompt()
        .map_err(|e| {
            AppError::Friendly(FriendlyError::new(format!("Prompt failed: {e}")))
        })?;

    // ------------------------------------------------------------------
    // Validate the token via auth.test.
    // ------------------------------------------------------------------
    let identity = slack_auth_test(&user_token).await.map_err(|e| {
        AppError::Friendly(FriendlyError::with_hint(
            format!("Slack auth.test failed: {e}"),
            "Make sure the token is valid and has the required scopes.".to_string(),
        ))
    })?;

    println!("  Authenticated as: {identity}");

    // ------------------------------------------------------------------
    // Prompt for poll interval (seconds, minimum 5).
    // ------------------------------------------------------------------
    let default_interval_s = config
        .slack
        .as_ref()
        .map(|s| (s.poll_interval_ms / 1000).to_string())
        .unwrap_or_else(|| "30".to_string());

    let interval_raw = Text::new("Poll interval in seconds (minimum 5):")
        .with_initial_value(&default_interval_s)
        .with_validator(|v: &str| match v.trim().parse::<u64>() {
            Ok(n) if n >= 5 => Ok(inquire::validator::Validation::Valid),
            _ => Ok(inquire::validator::Validation::Invalid(
                "Must be an integer >= 5".into(),
            )),
        })
        .prompt()
        .map_err(|e| {
            AppError::Friendly(FriendlyError::new(format!("Prompt failed: {e}")))
        })?;

    let poll_interval_ms = interval_raw.trim().parse::<u64>().unwrap_or(30) * 1000;

    // ------------------------------------------------------------------
    // Save updated config.
    // ------------------------------------------------------------------
    config.slack = Some(SlackConfig {
        user_token,
        poll_interval_ms,
    });

    write_config(&config, None)?;

    println!("\nSlack integration configured.");
    println!("Run `devm8 stop && devm8 start` to apply the new settings.");

    Ok(())
}

// ---------------------------------------------------------------------------
// Slack auth.test
// ---------------------------------------------------------------------------

/// Call the Slack `auth.test` method and return `"<user> @ <team>"`.
async fn slack_auth_test(token: &str) -> anyhow::Result<String> {
    let http = reqwest::Client::new();
    let resp = http
        .post("https://slack.com/api/auth.test")
        .bearer_auth(token)
        .send()
        .await?;

    let val: serde_json::Value = resp.json().await?;
    if !val.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        let err = val
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown_error");
        anyhow::bail!("auth.test returned error: {err}");
    }

    let user = val
        .get("user")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let team = val
        .get("team")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    Ok(format!("{user} @ {team}"))
}
