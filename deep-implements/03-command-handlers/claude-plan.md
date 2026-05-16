# Implementation Plan: 03-command-handlers

## Overview

This module is the orchestration layer of a personal Jira assistant Telegram bot. It receives Telegram slash commands (`/create`, `/move`, `/comment`, `/solve`, `/help`), routes them to dedicated handlers, calls the pre-built Jira and Claude integration clients, and returns user-facing replies. The module is the "glue" layer — it does not speak to Jira or Claude directly; that work is handled by `../02-integration-clients`.

The implementation targets TypeScript/Node.js and uses **grammY** as the Telegram bot framework, chosen for its first-class TypeScript support, active maintenance, and rich plugin ecosystem. The bot is single-user or small-team (not public), governed by an allowlist of Telegram user IDs.

---

## Section 1: Project Setup and Configuration

The module lives at `03-command-handlers/` alongside the other numbered modules in the repository. Its package dependencies are:
- `grammy` — core bot framework
- `@grammyjs/commands` — command group registration and Telegram menu sync
- `@grammyjs/transformer-throttler` — outbound rate limit management
- `@grammyjs/auto-retry` — 429 handling with `retry_after` delays
- Development: `vitest`, `@grammyjs/grammytest`

Configuration is loaded at startup from a `.env` file or a JSON config file. Required values:
- Bot token (`TELEGRAM_BOT_TOKEN`)
- Jira base URL, project key, user email, API token
- Claude API key
- Allowlist of Telegram user IDs (`ALLOWED_USER_IDS` as comma-separated string, or JSON array in config)

A `Config` type captures these fields with strict typing. A `loadConfig()` function validates all required fields at startup and throws if any are missing, preventing the bot from starting in a misconfigured state.

Directory structure:
```
03-command-handlers/
  src/
    bot.ts
    config.ts
    middleware/
      auth.ts
    commands/
      index.ts
      create.ts
      move.ts
      comment.ts
      solve.ts
      help.ts
    utils/
      splitMessage.ts
      parseArgs.ts
  tests/
    commands/
      create.test.ts
      move.test.ts
      comment.test.ts
      solve.test.ts
    utils/
      splitMessage.test.ts
```

---

## Section 2: Bot Initialization and Middleware Stack

The bot is initialized in `bot.ts` with the following middleware stack, applied in order:

1. **Auth middleware** (`middleware/auth.ts`): checks `ctx.from?.id` against the loaded allowlist. If not found, logs the attempt (chatId + command, never userId for PII reasons) and `return`s — never replies (silent drop). This middleware runs before every handler.

2. **Transformer throttler**: installed on `bot.api` before handlers are registered. Automatically queues outbound API calls to respect Telegram's rate limits (30 req/s global, 1 msg/s per chat). This is especially important when `/solve` splits a long Claude response into multiple messages.

3. **Auto-retry**: installed alongside the throttler. When the Telegram API returns HTTP 429 with a `retry_after` field, auto-retry waits the specified duration and retries transparently, without any changes to handler code.

4. **Command registration**: A `CommandGroup` from `@grammyjs/commands` is populated with all five commands and their descriptions. `await myCommands.setCommands(bot)` is called once at startup to sync the command list to Telegram's UI (the `/` menu). Note: `setCommands()` is purely a UI sync call — it does NOT register handlers. Handler registration happens via `bot.use(myCommands)` which installs the `CommandGroup` as a dispatcher. These are two separate steps with different effects.

5. **Unknown command fallback**: registered after all command handlers. When no handler matched, replies with `Unknown command. Try /help`.

The main entry point calls `loadConfig()`, constructs the three integration clients (passing config values), builds the bot, applies middleware, registers handlers, and calls `bot.start()` (long-polling).

**Graceful shutdown**: `process.on('SIGTERM', ...)` and `process.on('SIGINT', ...)` handlers must be registered in `bot.ts` to call `await bot.stop()` before the process exits. Without this, in-flight Telegram updates are lost and could result in orphaned Jira tickets or duplicate operations on restart.

**Deployment assumption**: Long-polling only. The bot is designed to run as a persistent process (macOS launchd daemon via `01-core-daemon`), not in a stateless serverless environment. Webhook mode is not supported.

---

## Section 3: Authorization Middleware

The auth middleware is a grammY middleware function that runs on every update before any command handler. Its responsibilities:

- Extract `ctx.from?.id` from the incoming update
- Check the ID against the pre-loaded `Set<number>` of allowed user IDs
- If unauthorized: log `{ event: 'unauthorized', chatId }` (no userId — PII; no message content) and `return` (drop silently — never reply)
- If authorized: `return next()`

The allowlist is loaded once at startup and stored as a `Set<number>` for O(1) lookup. No dynamic reloading is needed for this use case.

**Known limitation**: Telegram's command menu (populated via `setCommands()`) is visible to all users who can find the bot — including unauthorized ones. Unauthorized users will see `/create`, `/solve`, etc. in the UI, attempt to use them, and receive silence. This is acceptable for a personal bot and avoids leaking the bot's existence via replies. The command menu cannot be scoped per-user without individual `setCommands` calls per `chat_id`.

**`bot.catch` and authorization**: The global `bot.catch` handler must not send replies to unauthorized users. Since auth middleware only `return`s (never throws), `bot.catch` will only fire for errors in authorized handler execution paths.

Function signature:
```typescript
function createAuthMiddleware(allowedIds: Set<number>): MiddlewareFn<Context>
```

---

## Section 4: Utility — Argument Parsing

`utils/parseArgs.ts` exports two functions for extracting positional arguments from grammY's `ctx.match` string:

```typescript
function parseArgs(ctx: Context): string[]
function parseFirstAndRest(input: string): { first: string; rest: string } | null
```

`parseArgs` trims the match string, splits on whitespace, and filters empty strings. Used for simple cases where each token is independent (e.g., `/help`, `/solve <key>`).

`parseFirstAndRest` uses a regex `/^(\S+)\s+([\s\S]*)$/` to capture the first whitespace-delimited token and the raw unsplit remainder. This preserves multiple spaces, tabs, and other whitespace in the trailing text — important for `/comment` (comment body may have intentional spacing) and `/move` (status name reconstruction). Returns `null` if the input contains only one token or is empty.

---

## Section 5: Utility — Message Splitter

`utils/splitMessage.ts` handles the Telegram 4096-character limit. The algorithm:

1. If text ≤ limit chars, return as single-element array — done
2. Split text into paragraphs on `\n\n` (double-newline) boundaries
3. Accumulate paragraphs into a chunk, rejoining with `\n\n`, until the next paragraph would push the chunk over the effective limit
4. When a single paragraph itself exceeds the effective limit, fall through to word-boundary splitting: find the last space before the boundary and split there
5. Last resort for a word with no spaces: hard character cut

**Part numbering**: When the result has more than one chunk, prefix every chunk with `[N/M]` (e.g., `[1/3]`, `[2/3]`). This applies even for 2-part splits — never leave the user guessing whether the response is complete.

**Effective limit**: When part numbering is applied, the prefix (`[N/M] `) consumes characters. The algorithm must reserve space for the longest possible prefix before computing chunk boundaries, so that prefixed chunks never exceed `limit`. For up to 99 parts, reserve 8 characters (`[99/99] `).

Because all responses from Claude are plain text (the spec's prompt templates instruct Claude to return plain text only), there is no Markdown entity tracking needed. The splitter is a pure string utility.

Function signature:
```typescript
function splitMessage(text: string, limit?: number): string[]
```

---

## Section 6: Handler — /create

The `/create` handler lives in `commands/create.ts`. It uses `--` as an explicit separator between title and description.

**Parsing rule**: if the input contains ` -- ` (space-dash-dash-space), everything before it is the title and everything after is the description (Path A). If no ` -- ` separator is present, the entire input is the title (Path B). This avoids single-word titles and lets users include spaces in titles naturally.

Example: `/create Fix login timeout -- auth token expires before session ends` → title = `Fix login timeout`, description = `auth token expires before session ends`
Example: `/create Fix login timeout` → title = `Fix login timeout`, description = none (Path B)

**Path A — title + description:**
Calls Claude with the enrichment prompt (title + description → formatted Jira ticket description). If Claude fails, the raw description is used unchanged (silent fallback — the user's intent to create the ticket is still fulfilled). The issue is created via `JiraClient.createIssue(title, description)`. Note: `JiraClient.createIssue` accepts plain text and internally converts to ADF format — the handler does not need to do any ADF conversion.

**Path B — title only:**
Calls Claude with a title-expansion prompt asking Claude to write a 3–5 sentence description and 3–5 acceptance criteria bullet points. If Claude fails, the issue is created with an empty description. Reply format is the same.

In both paths:
- `sendChatAction("typing")` is sent before the Claude API call
- A `setInterval` re-sends `sendChatAction("typing")` every 4 seconds until the API call resolves (typing indicator lasts only ~5 seconds; without refresh it vanishes before Claude responds)
- The interval is cleared in the `finally` block
- Jira errors are caught and replied with user-friendly messages per the error matrix

**Prompt injection defense**: Claude prompt templates wrap untrusted content in XML-style delimiters. The `{title}` substitution uses `<title>...</title>` and `{description}` uses `<description>...</description>`. This signals to Claude that the content is data, not instructions. Substitution uses simple `.replace()` calls on the template string — no recursive templating.

Claude prompt templates are defined as exported string constants (not inline) to make them testable:

```typescript
export const ENRICH_PROMPT_TEMPLATE: string  // uses <title>…</title> and <description>…</description>
export const EXPAND_PROMPT_TEMPLATE: string  // uses <title>…</title>

async function handleCreate(ctx: Context, clients: Clients): Promise<void>
```

---

## Section 7: Handler — /move

The `/move` handler lives in `commands/move.ts`. It uses `parseFirstAndRest` to extract the ticket key (first token) and the status string (raw remainder — preserves spaces in status names like "In Progress").

**Tiered matching algorithm** (applied against the available transitions from `JiraClient.transitionIssue`):

Note: `transitionIssue` in `02-integration-clients` already performs case-insensitive exact matching internally. The command handler layer needs to handle the case where no exact match is found and provide a user-friendly disambiguation. The plan should call `JiraClient.transitionIssue(key, status)` and catch `InvalidTransitionError` (which carries the `available` list), rather than fetching transitions separately.

Revised flow:
1. Parse ticket key and status using `parseFirstAndRest`
2. Call `JiraClient.transitionIssue(key, status)` — internally does case-insensitive exact matching
3. On success: reply `Moved ENG-123 → <status>`
4. On `InvalidTransitionError`: reply `Cannot move to "{status}". Available: {available.join(', ')}`
5. Other Jira errors: reply per error matrix

`sendChatAction("typing")` is sent before the API call.

```typescript
async function handleMove(ctx: Context, clients: Clients): Promise<void>
```

---

## Section 8: Handler — /comment

The `/comment` handler uses `parseFirstAndRest` to extract the ticket key (first token) and the raw comment text (unsplit remainder — preserves the user's original spacing and formatting). It calls `JiraClient.addComment(key, text)` and replies `Comment added to ENG-123`. Note: `JiraClient.addComment` accepts plain text and internally converts to ADF.

`sendChatAction("typing")` is sent before the API call.

```typescript
async function handleComment(ctx: Context, clients: Clients): Promise<void>
```

---

## Section 9: Handler — /solve

The `/solve` handler is the most complex due to Claude integration and message splitting.

Steps:
1. Parse ticket key from args; if absent, reply with usage string and return
2. Send immediate intermediate reply: `Analyzing ENG-123 with Claude...`
3. Send `sendChatAction("typing")`, then start `setInterval` every 4 seconds to re-send it (typing indicator lasts ~5 seconds; periodic refresh keeps it alive during the Claude call)
4. Call `JiraClient.getIssue(key)` → `{ key, summary, status, description }`
5. Build the solve prompt by substituting issue fields into the template using `<field>...</field>` XML delimiters for prompt injection defense
6. Call `ClaudeClient.ask(prompt)` — no retry on failure; fail immediately
7. Clear the typing interval in `finally`
8. Pass response through `splitMessage()`
9. Send each chunk sequentially via `ctx.reply()` (throttler/auto-retry handle rate limits automatically)

**Known limitation**: Sequential chunk sending with the throttler's 1 msg/s limit means a 10-chunk response takes ~10 seconds to deliver. This is expected behavior for a personal bot.

**At-least-once delivery**: If the process crashes between steps 2 and 4 (after sending "Analyzing…" but before creating the ticket), Telegram will re-deliver the update on restart. This means a user could trigger the same command twice. No deduplication is implemented — this is an accepted risk for a personal bot.

Error handling: if Claude throws (timeout, API error), catch and reply with the appropriate message from the error matrix. Jira errors (not found, auth) are caught before the Claude call.

```typescript
export const SOLVE_PROMPT_TEMPLATE: string  // uses <key>, <title>, <status>, <description> delimiters

async function handleSolve(ctx: Context, clients: Clients): Promise<void>
```

---

## Section 10: Handler — /help

The `/help` handler sends a fixed string — no API calls, no parsing. It is a pure `ctx.reply()` call with the command reference text.

```typescript
async function handleHelp(ctx: Context): Promise<void>
```

The text is defined as a constant so it can be tested independently of the handler wiring.

---

## Section 11: Command Registration

`commands/index.ts` assembles everything:

1. Defines `interface Clients { jira: JiraClient; claude: ClaudeClient }` — the shape of the clients object passed to all handlers
2. Creates a `CommandGroup` instance
3. Registers all five command handlers. Handlers receive clients via closure capture — `registerCommands` captures the `clients` argument and each handler function closes over it. This is how handlers get access to `JiraClient` and `ClaudeClient` without global state or `ctx` decoration.
4. The `CommandGroup` is passed to `bot.use(myCommands)` — this is the step that installs actual handler dispatch logic
5. Separately, `await myCommands.setCommands(bot)` syncs the command list to Telegram's UI (the `/` menu). These two operations are distinct: `bot.use` wires handlers; `setCommands` updates the menu display.

Registration is called once from `bot.ts` during startup, after clients are constructed and middleware is installed.

```typescript
interface Clients {
  jira: JiraClient
  claude: ClaudeClient
}

async function registerCommands(bot: Bot, clients: Clients): Promise<void>
```

---

## Section 12: Error Handling Strategy

All handlers follow the same pattern:
- Wrap the entire handler body in a `try/catch`
- For expected typed error types (from `02-integration-clients`), match on `.type` discriminant and reply with the specific error matrix message
- For unexpected errors: log `{ event: 'error', command, errorMessage: error.message, errorType: error.type ?? 'unknown' }` — never log the full error object (can contain Authorization headers from fetch errors), never log ticket key values or prompt content

A global grammY error handler (`bot.catch`) is also installed as a last resort for unhandled rejections that escape the per-handler try/catch.

The Claude fallback in `/create` (use raw description if enrichment fails) is the only place where an error is swallowed without notifying the user — this is intentional because the user's intent (create a ticket) is still fulfilled.

---

## Section 13: Testing Strategy

Testing uses **Vitest** and **@grammyjs/grammytest**, which runs the bot fully in-memory without making real Telegram API calls.

### Unit tests (pure functions)

- `splitMessage.test.ts`: tests edge cases — exact limit, over limit, multiple paragraphs, single long word, part numbering threshold
- `parseArgs.test.ts`: empty match, single arg, multiple tokens, extra whitespace

### Integration-style tests (handler tests with grammytest)

Each command handler is tested with a mocked `JiraClient` and `ClaudeClient` (using Vitest's `vi.fn()` / mock objects). The grammytest harness fires an in-memory update (simulating a Telegram message) and asserts on the reply text.

Key test scenarios per handler:

**create.test.ts:**
- `/create Fix login bug` → Claude expand called, issue created, correct reply
- `/create Fix login bug because auth middleware is wrong` → Claude enrich called
- Claude failure fallback → issue still created with raw/empty description
- Missing args → usage reply

**move.test.ts:**
- Exact substring match → `Moved ENG-1 → In Progress`
- Zero matches → available statuses listed
- Multiple matches → disambiguation reply
- Ticket not found → error reply

**comment.test.ts:**
- Normal flow → `Comment added to ENG-1`
- Missing text → usage reply

**solve.test.ts:**
- Normal response (< 4096 chars) → single reply
- Long response (> 4096 chars) → multiple reply calls
- Claude timeout → error reply
- Ticket not found → error reply (before Claude is called)

**auth.test.ts (new):**
- Authorized user ID → middleware calls `next()`
- Unauthorized user ID → middleware returns without calling `next()`, no reply sent
- `ctx.from` is undefined → treated as unauthorized (no crash)

**Additional test cases:**
- `solve.test.ts`: add a case where the Jira description is very long (10,000+ chars) to verify prompt construction doesn't crash and the full description is passed to Claude
- `create.test.ts`: verify `/create` with no `--` separator uses the expand path; with `--` separator uses the enrich path

All tests run against real handler logic; only the integration clients (JiraClient, ClaudeClient) and Telegram API are mocked.
