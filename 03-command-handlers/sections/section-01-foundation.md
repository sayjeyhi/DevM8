I now have everything needed to generate the section content. Here is the complete section:

---

# Section 01: Foundation — Project Setup and Configuration

## Overview

This section establishes the project scaffolding for the `03-command-handlers` module. It has no dependencies on other sections and must be completed first, as all subsequent sections build on the package configuration, TypeScript setup, and `config.ts` module produced here.

The module is the orchestration ("glue") layer of a personal Jira assistant Telegram bot. It receives Telegram slash commands, routes them to handlers, and delegates actual Jira and Claude API work to the pre-built clients in `../02-integration-clients`. The bot framework is **grammY** (first-class TypeScript support, active maintenance).

---

## Files Created (Actual — root layout)

All code lives at the project root, not in `03-command-handlers/`.

```
src/bot/
  config.ts          ← Config interface + loadConfig()
  bot.ts             ← stub
  middleware/auth.ts ← stub
  commands/index.ts  ← stub
  commands/create.ts ← stub
  commands/move.ts   ← stub
  commands/comment.ts← stub
  commands/solve.ts  ← stub
  commands/help.ts   ← stub
  utils/splitMessage.ts ← stub
  utils/parseArgs.ts ← stub
tests/bot/
  config.test.ts     ← 8 bun:test cases (7 from plan + whitespace edge case)
package.json         ← added @grammyjs/commands, @grammyjs/transformer-throttler, @grammyjs/auto-retry
```

No separate `package.json`/`tsconfig.json`/`vitest.config.ts` for this module — root already provides them. Uses `bun:test` instead of vitest.

All other source files (`bot.ts`, `middleware/`, `commands/`, `utils/`) are stubs only in this section — they are fully implemented in later sections.

---

## Tests First

**File:** `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/tests/config.test.ts`

Write the following tests before implementing `loadConfig()`. Use Vitest. Each test manipulates `process.env` directly (set before calling `loadConfig()`, restore/delete after).

Test cases:

- `loadConfig()` with all required fields present returns a valid `Config` object with correct field values
- `loadConfig()` with `TELEGRAM_BOT_TOKEN` missing (deleted from `process.env`) throws with a descriptive error message mentioning the missing field
- `loadConfig()` with `ALLOWED_USER_IDS` missing throws
- `loadConfig()` with `ALLOWED_USER_IDS = "123,456"` returns a `Config` whose `allowedUserIds` `Set` contains `123` and `456` as numbers
- `loadConfig()` with `ALLOWED_USER_IDS = "123, 456"` (spaces around entries) returns a `Set` containing `123` and `456` — whitespace is trimmed before parsing
- `loadConfig()` with `ALLOWED_USER_IDS = "abc,456"` (non-numeric first entry) returns a `Set` containing only `456` — NaN entries are filtered out, no throw
- `loadConfig()` with `ALLOWED_USER_IDS = ""` (empty string) returns a `Config` whose `allowedUserIds` is an empty `Set` — no crash

Use `beforeEach`/`afterEach` to save and restore `process.env` state so tests are isolated.

---

## Implementation Details

### 1. `package.json`

Create `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/package.json`.

Runtime dependencies:
- `grammy` — core Telegram bot framework
- `@grammyjs/commands` — command group registration and Telegram menu sync
- `@grammyjs/transformer-throttler` — outbound rate limit management (30 req/s global, 1 msg/s per chat)
- `@grammyjs/auto-retry` — transparent retry on HTTP 429 with `retry_after` delays

Development dependencies:
- `vitest` — test runner
- `@grammyjs/grammytest` — in-memory bot simulation (no real HTTP calls)
- TypeScript toolchain: `typescript`, `@types/node`

Scripts:
- `test`: `vitest run`
- `build`: `tsc`
- `start`: run compiled `dist/bot.js`

### 2. `tsconfig.json`

Create `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/tsconfig.json`.

Use strict TypeScript settings:
- `"strict": true`
- `"target": "ES2022"` (or later — grammY requires modern async/await)
- `"module": "Node16"` or `"NodeNext"` for ESM-compatible module resolution
- `"outDir": "dist"`
- `"rootDir": "src"`
- Include `src/**/*`

### 3. `vitest.config.ts`

Create `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/vitest.config.ts`.

Minimal Vitest config pointing at the `tests/` directory. No special transforms needed — Vitest handles TypeScript natively when using `bun` or with `ts-node`.

### 4. `src/config.ts`

Create `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/config.ts`.

#### `Config` type

Define and export a `Config` interface with strict typing for all configuration fields:

```typescript
export interface Config {
  telegramBotToken: string
  jiraBaseUrl: string
  jiraProjectKey: string
  jiraUserEmail: string
  jiraApiToken: string
  claudeApiKey: string
  allowedUserIds: Set<number>
}
```

#### `loadConfig()` function

```typescript
export function loadConfig(): Config
```

Behavior:
- Reads all values from `process.env`
- Required fields (throws if absent or empty string): `TELEGRAM_BOT_TOKEN`, `JIRA_BASE_URL`, `JIRA_PROJECT_KEY`, `JIRA_USER_EMAIL`, `JIRA_API_TOKEN`, `CLAUDE_API_KEY`, `ALLOWED_USER_IDS`
- For each missing required field, throw an `Error` with a message that names the missing variable — do not throw a generic "missing config" message
- `ALLOWED_USER_IDS` parsing:
  1. Split on `,`
  2. Trim whitespace from each entry
  3. Parse each trimmed entry with `Number()`
  4. Filter out entries where `isNaN(result)` is true
  5. Construct a `Set<number>` from the surviving numbers
  6. An empty string or all-invalid entries results in an empty `Set` — this is not an error (the bot will simply be unusable for any user, but it starts successfully)
- Call this function once at startup (from `bot.ts`) — not imported lazily. A misconfigured bot must fail immediately on startup, not at first user interaction.

#### Security note

`JIRA_API_TOKEN` and `CLAUDE_API_KEY` are present on the `Config` object. They must never be logged. Error handlers in other sections reference this constraint — it originates here because `Config` is the source of these sensitive values.

---

## Directory Scaffolding (Stub Files)

Create empty stub files for the directories that later sections will populate. These stubs satisfy import resolution during incremental development:

- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/bot.ts` — stub only
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/middleware/auth.ts` — stub only
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/commands/index.ts` — stub only
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/commands/create.ts` — stub only
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/commands/move.ts` — stub only
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/commands/comment.ts` — stub only
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/commands/solve.ts` — stub only
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/commands/help.ts` — stub only
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/utils/splitMessage.ts` — stub only
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/03-command-handlers/src/utils/parseArgs.ts` — stub only

Stubs can be empty exports (`export {}`) or contain function signatures with `throw new Error("not implemented")` bodies.

---

## Acceptance Criteria

This section is complete when:

1. `npm install` (or `bun install`) succeeds — all dependencies resolve
2. `tsc --noEmit` passes with zero errors against the `config.ts` implementation
3. `vitest run tests/config.test.ts` passes all seven test cases listed above
4. The stub files exist so that later sections can be implemented without restructuring imports

---

## Dependencies

None. This section has no dependencies on other sections.

## Blocks

All other sections depend on this section:
- **section-02-auth-middleware** — depends on `Config` type and project setup
- **section-03-utils** — depends on project setup
- **section-04-create-handler** — depends on `Config` type and project setup
- **section-05-move-comment-help-handlers** — depends on project setup
- **section-06-solve-handler** — depends on project setup
- **section-07-registration-and-bot** — depends on `Config` type, `loadConfig()`, and all handler sections