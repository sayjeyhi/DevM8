# TDD Plan: 01-core-daemon

## Testing Approach

New project. Uses **Bun's built-in test runner** (`bun test`). Test files in `tests/` subdirectories mirroring `src/`. No additional testing libraries needed — Bun provides `describe`, `it`, `expect`, `mock`, `spyOn`, `beforeEach`, `afterEach`.

For shell integration tests (launchctl), mock `Bun.spawn` via `spyOn`. Integration tests touching launchd are tagged `@macos` and skipped in CI on non-macOS.

---

## Section 1: Project Setup and Build Pipeline

**Tests to write first:**

Static / smoke:
- Test: `./dist/jira-assistant --version` exits 0 and prints a semver string (run after build)
- Test: `./dist/jira-assistant --help` exits 0 (smoke test entry point routing)
- Test: compiled binary file exists and is executable (permissions check)
- Test: `bytecode` build attempts and falls back gracefully (build script validates output)

`shared/paths.ts`:
- Test: all `PATHS` values are absolute paths (no `~` prefix)
- Test: `PATHS.configDir` contains `os.homedir()` as prefix
- Test: `PATHS.plistFile` contains `Library/LaunchAgents` and ends with `.plist`
- Test: `PATHS.restartsFile` ends with `restarts.json`

---

## Section 2: Config System

**Tests to write first (`bun test`):**

`config/schema.ts`:
- Test: valid `AppConfig` object passes schema validation
- Test: missing `telegram.bot_token` → ZodError on `telegram.bot_token`
- Test: `bot_token` with value `"not-a-token"` → ZodError (fails regex `/^\d+:[A-Za-z0-9_-]{20,}$/`)
- Test: valid bot token `"123456:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef"` → passes
- Test: `jira.project_key = "mykey"` → ZodError (lowercase not allowed)
- Test: `jira.project_key = "MYKEY"` → passes
- Test: `jira.base_url` without `https://` → ZodError
- Test: `app.log_level` defaults to `"info"` when omitted
- Test: `app.log_level = "verbose"` → ZodError (not in enum)

`config/loader.ts`:
- Test: valid TOML file → `loadConfig` returns typed `AppConfig`
- Test: missing required field → throws `FriendlyError` listing all invalid fields (not just first)
- Test: malformed TOML → throws `FriendlyError` with parse error info
- Test: file not found → throws `FriendlyError` mentioning `jira-assistant config`
- Test: `configExists` returns false when file missing, true when present
- Test: `writeConfig` creates config dir if missing, writes valid TOML
- Test: `writeConfig` uses atomic write (temp file + rename) — verify final file exists even if interrupted
- Test: `writeConfig` sets file permissions to `0o600`
- Test: written TOML can be read back with `loadConfig` and produces identical `AppConfig`

---

## Section 3: Logger

**Tests to write first:**

`logger/index.ts`:
- Test: `createLogger` in JSON mode → each log call writes one valid JSON line with `level`, `ts`, `msg` fields
- Test: JSON mode → `meta` object fields are merged into root of log line
- Test: TTY mode with `NO_COLOR` set → output contains no ANSI escape codes
- Test: TTY mode with `CLICOLOR=0` → no ANSI codes
- Test: TTY mode with `TERM=dumb` → no ANSI codes
- Test: TTY mode with `process.stdout.isTTY = false` → no ANSI codes
- Test: `debug` messages suppressed when `level = "info"`
- Test: `debug` messages emitted when `level = "debug"`

`logger/rotate.ts`:
- Test: file size below `maxBytes` → no rotation, original file unchanged
- Test: file size at/above `maxBytes` → `app.log.1` created with original content
- Test: second rotation → `app.log.1` becomes `app.log.2`, new `app.log.1` has previous `app.log` content
- Test: when `keepCount` files exist → oldest file deleted, others shifted
- Test: non-existent log file → no-op (no error)

---

## Section 4: launchd Integration

**Tests to write first:**

`daemon/launchd.ts`:
- Test: `generatePlist("path/to/binary")` returns XML string containing `<key>Label</key><string>net.jira-assistant</string>`
- Test: plist contains `KeepAlive` dictionary with `SuccessfulExit = false` and `Crashed = true` (NOT simple `true`)
- Test: plist contains `<key>ThrottleInterval</key><integer>10</integer>`
- Test: plist contains `ProgramArguments` with binary path and `daemon` subcommand
- Test: plist does NOT contain `StandardOutPath` or `StandardErrorPath` keys
- Test: `writePlist` creates the file at `PATHS.plistFile`
- Test: `loadAgent` calls `Bun.spawn(["launchctl", "load", ...])` with correct plist path (mock Bun.spawn)
- Test: `unloadAgent` calls `Bun.spawn(["launchctl", "unload", ...])` (mock)
- Test: `loadAgent` non-zero exit → throws `LaunchctlError` with raw stderr
- Test: `agentStatus` parses `launchctl print` output for running process (fixture for macOS 12+ format)
- Test: `agentStatus` falls back to `launchctl list` when `print` fails (fixture for fallback format)
- Test: `agentStatus` when not loaded → returns `{ running: false }`

`daemon/pid.ts`:
- Test: `writePid(1234)` → `readPid()` returns `1234`
- Test: `writePid` uses atomic write (temp file exists during write, final file has correct content)
- Test: `readPid()` returns `null` when file missing
- Test: `removePid()` deletes the file; subsequent `readPid()` returns `null`
- Test: `isProcessRunning(process.pid)` returns `true` (current process is running)
- Test: `isProcessRunning(99999999)` returns `false` (non-existent PID)

`daemon/restart-tracker.ts`:
- Test: first `recordRestart()` → returns `false` (under limit), persists `[timestamp]` to file
- Test: `maxRestarts` calls within `windowMs` → final `recordRestart()` returns `true`
- Test: timestamps outside `windowMs` are pruned; pruned-then-restarted is under limit again
- Test: recreating `RestartTracker` pointing to same file reads existing timestamps (state survives process restart)
- Test: state file missing → starts fresh with empty array
- Test: `reset()` clears the persisted file

---

## Section 5: CLI Commands

**Tests to write first (mock all external calls):**

`commands/start.ts`:
- Test: `preflight()` on Linux → throws `FriendlyError` mentioning macOS
- Test: `preflight()` when `~/Library/LaunchAgents` missing → creates it
- Test: `preflight()` when `claude` binary path non-executable → throws `FriendlyError`
- Test: start when no config → triggers wizard flow (mock `runWizard` and `writeConfig`)
- Test: start when already running → calls `stopCommand` first (mock `agentStatus` returning running)
- Test: `writePlist` called with `realpathSync(Bun.argv[0])` (not `process.execPath` directly)
- Test: polls `agentStatus` until running (mock: not-running for first 2 calls, then running)
- Test: times out after 5s if never reaches running state → exits with failure message

`commands/stop.ts`:
- Test: calls `unloadAgent()` then `removePid()`
- Test: `unloadAgent` throws `LaunchctlError` → surfaces friendly error message

`commands/status.ts`:
- Test: running daemon → output contains `running`, `PID`, config info
- Test: stopped daemon → output contains `stopped`
- Test: no config file → output still shows launchd state (config section skipped)

`commands/config.ts`:
- Test: no existing config → runs wizard with no `existing` argument
- Test: existing config → runs wizard pre-filled with existing values (mock `loadConfig`)
- Test: on completion → calls `writeConfig` with wizard result

`commands/daemon.ts`:
- Test: SIGTERM → `shutdownController.abort()` is called (mock polling loop entry)
- Test: unhandled crash under limit → `restartTracker.recordRestart()` called, re-throws
- Test: unhandled crash at limit → `restartTracker.recordRestart()` returns true → `process.exit(0)` called
- Test: startup → `rotateIfNeeded()` called before polling loop

---

## Section 6: Error Handling

**Tests to write first:**

`shared/errors.ts`:
- Test: `FriendlyError` with hint → `error.hint` is accessible
- Test: `LaunchctlError` → `error.rawOutput` contains the stderr string
- Test: top-level error handler catches `FriendlyError` → prints message + hint to stderr, exits non-zero
- Test: known launchctl error pattern "No such file or directory" → hint mentions `jira-assistant start`

---

## Section 7: Interfaces for Downstream Modules

No automated tests. Verify by reading exports:
- `AppConfig` type is exported from `config/schema.ts`
- `loadConfig` is exported from `config/loader.ts`
- `Logger` type and `createLogger` are exported from `logger/index.ts`
- `daemon.ts` imports polling loop entry as static import (not dynamic)
- Polling loop entry accepts `AbortSignal` parameter (shutdown contract)

---

## Integration Smoke Tests (macOS only, `@macos` tag)

- Run `jira-assistant config` in a temp home dir; verify `config.toml` written with `0o600` permissions
- Run `jira-assistant start` → `status` → `stop` against a test-prefixed plist label
- Verify `restarts.json` persists across simulated restarts (write file, recreate tracker, read back)
- Verify compiled binary smoke: `./dist/jira-assistant --version` exits 0 with semver output
