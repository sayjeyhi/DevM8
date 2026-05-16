# Opus Review

**Model:** claude-opus-4
**Generated:** 2026-05-14T00:00:00Z

---

# Senior Architect Review: 03-command-handlers Implementation Plan

## Critical Issues

### 1. Auth Middleware Bug (Section 3)
The plan states the middleware should call `ctx.answerCallbackQuery()` for unauthorized users. This is wrong for two reasons:
- `answerCallbackQuery()` is for inline keyboard callbacks, not slash commands. Calling it on a regular message context will throw an error.
- The next line says "simply returns — never replies" which contradicts the call to `answerCallbackQuery()`.

Action: Remove the `answerCallbackQuery()` reference entirely. The middleware should just `return` (drop silently).

### 2. Auth Middleware Position vs. Command Registration (Sections 2 & 11)
The plan installs middleware "before handlers are registered" but `@grammyjs/commands` is installed via `bot.use(myCommands)` after auth. If `setCommands(bot)` is called once and the `CommandGroup` is the final `bot.use()`, you must verify ordering. Also note: `setCommands()` syncs the Telegram UI menu but does NOT register handlers — handlers are wired via `commands.command(...)` chains. The plan conflates these in Section 11. Clarify that `setCommands()` is purely UI sync, and `bot.use(commands)` is what installs the actual handler dispatcher.

### 3. Allowlist Bypass via Telegram Menu / Throttler Ordering (Section 2)
The transformer-throttler is installed on `bot.api`. This affects outgoing API calls regardless of authorization status. That is fine, but more critically: `setCommands(bot)` populates the bot's command menu for ALL users who can see the bot — including unauthorized ones. Unauthorized users will see the command menu in Telegram, attempt commands, and be silently dropped. This is confusing UX but more importantly leaks the command surface area. Consider using `scope: { type: "chat", chat_id: <allowed_id> }` per allowed user when calling `setCommands`, or accept this as a known limitation and document it.

### 4. Missing Authorization in `bot.catch` Path (Section 12)
The global `bot.catch` handler runs for all updates including unauthorized ones. If the auth middleware throws (instead of returning), unauthorized users could receive error replies — a side channel revealing the bot exists. Ensure `bot.catch` never replies unless the user passed auth.

## Footguns and Edge Cases

### 5. `parseArgs` Drops Multi-Word Quoting (Sections 4, 7, 8)
The plan says "splits on whitespace, and filters empty strings" but then `/move` and `/comment` reconstruct the text by joining tokens after the first. Splitting and rejoining is lossy:
- Multiple spaces collapse to single space
- Tabs/newlines normalize incorrectly
- Trailing/leading whitespace in the original is lost

For `/comment ENG-1 Hello   world` (multiple spaces), the comment becomes `Hello world`. Instead, capture `firstToken` plus `rest` (the unsplit remainder) using a regex like `/^(\S+)\s+([\s\S]+)$/`. This is especially important for `/comment` where formatting matters.

### 6. `/create` Title vs. Description Split is Fragile (Sections 4, 6)
"The handler extracts the first whitespace-delimited token as the title; everything after the first space is the description." This means `/create Fix login bug` would have title = `Fix` and description = `login bug`. That is almost certainly wrong. Titles are typically multi-word. The plan needs a clearer disambiguation rule:
- Option A: Title is the first sentence (split on `. `)
- Option B: Title is everything before a `--` or `|` separator
- Option C: Title is the first line (split on newline)

The current spec creates Jira issues with single-word titles in the common case. This is a major UX bug.

### 7. `/move` Status Matching: `.includes()` is Dangerous (Section 7)
Using `includes()` means input `"in"` matches `"In Progress"`, `"In Review"`, `"Done"` (no — but `"o"` matches `"To Do"` and `"Done"` and `"In Progress"`). The ambiguity branch covers this, but consider:
- Exact match (case-insensitive) should win over substring match — promote exact matches before falling back to substring.
- Prefix match (`startsWith`) is usually more intuitive than substring for command UIs.

Add a tiered matching algorithm: exact → prefix → substring, returning at the first tier with results.

### 8. `splitMessage` Paragraph Algorithm (Section 5)
"Split text into paragraphs on double-newline boundaries" loses the double newlines on rejoin. The chunks need to preserve the `\n\n` separators or the output text will be visually different from the source. Specify what rejoin character is used.

Also: the "2-part splits don't get part numbers" rule will surprise users who think they only got half the answer. Suggest always prefixing `[1/N]` when N > 1, or attaching a trailing `(continued)` marker.

### 9. `splitMessage` Limit Constant (Section 5)
4096 chars is the Telegram limit, but if any chunk needs a `[1/N]` prefix, the effective payload limit is 4096 minus prefix length. The algorithm must reserve space for the prefix when computing chunk size, or chunks at the boundary will exceed the limit.

### 10. `/solve` Race Condition with Multiple Replies (Section 9)
Step 2 sends "Analyzing ENG-123…", step 8 sends each chunk. If the user types `/solve` twice in rapid succession, two parallel flows interleave their replies. There's no concurrency control or in-flight tracking. Consider a per-user in-flight map keyed by command type, replying "Already analyzing — please wait."

### 11. `sendChatAction("typing")` is Time-Limited (Sections 6, 7, 8, 9)
The typing indicator only lasts ~5 seconds in Telegram. For long Claude calls in `/solve`, you need to re-send `sendChatAction` periodically (e.g., every 4 seconds via `setInterval`) until the API call completes. Otherwise the indicator vanishes and the user thinks the bot froze.

### 12. Prompt Template Substitution Injection (Sections 6, 9)
`SOLVE_PROMPT_TEMPLATE` substitutes `{description}` from Jira. Jira descriptions can contain `{key}`, `{summary}`, or other template-like strings. If you use naive `.replace("{description}", desc)` it works, but if you use a general templater that recurses, you could inject. More importantly, untrusted Jira content goes into a Claude prompt — prompt injection risk. Document mitigations: clearly delimit user content with markers like `<description>…</description>` so Claude treats it as data, not instructions.

### 13. Long-Polling Resilience (Section 2)
`bot.start()` is long-polling — what happens on transient network failure? grammY usually retries, but if the process crashes mid-handler, the update is re-delivered by Telegram. This means duplicate Jira tickets, duplicate comments. Consider idempotency:
- For `/create`, deduplication is hard without an idempotency key.
- For `/comment`, you could check for an existing identical comment in the last N seconds.

At minimum, document this risk and the lack of exactly-once delivery.

### 14. Config Validation Allowlist Type Coercion (Section 1)
`ALLOWED_USER_IDS` parsed from a comma-separated string into `Set<number>`: what about whitespace? Empty strings? Non-numeric entries? `Number("abc")` returns `NaN`, which then gets added to the Set. Spec the parser explicitly with rejection of NaN.

### 15. Token Leakage in Logs (Section 12)
Logging `{ command, ticketKey, error }` — if `error` is an Axios-style error object, it can include the full request including Authorization headers. Sanitize before logging or use a redacting logger.

## Missing Considerations

### 16. No Mention of Graceful Shutdown
On SIGTERM/SIGINT, `bot.stop()` must be called to drain in-flight updates. Without this, you lose updates and create orphaned Jira tickets. Add a shutdown handler to `bot.ts`.

### 17. No Webhook Mode Discussion
The plan commits to long-polling. Many small-team bots are fine with this, but if the bot needs to run in a stateless environment (Lambda, Cloud Run), webhooks are required. Document the deployment assumption.

### 18. No Mention of Per-Command Timeouts
Claude calls can hang. The handler should impose a wall-clock timeout (e.g., 60s) independent of the Claude client's internal timeout, and reply with a timeout error. Otherwise a stuck handler holds Telegram's update slot.

### 19. No Discussion of Reply Failures
If `ctx.reply()` itself fails (e.g., user blocked the bot mid-conversation), the throttler retry just keeps spinning. Should be caught and logged, not infinitely retried.

### 20. Help Text is Static (Section 10)
If commands are added/removed, help text drifts. Generate `/help` text from the same `CommandGroup` source of truth used for `setCommands()`. Single source of truth.

### 21. Missing /cancel Command
For `/solve` (potentially 30+ second operations), users have no way to abort. Not strictly required for a personal bot, but worth listing as a known gap.

### 22. No Specification of the `Clients` Type (Section 11)
The signature `registerCommands(bot: Bot, clients: Clients): Promise<void>` references a `Clients` type that's never defined. Specify its shape — presumably `{ jira: JiraClient, claude: ClaudeClient }`. Also, the handlers as written in Sections 6-10 take only `ctx` — how do they receive the clients? Via closure capture at registration time, or via `ctx` decoration? This is a critical detail missing from the plan.

### 23. No Mention of Jira ADF / Description Format
Jira Cloud requires Atlassian Document Format (ADF) for descriptions, not plain text/markdown. If Claude returns plain text, it must be wrapped in ADF before being sent to `createIssue`. The plan claims "Claude returns plain text only" but Jira does not accept plain text in the description field on Cloud. Verify with the integration client spec.

## Performance Issues

### 24. Allowlist `Set` is Fine, but Auth Logging Could Be DoS Vector (Section 3)
If an attacker spams the bot, every update spawns a log write. Consider rate-limiting auth-rejection logs (e.g., log once per user per minute).

### 25. Synchronous Chunk Sending in /solve (Section 9)
Step 8 sends chunks "sequentially via `ctx.reply()`." For 10 chunks, that's 10 round trips serialized. With the throttler enforcing 1 msg/s per chat, a 10-chunk reply takes 10 seconds. Document this as expected behavior, or batch into fewer larger chunks.

## Testing Gaps (Section 13)

### 26. No Tests for Auth Middleware
Unauthorized user IDs, missing `ctx.from`, malformed updates — none covered.

### 27. No Tests for Concurrency
What happens with parallel `/solve` from the same user? See item 10.

### 28. No Tests for Throttler / Auto-Retry Integration
At least one integration test should confirm a 429 response is handled gracefully.

### 29. No Test for Long Jira Descriptions in /solve Prompt
A description of 50,000 characters could blow Claude's context. Test the boundary.

## Summary of Required Plan Updates

The most impactful issues to fix before implementation:

1. Fix `/create` title-vs-description split rule (item 6) — this is a P0 UX bug
2. Fix the `ctx.answerCallbackQuery` bug in auth middleware (item 1)
3. Specify how clients are passed to handlers (item 22)
4. Verify Jira ADF requirement and adjust description formatting (item 23)
5. Specify the tiered status-matching algorithm for `/move` (item 7)
6. Add periodic `sendChatAction` refresh for long operations (item 11)
7. Define prompt injection mitigation strategy (item 12)
8. Document idempotency / at-least-once delivery risks (item 13)
