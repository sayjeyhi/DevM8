# Research Findings: 02-integration-clients

## Topic 1: Telegram Bot API getUpdates Long-Polling + Offset Management

### How Long-Polling Works

`getUpdates` is an HTTP GET/POST to `https://api.telegram.org/bot<TOKEN>/getUpdates`. Setting `timeout > 0` holds the connection open for up to that many seconds waiting for updates before returning. Setting `timeout=0` (default) is short polling — it returns immediately and hammers the server.

Official parameters:
- `offset` — first update ID to return
- `limit` — 1–100, defaults to 100
- `timeout` — seconds to hold connection; defaults to 0 (short poll)
- `allowed_updates` — filter which update types to receive

### Offset Management (Critical)

```
next_offset = last_received_update_id + 1
```

- An update is confirmed as soon as you call `getUpdates` with `offset` strictly greater than its `update_id`.
- All updates with `update_id <= offset - 1` are permanently discarded by Telegram.
- **Recalculate offset after each server response**, not after each update processed.
- Negative offsets supported: `-N` means "start from the N-th update from the end."
- If no updates for a full week, Telegram may reset `update_id` sequence (rare edge case).

```typescript
// Canonical loop skeleton
let offset = 0
while (running) {
  const updates = await getUpdates({ offset, timeout: 30 })
  for (const update of updates) {
    process(update)
    offset = update.update_id + 1
  }
}
```

### Recommended `timeout` Value

- **Production**: 25–30 seconds (30s is the de-facto standard used by most major libraries)
- **Development**: 0–5s is fine
- HTTP client read timeout must be slightly longer than the API timeout: `http_read_timeout = api_timeout + 5`

### Error Handling and Retry Patterns

| Error type | Correct response |
|---|---|
| `TimedOut` (empty response after timeout) | **Normal, expected** — log at DEBUG, retry immediately, no sleep |
| Network/API error | Exponential backoff; use `retry_after` field if present |
| HTTP 409 Conflict | Two processes polling same token — backoff ≥ 35s |
| HTTP 5xx | Exponential backoff with jitter |

Key insight: `TimedOut` exceptions are **not** errors — they are the normal end of a long-poll cycle with no new updates. Treat them like an empty update list and loop immediately.

**Startup probe pattern**: On bot startup, issue one `getUpdates(timeout=0)` first. This evicts stale sessions (prevents 409 on restart) and advances offset past offline-received updates.

### Gotchas

1. `getUpdates` returns nothing / 409 if a webhook is registered — call `deleteWebhook` first
2. `allowed_updates` does not apply retroactively; always send explicit list on first boot
3. By default, `chat_member`, `message_reaction`, etc. are excluded (must opt in)
4. Concurrent `getUpdates` from two processes on same token → 409 Conflict
5. Set socket-level read timeout or you may hang indefinitely on dead TCP connections

### Raw HTTP vs a Library (grammY / telegraf)

| Consideration | Raw HTTP | Library |
|---|---|---|
| Offset management | Manual | Automatic |
| Error classification | Manual | Built-in |
| Type safety | None | Full (grammY = TypeScript) |
| Startup/conflict handling | Manual | Built-in |
| Dependency overhead | Zero | ~1 dep |

**Recommendation**: Raw HTTP is viable for simple bots or zero-dependency requirements. A minimal library like grammY prevents common offset/error bugs. For this project (spec notes "raw HTTP keeps binary size smaller"), raw HTTP is acceptable but offset management and error classification must be implemented manually.

---

## Topic 2: Claude CLI Flags for Non-Interactive / Programmatic Use

### Core Non-Interactive Flag

```bash
claude -p "your prompt"
# or
claude --print "your prompt"
```

`-p` / `--print` runs Claude non-interactively: executes prompt, writes to stdout, exits.

### Passing a Prompt: Argument vs stdin

Both supported:

```bash
# Via argument
claude -p "Explain this project"

# Via stdin (pipe)
echo "prompt here" | claude -p ""
cat file.txt | claude -p "Summarize this"
```

**Stdin cap**: Piped stdin is capped at **10 MB**. Exceeding it causes immediate exit with non-zero status.

### Output Formats

```bash
claude -p "query" --output-format text          # default: plain text to stdout
claude -p "query" --output-format json          # structured JSON payload
claude -p "query" --output-format stream-json   # newline-delimited JSON events
```

JSON payload structure:
```json
{
  "type": "result",
  "subtype": "success",
  "result": "Response text",
  "is_error": false,
  "total_cost_usd": 0.001234,
  "duration_ms": 2500
}
```

### Model Selection

```bash
claude -p "query" --model claude-sonnet-4-6
claude -p "query" --model sonnet     # alias for latest Sonnet
claude -p "query" --model opus       # alias for latest Opus
```

### Subprocess Usage with Bun.spawn (Key Notes)

1. **Buffer size**: Set large maxBuffer (~10 MB) to avoid truncation on large responses
2. **Startup overhead**: Each `claude -p` invocation = new process (~1–2s startup). Add `--bare` to minimize
3. **`--bare` flag**: Skips auto-discovery of hooks, skills, plugins, MCP servers. Recommended for programmatic use — consistent behavior, faster startup. Will become the default for `-p` in a future release
4. **`--no-session-persistence`**: Skip session state save to disk for stateless calls
5. **`CLAUDECODE=1` env var inheritance**: Known bug — subprocess inherits parent Claude session env var, may prevent SDK usage from hooks/plugins. For Bun.spawn, either clear env or avoid running inside a Claude session

### Exit Codes

- `0` — success
- `1` — general failure (auth errors, API errors, hook failures, turn limit exceeded)

### Recommended Flags for Programmatic Use

```bash
claude -p "prompt" \
  --bare \
  --no-session-persistence \
  --output-format json \
  --model sonnet
```

Parse response: `JSON.parse(stdout).result`

### Useful Non-Interactive Flags

| Flag | Purpose |
|---|---|
| `--bare` | Skip all local config/hooks/plugins |
| `--no-session-persistence` | Don't save session to disk |
| `--dangerously-skip-permissions` | Skip permission prompts |
| `--max-turns N` | Limit agentic turns |
| `--max-budget-usd N` | Cap API spend |
| `--system-prompt "..."` | Replace system prompt |
| `--debug-file <path>` | Write debug logs to file |

---

## Testing Notes (New Project)

No existing test setup in the codebase — this is a new project. Testing preferences will be captured in the interview. Recommendations:
- Bun's built-in test runner (`bun test`) for unit tests
- Mock HTTP calls for Telegram/Jira clients using `jest`-compatible mocking
- Integration tests gated behind env vars (skip in CI without credentials)

---

## Sources

- [Telegram Bot API — getUpdates](https://core.telegram.org/bots/api#getupdates)
- [Claude Code CLI Reference](https://code.claude.com/docs/en/cli-reference)
- [Claude Code Headless / Agent SDK CLI docs](https://code.claude.com/docs/en/headless)
- [python-telegram-bot PR #1007 — timeout handling](https://github.com/python-telegram-bot/python-telegram-bot/pull/1007)
- [grammY Bot API guide](https://grammy.dev/guide/api)
- [Claude Agent SDK Python — subprocess issue #573](https://github.com/anthropics/claude-agent-sdk-python/issues/573)
