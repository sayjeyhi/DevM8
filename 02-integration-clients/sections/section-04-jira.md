Now I have all the context needed to generate the section content for `section-04-jira`.

# Section 04: JiraClient

## Overview

This section implements the Jira integration client. It depends on:

- **section-01-foundation** (must be complete): `src/errors.ts` with all typed error classes
- **section-02-adf-helpers** (must be complete): `src/jira/adf.ts` with `toADF()` and `adfToText()`

This section has no dependencies on section-03-telegram or section-05-claude.

---

## Files to Create

- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/02-integration-clients/src/jira/types.ts`
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/02-integration-clients/src/jira/JiraClient.ts`
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/02-integration-clients/tests/jira.test.ts`

---

## Background and Context

JiraClient communicates with the Jira Cloud REST API v3. It is scoped to a single project (`config.projectKey`). All requests carry a Basic auth header derived from `config.email` and `config.apiToken`. No Jira SDK is used — only native `fetch`.

The `host` field is stored without a protocol prefix (e.g. `"yourcompany.atlassian.net"`). The client always prepends `https://` when building URLs.

---

## Tests First

File: `tests/jira.test.ts`

All tests use `vi.stubGlobal('fetch', mockFetch)` to intercept HTTP calls. No real network calls are made.

### Error mapping tests

- Test: HTTP 401 response → `request()` throws `JiraAuthError` (verify `instanceof JiraAuthError` and `.type === 'JIRA_AUTH'`)
- Test: HTTP 403 response → `request()` throws `JiraPermissionError`
- Test: HTTP 404 response → `request()` throws `JiraNotFoundError`
- Test: HTTP 429 response with `Retry-After: 30` header → throws `JiraRateLimitError` with `retryAfter === 30`
- Test: HTTP 503 response → throws `JiraServerError` with `.status === 503`
- Test: `AbortSignal` fires (request timeout) → throws `JiraTimeoutError`

### `createIssue` tests

- Test: sends `POST` to `https://{host}/rest/api/3/issue` with body `fields.project.key === config.projectKey`, `fields.issuetype.name === config.issueType` (or `"Task"` by default), and `fields.description` being a valid ADF doc (not a plain string)
- Test: when `config.issueType` is set to `"Bug"`, the POST body uses `"Bug"` not `"Task"`
- Test: after creation, makes a follow-up `GET /issue/{key}` call and returns the full `JiraIssue` from that response (not the minimal POST response which only has `id`, `key`, `self`)

### `getIssue` tests

- Test: GET request path uses `encodeURIComponent(issueKey)` — a key with special characters is URL-encoded correctly
- Test: maps `fields.summary` to `.summary`, `fields.status.name` to `.status`, and `fields.description` (ADF) to plain text via `adfToText`
- Test: when `fields.description` is `null` → `.description` is `""` (no crash)
- Test: `.url` equals `https://{host}/browse/{key}`

### `transitionIssue` tests

- Test: first makes `GET /issue/{key}/transitions`, then `POST /issue/{key}/transitions` with body `{ transition: { id: foundId } }`
- Test: transition name matching is case-insensitive — input `"in progress"` matches a transition named `"In Progress"`
- Test: when no transition matches the target status → throws `InvalidTransitionError` with `.attempted === targetStatus` and `.available` containing the names of all available transitions
- Test: `issueKey` is URL-encoded in all path segments (both the GET and the POST)

### `addComment` tests

- Test: sends `POST /issue/{key}/comment` with body `{ body: toADF(text) }` — the body is ADF format, not a plain string

### Logging tests

- Test: after a request completes, the logger receives an object containing `method`, `path`, `status`, and `durationMs`
- Test: the logger is never called with any value containing the `apiToken` string

---

## Implementation

### `src/jira/types.ts`

Define and export the following:

```typescript
interface JiraConfig {
  host: string              // e.g. "yourcompany.atlassian.net" — no protocol prefix
  email: string
  apiToken: string
  projectKey: string
  issueType?: string        // default: "Task"
  requestTimeoutMs?: number // default: 15000
}

interface JiraIssue {
  key: string
  summary: string
  status: string
  description: string       // plain text, extracted from ADF via adfToText()
  url: string               // https://{host}/browse/{key}
}
```

Also export a minimal `AdfNode` type (or import it from `adf.ts` if defined there) for use in `adfToText`. This can be a simple recursive object shape: `{ type: string; text?: string; content?: AdfNode[]; attrs?: Record<string, unknown> }`.

### `src/jira/JiraClient.ts`

Import `Logger` from `01-core-daemon` (or accept it as `any` with a `{ info, error }` interface if the import path is not yet available — check what section-01 exported).

Import typed errors from `../errors`:
- `JiraAuthError`, `JiraPermissionError`, `JiraNotFoundError`, `JiraRateLimitError`, `JiraServerError`, `JiraTimeoutError`, `InvalidTransitionError`

Import ADF helpers from `./adf`:
- `toADF`, `adfToText`

Import config and issue types from `./types`.

#### Constructor

```typescript
class JiraClient {
  constructor(private config: JiraConfig, private logger: Logger) { ... }
}
```

Pre-compute and store the Base64 auth header value in the constructor. Use `btoa(email + ':' + apiToken)` (available in Bun's global scope). Store as a private field `authHeader: string`.

#### Private `buildAuthHeader()`

Returns the string `"Basic " + btoa(config.email + ':' + config.apiToken)`. Called once in the constructor, result stored and reused.

#### Private `request(method, path, body?)`

Signature stub:
```typescript
private async request<T>(method: string, path: string, body?: unknown): Promise<T>
```

Steps:
1. Build the URL: `https://${config.host}/rest/api/3/${path}`
2. Create an `AbortController`, call `setTimeout(() => controller.abort(), config.requestTimeoutMs ?? 15000)`
3. Call `fetch(url, { method, headers: { Authorization: this.authHeader, 'Content-Type': 'application/json', Accept: 'application/json' }, signal: controller.signal, body: body ? JSON.stringify(body) : undefined })`
4. Clear the abort timer in a `finally` block
5. Record `durationMs` from `Date.now()` before/after
6. Log `{ method, path, status: response.status, durationMs }` — never include `authHeader` or `apiToken`
7. If the `fetch` throws with `name === 'AbortError'`, throw `new JiraTimeoutError()`
8. Map non-2xx status codes:
   - `401` → throw `new JiraAuthError()`
   - `403` → throw `new JiraPermissionError()`
   - `404` → throw `new JiraNotFoundError({ issueKey: extractIssueKeyFromPath(path) })`
   - `429` → parse `Retry-After` header as integer; throw `new JiraRateLimitError({ retryAfter })`
   - `>= 500` → throw `new JiraServerError({ status: response.status })`
   - Other non-2xx → throw generic `Error` with status and body text
9. Return `response.json()` cast to `T`

Note: The abort timer must be cleared even if the request succeeds, to avoid timer leaks in long-running processes.

#### `createIssue(title, description)`

```typescript
async createIssue(title: string, description: string): Promise<JiraIssue>
```

1. Build POST body with `fields.project.key`, `fields.issuetype.name` (`config.issueType ?? 'Task'`), `fields.summary`, `fields.description` (pass `description` through `toADF()`)
2. `POST` to `issue` via `this.request()`
3. The response only has `{ id, key, self }` — call `this.getIssue(response.key)` and return the result

#### `getIssue(issueKey)`

```typescript
async getIssue(issueKey: string): Promise<JiraIssue>
```

1. `GET` `issue/${encodeURIComponent(issueKey)}`
2. Map response fields to `JiraIssue`:
   - `key`: from response top-level
   - `summary`: from `fields.summary`
   - `status`: from `fields.status.name`
   - `description`: `adfToText(fields.description)` — if `fields.description` is `null` or `undefined`, `adfToText` returns `""` (it handles null gracefully per section-02 contract)
   - `url`: `https://${config.host}/browse/${response.key}`

#### `transitionIssue(issueKey, targetStatus)`

```typescript
async transitionIssue(issueKey: string, targetStatus: string): Promise<void>
```

1. `GET` `issue/${encodeURIComponent(issueKey)}/transitions`
2. From response, extract `transitions` array — each item has `{ id: string, name: string }`
3. Find the transition where `transition.name.toLowerCase() === targetStatus.toLowerCase()`
4. If not found, throw `new InvalidTransitionError({ attempted: targetStatus, available: transitions.map(t => t.name) })`
5. `POST` `issue/${encodeURIComponent(issueKey)}/transitions` with body `{ transition: { id: found.id } }`

#### `addComment(issueKey, body)`

```typescript
async addComment(issueKey: string, body: string): Promise<void>
```

`POST` `issue/${encodeURIComponent(issueKey)}/comment` with body `{ body: toADF(body) }`.

---

## Implementation Notes (Actual)

**Status: COMPLETE — 20/20 new tests passing (81 total)**

### Files Created

- `02-integration-clients/src/jira/types.ts` — JiraConfig, JiraIssue interfaces
- `02-integration-clients/src/jira/JiraClient.ts` — JiraClient class with request(), createIssue(), getIssue(), transitionIssue(), addComment()
- `02-integration-clients/tests/jira.test.ts` — 20 tests
- `02-integration-clients/index.ts` — updated to export jira module and adf types

### Deviations from Plan

- `request()` handles 204 No Content without calling `response.json()` (review finding)
- AbortError check uses `err.name === 'AbortError' || controller.signal.aborted` (review hardening)
- `buildAuthHeader()` inlined in constructor (functionally equivalent, no named private method)
- `AdfNode` exported from `index.ts` via `src/jira/adf` re-export (review finding)

### Key Behaviors

- Auth header pre-computed in constructor, never logged
- AbortController timeout with `clearTimeout` in finally block (no timer leaks)
- transitionIssue: case-insensitive name match; throws InvalidTransitionError with all available names
- createIssue: POST then follow-up GET for full JiraIssue shape
- 204 No Content responses return `undefined` cleanly

## Key Constraints and Edge Cases

- **Jira `host` format**: Never include a protocol prefix in `config.host`. The client always prepends `https://`. Storing the host without the prefix avoids double-slash bugs.
- **Transition name matching is case-insensitive**: Always call `.toLowerCase()` on both `transition.name` and `targetStatus` before comparing.
- **`issueKey` URL encoding**: Always call `encodeURIComponent(issueKey)` before interpolating into URL paths, in every method.
- **ADF description null check**: Jira issues created without a description have `fields.description === null`. The `adfToText` function from section-02 handles `null` input and returns `""`. Do not guard against this separately in `getIssue` — rely on section-02's contract, but ensure the value flows through rather than being short-circuited.
- **Auth header logging**: The logger must never receive the raw `apiToken` or the computed `authHeader`. Only log `{ method, path, status, durationMs }`.
- **Abort timer must be cleared**: Store the `setTimeout` handle and call `clearTimeout` in the `finally` block of `request()`. If not cleared, timers will accumulate across many requests in long-running processes.
- **No automatic retry**: The client only throws typed errors. Retry logic is the caller's responsibility (command handler layer).
- **`createIssue` follow-up GET**: The Jira POST response is intentionally minimal. The follow-up `getIssue` call is mandatory to return a fully-populated `JiraIssue` object to callers.