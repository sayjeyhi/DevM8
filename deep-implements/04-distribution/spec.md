# Spec: 04-distribution

## What This Is

Build pipeline, GitHub Releases, and the one-line bash install script that lets users install the daemon with a single `curl | bash` command.

---

## Requirements Source

See `../requirements.md` for full project overview.  
Depends on: `../01-core-daemon/spec.md` (binary name, entry point)

---

## Scope

### Binary Compilation

Tool: `bun build --compile`

Targets:
| Platform | Arch | Binary name |
|---|---|---|
| macOS | arm64 (Apple Silicon) | `jira-assistant-macos-arm64` |
| macOS | x64 (Intel) | `jira-assistant-macos-x64` |
| Linux | x64 | `jira-assistant-linux-x64` |
| Windows | x64 | `jira-assistant-windows-x64.exe` (optional/stretch) |

Build command per target:
```bash
bun build --compile --target=bun-darwin-arm64 src/index.ts --outfile dist/jira-assistant-macos-arm64
bun build --compile --target=bun-darwin-x64   src/index.ts --outfile dist/jira-assistant-macos-x64
bun build --compile --target=bun-linux-x64    src/index.ts --outfile dist/jira-assistant-linux-x64
```

Verify Bun cross-compilation support for target flags during planning.

---

### GitHub Actions Workflow

File: `.github/workflows/release.yml`

Trigger: push of tag matching `v*.*.*`

Steps:
1. Checkout repo
2. Setup Bun
3. Install dependencies (`bun install`)
4. Build all targets
5. Create GitHub Release (using `gh` CLI or `softprops/action-gh-release`)
6. Upload all binaries as release assets

```yaml
on:
  push:
    tags:
      - 'v*.*.*'
```

Release notes: auto-generate from tag annotation or commits since last tag.

---

### install.sh

One-liner for users:
```bash
curl -fsSL https://raw.githubusercontent.com/{owner}/{repo}/main/install.sh | bash
```

Script logic:
1. Detect OS: `uname -s` (Darwin / Linux)
2. Detect arch: `uname -m` (arm64 / x86_64)
3. Fetch latest release tag from GitHub API: `https://api.github.com/repos/{owner}/{repo}/releases/latest`
4. Download correct binary from release assets
5. Install binary to `/usr/local/bin/jira-assistant` (or `~/.local/bin/` if no sudo)
6. Make executable: `chmod +x`
7. Run first-time wizard: `jira-assistant config` (if config file doesn't exist)
8. Start daemon: `jira-assistant start`
9. Print success message with next steps

Error handling:
- Unsupported OS/arch: print message and exit 1
- Download failure: retry once, then exit with error
- No write permission to `/usr/local/bin`: fall back to `~/.local/bin` and suggest adding to PATH

---

### README

Sections:
- One-liner install command (prominent, at the top)
- Requirements (macOS, claude CLI installed)
- Available Telegram commands
- Config file location and format
- Manual build instructions (for contributors)
- Uninstall instructions

---

## Key Decisions (from interview)

- **macOS primary:** arm64 + x64 binaries required; Linux x64 for completeness
- **GitHub Releases:** binaries attached as assets, not a package registry
- **install.sh auto-configures:** runs wizard + registers launchd — user should have working bot after one command
- **No package managers:** no brew formula, no npm package — direct binary download

---

## Depends On (from 01-core-daemon)

- Binary entry point: `src/index.ts`
- Binary name: `jira-assistant`
- `jira-assistant config` and `jira-assistant start` subcommands must exist before install.sh can invoke them

---

## Uncertainties to Resolve in Planning

- Bun cross-compilation: does `bun build --compile --target=bun-darwin-arm64` work from Linux CI runner? Verify.
- GitHub API rate limits for unauthenticated `releases/latest` requests — may need to handle 403
- Binary signing on macOS: Gatekeeper may block unsigned binaries — research ad-hoc signing or user instructions to bypass (`xattr -rd com.apple.quarantine`)
- PATH update in install.sh: should script modify `~/.zshrc` / `~/.bashrc` if `~/.local/bin` not in PATH?
- Uninstall: should install.sh include an `--uninstall` flag?
