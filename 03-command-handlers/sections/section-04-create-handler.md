Now I have all the context needed. Let me generate the section content for `section-04-create-handler`.

# Section 04: Create Handler (`/create`)

## Overview

This section implements `src/commands/create.ts`, the handler for the `/create` Telegram slash command. It is the first command handler section and introduces the pattern of periodic typing refresh and Claude integration with silent fallback behavior used by other handlers.

**Dependencies:**
- `section-01-foundation` must be complete: project is scaffolded, `Config` type and `loadConfig()` exist, dev dependencies installed.
- `section-03-utils` must be complete: `parseArgs` and `parseFirstAndRest` from `src/utils/parseArgs.ts` are available. Note: this handler does NOT use `parseFirstAndRest` — it uses its own `--` separator splitting logic.

**Blocks:** `section-07-registration-and-bot` (which imports and registers all handlers).

---

## Files Created (Actual — root layout)

- `src/bot/commands/create.ts` — handler + exported prompt templates
- `tests/bot/commands/create.test.ts` — 11 bun:test cases

Review fixes applied: prompt injection defense ({description} replaced before {title}), typing replyWithChatAction moved inside try, title/description trimmed after split.

---

## Tests First

**File:** `tests/commands/create.test.ts`

Use `@grammyjs/grammytest` to simulate Telegram updates in-memory. Mock `JiraClient` and `ClaudeClient` with `vi.fn()`. Mount only the create handler on a test bot instance.

Test cases to implement:

1. **Separator path (enrich):** `/create Fix login timeout -- auth expires too early`
   - The ` -- ` separator is detected.
   - `ClaudeClient.ask` is called with a prompt that includes both the title and description.
   - `JiraClient.createIssue` is called once.
   - Reply text contains `"Created:"` and `"ENG-"` (or whatever key the mock returns).

2. **No-separator path (expand):** `/create Fix login timeout`
   - No ` -- ` in the input.
   - `ClaudeClient.ask` is called with a prompt containing only the title.
   - `JiraClient.createIssue` is called once.
   - Reply text contains `"Created:"`.

3. **Claude failure during enrichment (fallback):** Enrich path, but `ClaudeClient.ask` rejects.
   - `JiraClient.createIssue` is still called (with the raw description, not empty).
   - Reply text still contains `"Created:"`.
   - No error reply is sent.

4. **Claude failure during expansion (fallback):** Expand path, but `ClaudeClient.ask` rejects.
   - `JiraClient.createIssue` is called with an empty description (or empty string).
   - Reply text still contains `"Created:"`.
   - No crash, no error reply.

5. **No arguments:** `/create` with empty or whitespace-only args.
   - No API calls made.
   - Reply contains usage guidance (e.g., `"/create <title>"` or `"Usage:"`).

6. **Jira auth error:** `JiraClient.createIssue` throws a `JiraAuthError`.
   - Reply contains `"auth"` or `"token"` (case-insensitive).

7. **Template delimiter assertion:** `ENRICH_PROMPT_TEMPLATE` (exported constant) contains the strings `"<title>"` and `"<description>"`.

8. **Template delimiter assertion:** `EXPAND_PROMPT_TEMPLATE` (exported constant) contains the string `"<title>"`.

Stub shape for the test file:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ENRICH_PROMPT_TEMPLATE, EXPAND_PROMPT_TEMPLATE } from '../../src/commands/create'

describe('ENRICH_PROMPT_TEMPLATE', () => {
  it('contains <title> and <description> XML delimiters', () => { /* ... */ })
})

describe('EXPAND_PROMPT_TEMPLATE', () => {
  it('contains <title> XML delimiter', () => { /* ... */ })
})

describe('/create handler', () => {
  // set up grammytest bot, mock JiraClient, mock ClaudeClient

  it('enrich path: -- separator detected, Claude called with title+description, issue created', async () => { /* ... */ })
  it('expand path: no -- separator, Claude called with title only, issue created', async () => { /* ... */ })
  it('enrich path: Claude fails → raw description used, issue still created', async () => { /* ... */ })
  it('expand path: Claude fails → empty description used, issue still created', async () => { /* ... */ })
  it('no args → usage reply, no API calls', async () => { /* ... */ })
  it('JiraAuthError → reply contains auth/token message', async () => { /* ... */ })
})
```

---

## Implementation Details

**File:** `src/commands/create.ts`

### Parsing Logic

The handler does NOT use `parseFirstAndRest`. It receives the full text after `/create` as a string (from `ctx.match`), trims it, and applies a single check:

- If the trimmed input contains the substring ` -- ` (space-dash-dash-space), split on the first occurrence only. Everything before is the title; everything after is the description. This is **Path A (enrich)**.
- If the trimmed input does NOT contain ` -- `, the entire input is the title; there is no description. This is **Path B (expand)**.
- If the trimmed input is empty or the input is absent (no `ctx.match`), reply with a usage string and return early.

The split should occur on the **first** ` -- ` only, in case a description itself contains ` -- `.

### Typing Indicator Refresh

Before making any asynchronous API call (Claude or Jira):

1. Call `await ctx.replyWithChatAction("typing")` (grammY method).
2. Start a `setInterval` that re-calls `ctx.replyWithChatAction("typing")` every **4000 ms** (typing indicator lasts ~5 seconds; 4s interval keeps it continuously alive).
3. Wrap the async API calls in a `try/finally` block. In the `finally` block, call `clearInterval(typingInterval)`.

### Path A — Enrich (title + description)

1. Build the enrich prompt by replacing `{title}` and `{description}` placeholders in `ENRICH_PROMPT_TEMPLATE` using `.replace()`. The template wraps the substituted values in XML-style tags: `<title>...</title>` and `<description>...</description>`.
2. Call `ClaudeClient.ask(enrichPrompt)`.
3. If Claude succeeds, use the returned text as the Jira description.
4. If Claude throws for any reason, catch it silently and use the original raw description text from the user's input. Do NOT reply with an error — the user's intent is still fulfilled.
5. Call `JiraClient.createIssue(title, description)`. The `createIssue` method accepts plain text and handles ADF conversion internally. No ADF work in this handler.
6. On success, reply with the created issue key (e.g., `"Created: ENG-123"`).

### Path B — Expand (title only)

1. Build the expand prompt by replacing `{title}` placeholder in `EXPAND_PROMPT_TEMPLATE`. The template wraps the value in `<title>...</title>` tags.
2. Call `ClaudeClient.ask(expandPrompt)`.
3. If Claude succeeds, use the returned text as the Jira description.
4. If Claude throws, catch silently and use an empty string as the description.
5. Call `JiraClient.createIssue(title, description)`.
6. On success, reply with the created issue key.

### Exported Prompt Templates

Both prompt templates are exported named constants (not inline strings). This is required so tests can assert on their content. The actual content of the prompts must include XML-style delimiters around untrusted user data:

```typescript
export const ENRICH_PROMPT_TEMPLATE: string
// Must contain: <title>{title}</title> and <description>{description}</description>

export const EXPAND_PROMPT_TEMPLATE: string
// Must contain: <title>{title}</title>
// Must instruct Claude to write a 3–5 sentence description and 3–5 acceptance criteria bullet points
```

### Exported Function

```typescript
import type { Context } from 'grammy'
import type { Clients } from './index'

export const ENRICH_PROMPT_TEMPLATE: string
export const EXPAND_PROMPT_TEMPLATE: string

export async function handleCreate(ctx: Context, clients: Clients): Promise<void>
```

`handleCreate` is the only exported function. It accesses `ctx.match` for the argument string and uses `clients.jira` and `clients.claude` for all external calls.

### Error Handling

Wrap the outer body of `handleCreate` in a `try/catch`. Handle these error types explicitly:

- `JiraAuthError` (check `.type === 'JiraAuthError'` or use `instanceof` depending on how `02-integration-clients` exports it): reply with a message indicating that the Jira API token is invalid or missing.
- `JiraNotFoundError`: should not occur on a `createIssue` call, but include a catch for completeness with a generic reply.
- Unknown/unexpected errors: log `{ event: 'error', command: 'create', errorMessage: error.message }` — never log the full error object, never log the prompt content or title/description values. Reply with a generic error message.

The Claude failure within each path is caught in an inner try/catch nested inside the outer handler try/catch — it swallows the Claude error silently. The outer catch handles only Jira and unexpected errors.

### Prompt Injection Defense

Template substitution uses `.replace()` on string constants. Untrusted user content (title and description) is placed inside XML-style delimiter tags. This signals to Claude that the content is data, not instructions. There is no recursive templating — a single `.replace()` per placeholder is sufficient.

Example template shape (exact wording is up to the implementer, this is structure only):

```
You are a Jira ticket writer. Given the following title and description provided by the user, write a well-formatted Jira ticket description in plain text.

<title>{title}</title>
<description>{description}</description>

Return only the formatted description. No preamble, no metadata.
```

---

## Clients Interface Reference

The `Clients` interface is defined in `src/commands/index.ts` (implemented in `section-07-registration-and-bot`). For this section, use a local type or import it from the index file once it exists. To keep this section independently compilable during development, you may declare a local `interface Clients` with the necessary methods:

```typescript
interface Clients {
  jira: {
    createIssue(title: string, description: string): Promise<{ key: string }>
  }
  claude: {
    ask(prompt: string): Promise<string>
  }
}
```

Replace with the real import from `./index` when `section-07-registration-and-bot` is complete.

---

## Acceptance Criteria

Before marking this section done, verify:

- [ ] All 8 test cases in `create.test.ts` pass under `vitest run`
- [ ] `ENRICH_PROMPT_TEMPLATE` exported, contains `<title>` and `<description>` delimiters
- [ ] `EXPAND_PROMPT_TEMPLATE` exported, contains `<title>` delimiter
- [ ] Path A uses enrich template; Path B uses expand template
- [ ] Typing indicator interval is started before API call and cleared in `finally`
- [ ] Claude failure in both paths is silently caught; issue creation proceeds
- [ ] Jira errors are caught and return user-friendly replies
- [ ] No ADF conversion logic in this file (that lives in `02-integration-clients`)
- [ ] No direct Telegram API or Jira API calls — all external access through `clients.jira` and `clients.claude`