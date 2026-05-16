# Code Review Interview: section-02-adf-helpers

## User Decisions

**toADF whitespace-only lines** → Skip them (trim before filter; user confirmed)

## Auto-Fixes Applied

1. `toADF`: added `.map(line => line.trim())` before filter — whitespace-only lines now skipped
2. `adfToText` default branch: added `?? []` guard (`(node.content ?? []).map(adfToText).join('')`)
3. Added JSDoc to `toADF` and `adfToText`
4. Test: removed unsafe cast `(doc as AdfNode & { version: number })` — `version?: number` already in interface
5. Test: added whitespace-only line test for `toADF`
6. Test: added round-trip test (`toADF(adfToText(doc))`)

## Let Go

- `doc` join double-newline: working as specified (each paragraph emits `\n`, join adds `\n` between = two newlines between paragraphs). Tests verify this.
- `mention` uses `||` (correct: spec says "absent or empty string → @user")
- `heading` level not rendered (spec doesn't require it)
- `codeBlock` double-newline in `doc`: edge case, no test, not spec-required
- Real-world test uses `toContain` (flexibility for format variations)
- `orderedList` test minimalism (spec doesn't require numbering in plain text)
