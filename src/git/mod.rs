#![allow(dead_code)]

use anyhow::Result;
use std::path::PathBuf;
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
        Self {
            repo_path: repo_path.into(),
        }
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
        self.run(&["checkout", "-b", branch_name, &start_point])
            .await?;
        Ok(())
    }

    /// Stash all changes (including untracked files), with an optional message.
    pub async fn stash(&self, message: Option<&str>) -> Result<()> {
        if let Some(msg) = message {
            self.run(&["stash", "push", "--include-untracked", "-m", msg])
                .await?;
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

    /// Count of files with uncommitted changes (staged + unstaged).
    pub async fn changed_files_count(&self) -> usize {
        let Ok(out) = self.exec(&["status", "--porcelain"]).await else {
            return 0;
        };
        out.stdout.lines().filter(|l| !l.trim().is_empty()).count()
    }

    /// Number of local commits not yet pushed to the upstream branch.
    /// Returns 0 when no upstream is configured.
    pub async fn commits_ahead(&self) -> usize {
        let out = self
            .exec(&["rev-list", "--count", "@{u}..HEAD"])
            .await
            .unwrap_or_else(|_| GitOutput {
                stdout: "0".into(),
                stderr: String::new(),
                exit_code: 1,
            });
        if out.exit_code == 0 {
            out.stdout.trim().parse().unwrap_or(0)
        } else {
            0
        }
    }

    /// Number of upstream commits not yet pulled locally.
    /// Returns 0 when no upstream is configured or not yet fetched.
    pub async fn commits_behind(&self) -> usize {
        let out = self
            .exec(&["rev-list", "--count", "HEAD..@{u}"])
            .await
            .unwrap_or_else(|_| GitOutput {
                stdout: "0".into(),
                stderr: String::new(),
                exit_code: 1,
            });
        if out.exit_code == 0 {
            out.stdout.trim().parse().unwrap_or(0)
        } else {
            0
        }
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
        self.run(&["push", "--set-upstream", remote, &branch])
            .await?;
        Ok(())
    }

    /// Returns the path for a user-scoped worktree: `<repo>/.worktrees/<user_id>`.
    pub fn worktree_path(&self, user_id: i64) -> PathBuf {
        self.repo_path.join(".worktrees").join(user_id.to_string())
    }

    /// Ensure `.worktrees/` is present in the repo's `.gitignore`.
    /// Appends the entry if missing; creates the file if absent.
    async fn ensure_gitignore_worktrees(&self) {
        use tokio::fs;
        use tokio::io::AsyncWriteExt;

        let gitignore = self.repo_path.join(".gitignore");
        let entry = ".worktrees/";

        let existing = fs::read_to_string(&gitignore).await.unwrap_or_default();
        if existing.lines().any(|l| l.trim() == entry) {
            return;
        }

        let append = if existing.is_empty() || existing.ends_with('\n') {
            format!("{}\n", entry)
        } else {
            format!("\n{}\n", entry)
        };

        if let Ok(mut file) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gitignore)
            .await
        {
            let _ = file.write_all(append.as_bytes()).await;
        }
    }

    /// Create an isolated worktree for `user_id` at `origin/main`.
    /// Any previous worktree for the same user is removed first.
    /// The worktree is placed at `<repo>/.worktrees/<user_id>/`.
    /// `.worktrees/` is automatically added to the repo's `.gitignore`.
    pub async fn create_worktree(&self, user_id: i64) -> Result<PathBuf> {
        self.ensure_gitignore_worktrees().await;
        let path = self.worktree_path(user_id);
        if path.exists() {
            let _ = self.remove_worktree(user_id).await;
        }
        let _ = self.exec(&["fetch", "origin", "main"]).await;
        let branch = format!("session/{}", user_id);
        let _ = self.exec(&["branch", "-D", &branch]).await;
        let path_str = path.to_string_lossy().into_owned();
        self.run(&["worktree", "add", &path_str, "-b", &branch, "origin/main"])
            .await?;
        Ok(path)
    }

    /// Remove the worktree for `user_id` and prune stale worktree metadata.
    pub async fn remove_worktree(&self, user_id: i64) -> Result<()> {
        let path = self.worktree_path(user_id);
        if path.exists() {
            let path_str = path.to_string_lossy().into_owned();
            let _ = self
                .run(&["worktree", "remove", "--force", &path_str])
                .await;
        }
        let _ = self.exec(&["worktree", "prune"]).await;
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
            return Ok(String::from_utf8_lossy(&create_output.stdout)
                .trim()
                .to_string());
        }

        // Fallback: view the existing PR URL.
        let view_output = Command::new("gh")
            .args(["pr", "view", "--json", "url", "--jq", ".url"])
            .current_dir(&self.repo_path)
            .output()
            .await?;

        if view_output.status.success() {
            return Ok(String::from_utf8_lossy(&view_output.stdout)
                .trim()
                .to_string());
        }

        let stderr = String::from_utf8_lossy(&create_output.stderr)
            .trim()
            .to_string();
        anyhow::bail!("gh pr create failed: {}", stderr)
    }
}
