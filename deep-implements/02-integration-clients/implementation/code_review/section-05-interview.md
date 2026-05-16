# Code Review Interview: section-05-claude

## Items Triaged

### Auto-fixed (no user input needed)

**1. proc.exitCode null check**
- Issue: `proc.exitCode !== 0` is true when exitCode is null; non-null assertion `!` would pass `null as number`
- Fix: changed to `proc.exitCode !== null && proc.exitCode !== 0`; removed `!` assertion

**2. timedOut declaration order**
- Issue: `let timedOut = false` was declared after `Bun.spawn`, spec says before
- Fix: moved to before `Bun.spawn` call

### User-approved fixes

**3. runtime type guard on parsed.result** [user: yes, add it]
- Issue: `(parsed as { result: string }).result` silently returns null/undefined if result is not a string
- Fix: added `if (typeof result !== 'string') throw new Error('unexpected result type')`
- Added test: `exit 0 but result is not a string → throws Error`

**4. Narrow kill signal type to NodeJS.Signals** [user: yes, fix it]
- Issue: `kill(signal: string)` too loose; real Bun uses `NodeJS.Signals | number`
- Fix: changed `BunSubprocess.kill(signal: string)` to `kill(signal: NodeJS.Signals)`

### Let go (no action)

**5. Timer ghost-async after timeout fires** — known design tradeoff per spec; benign in production
**6. declare const Bun vs. import from 'bun'** — no `@types/bun` in deps; ambient decl is acceptable workaround
**7. claude_done log outside try/finally** — acceptable; log fires only on clean drain; spec says "after drain"
**8. durationMs not tested** — minor; production concern only if log format becomes a contract
**9. Spec doc inconsistency (object bags vs positional)** — spec debt, not code debt

## Final state

All tests pass: 99/99 (18 in claude.test.ts)
