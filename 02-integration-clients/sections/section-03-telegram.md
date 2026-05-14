Now I have all the context needed to generate the section content for `section-03-telegram`.

# Section 03: TelegramClient

## Overview

This section implements the `TelegramClient` — a thin, typed wrapper around the grammY `Bot` instance. It handles the bot's lifecycle (polling start/stop), slash command registration with an authorization gate, error-to-reply middleware, and message chunking for the Telegram 4096-character limit.

**Depends on:** `section-01-foundation` (typed error classes from `src/errors.ts`, project scaffolding).

**Blocks:** Nothing. Can be implemented in parallel with `section-02-adf-helpers` and `section-05-claude`.

---

## Files to Create

```
02-integration-clients/src/telegram/types.ts
02-integration-clients/src/telegram/splitMessage.ts
02-integration-clients/src/telegram/TelegramClient.ts
02-integration-clients/tests/telegram.test.ts
```

The `src/index.ts` created in section-01-foundation must re-export everything from these files. Update it once this section is complete.

---

## Tests First

File: `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/02-integration-clients/tests/telegram.test.ts`

Write tests before implementation. Use **Vitest** (`vi.fn()`, `vi.spyOn()`). Mock the grammY `Bot` class constructor and its instance methods (`command`, `on`, `catch`, `start`, `stop`, `api.sendMessage`). No real Telegram network calls.

### Test Groups

#### `splitMessage` helper (pure unit, no mocks)

- Short text (< 4096 chars) is returned as a single-element array.
- Text containing `\n\n` paragraph breaks splits at those boundaries, producing one element per paragraph block.
- A paragraph block that still exceeds the limit is further split at word boundaries (spaces), not mid-word.
- A single word longer than the limit is hard-split at the character limit (no infinite loop).
- Text that is exactly 4096 characters produces a single-element array.

#### Authorization gate

- Authorized `userId` (present in `allowedUserIds`) — the registered command handler is called.
- Unauthorized `userId` (not in `allowedUserIds`) — the registered command handler is NOT called, and no reply is sent.
- `allowedUserIds: []` — all commands are blocked regardless of userId.

#### `sendMessage`

- `sendMessage(chatId, shortText)` results in exactly one `bot.api.sendMessage` call.
- `sendMessage(chatId, textOver4096)` results in multiple `bot.api.sendMessage` calls (one per chunk).

#### Lifecycle

- `onCommand('start', handler)` causes `bot.command('start', ...)` to be called at registration time.
- `startPolling()` calls `bot.start({ drop_pending_updates: true })`.
- `stopPolling()` awaits `bot.stop()`.

#### Error middleware

- `bot.catch(...)` is registered during construction (before any command is dispatched).
- `JiraAuthError` caught by middleware → reply contains `"authentication failed"`.
- `JiraNotFoundError` with `issueKey` → reply contains the issue key string.
- `InvalidTransitionError` → reply lists the `available` transitions.
- `ClaudeTimeoutError` → reply contains the timeout duration in milliseconds.
- Unknown/generic error → reply with a generic message; error is logged.

#### Non-command fallback

- `bot.on('message:text', ...)` is registered (for the `/help` hint).

---

## Types

File: `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/02-integration-clients/src/telegram/types.ts`

```typescript
interface TelegramConfig {
  token: string
  allowedUserIds: number[]  // only these user IDs may trigger commands; empty = block all
}

type CommandHandler = (ctx: CommandContext) => Promise<void>

interface CommandContext {
  chatId: number
  userId: number
  command: string
  args: string[]
  rawText: string
  reply(text: string): Promise<void>
}
```

Export all three. These are the only types this module owns. `Logger` is imported from `01-core-daemon` (treat as `{ info(obj: object): void; error(obj: object): void }` until that module is available — stub or import as needed).

---

## `splitMessage` Helper

File: `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/02-integration-clients/src/telegram/splitMessage.ts`

Signature:

```typescript
export function splitMessage(text: string, limit: number = 4096): string[]
```

Implementation strategy (in order):

1. Split the full text on `\n\n` (paragraph boundaries) to get candidate chunks.
2. Accumulate candidates into final chunks, each no longer than `limit`.
3. If a candidate paragraph block itself exceeds `limit`, split it further at word boundaries (spaces), then hard-split any word that still exceeds `limit`.
4. Return the array of final chunks. Never return an empty array for non-empty input.

This prevents Telegram's 4096-character message limit from causing API errors when the bot produces long replies.

---

## `TelegramClient` Implementation

File: `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/02-integration-clients/src/telegram/TelegramClient.ts`

### Constructor

```typescript
constructor(config: TelegramConfig, logger: Logger)
```

- Instantiate a grammY `Bot` with `config.token`.
- Immediately register the error middleware via `bot.catch(...)`.
- Register the non-command fallback via `bot.on('message:text', ...)` — register this last, after all `onCommand` calls would naturally occur, but the fallback registration itself happens at construction time.

### `onCommand(command: string, handler: CommandHandler): void`

Calls `bot.command(command, async (ctx) => { ... })`. Inside the handler:

1. Read `ctx.from?.id`. Treat `ctx.from` as always present (personal bot, direct messages only, no channel post support).
2. If `ctx.from.id` is not in `config.allowedUserIds`, silently return — no reply. Log the unauthorized attempt: `{ event: 'unauthorized', chatId, command }`. Never log the userId to avoid PII retention.
3. Extract from grammY context: `chatId = ctx.chat.id`, `userId = ctx.from.id`, `command`, `args = ctx.message?.text?.split(' ').slice(1) ?? []`, `rawText = ctx.message?.text ?? ''`.
4. Build a `CommandContext` where `reply(text)` calls `this.sendMessage(chatId, text)` (routes through chunking).
5. Call the registered `handler(commandContext)`.

### `sendMessage(chatId: number, text: string): Promise<void>`

1. Call `splitMessage(text, 4096)` to obtain an array of chunks.
2. For each chunk, call `await bot.api.sendMessage(chatId, chunk)` sequentially.

### `startPolling(): void`

```typescript
bot.start({ drop_pending_updates: true })
```

The `drop_pending_updates: true` option discards stale updates that accumulated while the daemon was offline — processing hour-old commands would produce confusing results. grammY manages the getUpdates loop, offset tracking, and `TimedOut` error suppression internally.

### `stopPolling(): Promise<void>`

```typescript
await bot.stop()
```

Gracefully stops the polling loop after the current update cycle completes.

### Error Middleware

Registered via `bot.catch(async (err) => { ... })` at construction time. Inspects `err.error` for typed error classes. Always replies via `err.ctx.reply(message)`.

Mapping rules:

| Error type | Reply message |
|---|---|
| `JiraAuthError` | `"Jira authentication failed. Check your API token."` |
| `JiraNotFoundError` | `"Issue {issueKey} not found."` |
| `InvalidTransitionError` | `"Transition '{attempted}' not found. Valid: {available.join(', ')}"` |
| `ClaudeTimeoutError` | `"Claude timed out after {timeoutMs}ms."` |
| `ClaudeExitError` | `"Claude subprocess failed (exit {exitCode})."` |
| Anything else | `"Something went wrong. Try again."` — also log: `{ event: 'error', errorType: 'unknown', chatId }` |

For all cases, log: `{ event: 'error', errorType: err.error?.type ?? 'unknown', chatId }`. Never log userId.

### Non-Command Fallback

Registered via:

```typescript
bot.on('message:text', ctx => ctx.reply('Use /help to see available commands.'))
```

This fires only when no command handler matches. Register it last in the constructor body so it does not intercept valid commands.

### Logging Rules

- On command receipt: `{ event: 'command', chatId, command, argCount }`.
- On unauthorized attempt: `{ event: 'unauthorized', chatId, command }`.
- On error handler: `{ event: 'error', errorType, chatId }`.
- Never log `userId` values anywhere (PII).
- Never log message content.

---

## Key Constraints and Edge Cases

- **Authorization is silent**: Unauthorized user messages produce no reply — silence provides no information to an attacker about why the request was rejected.
- **`allowedUserIds: []` blocks everyone**: The check `config.allowedUserIds.includes(userId)` returns `false` for an empty array, so all commands are blocked.
- **`reply` on `CommandContext` routes through `sendMessage`**: This ensures chunking applies to all outbound replies, not just direct calls to `sendMessage`.
- **Error middleware is registered at construction time**: It is not optional and must always be in place before polling starts.
- **Non-command fallback must be registered last**: `bot.on('message:text', ...)` fires after all `bot.command(...)` handlers have had a chance to match. Registering it first could cause it to intercept commands.
- **`ctx.from` assumed present**: The bot is personal (direct messages only). No channel post handling. If `ctx.from` is somehow absent, `ctx.from?.id` will be `undefined`, which will not match any `allowedUserIds` entry and will silently drop the request.
- **Telegram 4096-char limit**: Always route through `sendMessage` (not `ctx.reply` directly) inside `CommandContext.reply` so chunking is applied consistently.

---

## Implementation Notes (Actual)

**Status: COMPLETE — 25/25 new tests passing (61 total)**

### Files Created

- `02-integration-clients/src/telegram/types.ts` — TelegramConfig, CommandHandler, CommandContext
- `02-integration-clients/src/telegram/splitMessage.ts` — splitMessage(), splitParagraph()
- `02-integration-clients/src/telegram/TelegramClient.ts` — TelegramClient class
- `02-integration-clients/tests/telegram.test.ts` — 25 tests
- `02-integration-clients/index.ts` — updated to export telegram module

### Deviations from Plan

- `startPolling()` attaches `.catch()` to `bot.start()` promise to log startup failures (review finding)
- Error middleware wraps `err.ctx.reply()` in try/catch (review finding)
- Double-logging bug fixed: `logger.info` fires once per error; `logger.error` only for unknown errors (review finding)
- Logger is a local interface stub with TODO comment for 01-core-daemon integration

### Key Behaviors

- Unauthorized userId: silent drop, no reply, logs `{ event: 'unauthorized', chatId, command }`
- `CommandContext.reply` routes through `sendMessage` → chunking always applied
- Error middleware maps all 5 typed errors + unknown to user-friendly messages
- `splitMessage` splits on `\n\n` paragraph boundaries, then word boundaries, then hard-splits

## Dependency Notes

- `src/errors.ts` (from section-01-foundation) must exist and export: `JiraAuthError`, `JiraNotFoundError`, `InvalidTransitionError`, `ClaudeTimeoutError`, `ClaudeExitError`. These are used by the error middleware. Do not redefine them here.
- `Logger` type: import from `01-core-daemon` if available. If not yet available, use a local interface stub `{ info(obj: object): void; error(obj: object): void }` and replace on integration.
- grammY is listed as a dependency in `package.json` (created in section-01-foundation): `"grammy": "^1.x"`. It must be installed before this section can be built.