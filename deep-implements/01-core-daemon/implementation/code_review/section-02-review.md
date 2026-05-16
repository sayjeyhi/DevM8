# Code Review: section-02-config

## SECURITY
- **S1 loader.ts:22** — ENOENT check falls back to `.exists()` re-read; EACCES errors swallowed as "not found". Fix: check `code === 'ENOENT'` only; add `EACCES` branch.
- **S2 loader.ts:61** — `chmod` runs AFTER `rename`, brief window where config is world-readable. Fix: chmod the `.tmp` file BEFORE rename.
- **S3 schema.ts** — Custom EMAIL_REGEX too weak (`x@y.z` passes). Use `z.string().email()` instead.

## BUG
- **B3 wizard.ts:149** — `binary_path` validate callback is `async`; @clack/prompts `text()` validate is sync-only. Async validator silently passes. Fix: use `existsSync` or verify async support.
- **B4 wizard.ts:153** — `isCancel(result)` post-group doesn't work as expected. Need `onCancel` option passed to `group()` for clean Ctrl+C abort.
- **B5 schema.ts:93** — Double default: `.default('info')` on enum AND `.default({ log_level: 'info' })` on app object. Remove outer default; use `.optional()` + `.default()` on inner field only.

## DESIGN
- **D3 wizard.ts** — `outro('Config saved!')` is misleading (wizard doesn't write). Change to `'Setup complete!'`.

## IMPROVEMENT
- **I1 loader.ts:8** — `stat` import unused. Remove it.

## LET GO
- D2 flat key namespace in wizard — acceptable for now
- I2 test false alarm — message DOES contain "jira-assistant config"
- I3, N1-N3 — minor
