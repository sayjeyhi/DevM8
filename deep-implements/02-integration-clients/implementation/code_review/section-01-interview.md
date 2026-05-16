# Code Review Interview: section-01-foundation

## User Decisions

**JiraRateLimitError constructor arg ordering** → Keep positional args (user preference; TypeScript catches misuse)

**Object.setPrototypeOf** → Skip (spec explicitly says not needed for ESNext; user confirmed)

## Auto-Fixes Applied

1. Added `typecheck` script to `package.json` (`tsc --noEmit`)
2. Added `exports` field to `package.json` (`.` → `./index.ts`)
3. Added `include: ['tests/**/*.test.ts']` to `vitest.config.ts`
4. Added `.message` assertion to `JiraPermissionError` test
5. Added `.name` assertion to `JiraTimeoutError` test
6. Added exhaustiveness `default: never` branch to discriminant switch test
7. Added `expect(errors.length).toBe(9)` to confirm array length

## Let Go

- grammy version range mismatch (harmless, ^1.36 satisfies 1.42)
- tsconfig rootDir/outDir includes tests (not a runtime issue for this project)
- JiraServerError message format not tested (no spec mandate)
