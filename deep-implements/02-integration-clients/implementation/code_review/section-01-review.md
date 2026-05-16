# Code Review: section-01-foundation

## CRITICAL

1. **Missing `Object.setPrototypeOf`** — spec mentions ESNext doesn't need it, but reviewer flagged as latent risk if bundler target changes.

2. **Discriminant switch test not exhaustive** — no `default: never` branch; doesn't catch new colliding type strings at compile time.

## IMPORTANT

3. **`JiraRateLimitError` constructor arg ordering** — `(retryAfter?, message?)` makes passing only a message impossible without TypeScript error; poor ergonomics.

4. **Missing `typecheck` script** — spec acceptance criterion #5 requires `tsc --noEmit` but no npm script for it.

5. **vitest.config.ts missing `include`** — defaults pick up all `.test.ts` in tree; should pin to `tests/**/*.test.ts`.

6. **Missing `exports` field in package.json** — module resolution cannot verify imports from downstream packages.

## MINOR

7. **`JiraPermissionError` test** — doesn't assert `.message` or `.name`, only `.type`.
8. **`JiraTimeoutError` test** — doesn't assert `.name`.
9. **`JiraServerError` test** — doesn't verify auto-generated message format.

## NITPICK

10. **grammy range** — `^1.36.0` declared but `1.42.0` resolved (harmless).
11. **tsconfig** — `outDir: dist` + `include: tests/**` means tests compile to dist.
12. **vitest typecheck** — not configured but consistent with `globals: false`.
