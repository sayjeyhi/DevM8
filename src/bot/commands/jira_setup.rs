use std::sync::Arc;

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, MessageId, ParseMode};

use crate::bot::state::JiraPendingAction;
use crate::bot::AppState;
use crate::config::loader::{load_config, update_user_jira};
use crate::config::schema::UserJiraConfig;
use crate::config::validators::{validate_api_token, validate_email, validate_jira_base_url};

pub async fn handle_jira_setup_start(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
) -> Result<()> {
    if state.has_user_jira(user_id) {
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::callback("🔄 Reconnect", "jira:setup_reconnect"),
                InlineKeyboardButton::callback("🗑 Disconnect", "jira:setup_clear"),
            ],
            vec![InlineKeyboardButton::callback(
                "📋 My Projects",
                "jira:projects",
            )],
            vec![InlineKeyboardButton::callback(
                "⭐ Favorite Statuses",
                "jira:fav_statuses",
            )],
        ]);
        bot.send_message(
            chat_id,
            "You have a personal Jira account connected.\n\
             Reconnect to update credentials, manage your projects, or disconnect.",
        )
        .reply_markup(keyboard)
        .await?;
        return Ok(());
    }

    start_url_step(&bot, chat_id, &state).await
}

pub async fn start_url_step(bot: &Bot, chat_id: ChatId, state: &Arc<AppState>) -> Result<()> {
    state
        .chat_states
        .entry(chat_id.0)
        .or_default()
        .pending_jira_action = Some(JiraPendingAction::JiraSetupUrl);

    bot.send_message(
        chat_id,
        "Enter your Jira base URL:\n<code>https://yourcompany.atlassian.net</code>",
    )
    .parse_mode(ParseMode::Html)
    .await?;

    Ok(())
}

pub async fn handle_jira_setup_input(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
    action: JiraPendingAction,
    text: String,
) -> Result<()> {
    match action {
        JiraPendingAction::JiraSetupUrl => {
            if let Some(err) = validate_jira_base_url(&text) {
                bot.send_message(chat_id, format!("❌ {err}\nPlease enter a valid URL:"))
                    .await?;
                state
                    .chat_states
                    .entry(chat_id.0)
                    .or_default()
                    .pending_jira_action = Some(JiraPendingAction::JiraSetupUrl);
                return Ok(());
            }
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::JiraSetupEmail(text));
            bot.send_message(chat_id, "Enter your Jira account email:")
                .await?;
        }

        JiraPendingAction::JiraSetupEmail(base_url) => {
            if let Some(err) = validate_email(&text) {
                bot.send_message(chat_id, format!("❌ {err}\nPlease enter a valid email:"))
                    .await?;
                state
                    .chat_states
                    .entry(chat_id.0)
                    .or_default()
                    .pending_jira_action = Some(JiraPendingAction::JiraSetupEmail(base_url));
                return Ok(());
            }
            state
                .chat_states
                .entry(chat_id.0)
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::JiraSetupToken(base_url, text));
            bot.send_message(
                chat_id,
                "Enter your Jira API token:\n\
                 <i>Generate one at: Jira → Account settings → Security → API tokens</i>",
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }

        JiraPendingAction::JiraSetupToken(base_url, email) => {
            if let Some(err) = validate_api_token(&text) {
                bot.send_message(chat_id, format!("❌ {err}\nPlease enter your API token:"))
                    .await?;
                state
                    .chat_states
                    .entry(chat_id.0)
                    .or_default()
                    .pending_jira_action = Some(JiraPendingAction::JiraSetupToken(base_url, email));
                return Ok(());
            }

            // Build a temporary client with empty project_keys just to ping + list projects.
            let temp_cfg = UserJiraConfig {
                base_url: base_url.clone(),
                email: email.clone(),
                api_token: text.clone(),
                project_keys: vec![],
                favorite_statuses: vec![],
            };

            let thinking = bot.send_message(chat_id, "Testing connection...").await?;

            let client = match state.set_user_jira(user_id, &temp_cfg) {
                Err(e) => {
                    bot.edit_message_text(
                        chat_id,
                        thinking.id,
                        format!("❌ Failed to build Jira client: {e}"),
                    )
                    .await?;
                    return Ok(());
                }
                Ok(c) => c,
            };

            match client.ping().await {
                Err(e) => {
                    state.remove_user_jira(user_id);
                    bot.edit_message_text(
                        chat_id,
                        thinking.id,
                        format!(
                            "❌ Connection failed: {e}\n\
                             Check your credentials and try again."
                        ),
                    )
                    .await?;
                }
                Ok((name, email_addr)) => {
                    // Fetch available projects for the picker.
                    let projects: Vec<(String, String)> = match client.get_projects().await {
                        Ok(list) => list.into_iter().map(|p| (p.key, p.name)).collect(),
                        Err(_) => vec![],
                    };

                    bot.edit_message_text(
                        chat_id,
                        thinking.id,
                        format!(
                            "✅ Connected as <b>{name}</b> ({email_addr})\n\
                             Select the projects you want to access:",
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await?;

                    if projects.is_empty() {
                        // No projects returned — save with empty keys and finish.
                        let cfg = UserJiraConfig {
                            base_url,
                            email,
                            api_token: text,
                            project_keys: vec![],
                            favorite_statuses: vec![],
                        };
                        if let Err(e) = update_user_jira(user_id, Some(&cfg)) {
                            state.remove_user_jira(user_id);
                            bot.send_message(chat_id, format!("❌ Could not save config: {e}"))
                                .await?;
                            return Ok(());
                        }
                        bot.send_message(
                            chat_id,
                            "No projects found on this Jira instance.\n\
                             You can re-run setup after projects are created.",
                        )
                        .await?;
                        return Ok(());
                    }

                    let selected: Vec<String> = vec![];
                    let keyboard = project_picker_keyboard(&projects, &selected);
                    bot.send_message(chat_id, "Tap to toggle projects, then tap ✓ Done:")
                        .reply_markup(keyboard)
                        .await?;

                    state
                        .chat_states
                        .entry(chat_id.0)
                        .or_default()
                        .pending_jira_action = Some(JiraPendingAction::JiraSetupProjects(
                        base_url, email, text, projects, selected,
                    ));
                }
            }
        }

        _ => {}
    }

    Ok(())
}

pub async fn handle_jira_setup_project_toggle(
    bot: Bot,
    chat_id: ChatId,
    message_id: MessageId,
    state: Arc<AppState>,
    _user_id: i64,
    toggled_key: &str,
) -> Result<()> {
    let current = state
        .chat_states
        .get(&chat_id.0)
        .and_then(|s| s.pending_jira_action.clone());

    if let Some(JiraPendingAction::JiraSetupProjects(
        base_url,
        email,
        api_token,
        projects,
        mut selected,
    )) = current
    {
        if let Some(pos) = selected.iter().position(|k| k == toggled_key) {
            selected.remove(pos);
        } else {
            selected.push(toggled_key.to_string());
        }

        let keyboard = project_picker_keyboard(&projects, &selected);
        let _ = bot
            .edit_message_reply_markup(chat_id, message_id)
            .reply_markup(keyboard)
            .await;

        state
            .chat_states
            .entry(chat_id.0)
            .or_default()
            .pending_jira_action = Some(JiraPendingAction::JiraSetupProjects(
            base_url, email, api_token, projects, selected,
        ));
    }

    Ok(())
}

pub async fn handle_jira_setup_project_done(
    bot: Bot,
    chat_id: ChatId,
    message_id: MessageId,
    state: Arc<AppState>,
    user_id: i64,
) -> Result<()> {
    let current = state
        .chat_states
        .get(&chat_id.0)
        .and_then(|s| s.pending_jira_action.clone());

    let (base_url, email, api_token, selected) = match current {
        Some(JiraPendingAction::JiraSetupProjects(
            base_url,
            email,
            api_token,
            _projects,
            selected,
        )) => (base_url, email, api_token, selected),
        _ => return Ok(()),
    };

    state
        .chat_states
        .entry(chat_id.0)
        .or_default()
        .pending_jira_action = None;

    let cfg = UserJiraConfig {
        base_url,
        email,
        api_token,
        project_keys: selected.clone(),
        favorite_statuses: vec![],
    };

    // Rebuild the in-memory client with the final project_keys.
    if let Err(e) = state.set_user_jira(user_id, &cfg) {
        bot.edit_message_text(
            chat_id,
            message_id,
            format!("❌ Failed to update Jira client: {e}"),
        )
        .await?;
        return Ok(());
    }

    if let Err(e) = update_user_jira(user_id, Some(&cfg)) {
        state.remove_user_jira(user_id);
        bot.edit_message_text(
            chat_id,
            message_id,
            format!("❌ Could not save config: {e}"),
        )
        .await?;
        return Ok(());
    }

    let summary = if selected.is_empty() {
        "all projects (none selected)".to_string()
    } else {
        selected.join(", ")
    };

    bot.edit_message_text(
        chat_id,
        message_id,
        format!("✅ Jira account saved.\nProjects: <b>{summary}</b>"),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    Ok(())
}

pub async fn handle_jira_clear(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
) -> Result<()> {
    state.remove_user_jira(user_id);
    if let Err(e) = update_user_jira(user_id, None) {
        bot.send_message(chat_id, format!("❌ Could not update config: {e}"))
            .await?;
        return Ok(());
    }
    bot.send_message(
        chat_id,
        "🔌 Personal Jira account disconnected. Using the default account.",
    )
    .await?;
    Ok(())
}

fn project_picker_keyboard(
    projects: &[(String, String)],
    selected: &[String],
) -> InlineKeyboardMarkup {
    build_picker_keyboard(
        projects,
        selected,
        "jira:setup_proj_toggle:",
        "jira:setup_proj_done",
    )
}

fn manage_project_picker_keyboard(
    projects: &[(String, String)],
    selected: &[String],
) -> InlineKeyboardMarkup {
    build_picker_keyboard(
        projects,
        selected,
        "jira:manage_proj_toggle:",
        "jira:manage_proj_done",
    )
}

fn build_picker_keyboard(
    projects: &[(String, String)],
    selected: &[String],
    toggle_prefix: &str,
    done_callback: &str,
) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = projects
        .iter()
        .map(|(key, name)| {
            let mark = if selected.contains(key) { "✅" } else { "⬜" };
            vec![InlineKeyboardButton::callback(
                format!("{mark} {name} ({key})"),
                format!("{toggle_prefix}{key}"),
            )]
        })
        .collect();
    rows.push(vec![InlineKeyboardButton::callback(
        "✓ Done",
        done_callback,
    )]);
    InlineKeyboardMarkup::new(rows)
}

pub async fn handle_jira_projects_start(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
) -> Result<()> {
    if !state.has_user_jira(user_id) {
        bot.send_message(
            chat_id,
            "No personal Jira account found. Set one up via 🔧 My Jira first.",
        )
        .await?;
        return Ok(());
    }

    let thinking = bot.send_message(chat_id, "Loading projects...").await?;

    let Some(client) = state.jira_for_user(user_id) else {
        return Ok(());
    };
    let projects: Vec<(String, String)> = match client.get_projects().await {
        Ok(list) => list.into_iter().map(|p| (p.key, p.name)).collect(),
        Err(e) => {
            bot.edit_message_text(
                chat_id,
                thinking.id,
                format!("❌ Could not fetch projects: {e}"),
            )
            .await?;
            return Ok(());
        }
    };

    if projects.is_empty() {
        bot.edit_message_text(
            chat_id,
            thinking.id,
            "No projects found on this Jira instance.",
        )
        .await?;
        return Ok(());
    }

    let current_keys: Vec<String> = client.project_keys().to_vec();

    let keyboard = manage_project_picker_keyboard(&projects, &current_keys);
    bot.edit_message_text(
        chat_id,
        thinking.id,
        "Tap to toggle projects, then tap ✓ Done:",
    )
    .reply_markup(keyboard)
    .await?;

    state
        .chat_states
        .entry(chat_id.0)
        .or_default()
        .pending_jira_action = Some(JiraPendingAction::JiraManageProjects(
        projects,
        current_keys,
    ));

    Ok(())
}

pub async fn handle_jira_manage_project_toggle(
    bot: Bot,
    chat_id: ChatId,
    message_id: MessageId,
    state: Arc<AppState>,
    toggled_key: &str,
) -> Result<()> {
    let current = state
        .chat_states
        .get(&chat_id.0)
        .and_then(|s| s.pending_jira_action.clone());

    if let Some(JiraPendingAction::JiraManageProjects(projects, mut selected)) = current {
        if let Some(pos) = selected.iter().position(|k| k == toggled_key) {
            selected.remove(pos);
        } else {
            selected.push(toggled_key.to_string());
        }

        let keyboard = manage_project_picker_keyboard(&projects, &selected);
        let _ = bot
            .edit_message_reply_markup(chat_id, message_id)
            .reply_markup(keyboard)
            .await;

        state
            .chat_states
            .entry(chat_id.0)
            .or_default()
            .pending_jira_action = Some(JiraPendingAction::JiraManageProjects(projects, selected));
    }

    Ok(())
}

pub async fn handle_jira_manage_project_done(
    bot: Bot,
    chat_id: ChatId,
    message_id: MessageId,
    state: Arc<AppState>,
    user_id: i64,
) -> Result<()> {
    let current = state
        .chat_states
        .get(&chat_id.0)
        .and_then(|s| s.pending_jira_action.clone());

    let selected = match current {
        Some(JiraPendingAction::JiraManageProjects(_, selected)) => selected,
        _ => return Ok(()),
    };

    state
        .chat_states
        .entry(chat_id.0)
        .or_default()
        .pending_jira_action = None;

    let config = match load_config(None) {
        Ok(c) => c,
        Err(e) => {
            bot.edit_message_text(
                chat_id,
                message_id,
                format!("❌ Could not read config: {e}"),
            )
            .await?;
            return Ok(());
        }
    };

    let key = user_id.to_string();
    let existing = match config.user_jira.get(&key) {
        Some(c) => c.clone(),
        None => {
            bot.edit_message_text(chat_id, message_id, "❌ No personal Jira config found.")
                .await?;
            return Ok(());
        }
    };

    let updated = UserJiraConfig {
        project_keys: selected.clone(),
        ..existing
    };

    if let Err(e) = state.set_user_jira(user_id, &updated) {
        bot.edit_message_text(
            chat_id,
            message_id,
            format!("❌ Failed to update Jira client: {e}"),
        )
        .await?;
        return Ok(());
    }

    if let Err(e) = update_user_jira(user_id, Some(&updated)) {
        bot.edit_message_text(
            chat_id,
            message_id,
            format!("❌ Could not save config: {e}"),
        )
        .await?;
        return Ok(());
    }

    let summary = if selected.is_empty() {
        "all projects".to_string()
    } else {
        selected.join(", ")
    };

    bot.edit_message_text(
        chat_id,
        message_id,
        format!("✅ Projects updated.\nActive: <b>{summary}</b>"),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Favorite statuses picker
// ---------------------------------------------------------------------------

fn fav_status_picker_keyboard(
    all_statuses: &[String],
    selected: &[String],
) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = all_statuses
        .iter()
        .map(|name| {
            let mark = if selected.contains(name) {
                "⭐"
            } else {
                "☆"
            };
            vec![InlineKeyboardButton::callback(
                format!("{mark} {name}"),
                format!("jira:fav_status_toggle:{name}"),
            )]
        })
        .collect();
    rows.push(vec![InlineKeyboardButton::callback(
        "✓ Done",
        "jira:fav_status_done",
    )]);
    InlineKeyboardMarkup::new(rows)
}

pub async fn handle_jira_fav_statuses_start(
    bot: Bot,
    chat_id: ChatId,
    state: Arc<AppState>,
    user_id: i64,
) -> Result<()> {
    if !state.has_user_jira(user_id) {
        bot.send_message(
            chat_id,
            "No personal Jira account found. Set one up via 🔧 My Jira first.",
        )
        .await?;
        return Ok(());
    }

    let thinking = bot.send_message(chat_id, "Loading statuses...").await?;

    let Some(client) = state.jira_for_user(user_id) else {
        return Ok(());
    };
    let all_statuses: Vec<String> = match client.get_statuses().await {
        Ok(list) => list.into_iter().map(|s| s.name).collect(),
        Err(e) => {
            bot.edit_message_text(
                chat_id,
                thinking.id,
                format!("❌ Could not fetch statuses: {e}"),
            )
            .await?;
            return Ok(());
        }
    };

    if all_statuses.is_empty() {
        bot.edit_message_text(
            chat_id,
            thinking.id,
            "No statuses found on this Jira instance.",
        )
        .await?;
        return Ok(());
    }

    let current_favorites: Vec<String> = load_config(None)
        .ok()
        .and_then(|c| c.user_jira.get(&user_id.to_string()).cloned())
        .map(|c| c.favorite_statuses)
        .unwrap_or_default();

    let keyboard = fav_status_picker_keyboard(&all_statuses, &current_favorites);
    bot.edit_message_text(
        chat_id,
        thinking.id,
        "Tap to star/unstar statuses shown in the filter picker.\n\
         No selection = show all statuses.",
    )
    .reply_markup(keyboard)
    .await?;

    state
        .chat_states
        .entry(chat_id.0)
        .or_default()
        .pending_jira_action = Some(JiraPendingAction::JiraFavoriteStatuses(
        all_statuses,
        current_favorites,
    ));

    Ok(())
}

pub async fn handle_jira_fav_status_toggle(
    bot: Bot,
    chat_id: ChatId,
    message_id: MessageId,
    state: Arc<AppState>,
    toggled: &str,
) -> Result<()> {
    let current = state
        .chat_states
        .get(&chat_id.0)
        .and_then(|s| s.pending_jira_action.clone());

    if let Some(JiraPendingAction::JiraFavoriteStatuses(all_statuses, mut selected)) = current {
        if let Some(pos) = selected.iter().position(|s| s == toggled) {
            selected.remove(pos);
        } else {
            selected.push(toggled.to_string());
        }

        let keyboard = fav_status_picker_keyboard(&all_statuses, &selected);
        let _ = bot
            .edit_message_reply_markup(chat_id, message_id)
            .reply_markup(keyboard)
            .await;

        state
            .chat_states
            .entry(chat_id.0)
            .or_default()
            .pending_jira_action = Some(JiraPendingAction::JiraFavoriteStatuses(
            all_statuses,
            selected,
        ));
    }

    Ok(())
}

pub async fn handle_jira_fav_status_done(
    bot: Bot,
    chat_id: ChatId,
    message_id: MessageId,
    state: Arc<AppState>,
    user_id: i64,
) -> Result<()> {
    let current = state
        .chat_states
        .get(&chat_id.0)
        .and_then(|s| s.pending_jira_action.clone());

    let selected = match current {
        Some(JiraPendingAction::JiraFavoriteStatuses(_, selected)) => selected,
        _ => return Ok(()),
    };

    state
        .chat_states
        .entry(chat_id.0)
        .or_default()
        .pending_jira_action = None;

    let config = match load_config(None) {
        Ok(c) => c,
        Err(e) => {
            bot.edit_message_text(
                chat_id,
                message_id,
                format!("❌ Could not read config: {e}"),
            )
            .await?;
            return Ok(());
        }
    };

    let key = user_id.to_string();
    let existing = match config.user_jira.get(&key) {
        Some(c) => c.clone(),
        None => {
            bot.edit_message_text(chat_id, message_id, "❌ No personal Jira config found.")
                .await?;
            return Ok(());
        }
    };

    let updated = UserJiraConfig {
        favorite_statuses: selected.clone(),
        ..existing
    };

    if let Err(e) = update_user_jira(user_id, Some(&updated)) {
        bot.edit_message_text(
            chat_id,
            message_id,
            format!("❌ Could not save config: {e}"),
        )
        .await?;
        return Ok(());
    }

    let summary = if selected.is_empty() {
        "all statuses (no filter)".to_string()
    } else {
        selected.join(", ")
    };

    bot.edit_message_text(
        chat_id,
        message_id,
        format!("⭐ Favorite statuses saved.\nShown in filter: <b>{summary}</b>"),
    )
    .parse_mode(ParseMode::Html)
    .await?;

    Ok(())
}
