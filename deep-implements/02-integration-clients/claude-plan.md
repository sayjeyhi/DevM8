# Implementation Plan: 02-integration-clients

## What We Are Building

This module provides three thin, typed API client classes used by the command handler layer (`03-command-handlers`) of the jira-assistant Telegram bot. The three clients are:

1. **TelegramClient** — wraps grammY to manage the bot's long-polling lifecycle, command handler registration, and error-to-user reply pipeline.
2. **JiraClient** — communicates with Jira Cloud REST API v3 using HTTP Basic auth (email + API token), scoped to a single configured project.
3. **ClaudeClient** — spawns the local `claude` CLI binary as a subprocess, sends a prompt, captures the response, and enforces a configurable timeout.

Each client is a class that takes a typed config slice and a Logger instance. There is no global state. The module exports all three client classes and their supporting types from a single `index.ts`.

---

## Architecture Overview

```
03-command-handlers
       │ uses
       ▼
02-integration-clients/src/
  ├── telegram/   TelegramClient (grammY wrapper)
  ├── jira/       JiraClient (raw fetch, Jira Cloud v3)
  ├── claude/     ClaudeClient (Bun.spawn subprocess)
  └── index.ts    re-exports all clients and types
```

The three clients have no dependency on each other. Command handlers import them independently. The only shared dependency is the `Logger` type imported from `01-core-daemon`.

---

## Typed Error Hierarchy

A small, shared error module defines the typed errors all clients can throw. Each class has an explicit `readonly type` discriminant for tag-based error discrimination alongside `instanceof`. grammY's error middleware inspects the `type` field to compose a user-facing Telegram reply.

```typescript
// errors.ts (shared)
class JiraAuthError extends Error         // 401: readonly type = 'JIRA_AUTH' as const; carries no extra data
class JiraPermissionError extends Error   // 403: readonly type = 'JIRA_PERMISSION' as const
class JiraNotFoundError extends Error     // 404: readonly type = 'JIRA_NOT_FOUND' as const; carries issueKey
class JiraRateLimitError extends Error    // 429: readonly type = 'JIRA_RATE_LIMIT' as const; carries retryAfter?: number
class JiraServerError extends Error       // 5xx: readonly type = 'JIRA_SERVER' as const; carries status
class JiraTimeoutError extends Error      // request timed out: readonly type = 'JIRA_TIMEOUT' as const
class InvalidTransitionError extends Error // transition name not found: readonly type = 'INVALID_TRANSITION' as const; carries attempted + available[]
class ClaudeTimeoutError extends Error    // subprocess killed: readonly type = 'CLAUDE_TIMEOUT' as const; carries timeoutMs
class ClaudeExitError extends Error       // subprocess exited non-zero: readonly type = 'CLAUDE_EXIT' as const; carries exitCode + stderr
```

---

## Client 1: TelegramClient

### Role

TelegramClient is a thin orchestration wrapper around a grammY `Bot` instance. Its main responsibility is:
- Setting up the bot with the configured token.
- Registering slash command handlers via `onCommand()`.
- Wiring a fallback reply for non-command messages.
- Registering the error middleware that maps typed errors to Telegram replies.
- Starting and stopping the polling loop.

### Config

```typescript
interface TelegramConfig {
  token: string
  allowedUserIds: number[]  // only these user IDs can trigger commands; empty = block all
}
```

### Public Interface

```typescript
interface TelegramClient {
  startPolling(): void
  stopPolling(): Promise<void>
  sendMessage(chatId: number, text: string): Promise<void>
  onCommand(command: string, handler: CommandHandler): void
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

### Implementation Notes

**grammY setup**: Instantiate a grammY `Bot` with `config.token`. All method calls (`bot.command()`, `bot.on()`, `bot.catch()`, `bot.start()`, `bot.stop()`) delegate to this internal instance.

**Authorization gate**: Before dispatching any command, check `ctx.from?.id` against `config.allowedUserIds`. If not in the list, silently return (no reply — prevents oracle attacks). Log the unauthorized attempt (chatId + userId, no message content).

**onCommand registration**: `bot.command(command, async (ctx) => { ... })`. Extract `chatId`, `userId`, `command`, `args` from grammY's `Context` object. Map grammY's context to a `CommandContext` and call the registered handler. The `reply` method on `CommandContext` calls `sendMessage(chatId, text)` to go through the chunking path. Note: `ctx.from` is assumed to always be present (no channel post support — personal bot, direct messages only).

**Non-command fallback**: Register with `bot.on('message:text', ctx => ctx.reply('Use /help to see available commands.'))`. This fires only when no command handler matches and the user is authorized.

**Error middleware**: Register with `bot.catch(async (err) => { ... })`. The handler inspects `err.error` for typed error types and constructs a specific reply. It must call `err.ctx.reply(message)` to send the reply. For unknown errors, reply with "Something went wrong. Try again." and log the error.

Error reply mapping:
- `JiraAuthError` → "Jira authentication failed. Check your API token."
- `JiraNotFoundError` → "Issue {key} not found."
- `InvalidTransitionError` → "Transition '{attempted}' not found. Valid: {available.join(', ')}"
- `ClaudeTimeoutError` → "Claude timed out after {timeoutMs}ms."
- `ClaudeExitError` → "Claude subprocess failed (exit {exitCode})."
- Anything else → "Something went wrong. Try again."

**sendMessage chunking**: `sendMessage(chatId, text)` splits text using a `splitMessage(text, limit = 4096)` helper before calling `bot.api.sendMessage()`. The helper splits at paragraph boundaries (`\n\n`) first; if any chunk still exceeds the limit, splits at word boundaries. Each chunk is sent sequentially. This prevents Telegram's 4096-char limit from causing errors.

**startPolling()**: Call `bot.start({ drop_pending_updates: true })`. The `drop_pending_updates: true` option discards stale updates that accumulated while the daemon was offline — processing hour-old commands would produce confusing results. grammY handles the getUpdates loop, offset management, and `TimedOut` error suppression internally. No manual offset management is needed.

**stopPolling()**: Call `await bot.stop()`. This gracefully stops the polling loop after the current update cycle.

**Logging**: Log at command receipt: `{ event: 'command', chatId, command, argCount }`. Log at error handler: `{ event: 'error', errorType: err.error?.type ?? 'unknown', chatId }`. Never log `userId` values in log output to avoid PII retention.

---

## Client 2: JiraClient

### Role

JiraClient communicates with the Jira Cloud REST API v3. It is scoped to a single project (`config.projectKey`). All requests carry a Basic auth header derived from `config.email` and `config.apiToken`.

### Config

```typescript
interface JiraConfig {
  host: string              // e.g. "yourcompany.atlassian.net" (no protocol prefix)
  email: string
  apiToken: string
  projectKey: string
  issueType?: string        // default: "Task" — configurable for projects without a Task type
  requestTimeoutMs?: number // default: 15000 — per-request timeout via AbortController
}
```

### Public Interface

```typescript
interface JiraClient {
  createIssue(title: string, description: string): Promise<JiraIssue>
  transitionIssue(issueKey: string, targetStatus: string): Promise<void>
  addComment(issueKey: string, body: string): Promise<void>
  getIssue(issueKey: string): Promise<JiraIssue>
}

interface JiraIssue {
  key: string
  summary: string
  status: string
  description: string   // plain text, extracted from ADF
  url: string           // https://{host}/browse/{key}
}
```

### HTTP Helpers

The client should have two private helpers:

**`buildAuthHeader()`**: Encodes `email:apiToken` as Base64 and returns `Authorization: Basic <encoded>`. Constructed once in the constructor and reused.

**`request(method, path, body?)`**: Issues a fetch to `https://{host}/rest/api/3/{path}`. Attaches auth header, `Content-Type: application/json`, and `Accept: application/json`. Creates an `AbortController` with `config.requestTimeoutMs` (default 15000ms) to prevent hung requests from blocking command handlers indefinitely.

On non-2xx response, reads the response body and maps HTTP status to typed errors:
- 401 → `JiraAuthError`
- 403 → `JiraPermissionError`
- 404 → `JiraNotFoundError` (with `issueKey` if available from path)
- 429 → `JiraRateLimitError` with `retryAfter` parsed from `Retry-After` header (seconds as integer)
- 5xx → `JiraServerError` with the status code
- AbortError (timeout) → `JiraTimeoutError`
- Anything else → generic `Error` with status and message body

No automatic retry inside the client. The client throws typed errors; retry policy is the caller's responsibility.

**Logging**: Log each outbound request: `{ method, path, status, durationMs }`. Never log the `Authorization` header or `apiToken` value.

### ADF Helpers

Two pure utility functions in `adf.ts`:

**`toADF(text: string)`**: Splits input on `\n` and creates one ADF paragraph node per non-empty line. Empty lines produce natural paragraph breaks in Jira. Returns a complete ADF `doc` node with all paragraphs as top-level content. This preserves multi-line input that would otherwise be collapsed into a single unreadable line.

**`adfToText(node: AdfNode): string`**: Recursively walks an ADF node tree. Node type handling:
- `text` → return `text` value
- `hardBreak` → return `\n`
- `paragraph`, `heading`, `blockquote`, `listItem` → join children with `""`, add trailing `\n`
- `bulletList`, `orderedList` → recurse into content items
- `codeBlock` → extract text content, return with newline
- `mention` → return `@<attrs.text>` or `@user`
- `doc` → join top-level blocks with `\n`
- Unknown node type → recurse into `content` if present, otherwise return `""`
Strip leading/trailing whitespace from the final result. Handle null/undefined input gracefully (return `""`).

### createIssue

POST `/issue` with a body that includes:
- `fields.project.key` = `config.projectKey`
- `fields.issuetype.name` = `config.issueType ?? "Task"` — configurable because not all Jira projects have a "Task" type
- `fields.summary` = `title`
- `fields.description` = `toADF(description)`

The Jira POST response only contains `id`, `key`, `self` — not the full issue. After creating, call `getIssue(response.key)` to fetch the complete `JiraIssue` and return it. One extra round-trip is acceptable; returning an object with undefined fields silently breaks callers.

### getIssue

GET `/issue/${encodeURIComponent(issueKey)}`. Map response to `JiraIssue`. For the `description` field, check if the response has `fields.description` and pass it to `adfToText()`; if null/undefined, use an empty string.

### transitionIssue

1. GET `/issue/${encodeURIComponent(issueKey)}/transitions` to fetch available transitions.
2. Find the transition whose `name` matches `targetStatus` (case-insensitive).
3. If not found, throw `InvalidTransitionError` with `attempted = targetStatus` and `available = transitions.map(t => t.name)`.
4. POST `/issue/${encodeURIComponent(issueKey)}/transitions` with `{ transition: { id: foundTransition.id } }`.

### addComment

POST `/issue/${encodeURIComponent(issueKey)}/comment` with `{ body: toADF(body) }`.

---

## Client 3: ClaudeClient

### Role

ClaudeClient spawns the local `claude` CLI binary as a subprocess, passes a prompt, captures stdout as JSON, and returns the response string. It enforces a configurable timeout and kills the subprocess if it exceeds it.

### Config

```typescript
interface ClaudeConfig {
  binaryPath: string   // absolute path to the claude binary (from AppConfig.claude.binary_path)
  timeoutMs?: number   // default: 30000
  model?: string       // default: omit --model flag (claude uses its own default)
}
```

### Public Interface

```typescript
interface ClaudeClient {
  ask(prompt: string, options?: AskOptions): Promise<string>
}

interface AskOptions {
  timeoutMs?: number   // overrides config.timeoutMs for this call
  model?: string       // overrides config.model for this call
}
```

### Subprocess Invocation

Build the argument list:
```
[config.binaryPath, '--print', '--bare', '--no-session-persistence', '--output-format', 'json']
```

If `model` is set (from options or config), append `['--model', model]`.

**Prompt via stdin** (not as argument) — passing prompts in argv exposes them in `ps aux` and risks ARG_MAX limits on long prompts containing Jira content.

Spawn with `Bun.spawn`: `stdin: 'pipe'`, `stdout: 'pipe'`, `stderr: 'pipe'`. Immediately write the prompt to `proc.stdin` then close it:
```
proc.stdin.write(prompt)
proc.stdin.end()
```

**Env cleanup**: Clone `process.env`, then `delete clonedEnv.CLAUDECODE` before passing to spawn. Setting to `undefined` produces the literal string `"undefined"` in Bun — must use `delete`.

**Stdout/stderr drain**: Start reading both pipes immediately after spawn — do NOT wait for `proc.exited` first. If the subprocess writes more than the OS pipe buffer (~64KB), it blocks waiting for a reader, creating a deadlock. Use `Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited])` to drain concurrently.

**Logging**: Log subprocess start: `{ event: 'claude_spawn', model }`. Log completion: `{ event: 'claude_done', exitCode, durationMs }`. Never log prompt content.

### Timeout Enforcement

Set a `timedOut = false` flag before spawning. Immediately after spawning, set a `setTimeout` for the resolved `timeoutMs`. If the timer fires:
1. Set `timedOut = true`
2. Send SIGTERM: `proc.kill("SIGTERM")`
3. Wait 2 seconds
4. If `proc.exitCode === null` (still running), send SIGKILL: `proc.kill("SIGKILL")`

Always clear the timer in the `finally` block regardless of whether the process succeeded, failed, or was killed.

The post-exit code path checks `timedOut` first: if true, throw `ClaudeTimeoutError({ timeoutMs })` instead of `ClaudeExitError`. This prevents the race condition where the timeout fires, sets `timedOut = true`, the process exits non-zero due to the kill, and the wrong error type is thrown.

### Response Parsing

After `proc.exited` resolves (and the stdout/stderr drain is complete):
- If `timedOut`: throw `ClaudeTimeoutError({ timeoutMs })`.
- If exit code is non-zero: throw `ClaudeExitError({ exitCode, stderr })`.
- If exit code is 0: parse stdout with `JSON.parse(stdout)`, return `parsed.result` (the response string).

### Error on Parse Failure

If `JSON.parse` throws (malformed output), throw a generic `Error` with the raw stdout in the message. This should not happen in practice with `--output-format json` but must be guarded.

---

## File Structure

```
02-integration-clients/
  src/
    errors.ts                    # shared typed error classes (all types with readonly type discriminant)
    telegram/
      TelegramClient.ts
      splitMessage.ts            # splitMessage(text, limit) helper
      types.ts                   # TelegramConfig (with allowedUserIds), CommandContext, CommandHandler
    jira/
      JiraClient.ts
      adf.ts                     # toADF() (multi-paragraph), adfToText() (complete node coverage)
      types.ts                   # JiraConfig (with requestTimeoutMs, issueType), JiraIssue
    claude/
      ClaudeClient.ts
      types.ts                   # ClaudeConfig (with binaryPath), AskOptions
  index.ts                       # re-exports all clients, types, errors (under src/)
  tests/
    telegram.test.ts
    jira.test.ts
    claude.test.ts
  package.json
  tsconfig.json
  vitest.config.ts
```

---

## Dependencies

```json
{
  "dependencies": {
    "grammy": "^1.x"
  },
  "devDependencies": {
    "vitest": "^2.x",
    "typescript": "^5.x",
    "@types/node": "^20.x"
  }
}
```

No SDK for Jira or Claude. Jira uses native `fetch`. Claude uses `Bun.spawn`.

---

## Testing Strategy

Framework: **Vitest** with `vi.fn()` mocks and `vi.spyOn()` for module internals.

### TelegramClient tests

Mock the `grammy` `Bot` class constructor and its methods (`command`, `on`, `catch`, `start`, `stop`, `api.sendMessage`). Verify:
- `onCommand('start', handler)` registers a handler via `bot.command('start', ...)`
- Authorized userId → handler called; unauthorized userId → handler NOT called (silent drop)
- `startPolling()` calls `bot.start({ drop_pending_updates: true })`
- `stopPolling()` awaits `bot.stop()`
- Error middleware is registered at construction time via `bot.catch(...)`
- `bot.on('message:text', ...)` registered for the non-command fallback
- `sendMessage` with text > 4096 chars → calls `bot.api.sendMessage` multiple times
- `splitMessage("text", 4096)` — unit test the helper: splits at paragraph breaks, then word breaks

### JiraClient tests

Mock global `fetch` with `vi.stubGlobal('fetch', mockFetch)`. Test each method independently.

Key test cases:
- `createIssue` sends POST with correct ADF body and project key, then follows up with `getIssue`
- `getIssue` extracts plain text from a multi-paragraph ADF description
- `transitionIssue` fetches transitions, posts the matching ID
- `transitionIssue` case-insensitive match: `"in progress"` matches `"In Progress"` transition
- `transitionIssue` throws `InvalidTransitionError` with `available` list when name not found
- 401 response → `JiraAuthError`; 403 → `JiraPermissionError`; 404 → `JiraNotFoundError`
- 429 response → `JiraRateLimitError` with `retryAfter` from header
- 503 response → `JiraServerError`
- `issueKey` with special chars is URL-encoded in the request path
- Request timeout → `AbortController` cancels fetch → `JiraTimeoutError`

### ClaudeClient tests

Mock `Bun.spawn` with a test double that returns a controllable process-like object. Test:
- Normal response: exit code 0, JSON stdout → returns `parsed.result`
- Prompt is written to stdin, NOT passed as CLI argument
- `CLAUDECODE` is deleted from env (not set to `undefined`) — verify using `typeof env.CLAUDECODE === 'undefined'`
- Timeout: process does not exit within timeoutMs → SIGTERM sent, then SIGKILL after 2s → `ClaudeTimeoutError` thrown
- `timedOut` flag prevents `ClaudeExitError` racing with `ClaudeTimeoutError`
- Non-zero exit without timeout: `ClaudeExitError` with exit code and stderr
- Model flag included in args when `model` is set
- stdout + stderr drained concurrently (no deadlock on large output)

### ADF Helper tests

Pure unit tests, no mocks needed:
- `toADF("hello")` returns ADF doc with single paragraph, single text node
- `toADF("line1\nline2")` returns ADF doc with two paragraphs
- `toADF("line1\n\nline3")` (empty line) returns two paragraphs (middle empty line ignored)
- `adfToText(node)` with nested paragraphs, text nodes, hardBreak → correct plain text
- `adfToText` handles `bulletList`, `orderedList`, `codeBlock`, `mention` nodes
- Unknown node type → recurse into `content` or return `""`
- `adfToText(null)` returns `""`

---

## Key Implementation Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Telegram library | grammY | Automatic offset management, error classification, TypeScript types |
| Jira HTTP | Raw fetch | No Jira SDK needed; only 4 endpoints used |
| Claude invocation | Bun.spawn + `-p --bare --no-session-persistence --output-format json` | Non-interactive, consistent, no disk state |
| ADF wrapping | Plain text only | Sufficient for programmatic bot use |
| ADF extraction | Recursive text walk | Simple, covers the common ADF subset Jira uses |
| Error surface | Typed errors + grammY error middleware | Command handlers stay clean; user always gets a reply |
| Non-command messages | Reply with /help hint | Better UX than silent ignore |

---

## Constraints and Edge Cases

- **Jira transition names are case-insensitive**: Always compare with `.toLowerCase()` on both sides.
- **grammY non-command fallback order**: The `bot.on('message:text', ...)` handler fires after command handlers fail to match. Register it last to avoid intercepting valid commands.
- **ClaudeClient env inheritance**: Use `delete clonedEnv.CLAUDECODE` (not `= undefined`) to properly remove the `CLAUDECODE=1` env var inherited from parent Claude sessions.
- **ClaudeClient prompt via stdin**: Always pipe prompt through stdin — never as a CLI argument (security + ARG_MAX concerns).
- **Jira `host` format**: Store without protocol prefix (`yourcompany.atlassian.net`). The client always prepends `https://`.
- **ADF description null check**: Jira issues created without a description have `fields.description = null`. `adfToText` must handle this gracefully (return `""`).
- **grammY polling and bot token**: grammY validates the token on `bot.start()`. An invalid token throws on startup, not silently later.
- **Authorization is silent**: Unauthorized user messages produce no reply — silence provides no information to attackers about why their request was rejected.
- **issueKey URL encoding**: Always `encodeURIComponent(issueKey)` before interpolating into URL paths.
- **Telegram message length**: Always route through `sendMessage` (not `ctx.reply` directly) to benefit from chunking.
- **`CommandContext.userId` assumption**: `ctx.from` is treated as always present — personal bot, direct messages only, no channel post support.
