# Code Review Interview: section-04-jira

## No User Questions — All Auto-Fixes

## Auto-Fixes Applied

1. `request()`: Added `|| controller.signal.aborted` to AbortError check for robustness
2. `request()`: Added `if (response.status === 204) return undefined as T` before `response.json()` — prevents parse error on void Jira API responses
3. Logging test: Added `mockLogger.error.mock.calls` to apiToken check (both channels)
4. Added 204 No Content test for `transitionIssue`
5. `index.ts`: Added `export * from './src/jira/adf'` — exposes `AdfNode` type to package consumers

## Reviewer Errors (Let Go)

- "Failed requests never logged" — FALSE. `logger.info` fires before error throws (line 68)
- "extractIssueKey regex requires trailing slash" — FALSE. Final `/` is the regex literal delimiter, not a pattern character

## Let Go (Real)

- `buildAuthHeader` as private method vs inline (functionally equivalent, not worth refactoring)
- Logger interface forward-compat (same TODO pattern as section-03)
- createIssue test doesn't verify ADF content deeply (`.type === 'doc'` sufficient for this level)
- Double-slash URL defense (no callsite passes leading slash, no practical risk)
