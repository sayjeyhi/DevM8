use std::sync::Arc;

use anyhow::Result;

use crate::bot::state::JiraPendingAction;
use crate::bot::AppState;
use crate::channel::{Button, ChannelSender, Keyboard, SentMessageRef};
use crate::config::loader::{load_config, update_user_jira};
use crate::config::schema::UserJiraConfig;
use crate::config::validators::{validate_api_token, validate_email, validate_jira_base_url};

pub async fn handle_jira_setup_start(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
) -> Result<()> {
    if state.has_user_jira(user_id) {
        let keyboard: Keyboard = vec![
            vec![
                Button::new("\u{1f504} Reconnect", "jira:setup_reconnect"),
                Button::new("\u{1f5d1} Disconnect", "jira:setup_clear"),
            ],
            vec![Button::new("\u{1f4cb} My Projects", "jira:projects")],
            vec![Button::new(
                "\u{2b50} Favorite Statuses",
                "jira:fav_statuses",
            )],
        ];
        sender
            .send_with_keyboard(
                chat_id,
                "You have a personal Jira account connected.\n\
                 Reconnect to update credentials, manage your projects, or disconnect.",
                keyboard,
            )
            .await?;
        return Ok(());
    }

    start_url_step(&sender, chat_id, &state).await
}

pub async fn start_url_step(
    sender: &Arc<dyn ChannelSender>,
    chat_id: &str,
    state: &Arc<AppState>,
) -> Result<()> {
    state
        .chat_states
        .entry(chat_id.to_string())
        .or_default()
        .pending_jira_action = Some(JiraPendingAction::JiraSetupUrl);

    sender
        .send(
            chat_id,
            "Enter your Jira base URL:\n<code>https://yourcompany.atlassian.net</code>",
        )
        .await?;

    Ok(())
}

pub async fn handle_jira_setup_input(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
    action: JiraPendingAction,
    text: String,
) -> Result<()> {
    match action {
        JiraPendingAction::JiraSetupUrl => {
            if let Some(err) = validate_jira_base_url(&text) {
                sender
                    .send(
                        chat_id,
                        &format!("\u{274c} {err}\nPlease enter a valid URL:"),
                    )
                    .await?;
                state
                    .chat_states
                    .entry(chat_id.to_string())
                    .or_default()
                    .pending_jira_action = Some(JiraPendingAction::JiraSetupUrl);
                return Ok(());
            }
            state
                .chat_states
                .entry(chat_id.to_string())
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::JiraSetupEmail(text));
            sender
                .send(chat_id, "Enter your Jira account email:")
                .await?;
        }

        JiraPendingAction::JiraSetupEmail(base_url) => {
            if let Some(err) = validate_email(&text) {
                sender
                    .send(
                        chat_id,
                        &format!("\u{274c} {err}\nPlease enter a valid email:"),
                    )
                    .await?;
                state
                    .chat_states
                    .entry(chat_id.to_string())
                    .or_default()
                    .pending_jira_action = Some(JiraPendingAction::JiraSetupEmail(base_url));
                return Ok(());
            }
            state
                .chat_states
                .entry(chat_id.to_string())
                .or_default()
                .pending_jira_action = Some(JiraPendingAction::JiraSetupToken(base_url, text));
            sender
                .send(
                    chat_id,
                    "Enter your Jira API token:\n\
                     <i>Generate one at: Jira \u{2192} Account settings \u{2192} Security \u{2192} API tokens</i>",
                )
                .await?;
        }

        JiraPendingAction::JiraSetupToken(base_url, email) => {
            if let Some(err) = validate_api_token(&text) {
                sender
                    .send(
                        chat_id,
                        &format!("\u{274c} {err}\nPlease enter your API token:"),
                    )
                    .await?;
                state
                    .chat_states
                    .entry(chat_id.to_string())
                    .or_default()
                    .pending_jira_action = Some(JiraPendingAction::JiraSetupToken(base_url, email));
                return Ok(());
            }

            let temp_cfg = UserJiraConfig {
                base_url: base_url.clone(),
                email: email.clone(),
                api_token: text.clone(),
                project_keys: vec![],
                favorite_statuses: vec![],
            };

            let thinking_ref = sender.send(chat_id, "Testing connection...").await?;

            let client = match state.set_user_jira(user_id, &temp_cfg) {
                Err(e) => {
                    sender
                        .edit_text(
                            &thinking_ref,
                            &format!("\u{274c} Failed to build Jira client: {e}"),
                        )
                        .await?;
                    return Ok(());
                }
                Ok(c) => c,
            };

            match client.ping().await {
                Err(e) => {
                    state.remove_user_jira(user_id);
                    sender
                        .edit_text(
                            &thinking_ref,
                            &format!(
                                "\u{274c} Connection failed: {e}\n\
                                 Check your credentials and try again."
                            ),
                        )
                        .await?;
                }
                Ok((name, email_addr)) => {
                    let projects: Vec<(String, String)> = match client.get_projects().await {
                        Ok(list) => list.into_iter().map(|p| (p.key, p.name)).collect(),
                        Err(_) => vec![],
                    };

                    sender
                        .edit_text(
                            &thinking_ref,
                            &format!(
                                "\u{2705} Connected as <b>{name}</b> ({email_addr})\n\
                                 Select the projects you want to access:",
                            ),
                        )
                        .await?;

                    if projects.is_empty() {
                        let cfg = UserJiraConfig {
                            base_url,
                            email,
                            api_token: text,
                            project_keys: vec![],
                            favorite_statuses: vec![],
                        };
                        let uid_i64 = user_id.parse::<i64>().unwrap_or(0);
                        if let Err(e) = update_user_jira(uid_i64, Some(&cfg)) {
                            state.remove_user_jira(user_id);
                            sender
                                .send(chat_id, &format!("\u{274c} Could not save config: {e}"))
                                .await?;
                            return Ok(());
                        }
                        sender
                            .send(
                                chat_id,
                                "No projects found on this Jira instance.\n\
                                 You can re-run setup after projects are created.",
                            )
                            .await?;
                        return Ok(());
                    }

                    let selected: Vec<String> = vec![];
                    let keyboard = project_picker_keyboard(&projects, &selected);
                    sender
                        .send_with_keyboard(
                            chat_id,
                            "Tap to toggle projects, then tap \u{2713} Done:",
                            keyboard,
                        )
                        .await?;

                    state
                        .chat_states
                        .entry(chat_id.to_string())
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
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    msg_ref: SentMessageRef,
    state: Arc<AppState>,
    _user_id: &str,
    toggled_key: &str,
) -> Result<()> {
    let current = state
        .chat_states
        .get(chat_id)
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
        sender.edit_keyboard(&msg_ref, keyboard).await?;

        state
            .chat_states
            .entry(chat_id.to_string())
            .or_default()
            .pending_jira_action = Some(JiraPendingAction::JiraSetupProjects(
            base_url, email, api_token, projects, selected,
        ));
    }

    Ok(())
}

pub async fn handle_jira_setup_project_done(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    msg_ref: SentMessageRef,
    state: Arc<AppState>,
    user_id: &str,
) -> Result<()> {
    let current = state
        .chat_states
        .get(chat_id)
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
        .entry(chat_id.to_string())
        .or_default()
        .pending_jira_action = None;

    let cfg = UserJiraConfig {
        base_url,
        email,
        api_token,
        project_keys: selected.clone(),
        favorite_statuses: vec![],
    };

    if let Err(e) = state.set_user_jira(user_id, &cfg) {
        sender
            .edit_text(
                &msg_ref,
                &format!("\u{274c} Failed to update Jira client: {e}"),
            )
            .await?;
        return Ok(());
    }

    let uid_i64 = user_id.parse::<i64>().unwrap_or(0);
    if let Err(e) = update_user_jira(uid_i64, Some(&cfg)) {
        state.remove_user_jira(user_id);
        sender
            .edit_text(&msg_ref, &format!("\u{274c} Could not save config: {e}"))
            .await?;
        return Ok(());
    }

    let summary = if selected.is_empty() {
        "all projects (none selected)".to_string()
    } else {
        selected.join(", ")
    };

    sender
        .edit_text(
            &msg_ref,
            &format!("\u{2705} Jira account saved.\nProjects: <b>{summary}</b>"),
        )
        .await?;

    Ok(())
}

pub async fn handle_jira_clear(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
) -> Result<()> {
    state.remove_user_jira(user_id);
    let uid_i64 = user_id.parse::<i64>().unwrap_or(0);
    if let Err(e) = update_user_jira(uid_i64, None) {
        sender
            .send(chat_id, &format!("\u{274c} Could not update config: {e}"))
            .await?;
        return Ok(());
    }
    sender
        .send(
            chat_id,
            "\u{1f50c} Personal Jira account disconnected. Using the default account.",
        )
        .await?;
    Ok(())
}

fn project_picker_keyboard(projects: &[(String, String)], selected: &[String]) -> Vec<Vec<Button>> {
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
) -> Vec<Vec<Button>> {
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
) -> Vec<Vec<Button>> {
    let mut rows: Vec<Vec<Button>> = projects
        .iter()
        .map(|(key, name)| {
            let mark = if selected.contains(key) {
                "\u{2705}"
            } else {
                "\u{2b1c}"
            };
            vec![Button::new(
                format!("{mark} {name} ({key})"),
                format!("{toggle_prefix}{key}"),
            )]
        })
        .collect();
    rows.push(vec![Button::new("\u{2713} Done", done_callback)]);
    rows
}

pub async fn handle_jira_projects_start(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
) -> Result<()> {
    if !state.has_user_jira(user_id) {
        sender
            .send(
                chat_id,
                "No personal Jira account found. Set one up via \u{1f527} My Jira first.",
            )
            .await?;
        return Ok(());
    }

    let thinking_ref = sender.send(chat_id, "Loading projects...").await?;

    let Some(client) = state.jira_for_user(user_id) else {
        return Ok(());
    };
    let projects: Vec<(String, String)> = match client.get_projects().await {
        Ok(list) => list.into_iter().map(|p| (p.key, p.name)).collect(),
        Err(e) => {
            sender
                .edit_text(
                    &thinking_ref,
                    &format!("\u{274c} Could not fetch projects: {e}"),
                )
                .await?;
            return Ok(());
        }
    };

    if projects.is_empty() {
        sender
            .edit_text(&thinking_ref, "No projects found on this Jira instance.")
            .await?;
        return Ok(());
    }

    let current_keys: Vec<String> = client.project_keys().to_vec();

    let keyboard = manage_project_picker_keyboard(&projects, &current_keys);
    sender
        .edit_with_keyboard(
            &thinking_ref,
            "Tap to toggle projects, then tap \u{2713} Done:",
            keyboard,
        )
        .await?;

    state
        .chat_states
        .entry(chat_id.to_string())
        .or_default()
        .pending_jira_action = Some(JiraPendingAction::JiraManageProjects(
        projects,
        current_keys,
    ));

    Ok(())
}

pub async fn handle_jira_manage_project_toggle(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    msg_ref: SentMessageRef,
    state: Arc<AppState>,
    toggled_key: &str,
) -> Result<()> {
    let current = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_jira_action.clone());

    if let Some(JiraPendingAction::JiraManageProjects(projects, mut selected)) = current {
        if let Some(pos) = selected.iter().position(|k| k == toggled_key) {
            selected.remove(pos);
        } else {
            selected.push(toggled_key.to_string());
        }

        let keyboard = manage_project_picker_keyboard(&projects, &selected);
        sender.edit_keyboard(&msg_ref, keyboard).await?;

        state
            .chat_states
            .entry(chat_id.to_string())
            .or_default()
            .pending_jira_action = Some(JiraPendingAction::JiraManageProjects(projects, selected));
    }

    Ok(())
}

pub async fn handle_jira_manage_project_done(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    msg_ref: SentMessageRef,
    state: Arc<AppState>,
    user_id: &str,
) -> Result<()> {
    let current = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_jira_action.clone());

    let selected = match current {
        Some(JiraPendingAction::JiraManageProjects(_, selected)) => selected,
        _ => return Ok(()),
    };

    state
        .chat_states
        .entry(chat_id.to_string())
        .or_default()
        .pending_jira_action = None;

    let config = match load_config(None) {
        Ok(c) => c,
        Err(e) => {
            sender
                .edit_text(&msg_ref, &format!("\u{274c} Could not read config: {e}"))
                .await?;
            return Ok(());
        }
    };

    let existing = match config.user_jira.get(user_id) {
        Some(c) => c.clone(),
        None => {
            sender
                .edit_text(&msg_ref, "\u{274c} No personal Jira config found.")
                .await?;
            return Ok(());
        }
    };

    let updated = UserJiraConfig {
        project_keys: selected.clone(),
        ..existing
    };

    if let Err(e) = state.set_user_jira(user_id, &updated) {
        sender
            .edit_text(
                &msg_ref,
                &format!("\u{274c} Failed to update Jira client: {e}"),
            )
            .await?;
        return Ok(());
    }

    let uid_i64 = user_id.parse::<i64>().unwrap_or(0);
    if let Err(e) = update_user_jira(uid_i64, Some(&updated)) {
        sender
            .edit_text(&msg_ref, &format!("\u{274c} Could not save config: {e}"))
            .await?;
        return Ok(());
    }

    let summary = if selected.is_empty() {
        "all projects".to_string()
    } else {
        selected.join(", ")
    };

    sender
        .edit_text(
            &msg_ref,
            &format!("\u{2705} Projects updated.\nActive: <b>{summary}</b>"),
        )
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Favorite statuses picker
// ---------------------------------------------------------------------------

fn fav_status_picker_keyboard(all_statuses: &[String], selected: &[String]) -> Vec<Vec<Button>> {
    let mut rows: Vec<Vec<Button>> = all_statuses
        .iter()
        .map(|name| {
            let mark = if selected.contains(name) {
                "\u{2b50}"
            } else {
                "\u{2606}"
            };
            vec![Button::new(
                format!("{mark} {name}"),
                format!("jira:fav_status_toggle:{name}"),
            )]
        })
        .collect();
    rows.push(vec![Button::new("\u{2713} Done", "jira:fav_status_done")]);
    rows
}

pub async fn handle_jira_fav_statuses_start(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    state: Arc<AppState>,
    user_id: &str,
) -> Result<()> {
    if !state.has_user_jira(user_id) {
        sender
            .send(
                chat_id,
                "No personal Jira account found. Set one up via \u{1f527} My Jira first.",
            )
            .await?;
        return Ok(());
    }

    let thinking_ref = sender.send(chat_id, "Loading statuses...").await?;

    let Some(client) = state.jira_for_user(user_id) else {
        return Ok(());
    };
    let all_statuses: Vec<String> = match client.get_statuses().await {
        Ok(list) => list.into_iter().map(|s| s.name).collect(),
        Err(e) => {
            sender
                .edit_text(
                    &thinking_ref,
                    &format!("\u{274c} Could not fetch statuses: {e}"),
                )
                .await?;
            return Ok(());
        }
    };

    if all_statuses.is_empty() {
        sender
            .edit_text(&thinking_ref, "No statuses found on this Jira instance.")
            .await?;
        return Ok(());
    }

    let current_favorites: Vec<String> = load_config(None)
        .ok()
        .and_then(|c| c.user_jira.get(user_id).cloned())
        .map(|c| c.favorite_statuses)
        .unwrap_or_default();

    let keyboard = fav_status_picker_keyboard(&all_statuses, &current_favorites);
    sender
        .edit_with_keyboard(
            &thinking_ref,
            "Tap to star/unstar statuses shown in the filter picker.\n\
             No selection = show all statuses.",
            keyboard,
        )
        .await?;

    state
        .chat_states
        .entry(chat_id.to_string())
        .or_default()
        .pending_jira_action = Some(JiraPendingAction::JiraFavoriteStatuses(
        all_statuses,
        current_favorites,
    ));

    Ok(())
}

pub async fn handle_jira_fav_status_toggle(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    msg_ref: SentMessageRef,
    state: Arc<AppState>,
    toggled: &str,
) -> Result<()> {
    let current = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_jira_action.clone());

    if let Some(JiraPendingAction::JiraFavoriteStatuses(all_statuses, mut selected)) = current {
        if let Some(pos) = selected.iter().position(|s| s == toggled) {
            selected.remove(pos);
        } else {
            selected.push(toggled.to_string());
        }

        let keyboard = fav_status_picker_keyboard(&all_statuses, &selected);
        sender.edit_keyboard(&msg_ref, keyboard).await?;

        state
            .chat_states
            .entry(chat_id.to_string())
            .or_default()
            .pending_jira_action = Some(JiraPendingAction::JiraFavoriteStatuses(
            all_statuses,
            selected,
        ));
    }

    Ok(())
}

pub async fn handle_jira_fav_status_done(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    msg_ref: SentMessageRef,
    state: Arc<AppState>,
    user_id: &str,
) -> Result<()> {
    let current = state
        .chat_states
        .get(chat_id)
        .and_then(|s| s.pending_jira_action.clone());

    let selected = match current {
        Some(JiraPendingAction::JiraFavoriteStatuses(_, selected)) => selected,
        _ => return Ok(()),
    };

    state
        .chat_states
        .entry(chat_id.to_string())
        .or_default()
        .pending_jira_action = None;

    let config = match load_config(None) {
        Ok(c) => c,
        Err(e) => {
            sender
                .edit_text(&msg_ref, &format!("\u{274c} Could not read config: {e}"))
                .await?;
            return Ok(());
        }
    };

    let existing = match config.user_jira.get(user_id) {
        Some(c) => c.clone(),
        None => {
            sender
                .edit_text(&msg_ref, "\u{274c} No personal Jira config found.")
                .await?;
            return Ok(());
        }
    };

    let updated = UserJiraConfig {
        favorite_statuses: selected.clone(),
        ..existing
    };

    let uid_i64 = user_id.parse::<i64>().unwrap_or(0);
    if let Err(e) = update_user_jira(uid_i64, Some(&updated)) {
        sender
            .edit_text(&msg_ref, &format!("\u{274c} Could not save config: {e}"))
            .await?;
        return Ok(());
    }

    let summary = if selected.is_empty() {
        "all statuses (no filter)".to_string()
    } else {
        selected.join(", ")
    };

    sender
        .edit_text(
            &msg_ref,
            &format!("\u{2b50} Favorite statuses saved.\nShown in filter: <b>{summary}</b>"),
        )
        .await?;

    Ok(())
}
