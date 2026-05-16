# Complete Specification: 03-command-handlers

## What This Module Does

Implements the Telegram bot command layer for a Jira assistant. Registers slash commands with grammY, routes each command to a dedicated handler, wires the handler to pre-built Jira and Claude integration clients, and returns user-friendly replies. This is the main daemon loop that ties the whole system together.

---

## Source Documents

- Initial spec: `spec.md`
- Research: `claude-research.md`
- Interview: `claude-interview.md`

---

## Technology Stack

- **Language:** TypeScript (Node.js)
- **Bot framework:** grammY (`grammy`) — actively maintained, first-class TypeScript, middleware architecture
- **Jira/Claude clients:** imported from `../02-integration-clients` (interfaces already defined, treat as stable)
- **Testing:** Vitest + `@grammyjs/grammytest` (in-memory bot testing, no real Telegram connection)

---

## Authorization

All commands are gated by an allowlist of Telegram user IDs loaded from a config file (JSON or `.env`) at startup.

- Unauthorized requests: log the user ID + command, silently ignore (no reply)
- Allowlist checked in a global grammY middleware before any command handler runs

Config shape:
```json
{
  "allowedUserIds": [123456, 789012]
}
```
or via `.env`:
```
ALLOWED_USER_IDS=123456,789012
```

---

## Commands

### /create `<title> [description...]`

**Happy path A — title + description provided:**
1. Parse title (first token) and description (rest of text)
2. Send `sendChatAction("typing")`
3. Call Claude with enrichment prompt → get formatted description
4. If Claude fails: fall back to raw description (no error surfaced to user)
5. Call `JiraClient.createIssue({ title, description })` with default project settings (no component/priority)
6. Reply: `Created: ENG-123 — <title>\n<jira-url>`

**Happy path B — title only (no description):**
1. Parse title
2. Send `sendChatAction("typing")`
3. Call Claude with *title-expansion prompt* (see below) → generates 3-5 line paragraph + bullet points
4. If Claude fails: create issue with title only, no description
5. Create issue + reply same format as A

**Claude prompt — enrichment (description provided):**
```
You are a Jira ticket writer. Given the following input, write a clear, concise Jira ticket description in plain text (no markdown).

Title: {title}
Notes: {description}

Return only the description body, nothing else.
```

**Claude prompt — title expansion (no description):**
```
You are a Jira ticket writer. Given only a ticket title, write a short Jira ticket description in plain text (no markdown).
Expand the title into 3-5 sentences describing what needs to be done, then add 3-5 bullet points covering acceptance criteria or key tasks.

Title: {title}

Return only the description body, nothing else.
```

**Error cases:**
- Jira auth failure → `Jira auth failed — check your API token in config`
- Jira generic error → `Failed to create ticket — check logs`
- Claude failure → fall back silently, continue to create issue

---

### /move `<ticket-key> <status>`

1. Parse `<ticket-key>` and `<status>` (remainder of args joined)
2. Send `sendChatAction("typing")`
3. Call `JiraClient.getTransitions(ticketKey)` → list of available statuses
4. Match user's status string: **case-insensitive substring contains**
   - `"in progress".toLowerCase().includes(input.toLowerCase())`
   - Check each available transition name
5. If exactly one match: call `JiraClient.transitionIssue(ticketKey, transitionId)`
   - Reply: `Moved ENG-123 → In Progress`
6. If zero matches: reply `Cannot move to "{input}". Available: Todo, In Progress, Done`
7. If multiple matches: reply `Ambiguous status "{input}". Did you mean: In Progress, In Review?`

**Error cases:**
- Ticket not found → `Ticket ENG-123 not found`
- Jira auth failure → `Jira auth failed — check your API token in config`
- Transition API error → `Failed to move ticket — check logs`

---

### /comment `<ticket-key> <text...>`

1. Parse `<ticket-key>` (first token) and `<text>` (rest)
2. Send `sendChatAction("typing")`
3. Call `JiraClient.addComment(ticketKey, text)`
4. Reply: `Comment added to ENG-123`

**Error cases:**
- Ticket not found → `Ticket ENG-123 not found`
- Missing args → `Usage: /comment <ticket> <text>`

---

### /solve `<ticket-key>`

1. Parse `<ticket-key>`
2. Send intermediate reply: `Analyzing ENG-123 with Claude...`
3. Send `sendChatAction("typing")`
4. Call `JiraClient.getIssue(ticketKey)` → `{ key, summary, status, description }`
5. Build prompt (see below) and call `ClaudeClient.ask(prompt)`
6. If Claude response ≤ 4096 chars: send as single message
7. If > 4096 chars: split at paragraph/word boundaries, send as multiple messages (no part numbers needed unless splits > 2)
8. No auto-retry on Claude failure — fail immediately

**Claude prompt:**
```
You are a senior software engineer. Analyze this Jira ticket and suggest a solution or next steps.

Ticket: {key}
Title: {summary}
Status: {status}
Description: {description}

Provide a concise, actionable solution. Plain text only.
```

**Error cases:**
- Ticket not found → `Ticket ENG-123 not found`
- Claude timeout → `Claude timed out — try again`
- Claude error → `Claude returned an error — check logs`

---

### /help

Reply with fixed text (no API calls):
```
Available commands:

/create <title> [description] — Create a Jira ticket
/move <ticket> <status>       — Move ticket to new status
/comment <ticket> <text>      — Add comment to ticket
/solve <ticket>               — Get AI solution for ticket
/help                         — Show this message
```

---

## Message Splitting

Used by `/solve` (and potentially `/create` if Claude description is long).

Algorithm:
1. If text ≤ 4096 chars: send as-is
2. Otherwise: split on double-newline (paragraph) boundaries, accumulating until near limit
3. If single paragraph exceeds limit: split on word boundaries
4. Last resort: character boundary

No sleep between messages — use `@grammyjs/transformer-throttler` + `@grammyjs/auto-retry` to handle Telegram rate limits automatically.

---

## Error Handling Matrix

| Situation | Reply |
|---|---|
| Jira auth failure | `Jira auth failed — check your API token in config` |
| Ticket not found | `Ticket ENG-123 not found` |
| Invalid/ambiguous move status | `Cannot move to "{status}". Available: {list}` |
| Claude timeout | `Claude timed out — try again` |
| Claude error | `Claude returned an error — check logs` |
| Missing required args | `Usage: /command <args>` |
| Unknown command | `Unknown command. Try /help` |
| Unauthorized user | (silent — log only) |

---

## Module Structure

```
03-command-handlers/
  src/
    bot.ts               ← Bot init, middleware stack (auth, throttler, auto-retry)
    config.ts            ← Load config/env (allowedUserIds, bot token, project key)
    middleware/
      auth.ts            ← Allowlist middleware
    commands/
      index.ts           ← Merge all composers, register CommandGroup
      create.ts
      move.ts
      comment.ts
      solve.ts
      help.ts
    utils/
      splitMessage.ts    ← Paragraph/word boundary splitter
      parseArgs.ts       ← ctx.match → string[]
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

## Integration Contracts (from 02-integration-clients)

The handlers consume these methods (interfaces defined upstream):

```typescript
// JiraClient
createIssue(data: { title: string; description: string }): Promise<{ key: string; url: string }>
getIssue(key: string): Promise<{ key: string; summary: string; status: string; description: string }>
getTransitions(key: string): Promise<{ id: string; name: string }[]>
transitionIssue(key: string, transitionId: string): Promise<void>
addComment(key: string, text: string): Promise<void>

// ClaudeClient
ask(prompt: string): Promise<string>

// TelegramClient
onCommand(command: string, handler: Handler): void
// grammY handles the actual sending via ctx.reply()
```

---

## Resolved Uncertainties

| Uncertainty | Resolution |
|---|---|
| Long message splitting | Paragraph → word boundary; multiple messages; throttler plugin for rate limits |
| /create no-description | Claude expands title to 3-5 line paragraph + bullets |
| Status matching for /move | Case-insensitive substring contains; reply available list if zero match, disambiguation if multiple |
| Rate limiting | @grammyjs/transformer-throttler + auto-retry; no manual sleep |
