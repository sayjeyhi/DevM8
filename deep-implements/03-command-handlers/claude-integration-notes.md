# Integration Notes: Opus Review Feedback

## What I'm Integrating and Why

### 1. Remove `ctx.answerCallbackQuery()` from auth middleware (Issue 1) — CRITICAL BUG
**Integrating.** `answerCallbackQuery` is for inline keyboards, not command messages. Calling it on a command update will throw. Fix: replace the erroneous reference with a plain `return` (silent drop).

### 2. Clarify `setCommands()` vs `bot.use(commands)` (Issue 2)
**Integrating.** `setCommands(bot)` is purely a Telegram UI sync (the `/` menu). `bot.use(commands)` is what installs the handler dispatcher. The plan conflated these. Fix: add a clear note distinguishing the two in Section 11.

### 3. Document `setCommands` command visibility to unauthorized users (Issue 3)
**Integrating as documented limitation.** All Telegram users who can find the bot will see the command menu. For a personal bot this is acceptable. Fix: document in Section 3 that the command menu is visible to all; only execution is gated.

### 4. `bot.catch` must not reply to unauthorized users (Issue 4)
**Integrating.** Auth middleware silently drops unauthorized updates. If the auth middleware panics and throws instead of returning, `bot.catch` must not reply. Fix: clarify that `bot.catch` should always check authorization before replying, or that auth middleware must never throw.

### 5. Use remainder capture for `/comment` and `/move` args (Issue 5)
**Integrating.** Split-and-rejoin for multi-word trailing args collapses multiple spaces and corrupts formatting. Fix: use a regex `/^(\S+)\s+([\s\S]*)$/` to capture the first token and the raw remainder string unchanged. Apply to `parseArgs` or as a specialized `parseFirstAndRest()` helper.

### 6. Fix `/create` title/description split with `--` separator (Issue 6) — UX BUG
**Integrating.** "First whitespace-delimited token as title" produces single-word titles for nearly all real inputs. Fix: use `--` as an explicit separator. If `--` is present in the input, everything before it is the title and everything after is the description. If no `--`, the entire input is the title and the expand path (title-only) is used. Example: `/create Fix login timeout -- auth token expires too early`.

### 7. Tiered status matching for `/move` (Issue 7)
**Integrating.** Plain substring `.includes()` causes ambiguous matches. Fix: tiered algorithm — (1) exact case-insensitive match, (2) prefix match (input is prefix of status name), (3) substring match. Return at the first tier that yields results. If tier 1 yields exactly one result, transition immediately.

### 8. `splitMessage` rejoin character and part numbering (Issues 8, 9)
**Integrating.** Chunks must preserve `\n\n` between paragraph blocks when assembling. Part numbering: always add `[1/N]` / `[N/N]` prefix when N > 2 (as the plan had). Actually simplify: always prefix when N > 1. Reserve space for the prefix in chunk size calculation to avoid exceeding 4096 chars.

### 9. Periodic `sendChatAction("typing")` refresh (Issue 11)
**Integrating.** Typing indicator lasts ~5 seconds. For `/create` (Claude call) and `/solve` (Claude call), send initial typing action, then use `setInterval` every 4 seconds to re-send until the API call resolves. Clear the interval in `finally`. This prevents the bot looking frozen during long Claude calls.

### 10. Prompt injection mitigation with XML delimiters (Issue 12)
**Integrating.** Jira descriptions come from Jira and could contain prompt-injection content. Fix: wrap untrusted content in XML-style delimiters in the prompt templates: `<description>...</description>` and `<title>...</title>`. This signals to Claude that the content is data, not instructions.

### 11. Graceful shutdown on SIGTERM/SIGINT (Issue 16)
**Integrating.** Without calling `bot.stop()` on shutdown, in-flight updates are lost and orphaned Jira tickets can be created. Fix: add `process.on('SIGTERM', ...)` and `process.on('SIGINT', ...)` handlers in `bot.ts` that await `bot.stop()` before exiting.

### 12. Define `Clients` type explicitly (Issue 22)
**Integrating.** `registerCommands(bot, clients: Clients)` referenced an undefined type. Fix: define `interface Clients { jira: JiraClient; claude: ClaudeClient }` in `commands/index.ts`. Handlers receive clients via closure capture at registration time — the `Clients` object is captured when the handler function is created inside `registerCommands`.

### 13. ADF clarification — Claude returns plain text, JiraClient converts (Issue 23)
**Integrating as documentation.** This is not a bug — `JiraClient.createIssue()` calls `toADF()` internally on the description string. Claude returning plain text is correct and intentional. Fix: add a note in the `/create` handler section clarifying that the plain text description flows into `JiraClient.createIssue()` which wraps it in ADF automatically.

### 14. Sanitize errors before logging (Issue 15)
**Integrating.** Full error objects can include Authorization headers (e.g., from fetch error details). Fix: log only `error.message` and `error.type` (if typed error), never the full error object.

### 15. Add auth middleware tests (Issue 26)
**Integrating.** No tests for the auth gate is a gap. Fix: add `auth.test.ts` covering: authorized user passes through, unauthorized user is dropped (no reply), `ctx.from` absent is treated as unauthorized.

### 16. Add long description test for `/solve` (Issue 29)
**Integrating.** Fix: add a `solve.test.ts` case where the Jira description is very long (e.g., 10,000 chars) to verify it doesn't break the prompt construction.

---

## What I'm NOT Integrating and Why

### A. Per-command timeout layer (Issue 18)
**Not integrating.** `ClaudeClient` already has a configurable `timeoutMs`. Adding a second timeout layer at the handler level is redundant and adds complexity for no practical benefit. The ClaudeClient timeout propagates as `ClaudeTimeoutError` which the handler already catches.

### B. `/cancel` command (Issue 21)
**Not integrating.** Out of scope. Personal bot use case. A stuck `/solve` call will time out via the ClaudeClient timeout.

### C. Static help text single-source-of-truth (Issue 20)
**Not integrating.** The bot has exactly 5 fixed commands that will not change frequently. Generating help text from `CommandGroup` data adds indirection for minimal benefit.

### D. Rate-limiting auth rejection logs (Issue 24)
**Not integrating.** Personal bot, not public. A DoS attack via bot spam is not a realistic threat model.

### E. Concurrency tests for `/solve` (Issue 27)
**Not integrating.** Single-user personal bot. Concurrent `/solve` from the same user is not a realistic scenario.

### F. Throttler integration tests (Issue 28)
**Not integrating.** The throttler is a third-party grammY plugin with its own test suite. Testing its behavior is not our responsibility.

### G. Long-polling idempotency / at-least-once delivery (Issue 13)
**Documenting, not fixing.** At-least-once delivery is a known limitation of long-polling bots. Adding deduplication logic (idempotency keys, last-N-seconds comment checks) significantly increases complexity for a personal bot where restart-crash-duplicate is rare. Document the risk in the plan.

### H. `/solve` concurrency in-flight tracking (Issue 10)
**Not integrating.** Personal bot with a single authorized user. Parallel `/solve` is not a realistic scenario.

### I. Webhook mode (Issue 17)
**Documenting as assumption.** Long-polling is the chosen deployment model. Documenting the assumption is sufficient.

### J. Reply failure handling (Issue 19)
**Not integrating.** `bot.catch` already handles unhandled rejections. Catching every `ctx.reply()` failure individually adds boilerplate. The throttler auto-retry handles transient failures.
