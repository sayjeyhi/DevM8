Now I have all the context needed to generate the section content.

# Section 1: GitHub Actions Release Workflow

## Overview

This section covers creating the GitHub Actions release workflow for the `jira-assistant` project. The workflow compiles cross-platform binaries and publishes them as GitHub Release assets when a version tag is pushed.

No other sections need to be completed before implementing this section. It can be worked on in parallel with `section-02-binary-targets`.

Sections that depend on this one completing first: `section-03-checksums-generation`.

---

## Files to Create

- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/.github/workflows/release.yml`
- `/Users/sayjeyhi/Desktop/projects/github/sayjeyhi/jira-assistant/.github/workflows/lint.yml`

---

## Tests First

The following validation checks should be implemented as a CI `validate-workflow.sh` script (or inline `yamllint` + `grep` assertions) before writing the workflows. These are structural lint checks, not runtime tests.

**Test file: `tests/validate-workflow.sh`** (or run inline in CI)

```bash
# validate-workflow.sh
# Usage: bash tests/validate-workflow.sh
# Checks structural invariants of the release and lint workflows.

# 1. YAML is syntactically valid
yamllint .github/workflows/release.yml
yamllint .github/workflows/lint.yml

# 2. lint.yml contains shellcheck step
grep -q "shellcheck" .github/workflows/lint.yml

# 3. release.yml matrix has exactly 3 entries
# Count lines matching the target flags (darwin-arm64, darwin-x64, linux-x64)
count=$(grep -c "bun-darwin-arm64\|bun-darwin-x64\|bun-linux-x64" .github/workflows/release.yml)
[ "$count" -eq 3 ]

# 4. upload-artifact and download-artifact use @v4 (not @v3)
grep -q "upload-artifact@v4" .github/workflows/release.yml
grep -q "download-artifact@v4" .github/workflows/release.yml
! grep -q "upload-artifact@v3\|download-artifact@v3" .github/workflows/release.yml

# 5. prerelease expression is present
grep -q "contains(github.ref_name, '-')" .github/workflows/release.yml

# 6. Bun version is pinned to 1.3.11
grep -q "1.3.11" .github/workflows/release.yml

# 7. permissions: contents: write is set
grep -q "contents: write" .github/workflows/release.yml
```

All assertions must pass before the workflows are considered complete.

---

## Implementation Details

### `release.yml` — Structure

**Trigger:** push of any tag matching `v*.*.*`

**Permissions:** `contents: write` only — this lets `GITHUB_TOKEN` create releases and upload assets without additional secrets.

**Two jobs:**

#### Job 1: `build` (matrix)

Runs on `ubuntu-latest`. The matrix has exactly three entries:

| Matrix key | Bun target flag | Output filename |
|---|---|---|
| `darwin-arm64` | `bun-darwin-arm64` | `jira-assistant-macos-arm64` |
| `darwin-x64` | `bun-darwin-x64` | `jira-assistant-macos-x64` |
| `linux-x64` | `bun-linux-x64` | `jira-assistant-linux-x64` |

Steps in the build job:
1. `actions/checkout@v4`
2. `oven-sh/setup-bun@v2` with `bun-version: "1.3.11"` — **must be pinned to 1.3.11**. Bun v1.3.12 introduced a regression (GitHub issue #29120, PR #29272) that produces an invalid code signature on macOS ARM64 cross-compiled binaries, causing the OS to reject them at runtime with SIGKILL. Until the fix is confirmed, v1.3.11 must be used.
3. `bun install`
4. `bun build --compile --target=<bun_target> --outfile=<output_filename> src/index.ts`
5. `actions/upload-artifact@v4` — upload the compiled binary. The `name` and `path` should reference the matrix output filename.

**Critical:** Both `upload-artifact` and `download-artifact` must use `@v4`. v3 and v4 are not cross-compatible — mixing versions causes the download to fail silently or with a confusing error.

#### Job 2: `release`

Runs on `ubuntu-latest`. Has `needs: build` to wait for all three matrix entries.

Steps in the release job:
1. `actions/download-artifact@v4` with `merge-multiple: true` and `path: artifacts/` — downloads all three binaries into a flat `artifacts/` directory.
2. `sha256sum artifacts/* > checksums.txt` — generates the checksum file. The format is the standard `<64-hex-chars>  <filename>` per line.
3. `softprops/action-gh-release@v2` with:
   - `generate_release_notes: true`
   - `files: artifacts/* checksums.txt`
   - `prerelease: ${{ contains(github.ref_name, '-') }}` — tags like `v1.0.0-rc.1` or `v2.0.0-beta.2` are automatically marked as pre-release. Tags like `v1.0.0` are not.

**Note on action pinning:** `softprops/action-gh-release@v2` and `oven-sh/setup-bun@v2` use floating major version tags. SHA pinning (e.g., `@abc1234`) is best practice for supply-chain hardening but is intentionally deferred for this personal project.

#### Workflow skeleton

```yaml
name: Release

on:
  push:
    tags:
      - 'v*.*.*'

permissions:
  contents: write

jobs:
  build:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        include:
          - target: bun-darwin-arm64
            outfile: jira-assistant-macos-arm64
          - target: bun-darwin-x64
            outfile: jira-assistant-macos-x64
          - target: bun-linux-x64
            outfile: jira-assistant-linux-x64
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
        with:
          bun-version: "1.3.11"
      - run: bun install
      - run: bun build --compile --target=${{ matrix.target }} --outfile=${{ matrix.outfile }} src/index.ts
      - uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.outfile }}
          path: ${{ matrix.outfile }}

  release:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          merge-multiple: true
          path: artifacts/
      - run: sha256sum artifacts/* > checksums.txt
      - uses: softprops/action-gh-release@v2
        with:
          generate_release_notes: true
          prerelease: ${{ contains(github.ref_name, '-') }}
          files: |
            artifacts/*
            checksums.txt
```

---

### `lint.yml` — Structure

A separate workflow that runs on every push (not just tags) and on pull requests, to catch shell script issues before they reach users.

Steps:
1. `actions/checkout@v4`
2. Install shellcheck (available via `apt-get` on ubuntu-latest or via the `shellcheck` action)
3. `shellcheck install.sh` — full static analysis
4. `bash -n install.sh` — syntax-only check (fast secondary guard)

The `lint.yml` workflow triggers on `push` to all branches and `pull_request`. This ensures `install.sh` is always linted regardless of whether a release is being made.

#### Workflow skeleton

```yaml
name: Lint

on:
  push:
  pull_request:

jobs:
  shellcheck:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: shellcheck
        run: shellcheck install.sh
      - name: bash syntax check
        run: bash -n install.sh
```

---

## Implementation TODO

1. Create directory `.github/workflows/` if it does not already exist.
2. Create `.github/workflows/release.yml` following the skeleton above. Fill in the exact matrix values and step configuration as documented.
3. Verify that `src/index.ts` exists (provided by `section-01` of the `01-core-daemon` module) — this is the entry point for `bun build --compile`.
4. Create `.github/workflows/lint.yml` following the skeleton above.
5. Run `yamllint .github/workflows/release.yml` and `yamllint .github/workflows/lint.yml` locally to verify YAML syntax.
6. Run all assertions in `tests/validate-workflow.sh` and confirm they all pass.
7. Push a test tag `v0.0.1-test` to a fork or scratch repository to verify the end-to-end workflow triggers and both jobs succeed.

## Actual Implementation

**Files created:**
- `.github/workflows/release.yml`
- `.github/workflows/lint.yml`
- `tests/validate-workflow.sh` (11 structural assertions, all passing)
- `.yamllint.yml` (relaxed config: line-length 120, truthy allows `on`/`off`, document-start disabled)

**Deviation from plan — sha256sum command:**
Plan skeleton used `sha256sum artifacts/* > checksums.txt`. Changed to `cd artifacts && sha256sum * > ../checksums.txt` to produce bare filenames in the checksum file, not `artifacts/`-prefixed paths. Without this fix, `sha256sum -c checksums.txt` would fail for end users who download files to a flat directory.

**Tests: 11/11 pass** (yamllint on both files, shellcheck presence, matrix count, merge-multiple, upload/download @v4, no @v3, prerelease expression, bun pin, contents:write).

---

## Key Constraints and Gotchas

- **Bun must be pinned to `1.3.11`.** Do not update until the v1.3.12 regression fix is confirmed working. Track GitHub issue #29120 / PR #29272. When a fix ships, run the macOS arm64 smoke test (run the binary on a real ARM64 Mac, confirm it is not SIGKILL'd) before updating the pin.
- **`upload-artifact` and `download-artifact` must both be `@v4`.** Using `@v3` for one and `@v4` for the other silently breaks artifact exchange.
- **`merge-multiple: true`** is required in the download step so all three artifacts land in the same `artifacts/` directory as flat files, not subdirectories.
- **`prerelease` expression** must use `contains(github.ref_name, '-')`, not `contains(github.ref, '-')`. `github.ref` includes the `refs/tags/` prefix which would always evaluate to true.
- **`permissions: contents: write`** must be set at the workflow level (or job level). Without it, `GITHUB_TOKEN` cannot create releases or upload assets.
- All three builds run on `ubuntu-latest` using Bun's cross-compilation support. No macOS or ARM runners are needed.
- `-baseline` Bun target variants (e.g., `bun-darwin-arm64-baseline`) are not included in the initial release. Add them only if users report "Illegal instruction" errors on older hardware.
- Windows is excluded from the initial release. Adding it later requires one new matrix entry with `target: bun-windows-x64` and `outfile: jira-assistant-windows-x64.exe`.