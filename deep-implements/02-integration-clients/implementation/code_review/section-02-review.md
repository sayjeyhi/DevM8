# Code Review: section-02-adf-helpers

## CRITICAL

1. **AdfNode missing `version?: number`** — `toADF` sets `version: 1` but the interface doesn't declare it; test uses unsafe cast.

2. **`doc` join strategy** — `.join('\n')` on already-`\n`-terminated block children creates double newlines. Tests pass because spec expects this for paragraphs; inconsistency with `bulletList` (no trailing `\n`) not tested.

## IMPORTANT

3. **`mention` `||` vs `??`** — `attrs.text || 'user'` treats empty string as absent (spec-correct: "absent or empty string → @user").

4. **`toADF` whitespace-only lines** — `'   '` passes the `length > 0` filter, producing a paragraph with a space-only text node.

5. **`codeBlock` + `doc` double-newline** — code block emits trailing `\n`, then `join('\n')` adds another. Not tested.

6. **`heading` level not rendered** — spec doesn't require it; attrs.level silently dropped.

## MINOR

7. **Unsafe cast in test** — `(doc as AdfNode & { version: number })` hides missing interface field.
8. **Real-world test uses `toContain`** — exact output not verified.
9. **No round-trip test** — spec checklist calls for it.
10. **`orderedList` test** — one item, `toContain` only.

## NITPICK

11. **Missing `?? []` in default branch** — `null` content would throw; other branches use `?? []`.
12. **No JSDoc** — spec reference included JSDoc for exported functions.
