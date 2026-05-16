# Usage Guide — 03-command-handlers

## What Was Built

A Telegram bot that acts as a Jira assistant, powered by Claude AI. Users send slash commands to the bot to create, update, and analyze Jira issues.

## Quick Start

### Prerequisites

Set environment variables:

```bash
export TELEGRAM_BOT_TOKEN="bot123456:your-token"
export JIRA_BASE_URL="https://your-org.atlassian.net"
export JIRA_PROJECT_KEY="ENG"
export JIRA_USER_EMAIL="you@example.com"
export JIRA_API_TOKEN="your-jira-api-token"
export CLAUDE_API_KEY="your-anthropic-api-key"
export ALLOWED_USER_IDS="123456789"          # your Telegram user ID
export CLAUDE_BINARY_PATH="claude"           # path to claude CLI binary (optional)
```

### Run

```bash
bun run src/bot/bot.ts
```

---

## Commands

### `/create <title> [-- <description>]`

Create a new Jira ticket.

```
/create Fix login timeout
/create Fix login timeout -- Auth token expires after 5 minutes of inactivity
```

- Without `--`: Claude expands the title into a full description with acceptance criteria.
- With `--`: Claude enriches the provided description before creating the ticket.
- Reply: `Created: ENG-42`

### `/move <issue-key> <status>`

Transition a Jira issue to a new status. Status names with spaces work.

```
/move ENG-42 In Progress
/move ENG-42 Done
```

- Reply on success: `Moved ENG-42 → In Progress`
- Reply on invalid status: `Cannot move to "Blocked". Available: To Do, In Progress, Done`

### `/comment <issue-key> <text>`

Add a plain-text comment to a Jira issue. Internal spacing is preserved.

```
/comment ENG-42 Fixed by reverting commit abc123
```

- Reply: `Comment added to ENG-42`

### `/solve <issue-key>`

Fetch the Jira issue, send it to Claude, and post an analysis with actionable next steps.

```
/solve ENG-42
```

- First reply: `Analyzing ENG-42 with Claude...`
- Final reply: Claude's analysis (split across multiple messages if > 4096 chars)

### `/help`

Show all available commands.

```
/help
```

---

## Architecture

```
src/bot/
├── bot.ts              — startBot(): wires all components, starts long-polling
├── config.ts           — loadConfig(): reads required env vars
├── commands/
│   ├── index.ts        — Clients interface + registerCommands()
│   ├── create.ts       — handleCreate()
│   ├── move.ts         — handleMove()
│   ├── comment.ts      — handleComment()
│   ├── solve.ts        — handleSolve() + SOLVE_PROMPT_TEMPLATE
│   └── help.ts         — handleHelp() + HELP_TEXT
├── middleware/
│   └── auth.ts         — createAuthMiddleware() (allowlist by Telegram user ID)
└── utils/
    ├── parseArgs.ts     — parseArgs(), parseFirstAndRest()
    └── splitMessage.ts  — splitMessage() (4096-char chunks with [N/M] prefix)
```

## Middleware Stack (in order)

1. `apiThrottler()` — rate-limits outbound Telegram API calls
2. `autoRetry()` — retries on HTTP 429
3. `createAuthMiddleware()` — silently drops unauthorized users
4. `CommandGroup` — routes slash commands to handlers
5. Unknown command fallback — replies "Unknown command. Try /help"
6. `bot.catch` — logs sanitized errors, replies with generic message

## Error Handling

All handlers catch:
- `JiraAuthError` → "Authentication failed. Please check your Jira API token."
- `JiraNotFoundError` → "Issue ENG-42 not found."
- `InvalidTransitionError` → "Cannot move to X. Available: A, B, C"
- `ClaudeTimeoutError` → "Claude timed out. Please try again."
- `ClaudeExitError` → "Claude returned an error. Please try again."
- Unknown → "Something went wrong. Please try again." (logged to stderr)

## Running Tests

```bash
bun test tests/
```

277 tests, 0 failures (as of section-07 completion).

## Security Notes

- Auth middleware silently drops unauthorized users (no reply — bot existence not confirmed)
- Error logs never include full error objects, Authorization headers, or user-supplied content
- Claude prompts use XML delimiters (`<description>...</description>`) as prompt injection defense
- `ALLOWED_USER_IDS` accepts a comma-separated list; empty string = no authorized users
