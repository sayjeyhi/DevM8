Now I have all the necessary context. Let me generate the section content for `section-05-move-comment-help-handlers`.

# Section 05: Move, Comment, and Help Handlers

## Overview

This section implements three command handlers: `/move`, `/comment`, and `/help`. These handlers are simpler than `/create` and `/solve` but follow the same structural conventions established by `section-01-foundation` and rely on the utility functions from `section-03-utils`.

## Dependencies

- **section-01-foundation** must be complete: `Config` type, project setup, `tsconfig.json`, `package.json` with `grammy` installed.
- **section-03-utils** must be complete: `parseFirstAndRest` from `src/utils/parseArgs.ts` is used by both `/move` and `/comment`.
- **section-04-create-handler** is a sibling and does not block this section — they can be implemented in parallel.

## Files Created (Actual Paths)

Plan specified `03-command-handlers/src/commands/` but all code lives in the root `src/bot/commands/` (established by section-01). Actual paths:

- `src/bot/commands/move.ts`
- `src/bot/commands/comment.ts`
- `src/bot/commands/help.ts`
- `tests/bot/commands/move.test.ts`
- `tests/bot/commands/comment.test.ts`
- `tests/bot/commands/help.test.ts`

## Implementation Notes

- Used `ctx.replyWithChatAction("typing")` (not `sendChatAction`) — matches grammY convention from create.ts.
- Added `JiraAuthError` catch in move.ts and comment.ts (beyond plan spec) to prevent credential data reaching the generic logger.
- Tests use `bun:test` (not vitest) — 23 tests, 0 failures. Full suite: 256 pass.
- Duplicate "preserves spacing" test consolidated into "valid args" test in comment.test.ts.

---

## Tests First

All tests use Vitest and `@grammyjs/grammytest` for in-memory bot dispatch. `JiraClient` is mocked with `vi.fn()`. No real Telegram API calls are made.

### `tests/commands/move.test.ts`

Test cases to implement:

- `/move ENG-1 In Progress` → `JiraClient.transitionIssue("ENG-1", "In Progress")` is called; reply text is `"Moved ENG-1 → In Progress"`
- `transitionIssue` throws `InvalidTransitionError` (with `.available` list `["To Do", "Done"]`) → reply contains `"Available:"` and both transition names
- `/move ENG-1` (status token absent — `parseFirstAndRest` returns `null` for the rest) → usage string reply
- `/move` (no args at all) → usage string reply
- `transitionIssue` throws `JiraNotFoundError` → reply contains the issue key
- Status with multiple words `"In Progress"` is passed as a single string to `transitionIssue` (confirms `parseFirstAndRest` is used, not `split()`)
- `sendChatAction("typing")` is sent before the API call

### `tests/commands/comment.test.ts`

Test cases to implement:

- `/comment ENG-1 Fixed the bug with   extra spaces` → `addComment("ENG-1", "Fixed the bug with   extra spaces")` called with spaces preserved; reply is `"Comment added to ENG-1"`
- `/comment ENG-1` (no comment text — `parseFirstAndRest` returns `null`) → usage string reply
- `/comment` (no args) → usage string reply
- `addComment` throws `JiraNotFoundError` → reply contains issue key
- `sendChatAction("typing")` is sent before the API call

### `tests/commands/help.test.ts`

Test cases to implement:

- `/help` → reply contains all five command names: `/create`, `/move`, `/comment`, `/solve`, `/help`
- `JiraClient` and `ClaudeClient` are never called — pure reply, no API calls
- `HELP_TEXT` constant is exported from `src/commands/help.ts` and contains all five command names (testable without a bot harness)

---

## Implementation Details

### Handler: `/move` (`src/commands/move.ts`)

**Purpose:** Transition a Jira issue to a new status by ticket key and status name.

**Parsing:** Use `parseFirstAndRest` from `src/utils/parseArgs.ts` to split the command argument string into the ticket key (first token) and the raw status remainder. Using `parseFirstAndRest` (not `split()`) is essential because status names like `"In Progress"` contain spaces and must be passed as a single string to `transitionIssue`.

**Flow:**

1. Read `ctx.match` — the portion of the message after `/move`
2. If the match string is empty or blank, reply with a usage string and return
3. Call `parseFirstAndRest(ctx.match)`. If `null` is returned (only one token — key with no status), reply with a usage string and return
4. Send `ctx.sendChatAction("typing")` before the API call
5. Call `jira.transitionIssue(key, status)` — this client method performs case-insensitive exact matching internally. The handler does not need to do any fuzzy matching.
6. On success: reply `Moved <key> → <status>`
7. On `InvalidTransitionError`: reply with `Cannot move to "<status>". Available: <available.join(', ')>`. The `InvalidTransitionError` carries an `.available` string array with all valid transition names.
8. On `JiraNotFoundError`: reply with a message including the issue key
9. On other errors: reply with a generic error message

```typescript
/**
 * Handles the /move command.
 * Parses the ticket key and status from ctx.match using parseFirstAndRest,
 * then calls jira.transitionIssue. Catches InvalidTransitionError to surface
 * the available transitions list to the user.
 */
async function handleMove(ctx: Context, clients: Clients): Promise<void>
```

### Handler: `/comment` (`src/commands/comment.ts`)

**Purpose:** Add a plain-text comment to a Jira issue.

**Parsing:** Use `parseFirstAndRest` from `src/utils/parseArgs.ts` to split into the ticket key (first token) and the raw comment body (unsplit remainder). This preserves the user's original spacing, including multiple consecutive spaces and tabs, which may be intentional in comment text.

**Flow:**

1. Read `ctx.match`
2. If empty/blank, reply with usage and return
3. Call `parseFirstAndRest(ctx.match)`. If `null`, reply with usage and return
4. Send `ctx.sendChatAction("typing")` before the API call
5. Call `jira.addComment(key, text)` — `JiraClient.addComment` accepts plain text and converts to ADF internally; the handler passes the raw string unchanged
6. On success: reply `Comment added to <key>`
7. On `JiraNotFoundError`: reply with a message including the issue key
8. On other errors: reply with a generic error message

```typescript
/**
 * Handles the /comment command.
 * Parses the ticket key and raw comment text from ctx.match using parseFirstAndRest,
 * then calls jira.addComment. The comment text is passed unmodified (spaces preserved).
 */
async function handleComment(ctx: Context, clients: Clients): Promise<void>
```

### Handler: `/help` (`src/commands/help.ts`)

**Purpose:** Return a static command reference to the user. No API calls, no parsing.

**The `HELP_TEXT` constant** must be exported so it can be tested in isolation (without constructing a bot context). It must mention all five commands: `/create`, `/move`, `/comment`, `/solve`, `/help`, with at minimum a short description of the purpose and argument format of each.

**Flow:**

1. Call `ctx.reply(HELP_TEXT)` and return. No try/catch required — no API calls are made.

```typescript
/** Static command reference text. Exported for independent testability. */
export const HELP_TEXT: string

/**
 * Handles the /help command.
 * Sends HELP_TEXT as a reply. Makes no API calls.
 */
async function handleHelp(ctx: Context): Promise<void>
```

---

## Error Handling

All error types referenced below come from the `02-integration-clients` module. The handlers only need to import the error type discriminants (`.type` string or the error class itself) — they do not call Jira or Claude directly.

Error type reference:

- `InvalidTransitionError` — thrown by `transitionIssue` when the requested status does not match any available transition. Carries `.available: string[]` listing valid transition names.
- `JiraNotFoundError` — thrown when the ticket key does not exist in the project.
- `JiraAuthError` — thrown when the Jira API token is invalid or expired.

Each handler wraps its body in a `try/catch`. For `InvalidTransitionError` and `JiraNotFoundError`, the catch block replies with a specific user-facing message (see flows above). For unknown errors, reply with a generic message and log `{ event: 'error', command: '<name>', errorMessage: error.message }`. Never log the full error object (it may contain authorization headers). Never log ticket key values or comment/status text.

---

## Conventions to Follow

These handlers follow the same conventions as `section-04-create-handler`:

- Handlers are named `handleMove`, `handleComment`, `handleHelp` and exported as named exports.
- They accept `(ctx: Context, clients: Clients)` — except `handleHelp`, which needs no `clients` argument since it makes no API calls.
- `sendChatAction("typing")` is called before every I/O operation (not needed for `/help`).
- Usage replies are returned immediately with `return` after replying.
- There is no `setInterval` typing refresh for these handlers — the Jira calls are fast enough that a single `sendChatAction` is sufficient.
- Prompt injection defense (XML delimiters) is not applicable here — these handlers do not call Claude.

---

## Clients Interface Reference

The `Clients` interface (defined in `src/commands/index.ts` in `section-07-registration-and-bot`) has this shape:

```typescript
interface Clients {
  jira: JiraClient
  claude: ClaudeClient
}
```

`handleMove` and `handleComment` only use `clients.jira`. `handleHelp` uses neither. Import the `Clients` type from `src/commands/index.ts` once that section is complete, or define a local inline type for development and replace with the import later.