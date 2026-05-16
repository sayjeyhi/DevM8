# Combined Spec: 02-integration-clients

## Overview

Three thin, typed API clients used by command handlers to communicate with external systems: Telegram (via grammY), Jira Cloud (REST API v3), and the local Claude CLI subprocess. Each client is a class that receives its own config slice and a Logger instance — no global state.

This module is `02-integration-clients` in the jira-assistant project. It depends on types from `01-core-daemon` (Logger interface). It provides typed client instances to `03-command-handlers`.

---

## Config Shapes

Each client receives only its own config slice (not the full AppConfig):

```typescript
interface TelegramConfig {
  token: string
}

interface JiraConfig {
  host: string        // e.g. "yourcompany.atlassian.net" (no https://)
  email: string
  apiToken: string
  projectKey: string  // e.g. "ENG"
}

interface ClaudeConfig {
  timeoutMs?: number  // default: 30000
  model?: string      // default: use claude CLI's default
}
```

---

## Typed Errors

All three clients expose typed errors. grammY error middleware catches them and replies to the Telegram user with a short, formatted message.

```typescript
class JiraAuthError extends Error { type = 'JiraAuthError' }
class JiraNotFoundError extends Error { type = 'JiraNotFoundError'; issueKey: string }
class InvalidTransitionError extends Error {
  type = 'InvalidTransitionError'
  attempted: string
  available: string[]
}
class ClaudeTimeoutError extends Error { type = 'ClaudeTimeoutError'; timeoutMs: number }
class ClaudeExitError extends Error { type = 'ClaudeExitError'; exitCode: number; stderr: string }
```

---

## Client 1: Telegram (grammY)

### Library Choice

Use **grammY** (`grammy` npm package). Handles:
- getUpdates long-polling with automatic offset management
- Error classification (TimedOut = normal empty cycle, not an error)
- TypeScript type safety for Update, Message, Context objects

### Interface

```typescript
interface TelegramClient {
  startPolling(): void
  stopPolling(): void
  sendMessage(chatId: number, text: string): Promise<void>
  onCommand(command: string, handler: CommandHandler): void
}

type CommandHandler = (ctx: CommandContext) => Promise<void>
interface CommandContext {
  chatId: number
  userId: number
  command: string
  args: string[]         // space-split tokens after command
  rawText: string
  reply(text: string): Promise<void>
}
```

### Key Behaviors

**Error middleware (grammY)**: Register `bot.catch(handler)` that catches typed errors and calls `ctx.reply()` with formatted messages:
- `JiraAuthError` → "Jira authentication failed. Check your API token."
- `JiraNotFoundError` → "Issue {key} not found."
- `InvalidTransitionError` → "Transition '{attempted}' not found. Valid: {available.join(', ')}"
- `ClaudeTimeoutError` → "Claude timed out after {timeoutMs}ms."
- Unhandled errors → "Something went wrong. Try again."

**Non-command messages**: `bot.on('message:text', ctx => ctx.reply('Use /help to see available commands.'))`

**Startup probe**: On `startPolling()`, issue one getUpdates with timeout=0 to evict stale webhook/session before starting the polling loop.

**Exponential backoff**: grammY handles this internally via its retry plugin or native error handling. Ensure it is configured.

---

## Client 2: Jira Cloud

### Auth

Basic auth: `Authorization: Basic base64(email:apiToken)` header on every request.
Base URL: `https://{config.host}/rest/api/3`

### Interface

```typescript
interface JiraClient {
  createIssue(title: string, description: string): Promise<JiraIssue>
  transitionIssue(issueKey: string, targetStatus: string): Promise<void>
  addComment(issueKey: string, body: string): Promise<void>
  getIssue(issueKey: string): Promise<JiraIssue>
}

interface JiraIssue {
  key: string           // e.g. "ENG-123"
  summary: string
  status: string        // status name
  description: string   // plain text extracted from ADF
  url: string           // browser link: https://{host}/browse/{key}
}
```

### ADF Wrapping (plain text only)

For `createIssue` and `addComment`, wrap the input string in minimal ADF:

```typescript
function toADF(text: string) {
  return {
    version: 1,
    type: 'doc',
    content: [{
      type: 'paragraph',
      content: [{ type: 'text', text }]
    }]
  }
}
```

### ADF → Plain Text (getIssue description)

Implement a recursive text extractor that walks the ADF node tree and concatenates all `text` node values with newlines for paragraph/heading separators:

```typescript
function adfToText(node: AdfNode): string // recursive
```

### transitionIssue Details

1. `GET /issue/{key}/transitions` — fetch available transitions
2. Find transition by name match (case-insensitive)
3. If not found → throw `InvalidTransitionError({ attempted, available: transition names })`
4. `POST /issue/{key}/transitions` with `{ transition: { id } }`

### Error Mapping

| HTTP status | Situation | Error to throw |
|---|---|---|
| 401 | Auth failure | `JiraAuthError` |
| 404 | Issue not found | `JiraNotFoundError` |
| 400 on transition | Invalid transition body | `InvalidTransitionError` |
| Other 4xx/5xx | Generic | `Error` with message |

---

## Client 3: Claude CLI

### Interface

```typescript
interface ClaudeClient {
  ask(prompt: string, options?: ClaudeOptions): Promise<string>
}

interface ClaudeOptions {
  timeoutMs?: number    // default: 30000
  model?: string        // default: use claude's default
}
```

### Subprocess Invocation

Use `Bun.spawn` with the following flags (confirmed via research):

```bash
claude -p "<prompt>" --bare --no-session-persistence --output-format json
```

With optional model: `--model <model>` appended if `options.model` is set.

Parse stdout: `JSON.parse(stdout).result`

### Timeout and Kill

Set a timer for `timeoutMs` (default 30000). On expiry, call `process.kill()` and throw `ClaudeTimeoutError`.

### Exit Code Handling

If exit code is non-zero, throw `ClaudeExitError({ exitCode, stderr })`.

### Notes

- `--bare` skips local config/hooks/plugins — ensures consistent behavior regardless of local Claude Code setup
- `--no-session-persistence` prevents disk writes for stateless calls
- `--output-format json` gives structured output; parse `JSON.parse(stdout).result` for the response text
- Known issue: subprocess inherits `CLAUDECODE=1` env var from parent Claude sessions. If tests are run inside Claude Code, set `env: { ...process.env, CLAUDECODE: undefined }` in Bun.spawn options

---

## Testing Strategy

**Framework**: Vitest (`vitest` package)

**Approach**: Unit tests for each client with mocked external calls.

| Client | Mock target | Test focus |
|---|---|---|
| TelegramClient | Mock grammY `Bot` constructor | Command handler registration, non-command fallback, error middleware wiring |
| JiraClient | Mock `fetch` globally | createIssue ADF wrapping, transitionIssue offset management, getIssue ADF→text conversion, typed error throws |
| ClaudeClient | Mock `Bun.spawn` | Timeout kill, exit code error, stdout parsing, model flag injection |

Key test cases:
- `transitionIssue` throws `InvalidTransitionError` with available list when status name not found
- `getIssue` extracts plain text from multi-paragraph ADF description
- `ClaudeClient.ask` kills process and throws `ClaudeTimeoutError` after timeout
- `ClaudeClient.ask` throws `ClaudeExitError` on non-zero exit code
- Jira 401 → `JiraAuthError`
- Jira 404 → `JiraNotFoundError`

---

## Dependencies

```json
{
  "grammy": "^1.x",
  "vitest": "^2.x" (devDependency)
}
```

No SDK for Jira or Claude — both use raw fetch / Bun.spawn.

---

## File Structure

```
02-integration-clients/
  src/
    telegram/
      TelegramClient.ts
      types.ts
    jira/
      JiraClient.ts
      adf.ts          // toADF() and adfToText() helpers
      types.ts
      errors.ts
    claude/
      ClaudeClient.ts
      types.ts
      errors.ts
    index.ts          // re-exports all clients and types
  tests/
    telegram.test.ts
    jira.test.ts
    claude.test.ts
  package.json
  tsconfig.json
```
