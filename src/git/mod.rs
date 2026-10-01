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

    /// The remote's default branch (e.g. "main"), used as the base for worktrees
    /// and ahead/behind bookkeeping. Falls back to "main" when it can't be resolved.
    pub async fn default_branch(&self) -> String {
        self.exec(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
            .await
            .ok()
            .filter(|o| o.exit_code == 0)
            .and_then(|o| o.stdout.strip_prefix("origin/").map(str::to_string))
            .unwrap_or_else(|| "main".into())
    }

    /// Return `true` when the current branch has an upstream configured.
    async fn has_upstream(&self) -> bool {
        self.exec(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
            .await
            .map(|o| o.exit_code == 0)
            .unwrap_or(false)
    }

    /// Return `true` when a local ref (e.g. `refs/remotes/origin/main`) exists.
    async fn ref_exists(&self, refname: &str) -> bool {
        self.exec(&["show-ref", "--verify", "--quiet", refname])
            .await
            .map(|o| o.exit_code == 0)
            .unwrap_or(false)
    }

    /// Number of local commits not yet pushed. Prefers the branch's own
    /// remote-tracking ref (`origin/<branch>`), which `git push` updates, so the
    /// count drops to 0 after a push even while the branch keeps tracking the
    /// base branch it was created from. Falls back to the upstream, then 0.
    pub async fn commits_ahead(&self) -> usize {
        let range = if let Ok(branch) = self.current_branch().await {
            let remote_ref = format!("refs/remotes/origin/{branch}");
            if !branch.is_empty() && self.ref_exists(&remote_ref).await {
                remote_ref
            } else if self.has_upstream().await {
                "@{u}".to_string()
            } else {
                return 0;
            }
        } else {
            return 0;
        };
        let out = self
            .exec(&["rev-list", "--count", &format!("{range}..HEAD")])
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

    /// Fetch a branch from the given remote.
    pub async fn fetch(&self, remote: &str, branch: &str) -> Result<()> {
        let _ = self.exec(&["fetch", remote, branch]).await?;
        Ok(())
    }

    /// Pull with rebase, stashing uncommitted changes first.
    ///
    /// Worktree session branches are based on the remote's default branch and
    /// typically carry Claude's commits plus a dirty tree, so a plain
    /// `git pull` fails with "Need to specify how to reconcile divergent
    /// branches" or refuses outright on uncommitted changes. Rebase keeps the
    /// session commits linear on top of the base branch, and the manual
    /// stash/pop preserves uncommitted work while it runs. If the rebase
    /// conflicts it is aborted so the worktree is left in a clean, usable state.
    /// Falls back to `git pull --rebase <remote> <current-branch>` when no
    /// upstream is configured.
    pub async fn pull(&self, remote: &str) -> Result<String> {
        const AUTOSTASH_MSG: &str = "devm8: pull autostash";
        let dirty = !self.is_clean().await.unwrap_or(true);
        if dirty {
            self.stash(Some(AUTOSTASH_MSG)).await?;
        }
        let fallback_branch = if self.has_upstream().await {
            None
        } else {
            Some(self.current_branch().await?)
        };
        let args: Vec<&str> = match &fallback_branch {
            Some(branch) => vec!["pull", "--rebase", remote, branch],
            None => vec!["pull", "--rebase"],
        };
        let out = self.exec(&args).await?;
        let pulled = if out.exit_code == 0 {
            // Rebase progress is reported on stderr, not stdout.
            let text = if out.stdout.is_empty() {
                out.stderr
            } else {
                out.stdout
            };
            Ok(text)
        } else {
            anyhow::bail!("git {} failed: {}", args.join(" "), out.stderr)
        };
        if pulled.is_err() {
            // Leave the worktree usable instead of stuck mid-rebase; the pull
            // error above stays the primary message.
            let _ = self.exec(&["rebase", "--abort"]).await;
        }
        if dirty {
            if let Err(pop_err) = self.stash_pop().await {
                let note = format!(
                    "uncommitted changes were saved to stash '{AUTOSTASH_MSG}' and could not be \
                     restored automatically ({pop_err}); recover them with `git stash pop`"
                );
                return match pulled {
                    Ok(out) => Ok(format!("{out}\n\nNote: {note}")),
                    Err(e) => Err(anyhow::anyhow!("{e}\n\nNote: {note}")),
                };
            }
        }
        pulled
    }

    /// Push the current branch. Sets the upstream only when none is configured,
    /// so worktree session branches keep tracking the base branch they were
    /// created from — that ref is what the ahead/behind counters and Pull
    /// action measure against.
    pub async fn push(&self, remote: &str) -> Result<()> {
        let branch = self.current_branch().await?;
        if self.has_upstream().await {
            self.run(&["push", remote, &branch]).await?;
        } else {
            self.run(&["push", "--set-upstream", remote, &branch])
                .await?;
        }
        Ok(())
    }

    /// Returns the path for a user-scoped worktree: `<repo>/.worktrees/<user_id>`.
    /// `user_id` is a string so it works for both Telegram numeric IDs and Slack "U…" IDs.
    pub fn worktree_path(&self, user_id: &str) -> PathBuf {
        self.repo_path.join(".worktrees").join(user_id)
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

    /// Create an isolated worktree for `user_id` checked out onto `branch`.
    /// Any previous worktree for the same user is removed first, and any existing local
    /// branch with the same name is force-deleted. The branch starts from the remote's
    /// default branch — or, when the branch already exists on the remote, from that
    /// remote branch, so reusing a name continues the previous work instead of
    /// resetting to a state that `git push` would reject as non-fast-forward.
    /// The worktree is placed at `<repo>/.worktrees/<user_id>/`.
    /// `.worktrees/` is automatically added to the repo's `.gitignore`.
    /// `user_id` accepts both Telegram numeric strings and Slack "U…" IDs.
    pub async fn create_worktree(&self, user_id: &str, branch: &str) -> Result<PathBuf> {
        self.ensure_gitignore_worktrees().await;
        let path = self.worktree_path(user_id);
        if path.exists() {
            let _ = self.remove_worktree(user_id).await;
        }
        let branch_on_remote = self
            .exec(&["ls-remote", "--heads", "origin", branch])
            .await
            .map(|o| o.exit_code == 0 && !o.stdout.is_empty())
            .unwrap_or(false);
        let (fetch_branch, start_point) = if branch_on_remote {
            (branch.to_string(), format!("origin/{branch}"))
        } else {
            let base = self.default_branch().await;
            (base.clone(), format!("origin/{base}"))
        };
        let _ = self.exec(&["fetch", "origin", &fetch_branch]).await;
        let _ = self.exec(&["branch", "-D", branch]).await;
        let path_str = path.to_string_lossy().into_owned();
        self.run(&["worktree", "add", &path_str, "-b", branch, &start_point])
            .await?;
        // Point the branch at its start point so commits_behind/ahead, Pull and
        // Push all agree on what the session branch syncs against.
        let _ = self
            .exec(&["branch", "--set-upstream-to", &start_point, branch])
            .await;
        Ok(path)
    }

    /// Remove the worktree for `user_id` and prune stale worktree metadata.
    pub async fn remove_worktree(&self, user_id: &str) -> Result<()> {
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
