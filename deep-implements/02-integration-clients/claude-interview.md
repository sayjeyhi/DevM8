# Interview Transcript: 02-integration-clients

## Round 1 — Critical Architecture Decisions

### Q1: Telegram client implementation: raw HTTP or a library?

**A:** Use grammY.

grammY handles offset management automatically, provides built-in error classification (TimedOut vs real errors), and has full TypeScript type safety. One npm dependency. Preferred over raw HTTP despite the slightly larger binary size.

### Q2: Jira ADF wrapping complexity?

**A:** Plain text only — wrap in minimal ADF doc node.

Simple ADF structure is sufficient:
```json
{
  "version": 1,
  "type": "doc",
  "content": [{
    "type": "paragraph",
    "content": [{ "type": "text", "text": "<input>" }]
  }]
}
```
No markdown → ADF conversion needed for this use case.

### Q3: When Jira or Telegram API calls fail, what should the behavior be?

**A:** Reply to Telegram user with a short error message.

Typed errors propagate from clients up through grammY error middleware, which catches them and sends a formatted, user-friendly message to the Telegram chat. The typed error types allow the middleware to craft specific messages per error type.

---

## Round 2 — Edge Cases and Testing

### Q4: When transitionIssue is called with a status name that doesn't match?

**A:** Throw typed `InvalidTransitionError` with list of valid transitions.

The error should include both the attempted transition name and the list of valid transition names, so the error middleware can surface a useful message like "Transition 'Done' not found. Valid transitions: To Do, In Progress, Review."

### Q5: ClaudeClient behavior beyond basic ask()?

**A:** Timeout + process kill on hang (only).

No streaming output, no retry on non-zero exit code, no custom system prompt support. Keep it simple: spawn, capture, kill on timeout, throw typed error on non-zero exit code.

Research confirmed the correct invocation: `claude -p "prompt" --bare --no-session-persistence --output-format json`. Parse `JSON.parse(stdout).result` to extract the response text.

### Q6: Testing approach?

**A:** Vitest + mock HTTP.

Use Vitest for all client unit tests. Mock `fetch` for Telegram and Jira HTTP calls. Mock `Bun.spawn` for ClaudeClient subprocess calls. Integration tests (calling real APIs) are out of scope for this module.

---

## Round 3 — Config Shape and Remaining Edge Cases

### Q7: What does AppConfig look like for these clients?

**A:** Define config shape here — clients own their slice.

Each client receives only its own config slice:
```typescript
interface TelegramConfig {
  token: string
}

interface JiraConfig {
  host: string        // e.g. "yourcompany.atlassian.net"
  email: string
  apiToken: string
  projectKey: string  // e.g. "ENG"
}

interface ClaudeConfig {
  timeoutMs?: number  // default: 30000
  model?: string      // default: use claude's default
}
```

### Q8: When getIssue returns a JiraIssue, how should the ADF description be handled?

**A:** Convert ADF → plain text (strip formatting).

Implement a simple recursive text extractor that walks the ADF node tree and concatenates all `text` node values. This makes the `description` field in `JiraIssue` immediately usable in Telegram replies without any ADF knowledge at the call site.

### Q9: Should the Telegram client handle non-command messages?

**A:** Reply with "Use /help to see available commands."

Non-command messages (text not starting with `/`) should receive a polite redirect to the help command. This is handled inside the grammY bot setup via `bot.on('message:text', ...)` as a fallback.
