# Spec: 03-command-handlers

## What This Is

The orchestration layer — slash command routing and the business logic that wires Telegram commands to Jira and Claude operations.

---

## Requirements Source

See `../requirements.md` for full project overview.  
Depends on: `../02-integration-clients/spec.md` (TelegramClient, JiraClient, ClaudeClient)

---

## Scope

### Command Router

Registered with `TelegramClient.onCommand()` at startup.

Commands to handle:

| Command | Args | Description |
|---|---|---|
| `/create` | `<title> [description...]` | Create a Jira ticket, optionally enriched by Claude |
| `/move` | `<ticket-key> <status>` | Transition Jira issue to a new status |
| `/comment` | `<ticket-key> <comment text...>` | Add comment to a Jira issue |
| `/solve` | `<ticket-key>` | Fetch ticket from Jira, ask Claude for solution, reply |
| `/help` | — | List available commands with usage |

---

### Handler: /create

Flow:
1. Parse `<title>` and optional `<description>` from args
2. If description provided: pass title + description to Claude — ask it to write a well-formatted Jira ticket description
3. Create issue in Jira with the enriched (or raw) description
4. Reply to Telegram with: `Created: ENG-123 — <title>\n<jira-url>`

Claude prompt template for enrichment:
```
You are a Jira ticket writer. Given the following input, write a clear, concise Jira ticket description in plain text (no markdown).

Title: {title}
Notes: {description}

Return only the description body, nothing else.
```

---

### Handler: /move

Flow:
1. Parse `<ticket-key>` and `<status>` from args
2. Call `JiraClient.transitionIssue(ticketKey, status)`
3. On success: reply `Moved ENG-123 → In Progress`
4. On invalid status: reply with available statuses for that ticket

---

### Handler: /comment

Flow:
1. Parse `<ticket-key>` and `<comment text>` from args
2. Call `JiraClient.addComment(ticketKey, text)`
3. Reply: `Comment added to ENG-123`

---

### Handler: /solve

Flow:
1. Parse `<ticket-key>` from args
2. Fetch issue: `JiraClient.getIssue(ticketKey)`
3. Build Claude prompt with ticket title, description, and status
4. Reply intermediate message: `Analyzing ENG-123 with Claude...`
5. Call `ClaudeClient.ask(prompt)`
6. Reply with Claude's response (may be long — Telegram handles up to 4096 chars per message; split if needed)

Claude prompt template:
```
You are a senior software engineer. Analyze this Jira ticket and suggest a solution or next steps.

Ticket: {key}
Title: {summary}
Status: {status}
Description: {description}

Provide a concise, actionable solution. Plain text only.
```

---

### Handler: /help

Reply with formatted command list:
```
Available commands:

/create <title> [description] — Create a Jira ticket
/move <ticket> <status>       — Move ticket to new status
/comment <ticket> <text>      — Add comment to ticket
/solve <ticket>               — Get AI solution for ticket
/help                         — Show this message
```

---

### Error Handling

All handlers should catch errors and reply user-friendly messages via Telegram:

| Error type | Reply |
|---|---|
| Jira auth failure | `Jira auth failed — check your API token in config` |
| Issue not found | `Ticket ENG-123 not found` |
| Invalid transition | `Cannot move to "{status}". Available: {list}` |
| Claude timeout | `Claude timed out — try again` |
| Claude error | `Claude returned an error — check logs` |
| Unknown command | `Unknown command. Try /help` |

---

## Key Decisions (from interview)

- **Slash commands only:** No NLP intent parsing — args are positional
- **Claude enriches /create:** Not mandatory — if Claude fails, fall back to raw description
- **Single Jira project:** No project selection needed in commands — use config `project_key`

---

## Depends On (from 02-integration-clients)

```typescript
import { TelegramClient, JiraClient, ClaudeClient } from '../02-integration-clients'
```

---

## Provides

Running command bot — the main daemon loop is: start Telegram polling, register all handlers.

---

## Uncertainties to Resolve in Planning

- Long Telegram message splitting strategy (>4096 chars from Claude responses)
- Multi-step flows: should `/create` ask follow-up questions if no description given, or just create with title only?
- Status name matching for `/move`: exact match, fuzzy match, or case-insensitive prefix?
- Rate limiting: Telegram has per-second send limits — relevant if Claude response is split into many messages
