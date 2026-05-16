# Code Review: section-01-foundation

## Findings

**BUG — errors.ts — LaunchctlError message**
`super('launchctl failed: ' + stderr, hint)` — `.message` will contain full stderr, producing redundant noisy output when section-05 prints both `.message` and `.rawOutput` per the display contract. Fix: `super('launchctl invocation failed', hint)`.

**DESIGN — package.json — all deps pinned to 'latest'**
Reproducibility hazard. bun.lock records resolved versions but package.json carries no constraint. Pin to caret ranges matching resolved versions.

**DESIGN — tsconfig.json — no `noEmit: true`**
Without it, `tsc` (IDE tooling, CI type-check) may emit compiled output into source tree. Add `"noEmit": true`.

**IMPROVEMENT — paths.ts — child paths repeat parent literal**
`".config/jira-assistant"` is repeated in every child path. Compose from `configDir` constant. If `configDir` changes, all child paths update automatically.

**NITPICK — bun.lock — zod resolves to v4.4.3**
Zod v4 is a significant API change from v3. Confirm this is intended before section-02-config authors against it.
