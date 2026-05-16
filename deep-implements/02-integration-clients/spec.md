# Spec: 02-integration-clients

## What This Is

Three thin, typed API clients that the command handlers use to talk to external systems: Telegram, Jira Cloud, and the local Claude CLI.

---

## Requirements Source

See `../requirements.md` for full project overview.  
Depends on: `../01-core-daemon/spec.md` (config system, logger)

---

## Scope

### Telegram Client

Protocol: Long-polling (not webhook — daemon has no public URL)

Responsibilities:
- Start/stop polling loop (`getUpdates` with timeout)
- Parse incoming `Message` objects
- Detect slash commands (text starting with `/`)
- Send text replies to a chat ID
- Expose an `onCommand(handler)` registration interface

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

Error handling: retry on network errors with exponential backoff; log and continue on parse errors.

---

### Jira Cloud Client

Auth: Basic auth (email + API token, base64 encoded) against Jira Cloud REST API v3.

Scoped to a single configured project (`project_key` from config).

Methods needed:
- `createIssue(title: string, description: string): Promise<JiraIssue>`
- `transitionIssue(issueKey: string, targetStatus: string): Promise<void>` — must resolve status name to transition ID
- `addComment(issueKey: string, body: string): Promise<void>`
- `getIssue(issueKey: string): Promise<JiraIssue>`

```typescript
interface JiraIssue {
  key: string           // e.g. "ENG-123"
  summary: string
  status: string
  description: string
  url: string           // browser link to issue
}
```

Notes:
- `transitionIssue` requires fetching available transitions first (`GET /issue/{key}/transitions`), then finding the matching one by name (case-insensitive)
- Description uses Atlassian Document Format (ADF) for create/comment — client must handle ADF wrapping
- Return typed errors distinguishing: auth failure, issue not found, invalid transition

---

### Claude CLI Client

Protocol: Spawn `claude` subprocess, pass prompt via stdin or `--print` flag, capture stdout.

```typescript
interface ClaudeClient {
  ask(prompt: string, options?: ClaudeOptions): Promise<string>
}

interface ClaudeOptions {
  timeoutMs?: number    // default: 30000
  model?: string        // default: use claude's default
}
```

Implementation:
- Use `Bun.spawn` with `claude --print` (non-interactive mode)
- Write prompt to stdin or pass as argument (research exact claude CLI flags during planning)
- Capture stdout as response
- Kill process on timeout
- Throw typed error if exit code non-zero

---

## Key Decisions (from interview)

- **Telegram:** Long-polling (no webhook, no public URL needed)
- **Telegram UX:** Slash commands only — client only needs to detect `/`-prefixed messages
- **Jira:** Cloud only, API token auth, single project scope
- **Claude:** Shell out to existing local `claude` CLI binary — not Anthropic API directly

---

## Depends On (from 01-core-daemon)

```typescript
import type { AppConfig } from '../01-core-daemon'
import type { Logger } from '../01-core-daemon'
```

Each client constructor receives `config` and `logger` — no global state.

---

## Provides To (03-command-handlers)

Typed client instances: `TelegramClient`, `JiraClient`, `ClaudeClient`

---

## Uncertainties to Resolve in Planning

- Exact `claude` CLI flags for non-interactive use (`--print`, `--no-stream`, stdin input — verify against installed version)
- ADF format requirements for Jira Cloud description/comment fields — may need a small ADF builder helper
- Telegram `getUpdates` offset management to avoid duplicate message processing
- Whether to use a Telegram SDK (e.g. `grammy`, `telegraf`) or raw HTTP — raw HTTP keeps binary size smaller
