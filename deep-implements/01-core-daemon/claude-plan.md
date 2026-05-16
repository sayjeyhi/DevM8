# Implementation Plan: 01-core-daemon

## What We're Building

The core daemon layer of `jira-assistant` — a macOS CLI tool that manages a background Telegram bot polling process via launchd. This module is self-contained and provides the foundational runtime infrastructure (config loading, logging, process management) on which all other modules depend.

The output is two compiled Bun binaries: `jira-assistant` and `ja` (symlink or alias), distributed via GitHub Releases.

---

## Why This Architecture

**Bun + compiled binary:** Bun's `bun build --compile` produces a standalone executable with no Node.js or Bun runtime dependency on the target machine. The `--bytecode` flag may give faster startup; it is verified in `build.ts` before claiming — ESM-only dependencies (`smol-toml`, `@clack/prompts`) may not be compatible, in which case `--bytecode` is dropped.

**launchd over cron or manual daemonize:** macOS launchd is the OS-native process supervisor. It handles restarts, logging, boot persistence, and user session lifecycle without any custom daemon code. User-level LaunchAgents require no sudo, run under the user's account, and integrate cleanly with macOS.

**smol-toml + Zod:** Bun can import `.toml` files statically, but has no runtime `TOML.parse()` API. smol-toml fills this gap: it's the fastest TOML 1.1.0 parser for JavaScript, TypeScript-native, and ESM-only. Zod provides typed validation with clear error messages when required config fields are missing.

**@clack/prompts for wizard:** The smallest (~2KB) and most Bun-compatible interactive prompt library with TypeScript-native APIs. It handles TTY detection, Ctrl+C gracefully, and supports per-field validation with inline error display.

**citty for subcommand routing:** Zero-dependency CLI builder built on `util.parseArgs`. Supports lazy async subcommand loading (critical for compiled binaries to avoid loading all modules upfront) and automatic `--help`/`--version` generation.

---

## Directory Structure

```
01-core-daemon/
  src/
    index.ts              # Entry point; registers all subcommands
    commands/
      start.ts            # `jira-assistant start`
      stop.ts             # `jira-assistant stop`
      status.ts           # `jira-assistant status`
      config.ts           # `jira-assistant config` (wizard runner)
      daemon.ts           # `jira-assistant daemon` (polling loop entry)
    config/
      schema.ts           # Zod schema + AppConfig type
      loader.ts           # loadConfig(), config file path resolution
      wizard.ts           # Interactive setup wizard (@clack/prompts)
    daemon/
      launchd.ts          # Plist generation + launchctl wrappers
      pid.ts              # PID file read/write/remove
      restart-tracker.ts  # In-process restart counter
    logger/
      index.ts            # createLogger(); TTY-adaptive output
      rotate.ts           # Size-based log rotation (10MB, keep 5)
    shared/
      paths.ts            # All ~/.config/jira-assistant/* path constants
      errors.ts           # FriendlyError class; launchctl error formatter
  tests/
    config/
      loader.test.ts
      wizard.test.ts
    daemon/
      launchd.test.ts
      restart-tracker.test.ts
    logger/
      rotate.test.ts
  build.ts                # Build script: compiles both binaries
  package.json
  tsconfig.json
```

---

## Section 1: Project Setup and Build Pipeline

### 1.1 Package Configuration

`package.json` defines the project with Bun as the runtime. Dependencies: `smol-toml`, `zod`, `@clack/prompts`, `citty`. Dev dependencies: none required (Bun's built-in test runner, TypeScript compiler, and bundler cover all dev needs).

`tsconfig.json` targets `ESNext` module system with `bundler` moduleResolution (Bun's recommended setting). Strict mode enabled.

### 1.2 Build Script

`build.ts` is a Bun script (run with `bun run build.ts`) that:

1. Calls `Bun.build()` with `compile: true`, `minify: true`, `sourcemap: "linked"` targeting `./src/index.ts`. Attempts `bytecode: true` first; if the output binary fails the smoke test, rebuilds without `--bytecode` and prints a warning.
2. Outputs to `./dist/jira-assistant`
3. Creates `./dist/ja` as a copy or symlink of the binary
4. Validates the output binary runs (`./dist/jira-assistant --version`) before exiting — build fails if this exits non-zero.

### 1.3 Path Constants (`shared/paths.ts`)

All file paths are defined as a single exported object to avoid hardcoding paths across multiple files. Paths are resolved to **absolute strings** at module-load time using `os.homedir()` — never using `~` literals, which Bun does not expand. No consumer needs to do any path expansion.

```typescript
import { homedir } from "os"
import { join } from "path"

const home = homedir()

const PATHS = {
  configDir: join(home, ".config/jira-assistant"),
  configFile: join(home, ".config/jira-assistant/config.toml"),
  restartsFile: join(home, ".config/jira-assistant/restarts.json"),
  logsDir: join(home, ".config/jira-assistant/logs"),
  logFile: join(home, ".config/jira-assistant/logs/app.log"),
  pidFile: join(home, ".config/jira-assistant/daemon.pid"),
  plistFile: join(home, "Library/LaunchAgents/net.jira-assistant.plist"),
  launchAgentsDir: join(home, "Library/LaunchAgents"),
}
```

---

## Section 2: Config System

### 2.1 Schema (`config/schema.ts`)

Define the Zod schema for `AppConfig`. The schema validates types, required fields, and formats (URL for `jira.base_url`, email for `jira.email`). `app.log_level` is an enum defaulting to `"info"`. Export the inferred `AppConfig` type.

```typescript
// Type stub only — implementer writes the Zod schema
interface AppConfig {
  telegram: { bot_token: string }
  jira: {
    base_url: string    // validated as URL
    api_token: string
    email: string       // validated as email
    project_key: string
  }
  claude: { binary_path: string }
  app: { log_level: "info" | "debug" | "error" }
}
```

### 2.2 Config Loader (`config/loader.ts`)

```typescript
/** Reads and validates config.toml. Throws FriendlyError if file missing or invalid. */
async function loadConfig(configPath?: string): Promise<AppConfig>

/** Returns true if config file exists and is parseable */
async function configExists(configPath?: string): Promise<boolean>

/** Writes a validated AppConfig object back to config.toml */
async function writeConfig(config: AppConfig, configPath?: string): Promise<void>
```

`loadConfig` reads the file with `Bun.file().text()`, parses with `smol-toml`'s `parse()`, then validates with the Zod schema. On `ZodError`, it throws a `FriendlyError` listing **all** invalid fields (one per line: `field: reason`). On file-not-found, it throws a `FriendlyError` directing the user to run `jira-assistant config`.

`writeConfig` serializes an `AppConfig` object to TOML using smol-toml's `stringify()`, writes to a temp file in the same directory, then atomically `rename()`s to the final path. It creates the config directory first if it doesn't exist. After writing, sets file permissions to `0o600` (user-read/write only) to protect secrets.

### 2.3 Wizard (`config/wizard.ts`)

```typescript
/** Runs the interactive setup wizard. Returns the completed config. */
async function runWizard(existing?: AppConfig): Promise<AppConfig>
```

The wizard uses `@clack/prompts`. It:
1. Guards against non-TTY environments (throws if not interactive)
2. Shows `intro("jira-assistant setup")` 
3. Prompts each field sequentially using the `group()` API for unified Ctrl+C handling. When `existing` is provided, each prompt is pre-filled with the current value.
4. Validates each field inline using the **same validators as the Zod schema** (single source of truth in `schema.ts`):
   - `bot_token`: must match `/^\d+:[A-Za-z0-9_-]{20,}$/`
   - `base_url`: must start with `https://` and parse as URL
   - `email`: must match a standard email regex
   - `api_token`: non-empty
   - `project_key`: must match `/^[A-Z][A-Z0-9_]+$/` (error, not just warning — enforced)
   - `binary_path`: checks file exists via `Bun.file().exists()`; auto-fills from `Bun.which("claude")` if detected
5. Shows `outro("Config saved!")` on completion

The wizard does NOT write the file — it returns the completed `AppConfig`. The caller (`commands/config.ts`) calls `writeConfig`.

---

## Section 3: Logger

### 3.1 Logger Interface and Factory (`logger/index.ts`)

```typescript
interface Logger {
  info(msg: string, meta?: object): void
  error(msg: string, meta?: object): void
  warn(msg: string, meta?: object): void
  debug(msg: string, meta?: object): void
}

/** Creates a logger. Mode is auto-detected from TTY if not specified. */
function createLogger(level: "info" | "debug" | "error", mode?: "tty" | "json"): Logger
```

Mode detection: if `process.stdout.isTTY` is true, default to `"tty"`; otherwise `"json"`.

**TTY mode:** Human-readable, colored output. Format: `[LEVEL] message {meta json if present}`. Uses ANSI colors. Colors are suppressed when any of: `process.env.NO_COLOR` is set, `process.env.CLICOLOR === "0"`, `process.env.TERM === "dumb"`, or `!process.stdout.isTTY`. Debug lines are dimmed; errors are red; warns are yellow.

**JSON mode:** One JSON object per line. Fields: `{ "level": "info", "ts": "2025-01-01T00:00:00Z", "msg": "...", ...meta }`. This is the format launchd captures to `app.log`.

The `level` parameter gates output — `debug` messages are suppressed if level is `info` or `error`.

### 3.2 Log Rotation (`logger/rotate.ts`)

```typescript
/** Rotates logFile if its size exceeds maxBytes. Keeps up to keepCount rotated files. */
async function rotateIfNeeded(logFile: string, maxBytes?: number, keepCount?: number): Promise<void>
```

Defaults: `maxBytes = 10 * 1024 * 1024` (10MB), `keepCount = 5`.

**Important:** The daemon logger writes directly to the log file using Bun's `file().writer()` — the plist does NOT use `StandardOutPath`/`StandardErrorPath` for log capture. This means the daemon owns the file descriptor and can rotate without conflicting with launchd. Launchd stderr still goes to the system log.

Rotation logic: check `Bun.file(logFile).size`; if over limit, flush and close the current writer, rename `app.log` → `app.log.1`, shift `app.log.1` → `app.log.2` up to `keepCount`, delete oldest, then reopen a fresh `app.log` writer.

`rotateIfNeeded` is called by the daemon on startup and periodically (every hour) via `setInterval`.

---

## Section 4: launchd Integration

### 4.1 Plist Generation and launchctl Wrappers (`daemon/launchd.ts`)

```typescript
/** Generates the launchd plist XML string for the given binary path */
function generatePlist(binaryPath: string): string

/** Writes the plist to ~/Library/LaunchAgents/net.jira-assistant.plist */
async function writePlist(binaryPath: string): Promise<void>

/** Loads and enables the LaunchAgent via launchctl */
async function loadAgent(): Promise<void>

/** Unloads the LaunchAgent via launchctl */
async function unloadAgent(): Promise<void>

/** Returns launchctl list status: { running: boolean, pid?: number, exitCode?: number } */
async function agentStatus(): Promise<AgentStatus>
```

`generatePlist` produces the XML with hardcoded absolute paths (no shell variable expansion). Key plist properties:
- `KeepAlive` in **dictionary form**: `{ SuccessfulExit = false; Crashed = true }` — restarts on crash but NOT on clean exit. A `exit(0)` from the daemon actually stops the loop. Required for the RestartTracker "give up" logic to work.
- `ThrottleInterval = 10` — minimum 10 seconds between restart attempts.
- No `StandardOutPath`/`StandardErrorPath` — the daemon owns its log file directly.

`binaryPath` parameter to `writePlist` should be the result of `realpathSync(Bun.argv[0])` from the caller — not `process.execPath` directly, which may resolve unexpectedly.

All `launchctl` calls are made via `Bun.spawn(["launchctl", ...args])`. On non-zero exit, `loadAgent` and `unloadAgent` throw a `LaunchctlError` that contains both the raw stderr string and a human-friendly hint. The hint logic maps common launchctl error patterns to suggestions (e.g., "plist not found" → "run jira-assistant start first").

`agentStatus` tries `launchctl print gui/<uid>/net.jira-assistant` first (macOS 12+); falls back to `launchctl list net.jira-assistant` if that fails. Both are parsed to extract `{ running, pid?, exitCode? }`. Minimum supported macOS version: 12 (Monterey).

### 4.2 PID File (`daemon/pid.ts`)

```typescript
async function writePid(pid: number): Promise<void>
async function readPid(): Promise<number | null>  // null if file missing or unreadable
async function removePid(): Promise<void>
async function isProcessRunning(pid: number): Promise<boolean>
```

`isProcessRunning` sends signal 0 to the PID (`process.kill(pid, 0)`) — this doesn't kill the process but checks if it's alive.

**PID file ownership rule:** Only `daemon.ts` writes the PID file. `start.ts` and `stop.ts` only read it. `writePid` uses atomic write (temp file + rename) to prevent partial reads.

### 4.3 Restart Tracker (`daemon/restart-tracker.ts`)

```typescript
/** Tracks restarts in a sliding time window. Persists to disk so state survives launchd restarts. */
class RestartTracker {
  constructor(filePath: string, maxRestarts: number, windowMs: number)
  async recordRestart(): Promise<boolean>  // returns true if should give up
  async reset(): Promise<void>
}
```

**Critical:** state must be persisted to `PATHS.restartsFile` (`~/.config/jira-assistant/restarts.json`), otherwise every launchd restart creates a fresh in-process tracker starting at zero — the limit can never be exceeded.

On construction, `RestartTracker` reads and parses the file (empty array if missing). `recordRestart` appends the current timestamp, prunes entries older than `windowMs`, writes the updated array back to the file atomically, and returns whether count exceeds `maxRestarts`. Default: `maxRestarts = 10`, `windowMs = 60_000` (1 minute).

When the limit is exceeded, the daemon exits with code `0`. With `KeepAlive = { SuccessfulExit = false }`, launchd will NOT restart it — the process stops cleanly.

---

## Section 5: CLI Commands

### 5.1 Entry Point (`src/index.ts`)

Uses `citty`'s `defineCommand` and `runMain`. Registers subcommands as lazy async imports. The `version` is read from `package.json` (injected at build time via `--define`). Both `jira-assistant` and `ja` share the same entry point.

### 5.2 `start` Command (`commands/start.ts`)

```typescript
async function startCommand(): Promise<void>
```

Flow:
1. `preflight()` — verify running on macOS, `~/Library/LaunchAgents` dir exists (create if missing), `claude` binary at `config.claude.binary_path` is executable
2. Check `configExists()` → if false, `runWizard()` → `writeConfig()`
3. Check `agentStatus()` → if running, call `stopCommand()` first (with a status message)
4. `writePlist(realpathSync(Bun.argv[0]))` — re-generate plist on every start with canonical binary path
5. `loadAgent()`
6. Poll `agentStatus()` every 200ms, up to 5s timeout, until `running == true`. If timeout exceeded, print failure with last exit code and hint.
7. Print success with PID

### 5.3 `stop` Command (`commands/stop.ts`)

```typescript
async function stopCommand(): Promise<void>
```

Flow:
1. `unloadAgent()` (catches and rethrows as friendly error)
2. `removePid()`
3. Print confirmation

### 5.4 `status` Command (`commands/status.ts`)

```typescript
async function statusCommand(): Promise<void>
```

Reads `agentStatus()` and `readPid()`. Loads config (if it exists) to display `jira.base_url` and `jira.project_key`.

Output format:
```
jira-assistant status
  State:       running
  PID:         12345
  Uptime:      2h 14m
  Config:      ~/.config/jira-assistant/config.toml
  Jira URL:    https://myorg.atlassian.net
  Project:     ENG
  Log:         ~/.config/jira-assistant/logs/app.log
```

Uptime is derived from `launchctl print` output (start time) or from PID file mtime as fallback.

### 5.5 `config` Command (`commands/config.ts`)

```typescript
async function configCommand(): Promise<void>
```

Loads existing config (if present) as `existing`. Calls `runWizard(existing)`. On completion, calls `writeConfig(result)`. Prints path where config was written.

### 5.6 `daemon` Command (`commands/daemon.ts`)

```typescript
async function daemonCommand(): Promise<void>
```

This is the long-running entry point used by launchd. It:

1. Creates the logger (auto-detects TTY vs JSON mode)
2. Initializes the RestartTracker
3. Loads and validates config (exits with friendly error if invalid)
4. Writes PID file
5. Creates `shutdownController = new AbortController()`. Sets up `SIGTERM` handler: signals `shutdownController.abort()`, awaits graceful polling loop shutdown, removes PID file, logs shutdown, exits 0. This ensures in-flight Telegram requests complete before exit.
6. Calls `rotateIfNeeded()` on startup
7. Schedules periodic rotation via `setInterval` (every hour)
8. Calls into `02-integration-clients`'s Telegram polling loop — **statically imported** (not a dynamic import; compiled binaries bundle all imports at build time). Passes `shutdownController.signal` to the polling loop so it can stop cleanly on abort.
9. On unhandled crash: calls `restartTracker.recordRestart()`. If limit exceeded → exit 0 (launchd stops due to `SuccessfulExit = false`). Otherwise → re-throw (launchd restarts via `Crashed = true`).

---

## Section 6: Error Handling Strategy

### FriendlyError (`shared/errors.ts`)

```typescript
class FriendlyError extends Error {
  constructor(message: string, hint?: string)
  readonly hint?: string
}

class LaunchctlError extends FriendlyError {
  constructor(stderr: string, hint: string)
  readonly rawOutput: string
}
```

All CLI commands catch `FriendlyError` at the top level and print `message` + `hint` (if present) to stderr before exiting non-zero. `LaunchctlError` additionally prints the raw `launchctl` output in a dimmed block.

The hint formatter for launchctl errors maps known patterns:
- "No such file or directory" → "Make sure you ran `jira-assistant start` first"
- "Operation already in progress" → "Daemon may already be running; check `jira-assistant status`"
- "Permission denied" → "Check file permissions on the plist"

---

## Section 7: Interfaces for Downstream Modules

`02-integration-clients` and `03-command-handlers` import from this module. Exports:

```typescript
// from config/schema.ts
export type { AppConfig }

// from config/loader.ts
export { loadConfig }

// from logger/index.ts
export type { Logger }
export { createLogger }
```

The daemon command imports the Telegram polling loop from `02-integration-clients` as a **static import**. In `bun build --compile`, all imports are bundled at build time — "dynamic import" is not possible at true runtime. The clean architectural boundary is enforced by module design, not by runtime laziness. The polling loop entry point accepts an `AbortSignal` for clean shutdown.

---

## Implementation Order

Build and test in this order (each step's output is used by the next):

1. `shared/paths.ts` and `shared/errors.ts` — pure constants and types, no deps
2. `config/schema.ts` + `config/loader.ts` — config system, testable in isolation
3. `logger/index.ts` + `logger/rotate.ts` — logger, testable in isolation
4. `config/wizard.ts` — depends on schema; requires TTY to run manually
5. `daemon/pid.ts` + `daemon/restart-tracker.ts` — simple file ops and in-memory logic
6. `daemon/launchd.ts` — shell integration (use mocks in tests)
7. `commands/` — wire everything together
8. `src/index.ts` — entry point
9. `build.ts` — end-to-end binary build

---

## Testing Strategy

Use Bun's built-in test runner (`bun test`). Test files live alongside source in `tests/`.

**What to test:**

- `config/loader`: parse valid TOML → returns `AppConfig`; missing required field → throws `FriendlyError` listing all invalid fields; malformed TOML → throws with line info; `writeConfig` sets file permissions to `0o600`; atomic write (temp+rename) on failure leaves original intact
- `config/schema`: each required field validates correctly; `bot_token` regex rejects malformed tokens; `project_key` regex rejects lowercase; `log_level` defaults to `"info"`
- `daemon/restart-tracker`: under limit → returns false; at limit → returns true; old timestamps pruned; state persists across recreated instances (reads from disk); concurrent calls don't corrupt the file
- `daemon/pid.ts`: write/read/remove round-trip; atomic write (temp+rename); `readPid` returns null for missing file; `isProcessRunning` with current PID returns true
- `daemon/launchd.ts`: `generatePlist` produces XML with `KeepAlive` dictionary form (`SuccessfulExit = false; Crashed = true`), no `StandardOutPath`; `loadAgent`/`unloadAgent` mock `Bun.spawn` and verify correct args; `agentStatus` parses `launchctl print` output; error cases throw `LaunchctlError`
- `logger/rotate.ts`: file under limit → no rotation; file over limit → rotated files in correct order; old files deleted at `keepCount`; daemon owns file descriptor (no launchd stdout conflict)
- `shared/paths.ts`: all paths are absolute (no `~` literals); paths use actual home directory

**Integration tests** (marked with `@macos`, skipped in CI on non-macOS):
- Run `jira-assistant config` in a temp directory; verify `config.toml` is written with `0o600` permissions
- Run `jira-assistant start` then `status` then `stop` against a test plist label; verify launchd state

**What NOT to test:** The interactive wizard (requires TTY); the compiled binary itself beyond a smoke test (`./dist/jira-assistant --version`).
