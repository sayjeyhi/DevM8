Now I have all the context needed. Let me generate the section content for `section-06-build-integration`.

# Section 06: Build Integration

## Overview

This is the final section of the `01-core-daemon` module. It implements the build pipeline (`build.ts`) and the integration/smoke tests that validate the fully compiled binary end-to-end. This section depends on all prior sections being complete.

**Dependencies:**
- section-01-foundation: `shared/paths.ts`, `shared/errors.ts`
- section-02-config: `config/schema.ts`, `config/loader.ts`, `config/wizard.ts`
- section-03-logger: `logger/index.ts`, `logger/rotate.ts`
- section-04-launchd: `daemon/launchd.ts`, `daemon/pid.ts`, `daemon/restart-tracker.ts`
- section-05-cli-commands: `src/index.ts`, all `commands/`

---

## Files to Create

- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/01-core-daemon/build.ts`
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/01-core-daemon/tests/integration/binary-smoke.test.ts`
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/01-core-daemon/tests/integration/config-lifecycle.test.ts`
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/01-core-daemon/tests/integration/launchd-lifecycle.test.ts`
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/01-core-daemon/tests/integration/restart-persistence.test.ts`

---

## Tests First

All integration tests live under `tests/integration/`. Integration tests that require macOS and a real launchd are tagged `@macos` in the test description and include a skip guard:

```typescript
const isMacOS = process.platform === "darwin"
const macOSIt = isMacOS ? it : it.skip
```

### `tests/integration/binary-smoke.test.ts`

This file verifies the compiled binary is functional before any other integration tests run. It must be run after `bun run build.ts` completes.

Tests to write:

- `compiled binary file exists at ./dist/jira-assistant` — stat the file, assert it exists
- `compiled binary is executable` — check file mode has execute bit (mode & 0o111) via `fs.statSync`
- `./dist/jira-assistant --version exits 0 and prints a semver string` — spawn the binary, assert exit code 0 and stdout matches `/^\d+\.\d+\.\d+/`
- `./dist/jira-assistant --help exits 0` — smoke test that entry point routing works and all subcommands are listed
- `./dist/ja exists` — verify the `ja` alias binary or symlink is present at `./dist/ja`

```typescript
// Stub — fill in assertions
describe("binary smoke tests", () => {
  const binaryPath = new URL("../../dist/jira-assistant", import.meta.url).pathname

  it("compiled binary file exists", () => { /* ... */ })
  it("compiled binary is executable", () => { /* ... */ })
  it("--version exits 0 and prints semver", async () => { /* ... */ })
  it("--help exits 0", async () => { /* ... */ })
  it("ja alias exists at dist/ja", () => { /* ... */ })
})
```

### `tests/integration/config-lifecycle.test.ts`

Tests the full config write/read lifecycle against a real temp directory (no mocks). Tagged `@macos` where launchd is required; the config portion runs on all platforms.

Tests to write:

- `writeConfig creates config.toml with 0o600 permissions` — write a valid `AppConfig`, stat the file, assert `(mode & 0o777) === 0o600`
- `loadConfig reads back an identical AppConfig after writeConfig` — round-trip: write then load, deep-equal the objects
- `config directory is created if it does not exist` — point `configPath` to a fresh temp dir path that does not exist; assert `writeConfig` creates it
- `config.toml is not world-readable` — assert `(stat.mode & 0o044) === 0` (group and other read bits are clear)

```typescript
import { tmpdir } from "os"
import { join } from "path"
import { mkdtemp, rm, stat } from "fs/promises"
import { writeConfig, loadConfig } from "../../src/config/loader"

describe("config lifecycle", () => {
  let tempDir: string

  beforeEach(async () => {
    tempDir = await mkdtemp(join(tmpdir(), "ja-test-"))
  })

  afterEach(async () => {
    await rm(tempDir, { recursive: true, force: true })
  })

  it("writeConfig creates config.toml with 0o600 permissions", async () => { /* ... */ })
  it("round-trip: writeConfig then loadConfig returns identical AppConfig", async () => { /* ... */ })
  it("config directory is created if missing", async () => { /* ... */ })
  it("config.toml is not world-readable", async () => { /* ... */ })
})
```

### `tests/integration/launchd-lifecycle.test.ts`

Requires macOS and a live launchd. Uses a **test-specific plist label** (e.g., `net.jira-assistant-test`) to avoid colliding with a real installation. Tests must clean up after themselves by always calling `launchctl unload` in `afterAll`.

Tests to write (all `macOSIt`):

- `jira-assistant start registers a LaunchAgent and reports running` — call `startCommand()` against a temp config and binary; poll `agentStatus()` until running or timeout
- `jira-assistant status shows running state after start` — capture stdout from `statusCommand()`, assert it contains `running`
- `jira-assistant stop unloads the LaunchAgent` — call `stopCommand()`, then `agentStatus()`, assert `{ running: false }`
- `plist file is written at ~/Library/LaunchAgents/net.jira-assistant.plist` — assert file exists after `writePlist()`

```typescript
describe("launchd lifecycle @macos", () => {
  macOSIt("start registers LaunchAgent and reports running", async () => { /* ... */ })
  macOSIt("status shows running after start", async () => { /* ... */ })
  macOSIt("stop unloads LaunchAgent", async () => { /* ... */ })
  macOSIt("plist file written at correct path", async () => { /* ... */ })
})
```

### `tests/integration/restart-persistence.test.ts`

Validates that `RestartTracker` state genuinely persists across simulated process restarts (creates a new instance pointing at the same file and confirms it reads the prior state). Runs on all platforms.

Tests to write:

- `restart state persists across process restarts` — create tracker A, call `recordRestart()` N times, destroy A; create tracker B pointing to same file; call `recordRestart()` and verify count reflects the prior calls
- `restarts.json is written atomically` — spy on rename behaviour; verify no partial state is observable
- `reset() clears the persisted file` — call `reset()`, recreate tracker, assert count starts at zero
- `timestamps outside windowMs are pruned on reload` — write a file with old timestamps manually, create tracker, assert it treats them as expired

```typescript
import { tmpdir } from "os"
import { join } from "path"
import { mkdtemp, rm } from "fs/promises"
import { RestartTracker } from "../../src/daemon/restart-tracker"

describe("restart tracker persistence", () => {
  let tempDir: string
  let filePath: string

  beforeEach(async () => {
    tempDir = await mkdtemp(join(tmpdir(), "ja-restart-test-"))
    filePath = join(tempDir, "restarts.json")
  })

  afterEach(async () => {
    await rm(tempDir, { recursive: true, force: true })
  })

  it("state persists across simulated process restarts", async () => { /* ... */ })
  it("reset() clears the persisted file", async () => { /* ... */ })
  it("old timestamps are pruned on next load", async () => { /* ... */ })
})
```

---

## Build Script Implementation

### `build.ts`

`build.ts` is a standalone Bun script invoked with `bun run build.ts`. It must not be imported by any source file. It orchestrates two things:

1. Compile the binary (with optional `--bytecode` fast path)
2. Validate the output binary with a smoke test

**File:** `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/01-core-daemon/build.ts`

Implementation steps:

1. Define constants: `entrypoint = "./src/index.ts"`, `outDir = "./dist"`, `binaryName = "jira-assistant"`, `binaryPath = "./dist/jira-assistant"`.

2. Ensure `./dist` exists (`mkdirSync("./dist", { recursive: true })`).

3. **First attempt: build with `bytecode: true`.**
   Call `Bun.build({ entrypoints: [entrypoint], outdir: outDir, target: "bun", compile: true, minify: true, sourcemap: "linked", naming: binaryName, bytecode: true })`.
   Check `result.success`; if false, print `result.logs` and continue to second attempt.

4. **Smoke test the bytecode binary.** Spawn `[binaryPath, "--version"]` synchronously. If exit code is non-zero or stdout is empty, the bytecode binary is invalid — print a warning and rebuild without `bytecode`.

5. **Second attempt (fallback): build without `bytecode: true`.**
   Same `Bun.build()` call but omit `bytecode`. If this also fails, print `result.logs` and `process.exit(1)`.

6. **Create `./dist/ja`.** Use `Bun.file(binaryPath)` to copy to `./dist/ja`, then `chmodSync("./dist/ja", 0o755)`. Alternatively use a symlink: `symlinkSync("./dist/jira-assistant", "./dist/ja")` inside a try/catch that removes the existing file first.

7. **Final validation.** Spawn `[binaryPath, "--version"]`. Assert exit code is 0 and stdout matches `/^\d+\.\d+\.\d+/`. If validation fails, `process.exit(1)` with a clear error message. On success, print the binary path and version string.

```typescript
// build.ts — stub outline (implementer fills in bodies)
import { mkdirSync, symlinkSync, unlinkSync, chmodSync } from "fs"
import { existsSync } from "fs"

const ENTRYPOINT = "./src/index.ts"
const OUT_DIR = "./dist"
const BINARY_NAME = "jira-assistant"
const BINARY_PATH = `${OUT_DIR}/${BINARY_NAME}`
const JA_PATH = `${OUT_DIR}/ja`

async function buildWithBytecode(): Promise<boolean> {
  /** Attempt Bun.build with bytecode: true. Returns true on success. */
}

async function buildWithoutBytecode(): Promise<boolean> {
  /** Attempt Bun.build without bytecode. Returns true on success. */
}

function smokeTest(binaryPath: string): boolean {
  /** Spawn binaryPath --version, check exit code and semver in stdout. */
}

function createJaAlias(): void {
  /** Remove existing ./dist/ja if present, then symlink to jira-assistant. */
}

async function main(): Promise<void> {
  mkdirSync(OUT_DIR, { recursive: true })

  let built = await buildWithBytecode()
  if (!built || !smokeTest(BINARY_PATH)) {
    console.warn("bytecode build failed or smoke test failed — falling back to non-bytecode build")
    built = await buildWithoutBytecode()
    if (!built) process.exit(1)
  }

  createJaAlias()

  if (!smokeTest(BINARY_PATH)) {
    console.error("Final binary smoke test failed — aborting")
    process.exit(1)
  }

  console.log(`Build successful: ${BINARY_PATH}`)
}

main()
```

---

## `package.json` Build Script Entry

Add the following script to `package.json` so the build is runnable with `bun run build`:

```json
{
  "scripts": {
    "build": "bun run build.ts",
    "test": "bun test",
    "test:integration": "bun test tests/integration"
  }
}
```

---

## `--version` Definition at Build Time

The version string printed by `--version` must come from `package.json`. In `src/index.ts`, the version is injected via `--define` so it is embedded in the binary at build time. The `Bun.build()` call in `build.ts` must include:

```typescript
define: {
  "process.env.APP_VERSION": JSON.stringify(pkg.version)
}
```

Where `pkg` is imported at the top of `build.ts`:

```typescript
import pkg from "./package.json"
```

And in `src/index.ts`, the version is referenced as:

```typescript
const version = process.env.APP_VERSION ?? "0.0.0-dev"
```

---

## Integration Test Configuration

Integration tests require the binary to already be built. Add a check at the top of `binary-smoke.test.ts` that skips gracefully if `./dist/jira-assistant` does not exist:

```typescript
import { existsSync } from "fs"
import { resolve } from "path"

const BINARY_PATH = resolve(import.meta.dir, "../../dist/jira-assistant")
const binaryBuilt = existsSync(BINARY_PATH)

if (!binaryBuilt) {
  console.warn("Binary not found — run `bun run build` first. Skipping smoke tests.")
}

const smokeIt = binaryBuilt ? it : it.skip
```

---

## Per-Release Manual Smoke Checklist

After each release build, verify the following manually before publishing to GitHub Releases:

1. `./dist/jira-assistant --version` prints a semver string and exits 0
2. `./dist/jira-assistant --help` lists all subcommands (`start`, `stop`, `status`, `config`, `daemon`)
3. `./dist/ja --version` works identically (alias is functional)
4. `./dist/jira-assistant config` opens the interactive wizard in a real terminal (TTY present)
5. On macOS: `./dist/jira-assistant start` registers a LaunchAgent (visible in `launchctl list | grep jira-assistant`)
6. On macOS: `./dist/jira-assistant status` shows `running` state with PID
7. On macOS: `./dist/jira-assistant stop` removes the agent (gone from `launchctl list`)
8. Log file is written to `~/.config/jira-assistant/logs/app.log`
9. `restarts.json` is created in `~/.config/jira-assistant/` when daemon starts
10. Binary file size is reasonable (check it is not unexpectedly large due to bundling issues)

---

## Implementation Order for This Section

1. Write all test stubs first (create the test files with `it.todo` or empty test bodies)
2. Implement `build.ts` — run `bun run build.ts` and verify the binary is produced
3. Run `bun test tests/integration/binary-smoke.test.ts` — all smoke tests should pass
4. Implement config lifecycle integration tests and run them
5. On macOS: implement and run launchd lifecycle integration tests
6. Implement restart persistence integration tests and run them
7. Verify the full test suite passes: `bun test`
8. Walk through the manual smoke checklist at least once

---

## Key Invariants to Preserve

- The `--bytecode` fallback must be silent to the end user (it is a build-time optimisation detail)
- The smoke test inside `build.ts` must run the **actual compiled binary**, not source, so linking/bundling issues are caught at build time
- Integration tests must never leave LaunchAgents registered after they complete — `afterAll` cleanup is mandatory
- The `ja` alias must produce output identical to `jira-assistant` for all subcommands (same binary, same entry point)
- File permissions on the built binary must be `0o755` (owner rwx, group rx, other rx)