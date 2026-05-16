# Code Review: section-05-claude (ClaudeClient)

## Summary

Implementation is mostly faithful to the spec. Core flow (args, env, stdin write, concurrent drain, timedOut guard, timer in finally) is all present and correct. Several issues range from a production correctness gap (non-null assertion) to type-safety concerns and missing validation.

---

## Issues

### CRITICAL — Non-null assertion `proc.exitCode!` hides runtime risk

**ClaudeClient.ts line 110:**
```typescript
throw new ClaudeExitError(proc.exitCode!, stderr)
```
`proc.exitCode` is typed `number | null`. `null !== 0` is `true` in JS, so if exitCode stays null (Bun bug, or edge case after exited resolves), the `!` assertion lies and `ClaudeExitError` receives `null` cast to `number`.

**Fix:** `if (proc.exitCode !== null && proc.exitCode !== 0)`

### MAJOR — No runtime validation that `parsed.result` is a string

**ClaudeClient.ts line 119:**
```typescript
return (parsed as { result: string }).result
```
If Claude CLI returns `{ "result": null }`, `{ "result": 42 }`, or `{}`, the cast silently passes and the caller receives `null`/`undefined`/number where a string is expected.

**Fix:** Add type guard before return.

### MAJOR — `declare const Bun` hand-rolled type is too loose

The local ambient declaration overrides any real Bun global types. `kill` is typed as `string` instead of `NodeJS.Signals | number`. Better to import from `'bun'` if available, or at minimum tighten the signal type.

### MINOR — `timedOut` declared after stdin write; spec says before spawn

Spec states: "Declare `let timedOut = false` before spawning." Current code declares it at line 81, after `proc.stdin.write/end`. Not dangerous (timer cannot fire synchronously) but deviates from stated spec ordering.

### MINOR — `claude_done` log call outside try/finally

If `Promise.all` throws (stream error), `clearTimeout` fires but `claude_done` is never logged. Acceptable behavior but `claude_done` is not a reliable "drain completed" signal.

### MINOR — No test asserting shape of `claude_done` log

`durationMs` is computed but never asserted in tests. A regression logging `NaN` would go unnoticed.

### MINOR — Spec inconsistency: object-bag constructor calls vs positional

Spec lines 195–198 show `new ClaudeTimeoutError({ timeoutMs })` and `new ClaudeExitError({ exitCode, stderr })`. Actual `src/errors.ts` uses positional args. Implementation correctly uses positional form. Spec document needs correction.

### NITPICK — `makeHungProc` only resolves on SIGKILL (not documented)

Future test authors won't know SIGTERM alone leaves the promise pending. Document this constraint.

### NITPICK — Stdout drain test doesn't prove concurrency

Test only checks return value; doesn't verify both streams were consumed.

---

## Recommendations

1. Replace `proc.exitCode!` with explicit null check: `proc.exitCode !== null && proc.exitCode !== 0`
2. Add runtime type guard: `if (typeof (parsed as Record<string, unknown>).result !== 'string') throw new Error(...)`
3. Move `let timedOut = false` before `Bun.spawn` call
4. Add test asserting `claude_done` log carries `durationMs: number`
5. Update spec doc to use positional constructor signatures
