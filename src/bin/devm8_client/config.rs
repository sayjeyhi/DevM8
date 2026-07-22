use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub server: String,
    pub email: String,
    pub token: String,
}

fn credentials_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".config/devm8-client/credentials.toml")
}

pub fn load() -> Result<Credentials> {
    let path = credentials_path();
    let raw = fs::read_to_string(&path)
        .with_context(|| "Not logged in. Run `devm8-client login` first.".to_string())?;
    toml::from_str(&raw).context("credentials file is corrupt — run `devm8-client login` again")
}

/// Write the credentials file atomically with mode 0600, mirroring
/// `config::loader::write_config`'s pattern for the daemon's config.toml.
pub fn save(creds: &Credentials) -> Result<()> {
    let path = credentials_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }

    let toml_str = toml::to_string_pretty(creds)?;
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, &toml_str)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
    }

    fs::rename(&tmp_path, &path)?;
    Ok(())
}

pub fn clear() -> Result<()> {
    let path = credentials_path();
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}
