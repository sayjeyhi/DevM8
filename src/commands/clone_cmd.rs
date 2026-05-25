use inquire::Text;
use tokio::process::Command;

use crate::commands::add_project_cmd::register_project;
use crate::shared::errors::{AppError, FriendlyError};
use crate::shared::paths::expand_tilde;

pub async fn clone_command(url: Option<String>, path: Option<String>) -> Result<(), AppError> {
    let ssh_url = match url {
        Some(u) => u,
        None => Text::new("SSH repository URL (e.g. git@github.com:org/repo.git):")
            .with_validator(|v: &str| {
                let v = v.trim();
                if v.is_empty() {
                    return Ok(inquire::validator::Validation::Invalid(
                        "URL cannot be empty".into(),
                    ));
                }
                if !v.starts_with("git@") && !v.starts_with("ssh://") {
                    return Ok(inquire::validator::Validation::Invalid(
                        "Must be an SSH URL (git@host:... or ssh://...)".into(),
                    ));
                }
                Ok(inquire::validator::Validation::Valid)
            })
            .prompt()
            .map_err(|e| prompt_err("url", e))?,
    };

    let repo_name = repo_name_from_url(ssh_url.trim());

    let parent = match path {
        Some(p) => expand_tilde(p.trim()),
        None => {
            let raw = Text::new("Destination directory:")
                .with_validator(|v: &str| {
                    if v.trim().is_empty() {
                        return Ok(inquire::validator::Validation::Invalid(
                            "Path cannot be empty".into(),
                        ));
                    }
                    Ok(inquire::validator::Validation::Valid)
                })
                .prompt()
                .map_err(|e| prompt_err("path", e))?;
            expand_tilde(raw.trim())
        }
    };

    let dest_path = std::path::Path::new(&parent)
        .join(&repo_name)
        .to_string_lossy()
        .into_owned();

    println!("Cloning {} → {}", ssh_url.trim(), dest_path);

    let output = Command::new("git")
        .args(["clone", ssh_url.trim(), &dest_path])
        .output()
        .await
        .map_err(|e| {
            AppError::Friendly(FriendlyError::with_hint(
                format!("Failed to run git: {e}"),
                "Ensure git is installed and accessible in PATH.",
            ))
        })?;

    if output.status.success() {
        println!("Cloned into {dest_path}");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::Friendly(FriendlyError::with_hint(
            format!("git clone failed: {stderr}"),
            "Check that your SSH key is loaded and you have access to the repository.",
        )));
    }

    // Register as a project — default name is the repo name from the URL
    let default_name = repo_name.clone();

    let project_name = Text::new("Project name:")
        .with_initial_value(&default_name)
        .with_validator(|v: &str| {
            if v.trim().is_empty() {
                return Ok(inquire::validator::Validation::Invalid(
                    "Project name cannot be empty".into(),
                ));
            }
            Ok(inquire::validator::Validation::Valid)
        })
        .prompt()
        .map_err(|e| prompt_err("project_name", e))?;

    let project_name = project_name.trim().to_string();
    register_project(&dest_path, &project_name)?;
    println!("Added to projects under {project_name}");

    Ok(())
}

/// Extract the repo name from an SSH URL.
/// `git@github.com:org/my-app.git` → `my-app`
/// `ssh://git@github.com/org/my-app.git` → `my-app`
fn repo_name_from_url(url: &str) -> String {
    url.rsplit(['/', ':'])
        .next()
        .unwrap_or(url)
        .trim_end_matches(".git")
        .to_string()
}

fn prompt_err(field: &str, e: inquire::InquireError) -> AppError {
    AppError::Friendly(FriendlyError::new(format!(
        "Prompt for '{field}' failed: {e}"
    )))
}
