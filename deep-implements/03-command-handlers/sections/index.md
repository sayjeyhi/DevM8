<!-- PROJECT_CONFIG
runtime: typescript-bun
test_command: vitest run
END_PROJECT_CONFIG -->

<!-- SECTION_MANIFEST
section-01-foundation
section-02-auth-middleware
section-03-utils
section-04-create-handler
section-05-move-comment-help-handlers
section-06-solve-handler
section-07-registration-and-bot
END_MANIFEST -->

YOU ARE FORCED TO IMPLEMENT EVERTHING IN THE ROOT OF THIS PROJECT not 03-command-hanlders folder!

# Implementation Sections Index: 03-command-handlers

## Dependency Graph

| Section | Depends On | Blocks | Parallelizable |
|---|---|---|---|
| section-01-foundation | — | all | Yes (first) |
| section-02-auth-middleware | 01 | 07 | Yes (with 03, 04, 05, 06) |
| section-03-utils | 01 | 04, 05, 06 | Yes (with 02) |
| section-04-create-handler | 01, 03 | 07 | Yes (with 05, 06) |
| section-05-move-comment-help-handlers | 01, 03 | 07 | Yes (with 04, 06) |
| section-06-solve-handler | 01, 03 | 07 | Yes (with 04, 05) |
| section-07-registration-and-bot | 02, 04, 05, 06 | — | No |

## Execution Order

1. `section-01-foundation` — no dependencies
2. `section-02-auth-middleware`, `section-03-utils` — parallel after 01
3. `section-04-create-handler`, `section-05-move-comment-help-handlers`, `section-06-solve-handler` — parallel after 03
4. `section-07-registration-and-bot` — after all handlers

## Section Summaries

### section-01-foundation
Project scaffolding: `package.json` (grammy + plugins + vitest devDependencies), `tsconfig.json`, `vitest.config.ts`, `src/config.ts` (`Config` type, `loadConfig()` — validates all required fields at startup, parses `ALLOWED_USER_IDS` comma-separated string into `Set<number>` with NaN filtering). Tests: `tests/config.test.ts` — all required fields, missing field errors, allowlist parsing edge cases (spaces, non-numeric, empty).

### section-02-auth-middleware
`src/middleware/auth.ts` — `createAuthMiddleware(allowedIds: Set<number>): MiddlewareFn<Context>`. Checks `ctx.from?.id` against the allowlist, silently returns if unauthorized (no reply), logs `{ event: 'unauthorized', chatId }` (never userId for PII). Known limitation: Telegram command menu visible to all users — documented in code. Tests: `tests/middleware/auth.test.ts` — authorized/unauthorized/undefined `ctx.from`, empty allowlist, logging assertions.

### section-03-utils
Two pure utilities:
- `src/utils/parseArgs.ts` — `parseArgs(ctx)` (split + filter) and `parseFirstAndRest(input)` (regex `/^(\S+)\s+([\s\S]*)$/` to preserve raw remainder). Tests: `tests/utils/parseArgs.test.ts`.
- `src/utils/splitMessage.ts` — `splitMessage(text, limit?)`. Algorithm: split on `\n\n`, accumulate into chunks (rejoining with `\n\n`), word-boundary fallback, hard-cut last resort. Always prefix `[N/M]` when N > 1; reserves 8 chars for prefix in effective limit calculation. Tests: `tests/utils/splitMessage.test.ts` — exact limit, over limit, paragraph splits, word splits, hard cut, all-parts prefix, prefix space reservation.

### section-04-create-handler
`src/commands/create.ts`. Uses `--` separator to split title (before ` -- `) and description (after). No `--` → title-only expand path. Exported constants: `ENRICH_PROMPT_TEMPLATE` and `EXPAND_PROMPT_TEMPLATE` — both use `<title>...</title>` and `<description>...</description>` XML delimiters for prompt injection defense. Periodic `sendChatAction("typing")` refresh every 4s via `setInterval`, cleared in `finally`. Claude failure → silent fallback to raw/empty description. `JiraClient.createIssue` converts plain text to ADF internally. Tests: `tests/commands/create.test.ts`.

### section-05-move-comment-help-handlers
Three handlers:
- `src/commands/move.ts` — `parseFirstAndRest` for key + raw status remainder. Calls `JiraClient.transitionIssue(key, status)` which does case-insensitive exact matching internally. Catches `InvalidTransitionError` → replies with available list.
- `src/commands/comment.ts` — `parseFirstAndRest` for key + raw comment text (preserves spacing). Calls `JiraClient.addComment(key, text)`.
- `src/commands/help.ts` — pure `ctx.reply(HELP_TEXT)`. `HELP_TEXT` exported as constant.
Tests: `tests/commands/move.test.ts`, `tests/commands/comment.test.ts`, `tests/commands/help.test.ts`.

### section-06-solve-handler
`src/commands/solve.ts`. Most complex handler: sends intermediate "Analyzing…" reply, starts typing refresh interval, fetches issue, builds `SOLVE_PROMPT_TEMPLATE` with XML-delimited fields, calls `ClaudeClient.ask`, clears interval in `finally`, splits response with `splitMessage`, sends chunks. `SOLVE_PROMPT_TEMPLATE` exported. At-least-once delivery risk documented. Tests: `tests/commands/solve.test.ts` — normal flow, multi-chunk response, very long Jira description (10k+ chars), timeout error, not-found error.

### section-07-registration-and-bot
`src/commands/index.ts` — `interface Clients { jira: JiraClient; claude: ClaudeClient }` + `registerCommands(bot, clients)`. Clients captured via closure. `bot.use(commands)` installs handlers; `setCommands(bot)` syncs Telegram UI menu (separate operations documented).
`src/bot.ts` — constructs bot, installs auth middleware, transformer-throttler, auto-retry, registers commands, installs `bot.catch`, calls `bot.start()`. SIGTERM/SIGINT handlers call `await bot.stop()` before exit.
Error logging sanitized: only `error.message` and `error.type`, never full error object.
Tests: `tests/bot.test.ts` — bot construction, auth middleware ordering, graceful shutdown, `bot.catch` fires for handler errors.
