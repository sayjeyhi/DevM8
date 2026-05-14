## Implementation Status: COMPLETE

**Actual files created:**
- `01-core-daemon/src/daemon/pid.ts`
- `01-core-daemon/src/daemon/restart-tracker.ts`
- `01-core-daemon/src/daemon/launchd.ts`
- `01-core-daemon/tests/daemon/pid.test.ts`
- `01-core-daemon/tests/daemon/restart-tracker.test.ts`
- `01-core-daemon/tests/daemon/launchd.test.ts`

**Deviations from plan (with rationale):**
- `writePid`, `readPid`, `removePid`, `writePlist` accept optional `filePath` parameter (user approved — enables simple test isolation without mocking)
- `writePlist` is now atomic (temp+rename) — plan omitted this but review identified it as required for correctness
- `agentStatus` parses `state` and `last exit code` from `launchctl print` output (plan was vague; added for completeness)
- `agentStatus` throws `FriendlyError` if `process.getuid` unavailable instead of falling back to uid=0
- `restart-tracker.ts` uses `>= maxRestarts` (plan said "maxRestarts=10"; reviewer caught off-by-one in original `>` comparison)
- `launchctlHint` generic fallback corrected to `"launchctl exited with a non-zero status"` per plan spec table (section-01 had wrong string)
- `binaryPath` XML-escaped in `generatePlist` for safety

**Tests: 27 pass across 3 files (80 total suite, 0 fail)**

---

# Section 04: launchd Integration

## Overview

This section implements three files that together manage process lifecycle on macOS:

- `src/daemon/launchd.ts` — plist generation and launchctl wrappers
- `src/daemon/pid.ts` — PID file read/write/remove with atomic writes
- `src/daemon/restart-tracker.ts` — persisted sliding-window restart counter

This section can be built in parallel with `section-02-config` and `section-03-logger`, but all three must complete before `section-05-cli-commands` begins.

## Dependencies (must be completed first)

**section-01-foundation** provides:
- `src/shared/paths.ts` — `PATHS` object with all absolute file path constants
- `src/shared/errors.ts` — `FriendlyError` and `LaunchctlError` classes

Do not duplicate those definitions here. Import from them directly.

## Files to Create

```
01-core-daemon/
  src/
    daemon/
      launchd.ts
      pid.ts
      restart-tracker.ts
  tests/
    daemon/
      launchd.test.ts
      pid.test.ts
      restart-tracker.test.ts
```

All file paths below are relative to the `01-core-daemon/` project root.

---

## Tests First

Test files use Bun's built-in test runner. Run with `bun test`. No additional libraries needed — Bun provides `describe`, `it`, `expect`, `mock`, `spyOn`, `beforeEach`, `afterEach`.

### `tests/daemon/launchd.test.ts`

```typescript
import { describe, it, expect, spyOn, beforeEach, afterEach } from "bun:test"
import { generatePlist, writePlist, loadAgent, unloadAgent, agentStatus } from "../../src/daemon/launchd"
import { PATHS } from "../../src/shared/paths"

describe("generatePlist", () => {
  it("contains the correct Label key", () => { /* ... */ })
  it("contains KeepAlive as a dictionary with SuccessfulExit=false and Crashed=true (not simple boolean true)", () => { /* ... */ })
  it("contains ThrottleInterval of 10", () => { /* ... */ })
  it("contains ProgramArguments with binary path and 'daemon' subcommand", () => { /* ... */ })
  it("does NOT contain StandardOutPath key", () => { /* ... */ })
  it("does NOT contain StandardErrorPath key", () => { /* ... */ })
})

describe("writePlist", () => {
  it("creates the plist file at PATHS.plistFile", async () => { /* ... */ })
})

describe("loadAgent", () => {
  it("calls Bun.spawn with ['launchctl', 'load', PATHS.plistFile]", async () => { /* mock Bun.spawn */ })
  it("throws LaunchctlError containing raw stderr when launchctl exits non-zero", async () => { /* ... */ })
})

describe("unloadAgent", () => {
  it("calls Bun.spawn with ['launchctl', 'unload', PATHS.plistFile]", async () => { /* mock Bun.spawn */ })
  it("throws LaunchctlError on non-zero exit", async () => { /* ... */ })
})

describe("agentStatus", () => {
  it("parses running process from launchctl print output (macOS 12+ format)", async () => { /* fixture */ })
  it("falls back to launchctl list when print fails", async () => { /* fixture */ })
  it("returns { running: false } when agent is not loaded", async () => { /* ... */ })
})
```

Key assertions for plist structure (these are strict XML content checks, not just substring):

- Label must be `net.jira-assistant`
- `KeepAlive` must be the **dictionary form**, not `<true/>`. The generated XML must contain both `<key>SuccessfulExit</key><false/>` and `<key>Crashed</key><true/>` nested inside the `KeepAlive` dict.
- `ThrottleInterval` must be the integer `10`
- `ProgramArguments` array must contain the provided binary path as first element and the string `daemon` as the second element
- Neither `StandardOutPath` nor `StandardErrorPath` keys may appear anywhere in the output

### `tests/daemon/pid.test.ts`

```typescript
import { describe, it, expect, beforeEach, afterEach } from "bun:test"
import { writePid, readPid, removePid, isProcessRunning } from "../../src/daemon/pid"

// Use a temp directory for all file operations; never touch PATHS.pidFile directly in tests

describe("writePid / readPid", () => {
  it("round-trips: writePid(1234) then readPid() returns 1234", async () => { /* ... */ })
  it("uses atomic write (temp file + rename)", async () => { /* ... */ })
})

describe("readPid", () => {
  it("returns null when file is missing", async () => { /* ... */ })
})

describe("removePid", () => {
  it("deletes the file; subsequent readPid() returns null", async () => { /* ... */ })
})

describe("isProcessRunning", () => {
  it("returns true for the current process PID", async () => { /* process.pid */ })
  it("returns false for a non-existent PID like 99999999", async () => { /* ... */ })
})
```

Tests must use a temporary file path (e.g. created with `Bun.file(tmpdir + "/test.pid")`), never the real `PATHS.pidFile`, to avoid side effects.

### `tests/daemon/restart-tracker.test.ts`

```typescript
import { describe, it, expect, beforeEach, afterEach } from "bun:test"
import { RestartTracker } from "../../src/daemon/restart-tracker"

// Each test gets its own temp file path

describe("RestartTracker", () => {
  it("first recordRestart() returns false (under limit)", async () => { /* ... */ })
  it("reaching maxRestarts within windowMs returns true on final call", async () => { /* ... */ })
  it("timestamps outside windowMs are pruned; pruned count goes back under limit", async () => { /* ... */ })
  it("recreating tracker pointing to same file reads persisted timestamps", async () => { /* ... */ })
  it("starts with empty array when state file is missing", async () => { /* ... */ })
  it("reset() clears the persisted file", async () => { /* ... */ })
})
```

The persistence test (fourth case above) is critical: it creates a `RestartTracker`, calls `recordRestart()` several times, then creates a **new** `RestartTracker` instance pointing to the same file path, and verifies the restart count continues correctly — it does not restart from zero.

---

## Implementation Details

### `src/daemon/launchd.ts`

#### Types

```typescript
interface AgentStatus {
  running: boolean
  pid?: number
  exitCode?: number
}
```

#### Function stubs

```typescript
/** Generates the launchd plist XML string for the given binary path. */
function generatePlist(binaryPath: string): string

/** Writes the plist to ~/Library/LaunchAgents/net.jira-assistant.plist */
async function writePlist(binaryPath: string): Promise<void>

/** Loads and enables the LaunchAgent via launchctl bootstrap. */
async function loadAgent(): Promise<void>

/** Unloads the LaunchAgent via launchctl. */
async function unloadAgent(): Promise<void>

/** Returns launchctl list status for the agent. */
async function agentStatus(): Promise<AgentStatus>
```

#### `generatePlist` — required XML structure

The plist label is hardcoded as `net.jira-assistant`. All paths in the plist must be **absolute** — no shell variable expansion (`$HOME`, `~`) occurs inside plists. The plist XML must follow this structure for `KeepAlive`:

```xml
<key>KeepAlive</key>
<dict>
    <key>SuccessfulExit</key>
    <false/>
    <key>Crashed</key>
    <true/>
</dict>
```

Not the simpler `<key>KeepAlive</key><true/>`. This distinction is load-bearing: a clean `exit(0)` from the daemon must cause launchd to stop restarting it (the `RestartTracker` "give up" mechanism depends on this).

The `ThrottleInterval` key limits restart frequency to once per 10 seconds minimum, preventing tight crash loops from hammering the system:

```xml
<key>ThrottleInterval</key>
<integer>10</integer>
```

The `ProgramArguments` array must be:
```xml
<key>ProgramArguments</key>
<array>
    <string>/absolute/path/to/jira-assistant</string>
    <string>daemon</string>
</array>
```

There must be no `StandardOutPath` or `StandardErrorPath` keys. The daemon owns its log file directly via `Bun.file().writer()` (implemented in section-03-logger). Launchd stdout/stderr go to the system log, which is acceptable.

#### `loadAgent` and `unloadAgent` — launchctl invocation

Both functions use `Bun.spawn(["launchctl", ...args])` to invoke launchctl as a subprocess. The specific invocations:

- Load: `launchctl load -w <PATHS.plistFile>` (the `-w` flag persists the enabled state across reboots)
- Unload: `launchctl unload -w <PATHS.plistFile>`

On non-zero exit, read stderr from the process, then throw a `LaunchctlError` (imported from `shared/errors.ts`) constructed with the raw stderr string and a human-friendly hint. The hint is derived by matching the stderr against known patterns:

| stderr contains | hint |
|---|---|
| `"No such file or directory"` | `"Make sure you ran \`jira-assistant start\` first"` |
| `"Operation already in progress"` | `"Daemon may already be running; check \`jira-assistant status\`"` |
| `"Permission denied"` | `"Check file permissions on the plist"` |
| (no match) | Generic: `"launchctl exited with a non-zero status"` |

#### `agentStatus` — parsing launchctl output

First, attempt `launchctl print gui/<uid>/net.jira-assistant` where `<uid>` is `process.getuid()`. This is the macOS 12+ (Monterey) API that provides richer output. Parse the output for `"pid"` and `"state"` fields.

If that command exits non-zero, fall back to `launchctl list net.jira-assistant`. The list format is tab-separated: `PID\tLastExitCode\tLabel`. A running process has a numeric PID in the first column; a stopped process has `-`.

Return `{ running: false }` when the agent is not loaded at all (both commands exit with error codes indicating unknown service).

Minimum supported macOS: 12 (Monterey). Do not implement support for older `launchctl` syntax.

---

### `src/daemon/pid.ts`

#### Function stubs

```typescript
/** Writes PID to PATHS.pidFile using atomic temp-file + rename. */
async function writePid(pid: number): Promise<void>

/** Reads PID from PATHS.pidFile. Returns null if file missing or unreadable. */
async function readPid(): Promise<number | null>

/** Removes PATHS.pidFile. No-op if already missing. */
async function removePid(): Promise<void>

/** Returns true if a process with the given PID is alive (signal 0 check). */
async function isProcessRunning(pid: number): Promise<boolean>
```

#### Implementation notes

`writePid` must use an atomic write pattern: write to a temporary file in the same directory as `PATHS.pidFile` (same directory ensures the `rename` is atomic — cross-device renames are not atomic), then `rename` to the final path. This prevents a partial read if the process is interrupted mid-write. The file content is just the decimal string representation of the PID followed by a newline.

`readPid` wraps the read in a try/catch and returns `null` on any error (file not found, permission error, non-numeric content). Parse the integer with `parseInt`.

`isProcessRunning` uses `process.kill(pid, 0)`. Signal 0 does not send a signal — it only checks whether the process exists and the caller has permission to send signals to it. Returns `true` if `kill` does not throw, `false` if it throws with `ESRCH` (no such process). Re-throw any other error (e.g., `EPERM` means the process exists but is owned by another user — still "running").

**Ownership rule:** Only `commands/daemon.ts` calls `writePid`. The `start` and `stop` commands only call `readPid` and `removePid`. This prevents race conditions from multiple writers.

---

### `src/daemon/restart-tracker.ts`

#### Class stub

```typescript
/**
 * Tracks daemon restarts within a sliding time window.
 * Persists state to disk so counts survive launchd process restarts.
 * Default: maxRestarts=10, windowMs=60_000 (1 minute)
 */
class RestartTracker {
  constructor(filePath: string, maxRestarts?: number, windowMs?: number)

  /**
   * Records a restart timestamp. Prunes old entries. Persists to disk.
   * Returns true when the caller should give up (exit 0 to stop launchd restarts).
   */
  async recordRestart(): Promise<boolean>

  /** Clears all persisted restart state. */
  async reset(): Promise<void>
}
```

#### Implementation notes

The `filePath` parameter is `PATHS.restartsFile` (`~/.config/jira-assistant/restarts.json`) when used from the daemon. The file stores a JSON array of Unix timestamps (numbers): `[1700000000000, 1700000001000, ...]`.

On construction, do NOT read the file yet. Read it lazily on the first `recordRestart()` call, or read eagerly in the constructor — either approach is acceptable as long as the file is read from disk before the first decision is made (not cached from a previous process's in-memory state).

`recordRestart` logic:
1. Read the file (or start with `[]` if missing/unreadable)
2. Append `Date.now()`
3. Prune all entries where `now - timestamp > windowMs`
4. Write the pruned array back to the file atomically (temp + rename, same pattern as `writePid`)
5. Return `prunedArray.length > maxRestarts`

`reset` writes an empty array `[]` to the file (or deletes it — either is acceptable).

**Why persistence is required:** launchd restarts the process from scratch on crash. Without file persistence, a new `RestartTracker` instance would always start with zero entries, and the `maxRestarts` limit could never be exceeded. The process would restart indefinitely on crash.

**Why the daemon exits 0 when the limit is exceeded:** The plist uses `KeepAlive = { SuccessfulExit = false; Crashed = true }`. `SuccessfulExit = false` means launchd does NOT restart the process on clean exit (exit code 0). So exit 0 = "I decided to stop." The polling loop in `commands/daemon.ts` calls `recordRestart()` in its unhandled-error handler, and if it returns `true`, calls `process.exit(0)`.

---

## Integration with Other Sections

**Imports this section uses from section-01-foundation:**

```typescript
import { PATHS } from "../shared/paths"
import { FriendlyError, LaunchctlError } from "../shared/errors"
```

**What section-05-cli-commands imports from this section:**

```typescript
import { writePlist, loadAgent, unloadAgent, agentStatus } from "../daemon/launchd"
import { writePid, readPid, removePid, isProcessRunning } from "../daemon/pid"
import { RestartTracker } from "../daemon/restart-tracker"
```

The `RestartTracker` is instantiated in `commands/daemon.ts` with `PATHS.restartsFile` as the file path. The `launchd.ts` functions are called from `commands/start.ts` and `commands/stop.ts`. PID functions are called from `commands/daemon.ts` (write), `commands/start.ts` (read for status display), and `commands/stop.ts` (remove on stop).

---

## Implementation Order Within This Section

1. `src/daemon/pid.ts` — pure file I/O, no internal dependencies within this section
2. `src/daemon/restart-tracker.ts` — pure file I/O + time logic, no internal dependencies
3. `src/daemon/launchd.ts` — depends on `PATHS` and `LaunchctlError` from section-01-foundation; uses `Bun.spawn`

Write tests before each implementation step.

---

## Key Design Decisions to Preserve

1. `KeepAlive` must be the dictionary form `{SuccessfulExit=false; Crashed=true}`, not `<true/>`. Any simplification here breaks the "give up on crash loop" mechanism.
2. No `StandardOutPath`/`StandardErrorPath` in the plist. The daemon writes logs itself. Adding these would create a conflict where launchd and the daemon both try to write to the same file.
3. PID file atomic writes use temp-file-then-rename in the same directory. Cross-directory renames are not atomic on macOS (different inodes on different filesystems).
4. `RestartTracker` state must survive process restarts. In-memory-only state is not sufficient.
5. `isProcessRunning` uses signal 0, not `/proc` inspection (macOS has no `/proc`).
6. `agentStatus` targets macOS 12+ (`launchctl print gui/<uid>/...`). The fallback to `launchctl list` covers edge cases only.