<!-- PROJECT_CONFIG
runtime: typescript-bun
test_command: bun test
END_PROJECT_CONFIG -->

<!-- SECTION_MANIFEST
section-01-foundation
section-02-config
section-03-logger
section-04-launchd
section-05-cli-commands
section-06-build-integration
END_MANIFEST -->

# Implementation Sections Index: 01-core-daemon

## Dependency Graph

| Section | Depends On | Blocks | Parallelizable |
|---|---|---|---|
| section-01-foundation | — | 02, 03, 04 | Yes (first) |
| section-02-config | 01 | 05 | Yes (with 03, 04) |
| section-03-logger | 01 | 05 | Yes (with 02, 04) |
| section-04-launchd | 01 | 05 | Yes (with 02, 03) |
| section-05-cli-commands | 02, 03, 04 | 06 | No |
| section-06-build-integration | 05 | — | No |

## Execution Order

1. `section-01-foundation` — no dependencies
2. `section-02-config`, `section-03-logger`, `section-04-launchd` — parallel after 01
3. `section-05-cli-commands` — after 02 + 03 + 04
4. `section-06-build-integration` — after 05

## Section Summaries

### section-01-foundation
Project scaffolding: `package.json`, `tsconfig.json`, `shared/paths.ts` (all absolute paths via `os.homedir()`), `shared/errors.ts` (`FriendlyError`, `LaunchctlError`). This is the zero-dependency layer everything else imports. Tests verify path resolution and error class behavior.

### section-02-config
Config system: `config/schema.ts` (Zod schema + `AppConfig` type with shared validators), `config/loader.ts` (`loadConfig`, `configExists`, `writeConfig` — atomic writes, `chmod 600`), `config/wizard.ts` (`runWizard` using `@clack/prompts`). Tests cover schema validation (all fields), file I/O, atomic write, permission setting, and Zod error reporting (all fields, not just first).

### section-03-logger
Logging: `logger/index.ts` (`createLogger`, TTY vs JSON mode, ANSI suppression via `NO_COLOR`/`CLICOLOR=0`/`TERM=dumb`/`!isTTY`, level gating), `logger/rotate.ts` (`rotateIfNeeded` — copy-truncate style, daemon owns file descriptor, no launchd stdout conflict). Tests cover both modes, ANSI suppression conditions, level filtering, rotation trigger/skip, and file shifting.

### section-04-launchd
launchd integration: `daemon/launchd.ts` (`generatePlist` with `KeepAlive` dictionary form `{SuccessfulExit=false; Crashed=true}`, `ThrottleInterval=10`, no `StandardOutPath`; `writePlist`; `loadAgent`/`unloadAgent` via `Bun.spawn`; `agentStatus` using `launchctl print` with fallback to `launchctl list`), `daemon/pid.ts` (atomic write + rename, ownership rules), `daemon/restart-tracker.ts` (persisted to `restarts.json`, survives process restart). Tests mock `Bun.spawn`, verify plist XML structure, cover both macOS status parsing paths, and test tracker persistence.

### section-05-cli-commands
All CLI commands: `src/index.ts` (citty entry point, lazy subcommand registration, version from `--define`), `commands/start.ts` (`preflight()`, stop-if-running, `writePlist(realpathSync(Bun.argv[0]))`, poll status with 5s timeout), `commands/stop.ts`, `commands/status.ts`, `commands/config.ts`, `commands/daemon.ts` (`AbortController` for SIGTERM clean shutdown, `RestartTracker` integration, static import of polling loop, log rotation scheduling). Tests mock all external calls; verify SIGTERM abort flow, restart-limit exit-0 behavior, preflight rejection on Linux.

### section-06-build-integration
Build pipeline and integration tests: `build.ts` (`--bytecode` attempt with smoke-test fallback, `./dist/jira-assistant --version` validation), integration tests tagged `@macos` (config write with `0o600` permissions, start/stop/status launchd lifecycle, restart state persistence across simulated restarts). Smoke test checklist for per-release manual verification.
