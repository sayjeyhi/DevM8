# Opus Review

**Model:** claude-opus-4
**Generated:** 2026-05-14T00:00:00Z

---

# Architectural Review: 01-core-daemon Implementation Plan

## Critical Issues

### 1. `process.execPath` will point to the wrong binary (Section 5.2, step 3)

`writePlist(process.execPath)` is dangerous. When the user runs `jira-assistant start` from `/usr/local/bin/jira-assistant` (a symlink or copy), `process.execPath` may resolve to the actual binary location, but more importantly, this couples the plist to wherever the binary was when `start` was invoked. If the user later moves or upgrades the binary, the plist becomes stale and launchd will fail silently.

**Recommendation:** Resolve a canonical install path (e.g., `/usr/local/bin/jira-assistant`) or `realpath(process.execPath)`, and re-write the plist on every `start`. Document this behavior explicitly. Also consider whether the `ja` symlink could ever be used here — it would cause confusion in plist output.

### 2. `~` expansion is not done by the shell or `Bun.file()` (Section 1.3)

`Bun.file("~/.config/jira-assistant/config.toml")` does NOT expand `~`. The plan says "Paths expand `~` to the real home directory at runtime" but doesn't specify how. If implementers forget to call `os.homedir()` or `Bun.env.HOME`, every path will be wrong and files will be created literally in a `~` directory.

**Recommendation:** Either store paths as resolved absolute strings at module-load time using `os.homedir()`, or expose a helper `resolve(path)` and forbid raw use of the constants. Pick one and enforce it.

### 3. launchd `KeepAlive: true` plus exit-0-to-stop is brittle (Section 4.1, 5.6)

The plan says: "When the limit is exceeded, the daemon should exit with code `0` so launchd stops the loop." This is incorrect for `KeepAlive: true`. With unconditional `KeepAlive`, launchd restarts the process regardless of exit code. You need `KeepAlive` as a dictionary with `SuccessfulExit: false` (only restart on failure), OR use `KeepAlive: false` with `RunAtLoad: true` and handle the restart strategy entirely yourself.

**Recommendation:** Use `KeepAlive = { SuccessfulExit = false }`. Verify this in `generatePlist` tests. Without this fix, the "give up after 10 crashes" logic will not work — launchd will keep restarting forever.

### 4. RestartTracker state is in-process and lost on every restart (Section 4.3)

If launchd restarts the daemon, a new process starts and `RestartTracker` is constructed fresh, with zero recorded restarts. So `recordRestart()` will never observe more than zero entries in the window — the limit can never be exceeded. The "give up after 10 crashes" logic as designed is non-functional.

**Recommendation:** Persist restart timestamps to a file (e.g., `~/.config/jira-assistant/restarts.json`) or rely on launchd's `ThrottleInterval` + a separate watchdog. Alternatively, have launchd's launch agent check a crash-count file and exit immediately if it's full.

### 5. SIGTERM handler in `commands/daemon.ts` does not call back into integration loop (Section 5.6, step 5)

The daemon command says: "Sets up SIGTERM handler: removes PID file, logs shutdown, exits 0." But step 8 calls into `02-integration-clients`'s Telegram polling loop — there is no mechanism described for clean shutdown of the polling loop, in-flight requests, or pending Jira/Claude operations. Abrupt `exit(0)` will drop in-flight work and may leave Telegram polling in inconsistent state (missed `getUpdates` offset commits).

**Recommendation:** Define an `AbortController` or shutdown signal that gets passed into the polling loop. Document the shutdown contract in Section 7 (interfaces for downstream modules).

### 6. No file locking on config writes or PID files (Sections 2.2, 4.2)

`writeConfig` and `writePid` describe simple writes with no atomic-rename or locking. If a user runs `jira-assistant config` while the daemon is also writing (or two `start` invocations race), the config file can be corrupted. Same for the PID file.

**Recommendation:** Write to a temp file then `rename()` atomically. For the PID file specifically, document that only `daemon` writes it, and `start`/`stop` only read it.

## Significant Issues

### 7. `commands/start.ts` race condition (Section 5.2, steps 2–5)

Flow: check status, stop if running, write plist, load agent, wait 1s, check status. The "wait 1 second" is a smell. If launchd's load is slow on a busy machine, you'll get a false "not running" result and report failure. If load is fast and the daemon crashes immediately due to bad config, you'll report success.

**Recommendation:** Poll `agentStatus()` with a timeout (e.g., 5s with 200ms intervals) and require both `running == true` and PID stable across two consecutive checks.

### 8. Config wizard validation is weak (Section 2.3)

- `bot_token: length ≥ 20` is too loose — Telegram bot tokens follow a specific format `\d+:[A-Za-z0-9_-]+`.
- `email: basic regex email check` — no spec given; this needs a concrete regex or the same Zod validator as the schema, otherwise wizard and schema disagree.
- `project_key: warns if not uppercase` — Jira project keys are typically `[A-Z][A-Z0-9_]+`. A warning may be too lenient if the rest of the code assumes uppercase.
- No validation that `base_url` is reachable. Adding a `HEAD` request would catch typos before the daemon starts and fails repeatedly.
- The wizard never validates that the `bot_token` and `api_token` actually work via a test call. Without this, users will only discover bad credentials via daemon crash logs.

**Recommendation:** Share validators between schema and wizard (single source of truth). Optionally do live credential checks with a `--no-verify` escape hatch.

### 9. Secrets in TOML file with no permission enforcement (Section 2.2, 2.3)

`config.toml` contains `bot_token`, `api_token`, `email`, etc. The plan does not require setting `chmod 600` on the config file. On a multi-user macOS machine (or backups, Dropbox-synced home dirs), this is a data exfiltration risk.

**Recommendation:** After `writeConfig`, call `chmod(path, 0o600)`. Same for the logs directory if logs ever contain partial token echoes. Mention `NO_ECHO_TOKENS` policy in the logger section — there's no guidance that the logger must redact token-shaped strings.

### 10. Log rotation race condition (Section 3.2)

`rotateIfNeeded` is called on startup and via `setInterval`. But launchd is the one writing to `app.log` (via stdout redirection in the plist), and the daemon process tries to rotate by renaming it. On macOS, you can rename an open file, but launchd's file descriptor will continue writing to the renamed file (`app.log.1`), and the new `app.log` will be empty until launchd reopens the file (which it doesn't, unless `StandardOutPath` is reset).

**Recommendation:** Either (a) have launchd NOT manage stdout (use a Bun-internal logger that writes directly to a file and handles its own rotation with reopen-after-rotate), or (b) use a logrotate-style copy-and-truncate approach. The current design will silently break rotation. Also clarify which process owns `app.log` — the daemon or launchd.

### 11. `agentStatus` parsing is fragile (Section 4.1)

`launchctl list net.jira-assistant` output format changed between macOS versions (notably between Big Sur and later). Parsing PID and exit code from this output is non-trivial. On newer macOS, `launchctl print gui/<uid>/net.jira-assistant` is the recommended path and outputs structured data.

**Recommendation:** Detect macOS version or try `launchctl print` first, falling back to `launchctl list`. Plan test fixtures for at least two macOS versions. Document minimum macOS version supported.

### 12. No first-run / dependency checks (Section 5)

`jira-assistant start` doesn't verify:
- `claude` binary at `binary_path` is executable
- `launchctl` is available (always true on macOS, but the binary may be run on Linux by accident)
- The plist directory `~/Library/LaunchAgents` exists (it usually does, but in fresh user accounts it sometimes doesn't)

**Recommendation:** Add a `preflight()` step that runs before `loadAgent()`.

### 13. Missing `--config` flag handling consistency (Section 2.2)

`loadConfig`, `configExists`, `writeConfig` all accept an optional `configPath?`, but no command in Section 5 is documented as accepting `--config`. Either expose this on the CLI (`jira-assistant start --config /custom/path`) and remember to forward it into the plist, or remove the parameter from the loader signatures.

### 14. `bun build --compile` + `--bytecode` constraint (Section 1.2)

`--bytecode` only supports CommonJS. ESM-only packages like `smol-toml` and `@clack/prompts` may fail or silently fall back. The plan asserts ~2x startup gain but does not address compatibility.

**Recommendation:** Verify bytecode actually works with these dependencies via a smoke test in `build.ts`. If it does not, drop `--bytecode` and update the README.

### 15. Dynamic import of `02-integration-clients` in a compiled binary (Section 5.6, step 8 and Section 7)

Dynamic imports inside `bun build --compile` are bundled at build time unless they have a literal string. Calling out "imported at runtime (dynamic import)" implies runtime resolution — that won't work in a single-binary compile. The dependency is actually static after compile. Update wording, or accept that you cannot lazy-load this at all.

## Minor Issues and Suggestions

### 16. No `version` upgrade story

The plan does not address what happens when a new version of `jira-assistant` is installed and the user's plist still references the old binary path, or the config schema has changed. Add: schema versioning field in `config.toml` (e.g., `app.config_version`), and a migration path.

### 17. ANSI color handling (Section 3.1)

"reset on `NO_COLOR` env" — also respect `CLICOLOR=0`, `TERM=dumb`, and don't emit ANSI when piped (`!process.stdout.isTTY`). The current text only mentions `NO_COLOR`.

### 18. Uptime calculation fallback is wrong (Section 5.4)

"PID file mtime as fallback" — the PID file mtime is set when written, but it's only written once at daemon startup. Across restarts, it's overwritten, so this works only if you ensure the file is rewritten on every restart, NOT on rotation or any other event. Be explicit. Also handle the case where the PID file does not match the launchctl-reported PID (zombie/stale file).

### 19. Test plan misses key scenarios

- No test for `writeConfig` setting file permissions (related to issue 9)
- No test for plist regeneration when binary path changes
- No test for graceful shutdown via SIGTERM
- No test for concurrent `start` invocations
- No test verifying the bytecode binary actually works (claims in build are not verified)

### 20. Typos / inconsistencies in plan

- Section 1.1 mentions "smol-toml" is "TOML 1.1.0 parser." TOML's current spec is 1.0.0; 1.1.0 is a draft. Confirm before claiming.
- Section 7 says "dynamic import" but the implementation in a compiled binary cannot be truly dynamic — see issue 15.
- Section 4.1 says "ThrottleInterval of 10 seconds" but plist key is exactly that. Make sure the implementer doesn't typo it as `Throttle` or `Interval`.
- `net.jira-assistant.plist` as a label is reasonable; consider reverse-DNS that you actually own (e.g., `com.fasttask.jira-assistant` or similar) so it never collides with anything else.

### 21. Error messages for `loadConfig` only mention "the first invalid field" (Section 2.2)

Zod can return all errors at once. Showing only the first is friendlier for wizards, but the CLI should arguably show all errors so the user can fix them in one wizard run. Otherwise users fix one field, rerun, hit the next error, repeat.

### 22. Missing `uninstall` / cleanup command

There is no documented way to remove the LaunchAgent, config, logs, and PID file in one shot. Users will leave plist files behind when they uninstall via `rm /usr/local/bin/jira-assistant`. Consider adding `jira-assistant uninstall` or document the manual steps in the README.

### 23. No PID/agent identity verification in `stop` (Section 5.3)

If the agent has been re-loaded under a different binary path or a different user accidentally has a plist with the same label, `stop` will tear it down anyway. Verify the loaded plist points to the expected binary before unloading.

## Summary of Top Action Items

1. Fix `KeepAlive` semantics (issue 3) — the plan's "exit 0 to stop" is incompatible with `KeepAlive: true`.
2. Persist `RestartTracker` state across restarts (issue 4) — currently non-functional.
3. Resolve `~` and `process.execPath` correctly (issues 1, 2).
4. Define and implement a clean shutdown contract (issue 5).
5. Atomic file writes and `chmod 600` on config (issues 6, 9).
6. Audit `--bytecode` ESM compatibility (issue 14).
7. Fix log rotation strategy — current design will silently fail if launchd owns the file descriptor (issue 10).
