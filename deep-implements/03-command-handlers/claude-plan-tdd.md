# TDD Plan: 03-command-handlers

## Testing Approach

New project. Uses **Vitest** with `vi.fn()` mocks and `@grammyjs/grammytest` for in-memory bot dispatch. Test files in `tests/` mirroring `src/` structure. Run with `vitest run`. No real Telegram API calls, no real Jira or Claude calls.

---

## Section 1: Project Setup and Configuration

**Tests to write first (`tests/config.test.ts`):**

- Test: `loadConfig()` with all required fields present → returns valid `Config` object
- Test: `loadConfig()` with missing `TELEGRAM_BOT_TOKEN` → throws with a descriptive message
- Test: `loadConfig()` with missing `ALLOWED_USER_IDS` → throws
- Test: `loadConfig()` with `ALLOWED_USER_IDS = "123,456"` → Set contains `123` and `456`
- Test: `loadConfig()` with `ALLOWED_USER_IDS = "123, 456"` (spaces) → Set contains `123` and `456` (whitespace trimmed)
- Test: `loadConfig()` with `ALLOWED_USER_IDS = "abc,456"` (non-numeric) → NaN entry is filtered out, Set contains only `456`
- Test: `loadConfig()` with empty `ALLOWED_USER_IDS = ""` → Set is empty (not a crash)

---

## Section 2: Bot Initialization and Middleware Stack

**Tests to write first (`tests/bot.test.ts`):**

- Test: bot is constructed with the token from config
- Test: auth middleware is installed (verify it runs before command handlers — use grammytest to simulate unauthorized update and confirm handler never fires)
- Test: `registerCommands` is called during bot initialization
- Test: `bot.catch` is registered (verify it intercepts errors thrown in handlers)

---

## Section 3: Authorization Middleware

**Tests to write first (`tests/middleware/auth.test.ts`):**

- Test: update from authorized userId → `next()` is called
- Test: update from unauthorized userId → `next()` is NOT called, no reply sent
- Test: `ctx.from` is `undefined` → treated as unauthorized, `next()` not called
- Test: `allowedIds` is an empty `Set` → all users unauthorized
- Test: unauthorized attempt → logger receives `{ event: 'unauthorized', chatId }` (NOT userId)
- Test: authorized attempt → logger does NOT receive any 'unauthorized' event

---

## Section 4: Utility — Argument Parsing

**Tests to write first (`tests/utils/parseArgs.test.ts`):**

`parseArgs`:
- Test: empty match string → empty array
- Test: single token `"ENG-1"` → `["ENG-1"]`
- Test: multiple tokens `"ENG-1 In Progress"` → `["ENG-1", "In", "Progress"]`
- Test: extra whitespace `"  ENG-1   In  Progress  "` → `["ENG-1", "In", "Progress"]` (trimmed + filtered)

`parseFirstAndRest`:
- Test: `"ENG-1 Hello   world"` → `{ first: "ENG-1", rest: "Hello   world" }` (multiple spaces preserved)
- Test: `"ENG-1"` (single token) → `null`
- Test: `""` (empty) → `null`
- Test: `"ENG-1 "` (trailing space after first token) → result depends on implementation — document expected behavior (either null or `{ first, rest: "" }`)

---

## Section 5: Utility — Message Splitter

**Tests to write first (`tests/utils/splitMessage.test.ts`):**

- Test: text ≤ 4096 chars → single-element array, no prefix
- Test: text exactly 4096 chars → single-element array
- Test: text 4097 chars → two chunks, both prefixed `[1/2]` and `[2/2]`
- Test: text with `\n\n` boundaries → splits at paragraph boundaries, not mid-word
- Test: single paragraph longer than limit → splits at last space before limit
- Test: single word longer than limit → hard character cut (no infinite loop)
- Test: multi-part split → `\n\n` preserved between rejoined paragraphs within a chunk
- Test: 10-part split → all chunks prefixed `[1/10]` through `[10/10]`
- Test: prefix does not push any chunk over 4096 chars (the effective limit accounts for prefix length)
- Test: `splitMessage("", 4096)` → returns `[""]` or `[]` — document expected behavior

---

## Section 6: Handler — /create

**Tests to write first (`tests/commands/create.test.ts`):**

- Test: `/create Fix login timeout -- auth expires too early` → `--` separator detected, enrich path used, Claude called with title and description, issue created, reply contains `"Created:"` and `"ENG-"`
- Test: `/create Fix login timeout` (no `--`) → expand path used, Claude called with title only, issue created
- Test: Claude fails during enrichment → raw description used instead (issue still created)
- Test: Claude fails during expansion → issue created with empty description (no crash, no error reply)
- Test: `/create` with no args → usage string reply
- Test: Jira `createIssue` throws `JiraAuthError` → reply contains "auth" or "token"
- Test: ENRICH_PROMPT_TEMPLATE contains `<title>` and `<description>` XML delimiters
- Test: EXPAND_PROMPT_TEMPLATE contains `<title>` XML delimiter

---

## Section 7: Handler — /move

**Tests to write first (`tests/commands/move.test.ts`):**

- Test: `/move ENG-1 In Progress` → `JiraClient.transitionIssue("ENG-1", "In Progress")` called, reply `"Moved ENG-1 → In Progress"`
- Test: `transitionIssue` throws `InvalidTransitionError` → reply contains `"Available:"` and the available transitions list
- Test: `/move ENG-1` (no status) → usage string reply
- Test: `/move` (no args) → usage string reply
- Test: Jira `JiraNotFoundError` → reply contains issue key
- Test: status with multiple words `"In Progress"` preserved in `transitionIssue` call (uses `parseFirstAndRest`, not split+rejoin)

---

## Section 8: Handler — /comment

**Tests to write first (`tests/commands/comment.test.ts`):**

- Test: `/comment ENG-1 Fixed the bug with   extra spaces` → `addComment("ENG-1", "Fixed the bug with   extra spaces")` called (spaces preserved), reply `"Comment added to ENG-1"`
- Test: `/comment ENG-1` (no comment text) → usage string reply
- Test: `/comment` (no args) → usage string reply
- Test: `JiraNotFoundError` → reply contains issue key

---

## Section 9: Handler — /solve

**Tests to write first (`tests/commands/solve.test.ts`):**

- Test: `/solve ENG-1` → sends intermediate "Analyzing…" reply, calls `getIssue`, calls `ClaudeClient.ask`, sends Claude response as final reply
- Test: Claude response ≤ 4096 chars → single reply call (besides the intermediate)
- Test: Claude response > 4096 chars → multiple reply calls, each prefixed `[N/M]`
- Test: `JiraNotFoundError` → error reply sent before Claude is ever called
- Test: `ClaudeTimeoutError` → reply contains "timed out"
- Test: `ClaudeExitError` → reply contains "error"
- Test: `/solve` with no args → usage string reply
- Test: Jira description is 10,000+ chars → prompt construction completes, full description passed to `ClaudeClient.ask`
- Test: SOLVE_PROMPT_TEMPLATE contains `<description>` and `<title>` XML delimiters

---

## Section 10: Handler — /help

**Tests to write first (`tests/commands/help.test.ts`):**

- Test: `/help` → reply contains all five command names (`/create`, `/move`, `/comment`, `/solve`, `/help`)
- Test: reply is a string constant (no API calls made — `JiraClient` and `ClaudeClient` never called)
- Test: `HELP_TEXT` constant is exported and testable independently

---

## Section 11: Command Registration

**Tests to write first (part of `tests/bot.test.ts`):**

- Test: `registerCommands(bot, clients)` calls `setCommands(bot)` to sync Telegram UI menu
- Test: `registerCommands(bot, clients)` calls `bot.use(commands)` to install handler dispatch
- Test: handlers receive clients via closure — simulate a handler call with mocked `Clients` and confirm the mock is invoked
- Test: `Clients` type is exported and structurally `{ jira: JiraClient, claude: ClaudeClient }`

---

## Section 12: Error Handling Strategy

**Tests to write first (cross-cutting, in each handler test file):**

- Test: each handler catches `JiraAuthError` → reply contains auth error message
- Test: each handler catches `JiraNotFoundError` → reply contains issue key
- Test: each handler catches an unknown error → reply with generic message, logger called with `{ event: 'error', errorMessage: string }` (NOT the full error object, NOT the Authorization header)
- Test: `bot.catch` handler → verify it fires when a handler throws, and replies with generic message
- Test: logger never receives any string matching the Jira `apiToken` value

---

## Section 13: Testing Strategy (Meta)

Test runner: `vitest run`

Framework: `@grammyjs/grammytest` for in-memory bot simulation — fires real grammY middleware pipelines without HTTP calls.

Client mocks: `vi.fn()` for `JiraClient` and `ClaudeClient` methods.

Logging mock: inject a `vi.fn()` logger into each tested component; assert on logged objects.

All handler tests use the grammytest harness to simulate `ctx` rather than constructing raw grammY context objects manually.
