use std::sync::Arc;

use anyhow::Result;

use crate::bot::utils::parse_first_and_rest;
use crate::bot::AppState;
use crate::channel::ChannelSender;
use crate::commands::add_project_cmd::register_project;

pub async fn handle_add_project(
    sender: Arc<dyn ChannelSender>,
    chat_id: &str,
    _state: Arc<AppState>,
    args: String,
) -> Result<()> {
    let Some((path, project_name)) = parse_first_and_rest(&args) else {
        sender
            .send(
                chat_id,
                "Send the local path and project name:\n\
                 <code>/home/user/my-app MY_APP</code>",
            )
            .await?;
        return Ok(());
    };

    match register_project(&path, &project_name) {
        Ok(()) => {
            sender
                .send(
                    chat_id,
                    &format!(
                        "Registered <code>{}</code> as project <code>{}</code>",
                        sender.escape(&path),
                        sender.escape(&project_name)
                    ),
                )
                .await?;
        }
        Err(e) => {
            sender
                .send(
                    chat_id,
                    &format!("Failed: {}", sender.escape(&e.to_string())),
                )
                .await?;
        }
    }

    Ok(())
}
