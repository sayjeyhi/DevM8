Now I have all the context needed. Let me generate the section content for `section-01-foundation`.

# Section 01: Foundation — Project Scaffolding and Shared Infrastructure

## Overview

This is the zero-dependency foundation layer for the `01-core-daemon` module. All other sections (`section-02-config`, `section-03-logger`, `section-04-launchd`) depend on this section being complete before they can begin. This section must be implemented first.

You are building the macOS CLI tool `jira-assistant` — a Bun-compiled binary that manages a background Telegram bot polling process via launchd. This section covers the project scaffold (package.json, tsconfig.json) and two shared modules with no internal dependencies.

## What to Build

### Files to Create

```
01-core-daemon/
  package.json
  tsconfig.json
  src/
    shared/
      paths.ts
      errors.ts
  tests/
    shared/
      paths.test.ts
      errors.test.ts
```

---

## Tests First

Tests live in `01-core-daemon/tests/shared/`. Use Bun's built-in test runner (`bun test`). No additional libraries needed — Bun provides `describe`, `it`, `expect`, `mock`, `spyOn`, `beforeEach`, `afterEach`.

### `tests/shared/paths.test.ts`

Write tests for the following behaviors:

- All `PATHS` values are absolute paths — none start with `~`
- `PATHS.configDir` starts with the result of `os.homedir()`
- `PATHS.plistFile` contains `Library/LaunchAgents` in the path and ends with `.plist`
- `PATHS.restartsFile` ends with `restarts.json`
- `PATHS.logFile` ends with `app.log`
- `PATHS.pidFile` ends with `daemon.pid`
- `PATHS.configFile` ends with `config.toml`
- `PATHS.logsDir` is a prefix of `PATHS.logFile`

Stub shape:

```typescript
import { describe, it, expect } from "bun:test"
import { homedir } from "os"

describe("PATHS", () => {
  it("all values are absolute paths (no ~ literals)", async () => { /* ... */ })
  it("configDir starts with homedir()", async () => { /* ... */ })
  it("plistFile contains Library/LaunchAgents and ends with .plist", async () => { /* ... */ })
  it("restartsFile ends with restarts.json", async () => { /* ... */ })
})
```

### `tests/shared/errors.test.ts`

Write tests for the following behaviors:

- `FriendlyError` is an `instanceof Error`
- `FriendlyError` with a `hint` string → the `.hint` property is accessible and equals the passed string
- `FriendlyError` without a `hint` → `.hint` is `undefined`
- `FriendlyError` message is accessible via `.message`
- `LaunchctlError` is an `instanceof FriendlyError`
- `LaunchctlError` `.rawOutput` contains the stderr string passed to the constructor
- `LaunchctlError` `.hint` is accessible
- Known launchctl error pattern `"No such file or directory"` → hint mentions `jira-assistant start`
- Known launchctl error pattern `"Operation already in progress"` → hint mentions `jira-assistant status`
- Known launchctl error pattern `"Permission denied"` → hint mentions file permissions on the plist

Stub shape:

```typescript
import { describe, it, expect } from "bun:test"

describe("FriendlyError", () => {
  it("is an instance of Error", () => { /* ... */ })
  it("exposes hint property", () => { /* ... */ })
  it("hint is undefined when not provided", () => { /* ... */ })
})

describe("LaunchctlError", () => {
  it("is an instance of FriendlyError", () => { /* ... */ })
  it("exposes rawOutput property", () => { /* ... */ })
  it("maps known error patterns to hints", () => { /* ... */ })
})
```

---

## Implementation

### `package.json`

The project uses Bun as its runtime. Define these fields:

- `name`: `"jira-assistant"`
- `version`: `"0.1.0"` (semver)
- `scripts`:
  - `"build"`: `"bun run build.ts"`
  - `"test"`: `"bun test"`
  - `"start"`: `"bun run src/index.ts"`
- `dependencies`: `"smol-toml"`, `"zod"`, `"@clack/prompts"`, `"citty"`
- No `devDependencies` required — Bun's built-in test runner, TypeScript compiler, and bundler cover all dev needs.

### `tsconfig.json`

- `target`: `"ESNext"`
- `module`: `"ESNext"`
- `moduleResolution`: `"bundler"` (Bun's recommended setting)
- `strict`: `true`
- `skipLibCheck`: `true`
- Include `src/**/*` and `tests/**/*`

### `src/shared/paths.ts`

All file paths the entire application uses are centralized here as a single exported `PATHS` object. Key rules:

- Resolve all paths to **absolute strings** at module-load time using `os.homedir()`.
- Never use `~` literals — Bun does not expand them.
- No consumer of this module needs to do any path expansion — all values are ready to use as-is.

```typescript
import { homedir } from "os"
import { join } from "path"

const home = homedir()

export const PATHS = {
  configDir:       join(home, ".config/jira-assistant"),
  configFile:      join(home, ".config/jira-assistant/config.toml"),
  restartsFile:    join(home, ".config/jira-assistant/restarts.json"),
  logsDir:         join(home, ".config/jira-assistant/logs"),
  logFile:         join(home, ".config/jira-assistant/logs/app.log"),
  pidFile:         join(home, ".config/jira-assistant/daemon.pid"),
  plistFile:       join(home, "Library/LaunchAgents/net.jira-assistant.plist"),
  launchAgentsDir: join(home, "Library/LaunchAgents"),
}
```

This is the complete implementation — no logic beyond constant resolution.

### `src/shared/errors.ts`

Define two error classes. These are used throughout every other section, so the class shape is load-bearing:

**`FriendlyError`**

```typescript
/** User-facing error with an optional actionable hint. */
class FriendlyError extends Error {
  readonly hint?: string
  constructor(message: string, hint?: string)
}
```

**`LaunchctlError`**

```typescript
/** Error from a failed launchctl invocation. Carries raw stderr output. */
class LaunchctlError extends FriendlyError {
  readonly rawOutput: string
  constructor(stderr: string, hint: string)
}
```

`LaunchctlError`'s constructor receives the raw `stderr` string from the failed `launchctl` call and a pre-computed `hint`. The `hint` is computed by the caller (`daemon/launchd.ts` in section-04) using a mapping function — define that helper here too:

```typescript
/**
 * Maps known launchctl stderr patterns to human-friendly hint strings.
 * Used by daemon/launchd.ts when constructing LaunchctlError instances.
 */
export function launchctlHint(stderr: string): string
```

The hint mapper checks `stderr` for these patterns (in order):

| Pattern (case-insensitive substring) | Hint |
|---|---|
| `"No such file or directory"` | `"Make sure you ran \`jira-assistant start\` first"` |
| `"Operation already in progress"` | `"Daemon may already be running; check \`jira-assistant status\`"` |
| `"Permission denied"` | `"Check file permissions on the plist"` |
| _(fallback)_ | `"Run \`jira-assistant status\` for more info"` |

All three (`FriendlyError`, `LaunchctlError`, `launchctlHint`) must be exported from `errors.ts`.

**CLI error display contract** (implemented in section-05, but the contract is defined here):

All CLI commands catch `FriendlyError` at the top level and:
1. Print `error.message` to stderr
2. Print `error.hint` (if present) to stderr on a separate line, prefixed with a hint indicator
3. Exit with a non-zero code

`LaunchctlError` additionally prints `rawOutput` in a dimmed block before the hint.

---

## Dependencies

This section has no dependencies on other sections. It is the starting point.

## Blocked Sections

The following sections cannot start until this section is complete:

- `section-02-config` — imports `PATHS`, `FriendlyError`
- `section-03-logger` — imports `PATHS`
- `section-04-launchd` — imports `PATHS`, `FriendlyError`, `LaunchctlError`, `launchctlHint`

---

## Implementation Checklist

1. Create `01-core-daemon/package.json` with dependencies and scripts as specified
2. Create `01-core-daemon/tsconfig.json` with bundler module resolution and strict mode
3. Run `bun install` to install dependencies
4. Create `01-core-daemon/src/shared/paths.ts` — export the `PATHS` constant object
5. Create `01-core-daemon/src/shared/errors.ts` — export `FriendlyError`, `LaunchctlError`, `launchctlHint`
6. Create `01-core-daemon/tests/shared/paths.test.ts` — write and run tests
7. Create `01-core-daemon/tests/shared/errors.test.ts` — write and run tests
8. Run `bun test` from `01-core-daemon/` — all tests must pass before handing off to dependent sections

## As-Built Notes

**Deviations from plan (code review fixes applied):**

- `LaunchctlError` message changed to `"launchctl invocation failed"` (not `"launchctl failed: " + stderr`) — avoids duplicating stderr when section-05 prints both `.message` and `.rawOutput` per display contract.
- `package.json` dependencies pinned to caret ranges matching resolved versions (`zod: ^4.4.3`, `smol-toml: ^1.6.1`, `@clack/prompts: ^1.4.0`, `citty: ^0.2.2`) instead of `"latest"`. **Zod v4** is intentional.
- `tsconfig.json` has `"noEmit": true` added to prevent accidental tsc output into source tree.
- `paths.ts` composes child paths from `configDir` and `logsDir` locals rather than repeating string literals.

**Files created:**
- `01-core-daemon/package.json`
- `01-core-daemon/tsconfig.json`
- `01-core-daemon/bun.lock`
- `01-core-daemon/src/shared/paths.ts`
- `01-core-daemon/src/shared/errors.ts`
- `01-core-daemon/tests/shared/paths.test.ts`
- `01-core-daemon/tests/shared/errors.test.ts`

**Tests:** 19 tests across 2 files, all pass.