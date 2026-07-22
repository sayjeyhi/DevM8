use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use crate::bot::state::JiraPendingAction;
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender};
use crate::claude::types::AskOptions;
use crate::config::loader::load_config;
use crate::shared::paths::PATHS;

/// Description-drafting instructions used when the project has no
/// `project_ticket_templates` entry configured, or its file can't be read.
const DEFAULT_DESCRIPTION_TEMPLATE: &str = "\
Write a professional description including: brief overview, acceptance criteria as a bullet list, \
and relevant technical notes. Generate it from the title.";

/// Reads the Markdown template file configured for `project_key`, if any.
/// Relative paths resolve against the devm8 config directory.
fn load_description_template(project_key: &str) -> Option<String> {
    let path = load_config(None)
        .ok()?
        .project_ticket_templates
        .get(project_key)?
        .clone();
    let path = std::path::Path::new(&path);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        PATHS.config_dir.join(path)
    };
    std::fs::read_to_string(&resolved).ok().map(|s| s.trim().to_string())
}

fn build_improve_prompt(project_key: &str, title: &str) -> String {
    let template =
        load_description_template(project_key).unwrap_or_else(|| DEFAULT_DESCRIPTION_TEMPLATE.to_string());

    format!(
        "You are a technical project manager improving a Jira ticket.\n\n\
         Project: {project_key}\n\
         Original title: {title}\n\n\
         Your tasks:\n\
         1. Correct and improve the title — fix grammar, spelling, and clarity; keep it concise (under 80 chars).\n\
         2. {template}\n\n\
         Respond in this exact format with nothing else before or after:\n\
         TITLE: <corrected title>\n\n\
         DESCRIPTION:\n\
         <description here>"
    )
}

fn parse_claude_response(response: &str) -> (String, String) {
    let title = response
        .lines()
        .find(|l| l.starts_with("TITLE:"))
        .map(|l| l["TITLE:".len()..].trim().to_string())
        .unwrap_or_default();

    let desc = response
        .find("DESCRIPTION:\n")
        .map(|idx| response[idx + "DESCRIPTION:\n".len()..].trim().to_string())
        .unwrap_or_default();

    (title, desc)
}

/// Step 1: receives the raw title, corrects it with Claude, generates a description
/// suggestion, then shows it to the user with a "Use this" button.
pub async fn handle_create_suggest(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    _user_id: &str,
    project_key: String,
    title: String,
) -> Result<()> {
    if title.is_empty() {
        sender
            .send(
                chat_id,
                "Send the issue title:\n<code>New login page</code>",
            )
            .await?;
        return Ok(());
    }

    let thinking_ref = sender
        .send(chat_id, "Correcting title and generating description...")
        .await?;
    let _typing = sender.start_typing(chat_id);

    let prompt = build_improve_prompt(&project_key, &title);

    let claude_output = match state.ai.ask(&prompt, AskOptions::default()).await {
        Ok((text, _)) => text,
        Err(e) => {
            state.logger.error(
                &format!("create: Claude error: {e}"),
                Some(&json!({ "title": &title })),
            );
            sender
                .edit_text(&thinking_ref, &format!("Claude error: {e}"))
                .await?;
            return Ok(());
        }
    };

    let (corrected_title, suggested_desc) = parse_claude_response(&claude_output);
    let corrected_title = if corrected_title.is_empty() {
        title
    } else {
        corrected_title
    };

    state
        .chat_states
        .entry(chat_id.to_string())
        .or_default()
        .pending_jira_action = Some(JiraPendingAction::CreateDescription(
        project_key,
        corrected_title.clone(),
        suggested_desc.clone(),
    ));

    let keyboard = vec![vec![Button::new(
        "\u{2705} Use this description",
        "jira:create_confirm",
    )]];

    sender
        .edit_with_keyboard(
            &thinking_ref,
            &format!(
                "Corrected title: <b>{}</b>\n\nSuggested description:\n<pre>{}</pre>\n\n\
                 Tap <b>Use this description</b> or send your own below:",
                sender.escape(&corrected_title),
                sender.escape(&suggested_desc)
            ),
            keyboard,
        )
        .await?;

    Ok(())
}

/// Step 2: creates the Jira issue with the final (confirmed or custom) description.
pub async fn handle_create_confirm(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
    project_key: &str,
    title: &str,
    description: &str,
) -> Result<()> {
    state.logger.info(
        "create: creating Jira issue",
        Some(&json!({ "project": project_key, "title": title })),
    );

    let thinking_ref = sender.send(chat_id, "Creating issue...").await?;

    let Some(jira) = state.jira_for_user(user_id).await else {
        sender
            .edit_text(
                &thinking_ref,
                "Please set up your Jira account first. Use /jira \u{2192} My Jira.",
            )
            .await?;
        return Ok(());
    };
    let issue = match jira.create_issue(project_key, title, description).await {
        Ok(issue) => issue,
        Err(e) => {
            state.logger.error(
                &format!("create: Jira error: {e}"),
                Some(&json!({ "project": project_key, "title": title })),
            );
            sender
                .edit_text(&thinking_ref, &format!("Jira error: {e}"))
                .await?;
            return Ok(());
        }
    };

    state.logger.info(
        "create: issue created",
        Some(&json!({ "key": &issue.key, "url": &issue.url })),
    );

    sender
        .edit_text(
            &thinking_ref,
            &format!(
                "Created: <a href=\"{}\">{}</a> \u{2014} {}",
                issue.url, issue.key, issue.summary
            ),
        )
        .await?;

    Ok(())
}
