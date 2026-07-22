use anyhow::Result;

use crate::config::schema::AppConfig;

use super::Db;

/// Backfill `users`/`user_channel_identities` from the existing per-channel
/// `allowed_user_ids`/`admin_user_id` config so chat history is never dropped
/// for an install that predates the email-identity model. Idempotent — safe
/// to call on every daemon startup.
pub async fn backfill_from_config(db: &Db, config: &AppConfig) -> Result<()> {
    let telegram_admin = config.telegram.admin_user_id.map(|id| id.to_string());
    let mut telegram_ids: Vec<String> = config
        .telegram
        .allowed_user_ids
        .iter()
        .map(|id| id.to_string())
        .collect();
    telegram_ids.extend(telegram_admin.clone());

    for id in &telegram_ids {
        let email = db
            .get_or_create_user_for_channel("telegram", id, None)
            .await?;
        if Some(id) == telegram_admin.as_ref() {
            db.set_admin_by_email(&email, true).await?;
        }
    }

    if let Some(slack) = &config.slack {
        let mut slack_ids = slack.allowed_user_ids.clone();
        slack_ids.extend(slack.admin_user_id.clone());
        for id in &slack_ids {
            let email = db.get_or_create_user_for_channel("slack", id, None).await?;
            if Some(id) == slack.admin_user_id.as_ref() {
                db.set_admin_by_email(&email, true).await?;
            }
        }
    }

    if let Some(teams) = &config.teams {
        let mut teams_ids = teams.allowed_user_ids.clone();
        teams_ids.extend(teams.admin_user_id.clone());
        for id in &teams_ids {
            let email = db.get_or_create_user_for_channel("teams", id, None).await?;
            if Some(id) == teams.admin_user_id.as_ref() {
                db.set_admin_by_email(&email, true).await?;
            }
        }
    }

    Ok(())
}
