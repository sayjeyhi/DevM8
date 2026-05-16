# Code Review: section-04-jira

## CRITICAL

1. **AbortError check fragile** — `err.name === 'AbortError'` correct per spec but any third-party error with that name would convert. Should also check `controller.signal.aborted`.

2. **Failed requests not logged** — REVIEWER ERROR. The `logger.info` call IS before the error throws. All responses are logged. Not a real bug.

## IMPORTANT

3. **204 No Content: response.json() throws** — `transitionIssue` POST returns 204 with no body; `response.json()` will throw in production. Mock always returns `{}` masking this.

4. **Security logging test** — only checks `mockLogger.info`; `mockLogger.error` not checked for apiToken leakage.

5. **extractIssueKey regex** — REVIEWER ERROR. Regex `/issue\/([^/]+)/` correctly handles paths without trailing slash (final `/` is just the regex delimiter). Not a real bug.

## MINOR

6. **buildAuthHeader as method** — spec says private method; implementation inlines in constructor. Functionally equivalent.

7. **Logger interface forward-compat** — same as section-03, TODO comment present.

8. **AdfNode not exported from types.ts** — consumers can't import it from package index.

## NITPICK

9. **Double-slash URL defense** — callsites don't pass leading slashes; no practical risk.

10. **createIssue test** — doesn't verify ADF content, only `.type === 'doc'`.
