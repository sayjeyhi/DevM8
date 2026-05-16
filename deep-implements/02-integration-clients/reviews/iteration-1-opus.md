# Opus Review

**Model:** claude-opus-4
**Generated:** 2026-05-14T00:00:00Z

---

# Architectural Review: 02-integration-clients Implementation Plan

## Critical Issues

### 1. ClaudeClient: Race condition in timeout enforcement
The plan describes setting a `setTimeout`, calling `process.kill()`, then throwing `ClaudeTimeoutError`. Several gaps:
- `process.kill()` is asynchronous and does not guarantee immediate termination. SIGTERM may be ignored; the plan should specify SIGTERM first, then SIGKILL after a short grace period (e.g., 2s).
- The `await process.exited` promise needs explicit handling — if you kill the process, `process.exited` resolves with a non-zero exit code and the code path that throws `ClaudeExitError` could fire before/instead of `ClaudeTimeoutError`. The plan must define a `timedOut` flag set before killing so the post-exit branch can throw the correct error.
- No mention of cleaning up the timer in the error path (e.g., if `Bun.spawn` itself throws synchronously).

### 2. ClaudeClient: Prompt-as-argument is a footgun
Passing the prompt via `-p prompt` exposes it to:
- ARG_MAX limits (typically 128KB-2MB depending on platform). Long prompts will fail mysteriously.
- Process listings (`ps aux`) — anyone on the host can read prompts containing potentially sensitive Jira data.
- Shell metacharacter handling — `Bun.spawn` with an array avoids shell, but the plan doesn't explicitly document this.

Recommendation: pipe the prompt through stdin instead. The plan contradicts itself by saying `stdin: 'pipe'` and "close immediately — prompt is passed as argument, not stdin". Either commit to stdin (safer) or pass via argv and document the size cap.

### 3. ClaudeClient: `--bare` and `--no-session-persistence` flags are unverified
These flags are presented as established CLI options but the plan provides no validation that they exist in the `claude` CLI. If the binary doesn't support these, the subprocess will fail with a non-zero exit. The plan should specify a startup probe (`claude --help` or `claude --version`) on client init, or at least include a clear test against the real binary.

### 4. ClaudeClient: `env: { ...process.env, CLAUDECODE: undefined }` does NOT delete the var
Setting an env property to `undefined` results in the literal string `"undefined"` in many runtimes (including Bun's spawn semantics depending on version). Must use `delete env.CLAUDECODE` on a cloned object.

### 5. JiraClient: No retry/backoff for transient failures
Jira Cloud returns 429 (rate limit) and 5xx routinely. The plan throws a generic `Error` for "anything else" but the command-handler layer has no way to distinguish retryable from non-retryable failures. Add `JiraRateLimitError` (with `retryAfter` from `Retry-After` header) and `JiraServerError`, plus optional bounded retry.

### 6. JiraClient: No request timeout
`fetch` has no default timeout. A hung Jira request will block a command handler indefinitely. Add `AbortController` with configurable timeout (15s default) on every request.

### 7. JiraClient: 403 is not mapped
The plan only maps 401 → `JiraAuthError` and 404 → `JiraNotFoundError`. 403 (permission denied) should produce a clear `JiraPermissionError`.

### 8. JiraClient: `createIssue` does not return full state
`createIssue`'s Jira response only contains `id`, `key`, `self` — not `summary`, `status`, or `description`. The plan must either follow up with a `getIssue` call or document that only `key` and `url` are populated.

## Significant Issues

### 9. TelegramClient: No handling for long Telegram messages
Telegram has a 4096 character limit per message. The plan should specify automatic chunking in `sendMessage`/`reply` or document that callers must chunk themselves.

### 10. TelegramClient: No flood control
Telegram returns 429 with `retry_after`. The plan should state whether `autoRetry` plugin is used.

### 11. TelegramClient: Authorization model is missing
No allowlist of `userId`s. Anyone who finds the bot can create/read Jira issues and spawn `claude` subprocesses. Add `allowedUserIds: number[]` to `TelegramConfig`.

### 12. TelegramClient: No reconnection semantics
If grammY's polling loop dies due to a network blip, behavior is unspecified. Should specify whether `startPolling()` auto-restarts.

### 13. JiraClient: ADF newlines lost in `toADF`
`toADF(text)` creates a single paragraph. Newlines in user input are lost. Should split on `\n` into multiple paragraphs or use `hardBreak` nodes.

### 14. JiraClient: `adfToText` block list is incomplete
Missing: `bulletList`, `orderedList`, `codeBlock`, `hardBreak`, `mention`, inline links, `emoji`. Unknown nodes must have a fallback.

### 15. JiraClient: No URL encoding of `issueKey`
`issueKey` flows into URLs without `encodeURIComponent`. User input with `/`, `?`, `#` breaks routing.

### 16. JiraClient: Issue type "Task" hardcoded
Will break on Jira projects without a "Task" issue type. Should be configurable or documented.

### 17. ClaudeClient: stdout/stderr buffering can deadlock
With piped output, if subprocess writes more than pipe buffer (~64KB), it blocks. Must drain both pipes continuously.

### 18. ClaudeClient: No prompt sanitization
Jira content in prompts may contain prompt injection. Should document and use delimiters.

## Moderate Issues

### 19. Error class type discriminant missing
Plan says "discriminant `type` property" but doesn't show the field on class definitions.

### 20. Logger usage unspecified
Plan says each client takes a Logger but never describes what to log. Should specify: events to log, never log API tokens or prompt content.

### 21. No secret redaction policy
`apiToken`, `token`, and prompts may end up in error messages and log output.

### 22. Test: no `transitionIssue` case-insensitivity test

### 23. Test: no large stdout test for ClaudeClient

### 24. ClaudeClient working directory unspecified
`Bun.spawn` without `cwd` inherits daemon's cwd. Should be made explicit.

### 25. Concurrent ClaudeClient invocations
Two handlers calling `ask()` simultaneously = two subprocesses. Should confirm intention and optionally add concurrency limit.

### 26. `CommandContext.userId` edge case
`ctx.from.id` is optional (channel posts). Behavior when `from` is undefined is unspecified.

### 27. `drop_pending_updates` semantics
State whether pending updates should be dropped on bot startup (probably yes for a daemon that may have been offline).

## Top Action Items

1. **Authorization allowlist** in TelegramConfig — security critical
2. **Prompt via stdin** for ClaudeClient (not argv) — security + ARG_MAX
3. **Kill sequence** SIGTERM + grace + SIGKILL with `timedOut` flag
4. **Request timeout** + AbortController on every JiraClient request
5. **429/5xx retry + JiraRateLimitError/JiraServerError/JiraPermissionError**
6. **createIssue**: follow up with getIssue or narrow return type
7. **ADF newline handling** in `toADF`
8. **URL-encode issueKey** in all route paths
9. **Logging contract** + redaction rules
10. **Drain stdout/stderr** concurrently in ClaudeClient
