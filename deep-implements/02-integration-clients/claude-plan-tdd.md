# TDD Plan: 02-integration-clients

## Testing Approach

New project. Uses **Vitest** with `vi.fn()` mocks and `vi.spyOn()`. Test files in `tests/` (flat, one per client). Run with `vitest run`. No real network calls, no real subprocesses in unit tests.

---

## Section 1: Shared Error Module (`src/errors.ts`)

**Tests to write first:**

- Test: `JiraAuthError` is `instanceof Error`; `.type === 'JIRA_AUTH'`
- Test: `JiraPermissionError` has `.type === 'JIRA_PERMISSION'`
- Test: `JiraNotFoundError` carries `.issueKey`; `.type === 'JIRA_NOT_FOUND'`
- Test: `JiraRateLimitError` carries `.retryAfter?: number`; `.type === 'JIRA_RATE_LIMIT'`
- Test: `JiraServerError` carries `.status`; `.type === 'JIRA_SERVER'`
- Test: `JiraTimeoutError` has `.type === 'JIRA_TIMEOUT'`
- Test: `InvalidTransitionError` carries `.attempted` and `.available: string[]`
- Test: `ClaudeTimeoutError` carries `.timeoutMs`; `.type === 'CLAUDE_TIMEOUT'`
- Test: `ClaudeExitError` carries `.exitCode` and `.stderr`; `.type === 'CLAUDE_EXIT'`
- Test: all error classes are distinguishable via `type` field in a switch statement

---

## Section 2: ADF Helpers (`src/jira/adf.ts`)

**Tests to write first (pure unit, no mocks):**

`toADF`:
- Test: `toADF("hello")` → ADF doc with one paragraph, one text node containing `"hello"`
- Test: `toADF("line1\nline2")` → two paragraphs
- Test: `toADF("a\n\nb")` (empty line between) → two paragraphs (empty line skipped)
- Test: `toADF("")` → empty doc or doc with no paragraphs (graceful)

`adfToText`:
- Test: single text node → returns text value
- Test: paragraph with text node → returns text value
- Test: multiple paragraphs → joined with `\n`
- Test: `hardBreak` node → produces `\n` in output
- Test: `bulletList` / `orderedList` → recurses into list items
- Test: `codeBlock` → returns text content
- Test: `mention` with `attrs.text` → returns `@text`
- Test: unknown node type with `content` → recurses into content
- Test: unknown node type without `content` → returns `""`
- Test: `null` input → returns `""`
- Test: `undefined` input → returns `""`
- Test: real-world multi-element ADF (paragraph + bulletList + heading) → correct text extraction

---

## Section 3: TelegramClient

**Tests to write first (mock grammY Bot):**

Authorization:
- Test: authorized userId → command handler is called
- Test: unauthorized userId (not in `allowedUserIds`) → command handler NOT called, no reply
- Test: `allowedUserIds = []` → all commands blocked

sendMessage / reply:
- Test: `sendMessage(chatId, shortText)` → one `bot.api.sendMessage` call
- Test: `sendMessage(chatId, textOver4096)` → multiple `bot.api.sendMessage` calls

`splitMessage` helper (unit test separately):
- Test: short text → returned as single-element array
- Test: text with paragraph breaks → splits at `\n\n` first
- Test: single word longer than limit → splits at limit (no infinite loop)
- Test: exactly 4096 chars → one element

Lifecycle:
- Test: `onCommand('start', handler)` → `bot.command('start', ...)` called at registration
- Test: `startPolling()` → `bot.start({ drop_pending_updates: true })` called
- Test: `stopPolling()` → `bot.stop()` awaited

Error middleware:
- Test: `bot.catch(...)` registered during construction
- Test: `JiraAuthError` caught → reply contains "authentication failed"
- Test: `JiraNotFoundError` with `issueKey` → reply contains key
- Test: `InvalidTransitionError` → reply lists `available` transitions
- Test: `ClaudeTimeoutError` → reply contains timeout duration
- Test: unknown error → reply with generic message; error logged

Non-command fallback:
- Test: `bot.on('message:text', ...)` registered for help hint

---

## Section 4: JiraClient

**Tests to write first (mock global `fetch`):**

Error mapping:
- Test: 401 response → throws `JiraAuthError`
- Test: 403 response → throws `JiraPermissionError`
- Test: 404 response → throws `JiraNotFoundError`
- Test: 429 with `Retry-After: 30` header → throws `JiraRateLimitError` with `retryAfter === 30`
- Test: 503 response → throws `JiraServerError` with `status === 503`
- Test: request timeout (AbortSignal fires) → throws `JiraTimeoutError`

`createIssue`:
- Test: sends POST to `/issue` with correct `fields.project.key`, `fields.issuetype.name`, ADF description
- Test: uses `config.issueType` when set (not hardcoded "Task")
- Test: follows up with GET `/issue/{key}` after creation → returns full `JiraIssue`

`getIssue`:
- Test: GET request uses `encodeURIComponent(issueKey)` in path
- Test: maps `fields.summary`, `fields.status.name`, ADF description extraction
- Test: `fields.description = null` → `description` is `""`

`transitionIssue`:
- Test: fetches transitions from `GET /issue/{key}/transitions`
- Test: POST uses found transition ID
- Test: matching is case-insensitive (`"in progress"` matches `"In Progress"`)
- Test: no match → throws `InvalidTransitionError` with `available` list
- Test: issueKey URL-encoded in all path segments

`addComment`:
- Test: POST body has ADF `toADF(text)`

Logging:
- Test: request logger records `method`, `path`, `status`, `durationMs`
- Test: logger never receives the `apiToken` value

---

## Section 5: ClaudeClient

**Tests to write first (mock `Bun.spawn`):**

Invocation:
- Test: prompt written to `proc.stdin`, NOT in CLI args (`-p` flag absent)
- Test: `CLAUDECODE` key is absent from spawned env (not `=== undefined`, properly deleted)
- Test: `--print`, `--bare`, `--no-session-persistence`, `--output-format`, `json` in args
- Test: `--model` flag absent when model not configured; present when configured

Happy path:
- Test: exit code 0, valid JSON stdout → returns `parsed.result`
- Test: `parsed.result` is the string response

Error paths:
- Test: exit code non-zero (not timeout) → throws `ClaudeExitError` with exitCode and stderr
- Test: malformed JSON on exit 0 → throws generic Error with raw stdout

Timeout:
- Test: process doesn't exit within `timeoutMs` → SIGTERM sent first
- Test: after 2s grace, if still alive → SIGKILL sent
- Test: throws `ClaudeTimeoutError` (not `ClaudeExitError`) when `timedOut = true`
- Test: timer cleared in finally block (no timer leak on success)

Stdout drain:
- Test: stdout and stderr both read concurrently with `proc.exited`

---

## Integration Smoke Tests (manual, per release)

- Start the daemon with a valid config; send a test command from an authorized Telegram user → response received
- Send a command from an unauthorized Telegram user → no response (silent drop confirmed in logs)
- Test Jira connection by running `jira-assistant status` or triggering a `/create` → issue created
- Verify `claude` subprocess is invoked with stdin prompt (run `ps aux` during a request — no prompt visible in args)
- Kill the daemon mid-Claude-request → verify process cleanup (no zombie `claude` subprocess)
