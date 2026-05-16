# Code Review Interview: section-05-install-services

## Auto-fixes applied
- `echo ""` → bare `echo` (idiomatic)
- Added `mkdir -p "$HOME/Library/Logs"` before plist write (log dir may not exist on fresh macOS)
- `${USER}` → `$(id -un)` for portability

## User decisions
- **Add daemon-reload test**: YES — added `register_linux_service: systemctl daemon-reload called before enable`
- **Add start_service tests**: YES — added 4 tests for macOS/Linux running/not-detected paths

## Test improvements applied
- KeepAlive: adjacency check `grep -A1 '<key>KeepAlive</key>' | grep -q '<dict>'`
- RunAtLoad: adjacency check `grep -A1 '<key>RunAtLoad</key>' | grep -q '<true/>'`
- daemon-reload: added with ordering assertion (reload_line < enable_line)
- start_service: 4 tests covering all branches
