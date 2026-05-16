# Interview Transcript: 04-distribution

## Round 1

### Q1: GitHub repo owner/name
**Answer:** `sayjeyhi/jira-assistant`

### Q2: Windows binary — include or skip?
**Answer:** Skip. macOS (arm64 + x64) and Linux x64 only.

### Q3: Should install.sh register launchd service on macOS?
**Answer:** Yes. Register `~/Library/LaunchAgents/com.jira-assistant.plist` for auto-start on login.

### Q4: SHA256 checksum verification?
**Answer:** Yes. Publish `checksums.txt` in each release; install.sh verifies before installing.

---

## Round 2

### Q5: How does the first-time config wizard work?
**Answer:** Interactive prompts — the `jira-assistant config` subcommand asks for each required value (Jira URL, API token, Telegram bot token, allowed user IDs, etc.) and writes the config file.

### Q6: Linux service registration?
**Answer:** Yes. Register systemd --user service (`~/.config/systemd/user/jira-assistant.service`) and enable it with `systemctl --user enable --now`.

### Q7: Auto-restart on crash?
**Answer:** Yes. `KeepAlive true` in the macOS launchd plist; `Restart=on-failure` in the Linux systemd unit.

---

## Key Decisions Summary

| Decision | Choice |
|---|---|
| Repo | `sayjeyhi/jira-assistant` |
| Targets | macOS arm64, macOS x64, Linux x64 (no Windows) |
| Bun version in CI | v1.3.11 (avoid v1.3.12 regression) |
| CI runner | Single ubuntu-latest with 3-target matrix |
| Release action | `softprops/action-gh-release@v2` |
| install.sh version fetch | `/releases/latest/download/FILENAME` redirect (no API) |
| Checksum verification | Yes — `checksums.txt` published + verified |
| macOS service | launchd plist, KeepAlive=true |
| Linux service | systemd --user, Restart=on-failure |
| Config wizard | Interactive prompts via `jira-assistant config` |
| PATH fallback | `~/.local/bin`, idempotent RC file update |
| Uninstall | `--uninstall` flag in install.sh |
