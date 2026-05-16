# Usage Guide — 02-integration-clients

## Quick Start

Install dependencies and run tests:

```bash
cd 02-integration-clients
bun install
bun run test
```

---

## Package Exports

```typescript
import {
  // Errors
  JiraAuthError, JiraPermissionError, JiraNotFoundError,
  JiraRateLimitError, JiraServerError, JiraTimeoutError,
  InvalidTransitionError, ClaudeTimeoutError, ClaudeExitError,

  // ADF helpers
  adfDoc, adfParagraph, adfText, adfBold, adfCode,
  adfBulletList, adfOrderedList, adfListItem,
  adfMention, adfEmoji, adfHeading,
  adfToMarkdown,

  // Telegram
  TelegramClient, splitMessage,

  // Jira
  JiraClient,

  // Claude
  ClaudeClient,
} from '02-integration-clients'
```

---

## API Reference

### ClaudeClient

Spawns the local `claude` CLI as a subprocess; sends prompt via stdin; returns the `result` field from JSON output.

```typescript
import { ClaudeClient, ClaudeConfig } from '02-integration-clients'

const config: ClaudeConfig = {
  binaryPath: '/usr/local/bin/claude',
  timeoutMs: 30000,          // optional, default 30s
  model: 'claude-opus-4-7',  // optional
}

const client = new ClaudeClient(config, logger)

// Basic call
const response = await client.ask('Summarize this ticket: ...')

// Per-call overrides
const response2 = await client.ask('Draft a comment', {
  timeoutMs: 60000,
  model: 'claude-sonnet-4-6',
})
```

**Error handling:**

```typescript
import { ClaudeTimeoutError, ClaudeExitError } from '02-integration-clients'

try {
  const result = await client.ask(prompt)
} catch (err) {
  if (err instanceof ClaudeTimeoutError) {
    console.error(`Timed out after ${err.timeoutMs}ms`)
  } else if (err instanceof ClaudeExitError) {
    console.error(`claude CLI exited ${err.exitCode}: ${err.stderr}`)
  }
}
```

**Key behaviors:**
- Prompt is sent via stdin, never in argv (avoids `ps aux` exposure)
- `CLAUDECODE` env var is deleted from child process env
- stdout and stderr are drained concurrently with `proc.exited` (prevents pipe-buffer deadlock)
- On timeout: SIGTERM → 2s grace → SIGKILL; throws `ClaudeTimeoutError`
- Kill-caused non-zero exit is reported as `ClaudeTimeoutError`, not `ClaudeExitError`

---

### JiraClient

```typescript
import { JiraClient, JiraConfig } from '02-integration-clients'

const config: JiraConfig = {
  host: 'mycompany.atlassian.net',
  email: 'user@example.com',
  apiToken: process.env.JIRA_TOKEN!,
  projectKey: 'PROJ',
  issueType: 'Task',
  requestTimeoutMs: 10000,
}

const jira = new JiraClient(config, logger)

// Fetch issue
const issue = await jira.getIssue('PROJ-123')

// Create issue
const key = await jira.createIssue({ summary: 'Fix bug', description: adfDoc([...]) })

// Update fields
await jira.updateIssue('PROJ-123', { summary: 'Updated title' })

// Transition
await jira.transitionIssue('PROJ-123', 'In Progress')

// Add comment
await jira.addComment('PROJ-123', adfDoc([adfParagraph([adfText('comment text')])]))
```

---

### TelegramClient

```typescript
import { TelegramClient, TelegramConfig } from '02-integration-clients'

const config: TelegramConfig = {
  botToken: process.env.TELEGRAM_TOKEN!,
  chatId: '-1001234567890',
  parseMode: 'HTML',        // optional, default HTML
  splitLongMessages: true,  // optional, default true
}

const telegram = new TelegramClient(config, logger)
await telegram.sendMessage('Hello from Jira Assistant!')
```

---

### ADF Helpers

Build Atlassian Document Format nodes for Jira API calls:

```typescript
import { adfDoc, adfParagraph, adfText, adfBold, adfToMarkdown } from '02-integration-clients'

const doc = adfDoc([
  adfParagraph([
    adfText('Normal text and '),
    adfBold('bold text'),
  ]),
])

// Convert ADF to plain markdown (for Telegram messages)
const markdown = adfToMarkdown(doc)
```

---

## Example Output

```
Tests: 99 passed across 5 test files
  errors.test.ts   — 12 tests
  adf.test.ts      — 24 tests
  jira.test.ts     — 20 tests
  telegram.test.ts — 25 tests
  claude.test.ts   — 18 tests
```
