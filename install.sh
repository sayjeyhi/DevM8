#!/usr/bin/env bash
set -euo pipefail

REPO="sayjeyhi/jira-assistant"
CONFIG_FILE="${HOME}/.config/jira-assistant/config.json"
TMP_DIR=""

detect_platform() {
  local os arch
  os=$(uname -s)
  arch=$(uname -m)

  case "$os" in
    Darwin)
      OS=macos
      case "$arch" in
        arm64)  ARCH=arm64 ;;
        x86_64) ARCH=x64 ;;
        *) echo "Unsupported macOS architecture: $arch" >&2; exit 1 ;;
      esac
      ;;
    Linux)
      if [[ "$arch" == "aarch64" ]]; then
        echo "Linux ARM64 is not yet supported. Only x64 binaries are available for Linux." >&2
        exit 1
      fi
      if ldd --version 2>&1 | grep -q musl || { [[ -f /etc/os-release ]] && grep -qi alpine /etc/os-release; }; then
        echo "Alpine/musl Linux is not supported. Bun binaries require glibc." >&2
        exit 1
      fi
      OS=linux
      ARCH=x64
      ;;
    *)
      echo "Unsupported OS: $os. Only macOS and Linux x64 are supported." >&2
      exit 1
      ;;
  esac
}

build_binary_name() {
  BINARY="jira-assistant-${OS}-${ARCH}"
}

resolve_version() {
  if [[ -n "${JIRA_ASSISTANT_VERSION:-}" ]]; then
    VERSION="$JIRA_ASSISTANT_VERSION"
  else
    local url
    url=$(curl -fsSL -o /dev/null -w "%{url_effective}" \
      "https://github.com/$REPO/releases/latest")
    VERSION="${url##*/}"
  fi
  echo "Resolved version: $VERSION"
}

stop_existing_service() {
  if [[ "$OS" == "macos" ]]; then
    launchctl unload "${HOME}/Library/LaunchAgents/com.jira-assistant.plist" 2>/dev/null || true
  else
    systemctl --user stop jira-assistant 2>/dev/null || true
  fi
}

select_install_dir() {
  if [[ -w /usr/local/bin ]]; then
    INSTALL_DIR=/usr/local/bin
  else
    INSTALL_DIR="${HOME}/.local/bin"
    mkdir -p "$INSTALL_DIR"
  fi
  if command -v jira-assistant &>/dev/null; then
    local prev_dir
    prev_dir=$(dirname "$(command -v jira-assistant)")
    if [[ "$prev_dir" != "$INSTALL_DIR" ]]; then
      echo "Warning: previous install found at $prev_dir — there may be a stale binary." >&2
    fi
  fi
}

ensure_path() {
  local dir="$1"
  local marker="# jira-assistant"
  local rc
  for rc in "${HOME}/.zshrc" "${HOME}/.bashrc" "${HOME}/.bash_profile" "${HOME}/.profile"; do
    if [[ -f "$rc" ]] && ! grep -qF "$marker" "$rc"; then
      printf '\n%s\nexport PATH="%s:$PATH"\n' "$marker" "$dir" >> "$rc"
    fi
  done
  export PATH="$dir:$PATH"
}

download_with_retry() {
  local url="$1" dest="$2"
  if ! curl -fsSL --fail-with-body -o "$dest" "$url"; then
    echo "Download failed, retrying..." >&2
    if ! curl -fsSL --fail-with-body -o "$dest" "$url"; then
      echo "Download failed after retry: $url" >&2
      exit 1
    fi
  fi
}

verify_checksum() {
  local binary="$1" checksums_file="$2"
  local name expected actual

  name=$(basename "$binary")
  expected=$(grep "  ${name}$" "$checksums_file" | awk '{print $1}')
  if [[ -z "$expected" ]]; then
    echo "Binary name '$name' not found in checksums.txt" >&2
    exit 1
  fi

  if [[ "$OS" == "macos" ]]; then
    actual=$(shasum -a 256 "$binary" | awk '{print $1}')
  else
    actual=$(sha256sum "$binary" | awk '{print $1}')
  fi

  if [[ "$actual" != "$expected" ]]; then
    echo "Checksum mismatch — download may be corrupted. Delete ${TMP_DIR:-<temp download directory>} and retry." >&2
    exit 1
  fi
}

install_binary() {
  local src="$1" dest="$2"
  mv "$src" "$dest"
  chmod +x "$dest"
}

strip_quarantine() {
  local path="$1"
  xattr -d com.apple.quarantine "$path" 2>/dev/null || true
}

run_config_if_needed() {
  if [[ -f "$CONFIG_FILE" ]]; then
    return 0
  fi
  if [[ ! -t 0 ]]; then
    echo "Run \`jira-assistant config\` to complete setup."
    return 0
  fi
  jira-assistant config
}

print_success() {
  echo ""
  echo "jira-assistant ${VERSION} installed successfully!"
  echo "  Binary:  $INSTALL_DIR/jira-assistant"
  echo "  Service: ${SERVICE_STATUS:-registered}"
  if [[ "${PATH_MODIFIED:-false}" == "true" ]]; then
    echo "  PATH: $INSTALL_DIR added — restart your shell or run: source ~/.zshrc"
  fi
}

register_macos_service() {
  local binary_path="$1"
  local plist_dir="$HOME/Library/LaunchAgents"
  local plist_path="$plist_dir/com.jira-assistant.plist"
  mkdir -p "$plist_dir"
  mkdir -p "$HOME/Library/Logs"
  launchctl unload "$plist_path" 2>/dev/null || true
  cat > "$plist_path" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.jira-assistant</string>
    <key>ProgramArguments</key>
    <array>
        <string>${binary_path}</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>Crashed</key>
        <true/>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>30</integer>
    <key>StandardOutPath</key>
    <string>${HOME}/Library/Logs/jira-assistant.log</string>
    <key>StandardErrorPath</key>
    <string>${HOME}/Library/Logs/jira-assistant.log</string>
</dict>
</plist>
EOF
  launchctl load "$plist_path"
}

register_linux_service() {
  local binary_path="$1"
  local unit_dir="$HOME/.config/systemd/user"
  local unit_path="$unit_dir/jira-assistant.service"
  mkdir -p "$unit_dir"
  cat > "$unit_path" <<EOF
[Unit]
Description=Jira Assistant Telegram Bot
After=network.target

[Service]
Type=simple
ExecStart=${binary_path} start
Restart=on-failure
RestartSec=5
StartLimitIntervalSec=300
StartLimitBurst=5

[Install]
WantedBy=default.target
EOF
  systemctl --user daemon-reload
  systemctl --user enable --now jira-assistant
  echo
  echo "Optional: to start jira-assistant at boot even when you are not logged in, run:"
  echo "  loginctl enable-linger $(id -un)"
  echo "Note: this may require sudo on some systems."
}

start_service() {
  if [[ "$OS" == "macos" ]]; then
    if launchctl list 2>/dev/null | grep -q "com.jira-assistant"; then
      echo "Service running (launchd)."
    else
      echo "Service not detected in launchd — check ~/Library/LaunchAgents/com.jira-assistant.plist"
    fi
  else
    if systemctl --user is-active jira-assistant &>/dev/null; then
      echo "Service running (systemd)."
    else
      echo "Service not detected — check: systemctl --user status jira-assistant"
    fi
  fi
}

do_uninstall() {
  : # implemented in section-06-install-uninstall
}

main() {
  PATH_MODIFIED=false

  for arg in "$@"; do
    case "$arg" in
      --uninstall) do_uninstall; exit 0 ;;
      --help)
        echo "Usage: install.sh [--uninstall] [--help]"
        echo "  JIRA_ASSISTANT_VERSION=vX.Y.Z  install specific version"
        exit 0
        ;;
    esac
  done

  TMP_DIR=$(mktemp -d)
  trap 'rm -rf "$TMP_DIR"' EXIT

  resolve_version
  detect_platform
  build_binary_name
  select_install_dir

  local RELEASE_URL="https://github.com/$REPO/releases/download/${VERSION}"

  if [[ "$INSTALL_DIR" == "${HOME}/.local/bin" ]]; then
    ensure_path "$INSTALL_DIR"
    PATH_MODIFIED=true
  fi

  stop_existing_service
  download_with_retry "$RELEASE_URL/$BINARY" "$TMP_DIR/$BINARY"
  download_with_retry "$RELEASE_URL/checksums.txt" "$TMP_DIR/checksums.txt"
  verify_checksum "$TMP_DIR/$BINARY" "$TMP_DIR/checksums.txt"
  install_binary "$TMP_DIR/$BINARY" "$INSTALL_DIR/jira-assistant"

  if [[ "$OS" == "macos" ]]; then
    strip_quarantine "$INSTALL_DIR/jira-assistant"
    register_macos_service "$INSTALL_DIR/jira-assistant"
  else
    register_linux_service "$INSTALL_DIR/jira-assistant"
  fi

  run_config_if_needed
  start_service
  print_success
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  main "$@"
fi
