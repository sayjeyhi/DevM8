# Section 05 Code Review Interview

## Reviewed Findings

### AUTO-FIX: SIGTERM race condition (daemon.ts)
`pollingPromise` is `undefined` when SIGTERM fires before startPolling is called. Fix: initialize to `Promise.resolve()` so the await in the SIGTERM handler always resolves instantly if polling hasn't started.

### AUTO-FIX: setInterval handle not stored (daemon.ts)
Hourly rotation interval handle discarded. Fix: store handle, `clearInterval` in SIGTERM handler to prevent timer leaks in tests and after shutdown.

### AUTO-FIX: Stub startPolling ignores AbortSignal (02-integration-clients/src/index.ts)
Stub runs forever even after abort. Fix: resolve when signal fires.

### AUTO-FIX: preflight catch too broad (start.ts)
Bare `catch {}` in preflight swallows ALL config errors (including parse errors, permission errors), not just "config doesn't exist". Fix: only swallow `FriendlyError`.

### AUTO-FIX: Double-hint in stop.ts FriendlyError wrapping
`err.hint` used as both suffix in message and as hint param. Fix: use `err.message` in the message string.

### USER DECISION: Explicit process.exit(1) on crash (daemon.ts)
User chose explicit `process.exit(1)` over implicit unhandled rejection for launchd restart. More deterministic.

### SKIPPED: Dynamic subcommand imports in index.ts
Plan's static import concern is specifically about `startPolling` in daemon.ts (already a static import). Lazy citty subcommands is intentional.

### SKIPPED: Log level type mismatch
AppConfigSchema restricts log_level to `"info" | "debug" | "error"` — matches createLogger signature exactly. False positive.

### SKIPPED: configDir not pre-created in start.ts
writeConfig creates configDir on wizard path; subsequent runs have existing configDir. Edge case of manually deleting configDir is out of scope.
