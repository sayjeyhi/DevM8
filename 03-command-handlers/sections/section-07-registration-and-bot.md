Now I have all the context needed. Let me generate the section content.

# Section 07: Registration and Bot

## Overview

This is the final integration section that wires all previously implemented components into a running bot. It depends on all prior sections being complete and produces the top-level entry points: `src/commands/index.ts` (command registration) and `src/bot.ts` (bot construction and startup).

This section has no downstream dependents — it is the last step.

## Dependencies (must be complete before starting)

- **section-02-auth-middleware**: `src/middleware/auth.ts` — `createAuthMiddleware()`
- **section-04-create-handler**: `src/commands/create.ts` — `handleCreate()`
- **section-05-move-comment-help-handlers**: `src/commands/move.ts`, `src/commands/comment.ts`, `src/commands/help.ts`
- **section-06-solve-handler**: `src/commands/solve.ts` — `handleSolve()`
- **section-01-foundation**: `src/config.ts` — `loadConfig()`, `Config` type

## Files Created (Actual Paths)

Plan paths differ from actual (all code at root src/bot/, established by section-01):

- `src/bot/commands/index.ts` — exports Clients interface and registerCommands()
- `src/bot/bot.ts` — startBot() wiring function
- `tests/bot/commands/index.test.ts` — registerCommands unit tests

## Implementation Notes

- `@grammyjs/commands` has CJS/ESM interop conflict with grammy when running full bun test suite. Fixed with `mock.module("@grammyjs/commands", ...)` in tests.
- `process.env.ANTHROPIC_API_KEY = config.claudeApiKey` sets key for claude CLI subprocess. User chose to keep this approach.
- SIGTERM/SIGINT handlers call `await bot.stop()` then `process.exit(0)` to ensure open fetch handles don't keep process alive.
- `setCommands` failure is non-fatal (wrapped in try/catch) — bot routing works even without menu sync.
- bot.on listens on `message:text` with `/` prefix check to avoid "Unknown command" on plain-text messages.
- bot.ts unit test gap: `startBot()` requires grammY long-polling which can't run in bun:test. Coverage via component tests.
- 5 tests (index.test.ts), 277 pass full suite.

## Tests First

Write these tests in `tests/bot.test.ts` before implementing the files. The harness is `@grammyjs/grammytest` with `vi.fn()` mocks for integration clients.

### Test stubs for `tests/bot.test.ts`

```typescript
// tests/bot.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
// Import bot construction helpers and registerCommands after implementation

describe('Bot construction', () => {
  it('bot is constructed with the token from config')
  // Verify the Bot instance receives the token string from the loaded Config
})

describe('Auth middleware ordering', () => {
  it('auth middleware runs before command handlers — unauthorized update does not reach handler')
  // Use grammytest to fire a message from an unknown userId; assert the command handler vi.fn() was never called
})

describe('registerCommands', () => {
  it('calls bot.use(commands) to install handler dispatch')
  it('calls setCommands(bot) to sync Telegram UI menu')
  it('handlers receive Clients via closure — mocked JiraClient method is invoked when handler fires')
  it('Clients type is structurally { jira: JiraClient, claude: ClaudeClient }')
})

describe('bot.catch', () => {
  it('fires when a handler throws an unhandled error')
  it('replies with a generic error message (does not re-throw or crash)')
})

describe('Graceful shutdown', () => {
  it('SIGTERM triggers bot.stop()')
  it('SIGINT triggers bot.stop()')
})
```

### Additional cross-cutting error tests (add to each existing handler test file)

These tests belong in the individual handler test files but are specified here for completeness. Each handler test file should verify:

- Handler catches `JiraAuthError` → reply contains an auth error message
- Handler catches an unknown error → logger receives `{ event: 'error', errorMessage: string }` — the full error object and any Authorization header string are NOT logged
- `bot.catch` receives the error if an unhandled rejection escapes a handler

## Implementation: `src/commands/index.ts`

This file defines the `Clients` interface and the `registerCommands` function. All handlers receive their integration clients through closure capture rather than global state or context decoration.

### Key design points

- `interface Clients` is exported so `bot.ts` and tests can import it for type-safe mock construction.
- `registerCommands` creates a `CommandGroup` from `@grammyjs/commands`, registers all five commands with their descriptions (used for Telegram's `/` menu display), installs handler dispatch via `bot.use(commands)`, and syncs the menu via `await commands.setCommands(bot)`.
- These two operations (`bot.use` vs `setCommands`) are intentionally distinct and must not be conflated:
  - `bot.use(commands)` — installs the actual middleware dispatch so commands are routed to handlers.
  - `await commands.setCommands(bot)` — makes a Telegram Bot API call to update the command list shown in the UI. It has no effect on routing.
- Each handler lambda closes over `clients`. Example shape: `commands.command('create', 'Create a new Jira ticket', (ctx) => handleCreate(ctx, clients))`.

### Stub definition

```typescript
// src/commands/index.ts
import type { Bot } from 'grammy'
import type { JiraClient } from '../../02-integration-clients/src/jira'
import type { ClaudeClient } from '../../02-integration-clients/src/claude'

export interface Clients {
  jira: JiraClient
  claude: ClaudeClient
}

/**
 * Registers all five command handlers on the bot and syncs the Telegram command menu.
 * Handlers capture `clients` via closure — no global state.
 * Calls bot.use(commands) to install dispatch, then await commands.setCommands(bot) to sync UI.
 */
export async function registerCommands(bot: Bot, clients: Clients): Promise<void>
```

## Implementation: `src/bot.ts`

This is the main entry point. It orchestrates startup in this exact order:

1. Call `loadConfig()` — throws immediately if any required field is missing.
2. Construct `JiraClient` and `ClaudeClient` from `02-integration-clients`, passing the config values.
3. Construct `new Bot(config.telegramBotToken)`.
4. Install the transformer throttler on `bot.api` via `apiThrottler()` transformer.
5. Install `autoRetry()` transformer on `bot.api`.
6. Build the `allowedIds` `Set<number>` from `config.allowedUserIds`.
7. Install auth middleware: `bot.use(createAuthMiddleware(allowedIds))`.
8. Call `await registerCommands(bot, { jira, claude })`.
9. Register the unknown command fallback after all command handlers: any unmatched update replies with `Unknown command. Try /help`.
10. Install `bot.catch` — logs sanitized error info and attempts a generic reply.
11. Register `process.on('SIGTERM', ...)` and `process.on('SIGINT', ...)` to call `await bot.stop()`.
12. Call `bot.start()` (long-polling — webhook mode is not supported).

### Graceful shutdown requirement

Both `SIGTERM` and `SIGINT` handlers must call `await bot.stop()` before the process exits. Without this, in-flight Telegram updates are lost on restart, which could produce duplicate Jira tickets or orphaned operations. The handlers should be registered before `bot.start()` is called.

### `bot.catch` sanitization rules

The global error handler must follow the same logging rules as per-handler `catch` blocks:

- Log only `{ event: 'error', command: string | undefined, errorMessage: error.message, errorType: error.type ?? 'unknown' }`.
- Never log the full error object (fetch errors may embed Authorization headers in `error.cause`).
- Never log ticket key values or prompt content.
- Attempt a generic reply to the user: `"An unexpected error occurred. Please try again."`.

### Stub definition

```typescript
// src/bot.ts
import { Bot } from 'grammy'
import { apiThrottler } from '@grammyjs/transformer-throttler'
import { autoRetry } from '@grammyjs/auto-retry'
import { loadConfig } from './config'
import { createAuthMiddleware } from './middleware/auth'
import { registerCommands } from './commands/index'
// import JiraClient and ClaudeClient from 02-integration-clients

/**
 * Builds and starts the bot. Loads config, constructs clients, applies middleware,
 * registers commands, installs bot.catch, registers SIGTERM/SIGINT, and calls bot.start().
 * Long-polling only — webhook mode not supported.
 */
export async function startBot(): Promise<void>
```

The file should export `startBot` and call it from a top-level `startBot().catch(console.error)` at the module bottom (for direct execution via `bun run src/bot.ts`).

## Middleware Stack Order (Critical)

The order in which middleware is installed on the bot determines execution order for every incoming update. Install in this sequence:

1. `bot.api.config.use(apiThrottler())` — rate-limits outbound API calls (must be on `bot.api`, not `bot`)
2. `bot.api.config.use(autoRetry())` — retries on HTTP 429 (must be on `bot.api`)
3. `bot.use(createAuthMiddleware(allowedIds))` — auth check, silently drops unauthorized updates
4. `await registerCommands(bot, clients)` — installs command dispatch and syncs menu
5. Unknown command fallback registered via `bot.on('message', ...)` or similar after commands

## Error Matrix Reference

All handlers and `bot.catch` should map error types to these user-facing messages (imported from the integration clients' error types):

| Error type | Reply message |
|---|---|
| `JiraAuthError` | `"Jira authentication failed. Check your API token."` |
| `JiraNotFoundError` | `"Issue {key} not found."` |
| `JiraRateLimitError` | `"Jira rate limit hit. Please wait a moment and try again."` |
| `ClaudeTimeoutError` | `"Claude timed out. Please try again."` |
| `ClaudeExitError` | `"Claude returned an error. Please try again."` |
| Unknown | `"An unexpected error occurred. Please try again."` |

## Telegram Command Descriptions (for `setCommands`)

These descriptions appear in Telegram's `/` menu. Register them in `registerCommands`:

| Command | Description |
|---|---|
| `/create` | `Create a new Jira ticket` |
| `/move` | `Move a ticket to a new status` |
| `/comment` | `Add a comment to a ticket` |
| `/solve` | `Ask Claude for a solution to a ticket` |
| `/help` | `Show available commands` |

## Known Limitations and Constraints

- **Long-polling only**: `bot.start()` uses long-polling. No webhook support. The bot is designed to run as a persistent macOS launchd daemon (managed by `01-core-daemon`).
- **At-least-once delivery**: If the process crashes after sending an intermediate reply but before completing the Jira action, Telegram re-delivers the update on restart. No deduplication is implemented.
- **Command menu visibility**: `setCommands()` makes the command menu visible to all users who find the bot — including unauthorized ones. Unauthorized users see the commands but receive silence (the auth middleware drops their updates). This is intentional for a personal bot.
- **Single-user or small-team**: The allowlist is a `Set<number>` loaded once at startup. No dynamic reloading.

## TODO List for Implementer

1. Create `src/commands/index.ts` — export `Clients` interface, implement `registerCommands`
2. Create `src/bot.ts` — implement `startBot()` with full middleware stack, graceful shutdown, `bot.catch`
3. Write `tests/bot.test.ts` — all test stubs listed above, using grammytest harness and `vi.fn()` client mocks
4. Verify that `vitest run` passes all tests
5. Verify that `bot.use` and `setCommands` are called as separate operations in `registerCommands` (not conflated)
6. Confirm SIGTERM and SIGINT handlers are registered before `bot.start()` is called
7. Confirm error logging in `bot.catch` does not log the full error object or Authorization headers