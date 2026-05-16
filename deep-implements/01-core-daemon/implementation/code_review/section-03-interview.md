# Code Review Interview: section-03-logger

## Auto-fixes Applied

**C1 — Bun.file().delete?.() → fs/promises.unlink**
- Problem: `Bun.file(path).delete?.()` not stable in Bun 1.3; silently no-ops
- Fix: import `unlink` from `fs/promises` and call `unlink(overflow)`

**C2 — Bun.file(logFile).size → stat from fs/promises**
- Problem: `Bun.File.size` lazy property may read 0 before file is opened
- Fix: Use `stat(logFile)` catch-null pattern for existence+size check

**I1 — isTTY test: restore full property descriptor**
- Problem: `Object.defineProperty({ value: origIsTTY })` replaces getter with plain value,
  mutating the descriptor permanently for remaining tests
- Fix: save descriptor with `Object.getOwnPropertyDescriptor`, restore after test

## User Decision

**I3 — warn level in createLogger parameter**
- Question: Add 'warn' to level parameter to resolve asymmetry with Logger.warn() method?
- Decision: Follow spec — keep 'info'|'debug'|'error' only
- Rationale: Per section-03-logger.md spec. Consistency with planned downstream consumers.

## Let Go

- I4: meta field shadowing of structured keys — intentional per spec
- I2: keepCount rename clobber — correct POSIX behavior, oldest file deleted via implicit replace
- M1, M2: minor nitpicks
