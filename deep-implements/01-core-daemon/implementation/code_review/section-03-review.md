# Code Review: section-03-logger

## CRITICAL

**C1. `Bun.file().delete?.()` not stable in Bun 1.3**
`rotate.ts` uses `Bun.file(overflow).delete?.()`. The `?.` silently no-ops if the API doesn't exist (not stable in Bun 1.3). Should use `fs/promises.unlink` instead.

**C2. `Bun.file(logFile).size` may not eager-stat**
`f.size` is a synchronous property on a lazy BunFile handle. Under Bun 1.3 the size may read as 0 for existing files before the file is opened. Safer to use `(await stat(logFile)).size` from fs/promises.

## IMPORTANT

**I1. isTTY test replaces getter with plain value**
`index.test.ts` restores `process.stdout.isTTY` via `Object.defineProperty` with `value:`, which replaces the original getter/setter descriptor. Subsequent tests that depend on real TTY detection see a stale value. Fix by saving/restoring the full descriptor.

**I2. Shift loop clobbers app.log.keepCount silently**
The loop renames `app.log.(keepCount-1)` → `app.log.keepCount`, which POSIX-atomically replaces the existing file. The overflow guard targets `keepCount+1` (unreachable in normal operation). Oldest file is correctly deleted via implicit rename overwrite — behavior is correct but the keepCount test only checks existence, not content integrity of shifted files.

**I3. `level` parameter omits `'warn'` — Logger interface exposes `.warn()` method**
Asymmetry: callers can emit warn logs but cannot configure threshold at warn. Per spec this is intentional (`level: "info" | "debug" | "error"`), but noted for user awareness.

**I4. Meta fields can shadow `level`/`ts`/`msg` in JSON output**
The spread `{ level, ts, msg, ...meta }` allows meta keys to overwrite structured fields. Intentional per spec, no guard required.

## MINOR

**M1. LEVEL_COLOR maps `info` to `""` — falsy empty string used as "no color"**
Works but is a subtle truthiness trap. Not worth fixing.

**M2. Test for keepCount boundary only checks existence, not shift chain correctness**
A broken shift would still pass. Not critical given the other rotation tests.
