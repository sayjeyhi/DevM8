# Integration Notes: Opus Review Feedback

## What I'm Integrating and Why

### 1. Authorization allowlist in `TelegramConfig` (Issue 11) — CRITICAL
**Integrating.** Without it, anyone who finds the bot token can create Jira issues, read project data, and spawn `claude` subprocesses on the host. Fix: add `allowedUserIds: number[]` to `TelegramConfig`. Gate `onCommand` dispatch on this list. Unauthorized messages are silently ignored (no reply = no oracle for attackers to probe).

### 2. Prompt via stdin instead of `-p arg` (Issue 2)
**Integrating.** Passing prompt as a CLI argument exposes it in `ps aux` (Jira data visible to any host user) and hits ARG_MAX on long prompts. Fix: pipe prompt through stdin, read all at once, then close stdin. Remove the `-p` flag. Update `Bun.spawn` to write to `stdin` then close it.

### 3. Timeout kill sequence: SIGTERM → grace → SIGKILL + `timedOut` flag (Issue 1, 3)
**Integrating.** SIGTERM may be ignored; a `timedOut = true` flag prevents the post-exit branch from racing with the timeout error. Fix: on timeout, set `timedOut = true`, send SIGTERM, wait 2s, send SIGKILL if still alive. Post-exit branch checks `timedOut` before deciding which error to throw. Always clear the timer in the finally block.

### 4. Delete `CLAUDECODE` properly — `delete env.CLAUDECODE` (Issue 4)
**Integrating.** Setting env property to `undefined` produces the literal string `"undefined"` in Bun. Fix: clone `process.env`, then `delete clonedEnv.CLAUDECODE`.

### 5. JiraClient request timeout via `AbortController` (Issue 6)
**Integrating.** A hung Jira request blocks command handlers indefinitely. Fix: add a `requestTimeoutMs` field to `JiraConfig` (default 15000). Create `AbortController`, pass `signal` to every `fetch` call, cancel after `requestTimeoutMs`. Throw `JiraTimeoutError` on abort.

### 6. Add `JiraRateLimitError`, `JiraServerError`, `JiraPermissionError` (Issues 5, 7)
**Integrating.** The command handler layer cannot distinguish retryable from non-retryable failures. Fix:
- 403 → `JiraPermissionError`
- 429 → `JiraRateLimitError` with `retryAfter` parsed from `Retry-After` header
- 5xx → `JiraServerError`
No automatic retry inside the client — retrying is responsibility of the caller or left to launchd restart. Document this.

### 7. `createIssue` follow-up `getIssue` for full `JiraIssue` (Issue 8)
**Integrating.** Jira's POST `/issue` response only has `id`, `key`, `self`. Returning partial `JiraIssue` with undefined fields silently breaks callers. Fix: after creating, call `getIssue(response.key)` to get the full object. One extra round-trip, clearly documented.

### 8. ADF newlines in `toADF` — multi-paragraph (Issue 13)
**Integrating.** Single-paragraph `toADF` discards all newlines. Fix: split input on `\n` and create one paragraph per non-empty line. Empty lines between paragraphs produce natural paragraph breaks in Jira rendering.

### 9. `adfToText` — complete node type coverage (Issue 14)
**Integrating.** Missing node types silently drop content. Fix: add `hardBreak` (→ `\n`), `bulletList`/`orderedList` (recurse into content), `codeBlock` (return text content with newline), `mention` (return `@displayName`). Unknown nodes: recurse into `content` if present, otherwise empty string.

### 10. URL-encode `issueKey` in all route paths (Issue 15)
**Integrating.** Use `encodeURIComponent(issueKey)` in all methods that put it in a URL path. Trivial fix, prevents incorrect routing.

### 11. Explicit `type` discriminant on error classes (Issue 19)
**Integrating.** Add `readonly type = 'JIRA_AUTH' as const` (etc.) to each error class for tag-based discrimination alongside `instanceof`. Makes error handling more robust in catch blocks.

### 12. Logging contract + redaction rules (Issues 20, 21)
**Integrating.** Plan currently never describes what to log. Fix: define the logging contract:
- Telegram: log incoming command event (chatId, command, arg count — no arg values) and error events.
- Jira: log request method + path + response status + duration. Never log auth header or token.
- Claude: log subprocess start/end + timeout events + exit codes. Never log prompt content.

### 13. Concurrent stdout/stderr drain to prevent deadlock (Issue 17)
**Integrating.** If subprocess writes >64KB to a pipe before we read it, the subprocess blocks. Fix: start reading stdout and stderr into buffers immediately after spawn (do not wait for `proc.exited`). Use `new Response(proc.stdout).text()` and `new Response(proc.stderr).text()` in a `Promise.all` alongside `proc.exited`.

### 14. Long message chunking in `sendMessage` / `reply` (Issue 9)
**Integrating.** Telegram enforces 4096 char limit. Fix: add a `splitMessage(text, limit = 4096)` helper in `telegram/`. `sendMessage` and `reply` call it and send multiple messages if needed. Split at paragraph boundaries first, then word boundaries.

### 15. `drop_pending_updates` on bot startup (Issue 27)
**Integrating.** A daemon that was offline accumulates pending updates. Processing stale commands (minutes/hours old) produces confusing results. Fix: pass `{ drop_pending_updates: true }` to `bot.start()`.

### 16. Fix file structure — `src/index.ts` (Issue 27)
**Integrating.** The plan showed `index.ts` at the root of `02-integration-clients/`, not under `src/`. Fix: move to `src/index.ts`, referenced by `package.json main/exports`.

---

## What I'm NOT Integrating and Why

### A. grammY version pinning to a specific minor (Issue 21)
**Not integrating at plan level.** Version pinning is implementation concern — set it in `package.json` at implementation time. The plan specifies the major; implementer picks a stable minor.

### B. Concurrent `ClaudeClient` invocations / Mutex (Issue 25)
**Not integrating.** Personal single-user bot. Multiple concurrent Claude calls are unlikely and the OS handles multiple subprocesses fine. A concurrency limit adds complexity for no practical benefit.

### C. Integration test layer with real credentials (Issue 26)
**Not integrating.** Out of scope for the planning phase. Unit tests with mocks are the plan's scope. Integration tests can be added separately and manually.

### D. Claude startup probe (`claude --help` on init) (Issue 3 partial)
**Not integrating.** Adds latency to every daemon startup. The binary path is validated in the config wizard (`01-core-daemon`). A bad path will fail fast on the first `ask()` call with a clear `ClaudeExitError`. Startup probing is redundant.

### E. Claude concurrency / queue (Issue 25)
**Not integrating.** Same rationale as B. Personal use case doesn't warrant a task queue.

### F. `commandContext.userId` optional handling (Issue 26)
**Not integrating.** Channel posts (where `from` is undefined) won't happen in a private bot context. The plan operates on the assumption of direct user messages. Document the assumption in the types file.
