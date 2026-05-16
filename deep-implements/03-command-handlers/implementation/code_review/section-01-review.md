# Code Review: section-01-foundation

## Overall
Core logic correct, 7 tests present and isolated. Several issues found.

## Issues

### 1. SECURITY - ALLOWED_USER_IDS empty-string inconsistency (moderate)
Guard only checks `=== undefined`, not `=== ""`. Empty string silently produces an empty Set — bot starts but nobody can use it. The plan allows this intentionally, but it's architecturally incoherent vs `required()` which rejects empty strings. Needs a comment explaining the deliberate behavior.

### 2. BUG - ALLOWED_USER_IDS checked before other required fields (minor)
If multiple fields are missing simultaneously, ALLOWED_USER_IDS error surfaces first. Order is arbitrary and non-obvious. Acceptable for single-field messages, just document the order.

### 3. MISSING DEPS - @grammyjs packages not in package.json (high)
Plan requires: `@grammyjs/commands`, `@grammyjs/transformer-throttler`, `@grammyjs/auto-retry` (runtime) and `@grammyjs/grammytest` (dev). None present. Later sections will fail at import resolution.

### 4. TEST COVERAGE GAP - whitespace-only ALLOWED_USER_IDS not tested (minor)
`ALLOWED_USER_IDS = "  "` works correctly (produces empty Set via trim+filter) but has no test coverage.

### 5. PLAN DEVIATION - test location and no vitest.config (acknowledged)
Tests at `tests/bot/config.test.ts` instead of `03-command-handlers/tests/config.test.ts`. bun:test used instead of vitest. Acceptable given root-layout convention.

### 6. MINOR - Number("") edge case needs comment
`.filter(s => s !== "")` before `Number()` prevents `0` being inserted as fake user ID. Needs comment to prevent future regression.
