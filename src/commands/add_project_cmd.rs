use std::path::Path;

use inquire::Text;

use crate::config::loader::{load_config, write_config};
use crate::daemon::pid::signal_daemon_reload;
use crate::shared::errors::{AppError, FriendlyError};
use crate::shared::paths::expand_tilde;

pub async fn add_project_command(
    path: Option<String>,
    key: Option<String>,
) -> Result<(), AppError> {
    let path_str = match path {
        Some(p) => p,
        None => Text::new("Path to local git repository:")
            .with_validator(|v: &str| {
                if v.trim().is_empty() {
                    return Ok(inquire::validator::Validation::Invalid(
                        "Path cannot be empty".into(),
                    ));
                }
                Ok(inquire::validator::Validation::Valid)
            })
            .prompt()
            .map_err(|e| prompt_err("path", e))?,
    };

    let path_str = expand_tilde(path_str.trim());

    let key_str = match key {
        Some(k) => k.trim().to_string(),
        None => Text::new("Project name (e.g. MYAPP or my-personal-project):")
            .with_validator(|v: &str| {
                if v.trim().is_empty() {
                    return Ok(inquire::validator::Validation::Invalid(
                        "Project name cannot be empty".into(),
                    ));
                }
                Ok(inquire::validator::Validation::Valid)
            })
            .prompt()
            .map_err(|e| prompt_err("project_name", e))?
            .trim()
            .to_string(),
    };

    register_project(&path_str, &key_str)?;
    println!("Added {} to project {}", path_str, key_str);

    Ok(())
}

/// Register `path` under `key` in `[projects]` config, creating the entry if absent.
/// Skips silently if the path is already registered under that key.
pub fn register_project(path: &str, key: &str) -> Result<(), AppError> {
    // Validate it's a git repo
    if !Path::new(path).join(".git").exists() {
        return Err(AppError::Friendly(FriendlyError::with_hint(
            format!("{path} is not a git repository"),
            "Run `git init` or clone a repository first.",
        )));
    }

    let mut config = load_config(None)?;
    let projects = config.projects.get_or_insert_with(Default::default);

    let paths = projects.entry(key.to_string()).or_default();
    if !paths.contains(&path.to_string()) {
        paths.push(path.to_string());
        write_config(&config, None)?;
        signal_daemon_reload();
    }

    Ok(())
}

fn prompt_err(field: &str, e: inquire::InquireError) -> AppError {
    AppError::Friendly(FriendlyError::new(format!(
        "Prompt for '{field}' failed: {e}"
    )))
}
