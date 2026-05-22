#![allow(dead_code)]

use std::path::PathBuf;
use anyhow::Result;
use tokio::process::Command;

/// Raw output of a git subcommand execution.
#[derive(Debug, Clone)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Thin wrapper around `git` CLI for a single repository.
#[derive(Debug)]
pub struct GitClient {
    pub repo_path: PathBuf,
}

impl GitClient {
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self { repo_path: repo_path.into() }
    }

    /// Execute a git command and return the full output (stdout + stderr + exit code).
    pub async fn exec(&self, args: &[&str]) -> Result<GitOutput> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.repo_path)
            .output()
            .await?;
        Ok(GitOutput {
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Execute a git command and return stdout on success, bail on failure.
    async fn run(&self, args: &[&str]) -> Result<String> {
        let out = self.exec(args).await?;
        if out.exit_code == 0 {
            Ok(out.stdout)
        } else {
            anyhow::bail!("git {} failed: {}", args.join(" "), out.stderr)
        }
    }

    /// Return the name of the current branch.
    pub async fn current_branch(&self) -> Result<String> {
        self.run(&["rev-parse", "--abbrev-ref", "HEAD"]).await
    }

    /// Return `true` if the working tree has no uncommitted changes.
    pub async fn is_clean(&self) -> Result<bool> {
        let out = self.run(&["status", "--porcelain"]).await?;
        Ok(out.is_empty())
    }

    /// Return `true` if the repo_path is inside a valid git repository.
    pub async fn is_git_repo(&self) -> Result<bool> {
        let out = self.exec(&["rev-parse", "--git-dir"]).await?;
        Ok(out.exit_code == 0)
    }

    /// Fetch `origin/<base>` and create a new local branch starting from it.
    pub async fn checkout_new_branch_from_main(
        &self,
        branch_name: &str,
        remote: &str,
        base: &str,
    ) -> Result<()> {
        let _ = self.run(&["fetch", remote, base]).await;
        let start_point = format!("{}/{}", remote, base);
        self.run(&["checkout", "-b", branch_name, &start_point]).await?;
        Ok(())
    }

    /// Stash all changes (including untracked files), with an optional message.
    pub async fn stash(&self, message: Option<&str>) -> Result<()> {
        if let Some(msg) = message {
            self.run(&["stash", "push", "--include-untracked", "-m", msg]).await?;
        } else {
            self.run(&["stash", "push", "--include-untracked"]).await?;
        }
        Ok(())
    }

    /// Pop the most recent stash entry.
    pub async fn stash_pop(&self) -> Result<()> {
        self.run(&["stash", "pop"]).await?;
        Ok(())
    }

    /// Return `git diff HEAD --stat` output.
    pub async fn get_diff_stat(&self) -> Result<String> {
        self.run(&["diff", "HEAD", "--stat"]).await
    }

    /// Stage all changes (`git add .`).
    pub async fn stage_all(&self) -> Result<()> {
        self.run(&["add", "."]).await?;
        Ok(())
    }

    /// Create a commit with the given message.
    pub async fn commit(&self, message: &str) -> Result<()> {
        self.run(&["commit", "-m", message]).await?;
        Ok(())
    }

    /// Pull the current branch from the given remote.
    pub async fn pull(&self, remote: &str) -> Result<String> {
        let branch = self.current_branch().await?;
        self.run(&["pull", remote, &branch]).await
    }

    /// Push the current branch and set the upstream.
    pub async fn push(&self, remote: &str) -> Result<()> {
        let branch = self.current_branch().await?;
        self.run(&["push", "--set-upstream", remote, &branch]).await?;
        Ok(())
    }

    /// Create a PR using the `gh` CLI and return its URL.
    /// Falls back to `gh pr view` if the PR already exists.
    pub async fn create_pr(&self) -> Result<String> {
        let create_output = Command::new("gh")
            .args(["pr", "create", "--fill"])
            .current_dir(&self.repo_path)
            .output()
            .await?;

        if create_output.status.success() {
            return Ok(String::from_utf8_lossy(&create_output.stdout).trim().to_string());
        }

        // Fallback: view the existing PR URL.
        let view_output = Command::new("gh")
            .args(["pr", "view", "--json", "url", "--jq", ".url"])
            .current_dir(&self.repo_path)
            .output()
            .await?;

        if view_output.status.success() {
            return Ok(String::from_utf8_lossy(&view_output.stdout).trim().to_string());
        }

        let stderr = String::from_utf8_lossy(&create_output.stderr).trim().to_string();
        anyhow::bail!("gh pr create failed: {}", stderr)
    }
}
