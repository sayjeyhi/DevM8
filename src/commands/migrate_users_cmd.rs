use inquire::{Confirm, Text};

use crate::config::loader::load_config;
use crate::config::validators::validate_email;
use crate::db::{migrate::backfill_from_config, Db};
use crate::shared::errors::{AppError, FriendlyError};
use crate::shared::paths::PATHS;

/// Interactive one-time (repeatable) migration: attach a real email to every
/// channel identity (Telegram/Slack/Teams user ID) that devm8 has only ever
/// seen as a raw platform ID, and optionally issue a devm8-client pairing code.
pub async fn migrate_users_command() -> Result<(), AppError> {
    let config = load_config(None).map_err(|e| {
        if let AppError::ConfigMissing(_) = &e {
            AppError::Friendly(FriendlyError::with_hint(
                "No config found.".to_string(),
                "Run `devm8 config` first to set up your configuration.".to_string(),
            ))
        } else {
            e
        }
    })?;

    let db = Db::open(&PATHS.db_file).map_err(|e| {
        AppError::Friendly(FriendlyError::new(format!("Failed to open database: {e}")))
    })?;

    backfill_from_config(&db, &config).await.map_err(|e| {
        AppError::Friendly(FriendlyError::new(format!(
            "Migration backfill failed: {e}"
        )))
    })?;

    let placeholders = db.list_placeholder_users().await.map_err(|e| {
        AppError::Friendly(FriendlyError::new(format!("Failed to read database: {e}")))
    })?;

    if placeholders.is_empty() {
        println!("No unmapped channel identities found — every known user already has an email.");
    } else {
        println!(
            "Found {} channel identit{} without a real email attached:\n",
            placeholders.len(),
            if placeholders.len() == 1 { "y" } else { "ies" }
        );

        for (channel, external_id, placeholder_email) in &placeholders {
            println!("  {channel}: {external_id}");
            let email = Text::new(&format!("  Email for this {channel} user (blank to skip):"))
                .prompt_skippable()
                .map_err(|e| AppError::Friendly(FriendlyError::new(format!("Prompt failed: {e}"))))?
                .unwrap_or_default();

            let email = email.trim();
            if email.is_empty() {
                continue;
            }
            if let Some(msg) = validate_email(email) {
                println!("  Skipping — {msg}");
                continue;
            }

            db.rename_or_merge_user_email(placeholder_email, email)
                .await
                .map_err(|e| {
                    AppError::Friendly(FriendlyError::new(format!("Failed to update user: {e}")))
                })?;
            println!("  Mapped {channel}:{external_id} -> {email}\n");
        }
    }

    let issue_code = Confirm::new("Issue a devm8-client pairing code now?")
        .with_default(false)
        .prompt()
        .unwrap_or(false);

    if issue_code {
        let email = Text::new("Email to pair:")
            .prompt()
            .map_err(|e| AppError::Friendly(FriendlyError::new(format!("Prompt failed: {e}"))))?;
        let email = email.trim();
        if let Some(msg) = validate_email(email) {
            return Err(AppError::Friendly(FriendlyError::new(msg)));
        }
        let code = db.create_pairing_code(email, 10).await.map_err(|e| {
            AppError::Friendly(FriendlyError::new(format!(
                "Failed to create pairing code: {e}"
            )))
        })?;
        println!(
            "\nPairing code for {email}: {code}\n\
             Valid for 10 minutes. On the client machine, run:\n\
             \n  devm8-client login --server <your-server-url> --code {code}\n"
        );
    }

    Ok(())
}
