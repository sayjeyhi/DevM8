# Integration Notes: Opus Review Feedback

## What I'm Integrating and Why

### 1. `~` expansion via `os.homedir()` in `paths.ts` (Issue 2) — CRITICAL
**Integrating.** `Bun.file("~/.config/...")` does NOT expand `~`. The plan promised runtime expansion but didn't specify how. Fix: `paths.ts` calls `os.homedir()` at module-load time and stores fully-resolved absolute paths. All consumers get `string` constants; no consumer needs to expand anything.

### 2. `process.execPath` → `realpath(Bun.argv[0])` (Issue 1) — CRITICAL
**Integrating.** `process.execPath` resolves to wherever the binary was when `start` was invoked — stale after upgrades. Fix: use `realpathSync(Bun.argv[0])` in `start.ts` to get the canonical binary path, and re-write the plist on every `start` call. Document: plist is always regenerated on start.

### 3. `KeepAlive` dictionary form — fixes broken restart-limit logic (Issue 3) — CRITICAL
**Integrating.** `KeepAlive: true` restarts regardless of exit code. The plan's "exit 0 to stop the loop" logic doesn't work with it. Fix: use dictionary form `KeepAlive = { SuccessfulExit = false; Crashed = true }`. This means launchd only restarts on crash (non-zero exit), and a clean `exit(0)` actually stops it. This is consistent with what `04-distribution` already specifies.

### 4. Persist RestartTracker state to a file (Issue 4) — CRITICAL
**Integrating.** In-process tracker is zeroed on every launchd restart — non-functional by design. Fix: persist timestamps to `~/.config/jira-assistant/restarts.json`. On startup, read and prune stale entries. This makes the "give up after 10 crashes in 1 minute" logic actually work.

### 5. `AbortController` for clean SIGTERM shutdown (Issue 5) — CRITICAL
**Integrating.** The SIGTERM handler just `exit(0)`s, dropping in-flight Telegram polling. Fix: define an `AbortController` (`shutdownController`) in `daemon.ts`, pass it into the polling loop entry, and in the SIGTERM handler call `shutdownController.abort()` then await graceful completion. Add this to Section 7 (downstream interface contract).

### 6. Atomic writes + `chmod 600` on config (Issues 6, 9)
**Integrating.** Config contains secrets; other processes could read a partially-written file. Fix: `writeConfig` and `writePid` write to a temp file then `fs.rename()` atomically. After `writeConfig`, set file permissions to `0o600`. Same on first wizard completion. PID file: document that only `daemon.ts` writes it; `start`/`stop` only read it.

### 7. Poll start status with timeout (Issue 7)
**Integrating.** The 1s sleep + check is a race. Fix: poll `agentStatus()` with 200ms interval, 5s timeout, require `running == true`. Report failure clearly if timeout exceeded.

### 8. Proper wizard validation with shared validators (Issue 8)
**Integrating.** Bot token regex `\d+:[A-Za-z0-9_-]+{20,}` (not just length ≥ 20). Jira project key regex `[A-Z][A-Z0-9_]+` (error, not just warn). Share validators between Zod schema and wizard — single source of truth in `schema.ts`.

### 9. Daemon owns log file (fixes log rotation race) (Issue 10)
**Integrating.** If launchd owns the `app.log` file descriptor via `StandardOutPath`, rotation by the process silently breaks (launchd keeps writing to the renamed file). Fix: remove `StandardOutPath`/`StandardErrorPath` from plist. The daemon logger opens the log file directly with `Bun.file().writer()` and manages rotation internally. Stderr from launchd still goes to the system log.

### 10. `launchctl print` for status parsing (Issue 11)
**Integrating.** `launchctl list` output format varies across macOS versions. Fix: try `launchctl print gui/$(id -u)/net.jira-assistant` first (newer macOS); fall back to `launchctl list net.jira-assistant`. Document minimum macOS version: 12 (Monterey).

### 11. `preflight()` before `loadAgent()` (Issue 12)
**Integrating.** Low-cost, high-value. Before loading the agent: verify `claude` binary is executable, create `~/Library/LaunchAgents` if missing, confirm on macOS (not Linux).

### 12. Remove `--bytecode` claim or gate behind verification (Issue 14)
**Integrating.** `--bytecode` requires CommonJS but `smol-toml` and `@clack/prompts` are ESM-only. The plan claims ~2x startup gain from bytecode but this may not work. Fix: `build.ts` smoke-tests the binary after build. If bytecode causes failures, drop it and update the README. Remove the unverified "~2x faster startup" claim.

### 13. Fix "dynamic import" wording (Issue 15)
**Integrating.** In a compiled binary, all imports are statically bundled at build time. "Dynamic import" is misleading. Fix: say "the Telegram polling loop entry is statically imported from `02-integration-clients`." The clean dependency boundary is maintained architecturally — not by runtime laziness.

### 14. ANSI output: respect `CLICOLOR=0` and `TERM=dumb` (Issue 17)
**Integrating.** Add: suppress ANSI color when `process.env.CLICOLOR === "0"`, `process.env.TERM === "dumb"`, or `!process.stdout.isTTY`.

### 15. Show all Zod errors on `loadConfig` failure (Issue 21)
**Integrating.** Show all invalid fields, not just first. Each error on its own line: `field: reason`.

---

## What I'm NOT Integrating and Why

### A. Remove `configPath?` from loader signatures (Issue 13)
**Not integrating.** The optional override is useful for tests and is a trivial internal flexibility. Not exposing it on the CLI is fine. No change needed.

### B. Config schema version + migration path (Issue 16)
**Not integrating.** This is a v2 concern for a personal tool at v0. Adding migration infrastructure before there's anything to migrate from is premature. If the schema changes, users re-run `jira-assistant config`.

### C. `loginctl enable-linger` (this plan's scope)
**Not integrating into this module.** Already covered in `04-distribution`'s `install.sh`. The daemon itself doesn't need to know.

### D. Verify plist identity before `stop` (Issue 23)
**Not integrating.** Personal single-user tool; label collision risk is negligible. The extra preflight complexity isn't worth it.

### E. `jira-assistant uninstall` command (Issue 22)
**Not integrating.** Uninstall is covered by `04-distribution`'s `install.sh --uninstall`. The daemon doesn't need a duplicate.

### F. `HEAD` request to validate Jira URL reachability (Issue 8 partial)
**Not integrating.** Network validation during setup is a nice-to-have but adds latency, error handling, and offline-mode problems. Sticking with format validation only.
