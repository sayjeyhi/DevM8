<!-- PROJECT_CONFIG
runtime: typescript-bun
test_command: vitest run
END_PROJECT_CONFIG -->

<!-- SECTION_MANIFEST
section-01-foundation
section-02-adf-helpers
section-03-telegram
section-04-jira
section-05-claude
END_MANIFEST -->

# Implementation Sections Index: 02-integration-clients

## Dependency Graph

| Section | Depends On | Blocks | Parallelizable |
|---|---|---|---|
| section-01-foundation | — | 02, 03, 04, 05 | Yes (first) |
| section-02-adf-helpers | 01 | 04 | Yes (with 03, 05) |
| section-03-telegram | 01 | — | Yes (with 02, 05) |
| section-04-jira | 01, 02 | — | No |
| section-05-claude | 01 | — | Yes (with 02, 03) |

## Execution Order

1. `section-01-foundation` — no dependencies
2. `section-02-adf-helpers`, `section-03-telegram`, `section-05-claude` — parallel after 01
3. `section-04-jira` — after 02 (needs adf helpers)

## Section Summaries

### section-01-foundation
Project scaffolding: `package.json` (grammy + vitest devDependencies), `tsconfig.json`, `vitest.config.ts`, `src/errors.ts` (all 9 typed error classes with `readonly type` discriminants), `src/index.ts` (re-exports all clients + types + errors). Zero external calls — pure type/error definitions. Tests verify every error class: `instanceof Error`, correct `type` value, carried payload fields (`issueKey`, `retryAfter`, `status`, `timeoutMs`, `exitCode`, `stderr`, `attempted`, `available`).

### section-02-adf-helpers
`src/jira/adf.ts` containing two pure utility functions with no external dependencies: `toADF(text)` (splits input on `\n`, creates one ADF paragraph per non-empty line, returns a complete ADF `doc` node) and `adfToText(node)` (recursive walker covering all node types: `text`, `hardBreak`, `paragraph`, `heading`, `blockquote`, `listItem`, `bulletList`, `orderedList`, `codeBlock`, `mention`, `doc`, unknown-recurse fallback, null/undefined guard). Tests in `tests/adf.test.ts` — pure unit, no mocks needed.

### section-03-telegram
`src/telegram/types.ts` (`TelegramConfig` with `allowedUserIds: number[]`, `CommandContext`, `CommandHandler`), `src/telegram/splitMessage.ts` (`splitMessage(text, limit)` helper — splits at `\n\n` paragraph boundaries first, then word boundaries), `src/telegram/TelegramClient.ts` (grammY `Bot` wrapper: authorization gate, `onCommand` registration, error middleware mapping typed errors to user replies, `sendMessage` routing through `splitMessage`, `startPolling` with `drop_pending_updates: true`, `stopPolling`). Tests in `tests/telegram.test.ts` mock the grammY `Bot` constructor and its methods.

### section-04-jira
`src/jira/types.ts` (`JiraConfig` with `requestTimeoutMs`, `issueType`; `JiraIssue`), `src/jira/JiraClient.ts` (Basic auth header construction, `request()` private method with `AbortController` timeout + full HTTP status error mapping: 401→`JiraAuthError`, 403→`JiraPermissionError`, 404→`JiraNotFoundError`, 429→`JiraRateLimitError` with `Retry-After` header parsing, 5xx→`JiraServerError`, abort→`JiraTimeoutError`; `createIssue` with follow-up `getIssue`; `getIssue` with ADF extraction; `transitionIssue` with case-insensitive match + `InvalidTransitionError`; `addComment`; `encodeURIComponent` on all issueKey path segments; request logging without auth header). Tests in `tests/jira.test.ts` mock global `fetch`. Depends on section-02 for `toADF`/`adfToText`.

### section-05-claude
`src/claude/types.ts` (`ClaudeConfig` with `binaryPath`, `timeoutMs`, `model`; `AskOptions`), `src/claude/ClaudeClient.ts` (`Bun.spawn` with prompt via stdin (not argv), `delete clonedEnv.CLAUDECODE` on cloned env, `timedOut` flag set before kill, SIGTERM→2s grace→SIGKILL kill sequence, timer cleared in `finally`, concurrent stdout+stderr+exited drain via `Promise.all`, JSON response parsing, `ClaudeTimeoutError` vs `ClaudeExitError` discrimination). Tests in `tests/claude.test.ts` mock `Bun.spawn`. No dependency on sections 02–04.
