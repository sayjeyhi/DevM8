# Research: Bun Cross-Compilation, macOS Gatekeeper, GitHub Actions Releases, install.sh (2025)

## Topic 1: Bun Cross-Compilation Support

### Does it work from Linux CI?

**Yes** — Bun v1.1.5+ supports cross-compilation. `bun build --compile --target=bun-darwin-arm64` works from a Linux GitHub Actions runner.

### Supported targets

| Target | Platform |
|---|---|
| `bun-linux-x64` / `bun-linux-x64-baseline` | Linux x64 |
| `bun-linux-arm64` | Linux ARM64 |
| `bun-linux-x64-musl` / `bun-linux-arm64-musl` | Linux musl |
| `bun-darwin-x64` / `bun-darwin-x64-baseline` | macOS Intel |
| `bun-darwin-arm64` | macOS Apple Silicon |
| `bun-windows-x64` / `bun-windows-x64-baseline` | Windows x64 |
| `bun-windows-arm64` | Windows ARM64 |

### CRITICAL: v1.3.12 regression

**Bun v1.3.12 has a broken code signature bug** when cross-compiling to `bun-darwin-arm64` on Linux. The binary runs fine on Linux but is **immediately SIGKILL'd (exit 137) on macOS**. Root cause: `sig_size` miscalculation in `src/macho.zig` — SuperBlob header declares wrong length.

**Workaround options:**
1. **Pin to Bun v1.3.11** in CI — most reliable
2. Build with `BUN_NO_CODESIGN_MACHO_BINARY=1` then run `codesign --sign -` on a macOS machine
3. Wait for the fix (PR #29272) to be included in a release

**Decision: Pin to Bun v1.3.11 in the GitHub Actions workflow.**

Other limitations:
- Windows-specific metadata (custom icon, console flags) cannot be cross-compiled
- For older x64 hardware, use `-baseline` variant to avoid "Illegal instruction" errors

Sources: [Bun docs](https://bun.com/docs/bundler/executables) | [v1.1.5 release](https://bun.sh/blog/bun-v1.1.5) | [issue #29120](https://github.com/oven-sh/bun/issues/29120)

---

## Topic 2: macOS Gatekeeper & Unsigned Binary Distribution

### What users experience

- Binaries downloaded via **browser** get `com.apple.quarantine` attribute → Gatekeeper blocks on first run
- Binaries downloaded via **curl** do NOT get the quarantine attribute → Gatekeeper won't block them
- **install.sh using curl bypasses Gatekeeper entirely**

### macOS 15.1 Sequoia change (Nov 2024)

Apple removed the Control+click "Open" shortcut. Blocked binaries now require:
1. Try to open → gets blocked
2. System Settings → Privacy & Security → scroll to Security section → "Open Anyway"
3. Enter admin credentials

### ARM64 binaries must be signed

All native ARM64 binaries need at minimum an **ad-hoc signature**. Bun's `--compile` applies this automatically (when not affected by v1.3.12 regression). Ad-hoc signing is free (`codesign --sign -`) but provides no developer identity.

### Options matrix

| Approach | Cost | Practical for personal OSS? |
|---|---|---|
| Apple Developer ID + Notarization | $99/yr | No |
| Ad-hoc signing (`codesign --sign -`) | Free | Yes — applied by Bun automatically |
| `xattr -rd com.apple.quarantine` | Free | User runs this after manual download |
| curl in install.sh | Free | Best — quarantine never set |

### Recommendation

1. Distribute via `install.sh` using `curl` — avoids quarantine entirely
2. Ensure ad-hoc signed (Bun handles this; verify with pinned v1.3.11)
3. For manual downloads, document in README:
   ```bash
   xattr -d com.apple.quarantine /usr/local/bin/jira-assistant
   # or re-sign:
   codesign --force --deep --sign - /usr/local/bin/jira-assistant
   ```

Sources: [Hackaday — macOS 15.1 signing](https://hackaday.com/2024/11/01/apple-forces-the-signing-of-applications-in-macos-sequoia-15-1/) | [xattr blog](https://a4z.noexcept.dev/blog/2024/07/11/xattr-on-macos.html)

---

## Topic 3: GitHub Actions Release Workflow

### `softprops/action-gh-release` vs `gh release create`

| Aspect | `softprops/action-gh-release` | `gh release create` |
|---|---|---|
| Setup | One YAML step | `gh` CLI (pre-installed on GH runners) |
| File upload | Glob pattern | Space-separated files |
| Auto release notes | `generate_release_notes: true` | `--generate-notes` |
| Dependencies | Third-party (well-maintained) | None (official GitHub CLI) |

**Recommendation:** `softprops/action-gh-release@v2` for automation simplicity.

### Complete workflow pattern (matrix build from single Linux runner)

```yaml
name: Release
on:
  push:
    tags: ["v*.*.*"]

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
        with: { bun-version: "1.3.11" }   # pin away from v1.3.12 regression
      - run: bun install
      - run: |
          bun build src/index.ts --compile \
            --target=${{ matrix.target }} \
            --outfile=${{ matrix.outfile }}
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
          path: artifacts/
          merge-multiple: true
      - uses: softprops/action-gh-release@v2
        with:
          generate_release_notes: true
          files: artifacts/*
```

### Auto-generated release notes

`generate_release_notes: true` uses GitHub's built-in changelog (merged PRs + commits since last tag). Can be customized via `.github/release.yml`. Alternatively, `--notes-from-tag` uses the annotated git tag message.

### Permissions

Only `contents: write` needed. `GITHUB_TOKEN` is automatic — no additional secrets.

Sources: [softprops/action-gh-release](https://github.com/softprops/action-gh-release) | [Bun CI/CD docs](https://bun.com/docs/guides/runtime/cicd)

---

## Topic 4: install.sh Best Practices

### OS + arch detection

```bash
OS="$(uname -s)"   # Darwin | Linux
ARCH="$(uname -m)" # arm64 | aarch64 | x86_64

case "$OS" in
  Darwin) OS="macos" ;;
  Linux)  OS="linux" ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64) ARCH="x64"   ;;
  arm64|aarch64) ARCH="arm64" ;;
  *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
esac
```

Note: Linux reports Apple Silicon as `aarch64`, macOS as `arm64` — both must be handled.

### Fetching latest version — avoid GitHub API rate limits

Best approach: use `/releases/latest/download/FILENAME` URL redirect — **no API call, no rate limits**:

```bash
BASE_URL="https://github.com/OWNER/REPO/releases/latest/download"
BINARY="jira-assistant-${OS}-${ARCH}"
curl -fsSL "$BASE_URL/$BINARY" -o /tmp/jira-assistant
```

If API version lookup is needed, handle 403/rate limit with redirect fallback:
```bash
# Redirect trick to get version without API
VERSION=$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
  "https://github.com/OWNER/REPO/releases/latest" | grep -o 'v[^/]*$')
```

### Install directory strategy

```bash
if [ -w "/usr/local/bin" ] || [ "$(id -u)" -eq 0 ]; then
  INSTALL_DIR="/usr/local/bin"
else
  INSTALL_DIR="$HOME/.local/bin"
  mkdir -p "$INSTALL_DIR"
  ensure_path "$INSTALL_DIR"
fi
```

### PATH update for ~/.local/bin

```bash
ensure_path() {
  local dir="$1"
  for rc in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile"; do
    [ -f "$rc" ] && grep -qF "$dir" "$rc" && continue
    [ -f "$rc" ] && printf '\nexport PATH="%s:$PATH"\n' "$dir" >> "$rc"
  done
  export PATH="$dir:$PATH"
}
```

- Check before appending (idempotent)
- Export for current session so install works immediately
- Tell user to restart shell

### `--uninstall` flag — recommended

```bash
if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$HOME/.local/bin/jira-assistant" "/usr/local/bin/jira-assistant"
  echo "Removed. Clean up PATH entries in ~/.zshrc or ~/.bashrc manually."
  exit 0
fi
```

### Security best practices

1. `set -euo pipefail` at top — prevents partial execution on download failure
2. `trap 'rm -rf "$TMP_DIR"' EXIT` — clean temp files
3. Always use HTTPS (`curl -fsSL`)
4. Optional: publish `checksums.txt` in release and verify SHA256
5. Avoid `sudo` — prefer user-space installs

### Download retry

Spec requires one retry on failure:
```bash
download_binary() {
  local url="$1" out="$2"
  curl -fsSL "$url" -o "$out" || {
    echo "Download failed, retrying..." >&2
    curl -fsSL "$url" -o "$out"
  }
}
```

### Real-world references

- [Deno install.sh](https://github.com/denoland/deno_install/blob/master/install.sh) — OS detection, `set -e`, env var override
- [rustup](https://sh.rustup.rs) — PATH modification, user consent
- [Homebrew install.sh](https://github.com/Homebrew/install) — sudo handling, color output

Sources: [GitHub API rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api) | [GitHub rate limit changes May 2025](https://github.blog/changelog/2025-05-08-updated-rate-limits-for-unauthenticated-requests/) | [Deno install](https://github.com/denoland/deno_install)

---

## Key Decisions for Implementation

| Decision | Choice |
|---|---|
| Bun version in CI | Pin to **v1.3.11** (avoid v1.3.12 SIGKILL regression) |
| CI runner | Single `ubuntu-latest` with matrix (all 3 targets) |
| Release action | `softprops/action-gh-release@v2` |
| macOS Gatekeeper | curl in install.sh bypasses quarantine; ad-hoc signing via Bun |
| Version fetching in install.sh | `/releases/latest/download/FILENAME` redirect — no API needed |
| PATH management | `/usr/local/bin` first; `~/.local/bin` fallback with idempotent RC file update |
| Uninstall | `--uninstall` flag in install.sh |
| Script safety | `set -euo pipefail` + `trap` cleanup + HTTPS only |
