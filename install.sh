#!/usr/bin/env bash
set -euo pipefail

REPO="sayjeyhi/DevM8"
CONFIG_FILE="${HOME}/.config/devm8/config.json"
TMP_DIR=""
CLIENT_MODE="${CLIENT_MODE:-false}"
TARGET_NAME="${TARGET_NAME:-devm8}"

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
  if [[ "$CLIENT_MODE" == "true" ]]; then
    BINARY="devm8-client-${OS}-${ARCH}"
  else
    BINARY="devm8-${OS}-${ARCH}"
  fi
}

resolve_version() {
  if [[ -n "${DEVM8_VERSION:-}" ]]; then
    VERSION="$DEVM8_VERSION"
  else
    local url
    url=$(curl -fsSL -o /dev/null -w "%{url_effective}" \
      "https://github.com/$REPO/releases/latest")
    VERSION="${url##*/}"
  fi
  echo "Resolved version: $VERSION"
}

is_root() {
  [[ "$EUID" -eq 0 ]]
}

stop_existing_service() {
  if [[ "$OS" == "macos" ]]; then
    launchctl unload "${HOME}/Library/LaunchAgents/com.devm8.plist" 2>/dev/null || true
  elif is_root; then
    systemctl stop devm8 2>/dev/null || true
  else
    systemctl --user stop devm8 2>/dev/null || true
  fi
}

select_install_dir() {
  if [[ -w /usr/local/bin ]]; then
    INSTALL_DIR=/usr/local/bin
  else
    INSTALL_DIR="${HOME}/.local/bin"
    mkdir -p "$INSTALL_DIR"
  fi
  if command -v "$TARGET_NAME" &>/dev/null; then
    local prev_dir
    prev_dir=$(dirname "$(command -v "$TARGET_NAME")")
    if [[ "$prev_dir" != "$INSTALL_DIR" ]]; then
      echo "Warning: previous install found at $prev_dir — there may be a stale binary." >&2
    fi
  fi
}

ensure_path() {
  local dir="$1"
  local marker="# devm8"
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
  if ! curl -fsSL -o "$dest" "$url"; then
    echo "Download failed, retrying..." >&2
    if ! curl -fsSL -o "$dest" "$url"; then
      echo "Download failed after retry: $url" >&2
      exit 1
    fi
  fi
}

verify_checksum() {
  local binary="$1" checksums_file="$2"
  local name expected actual

  name=$(basename "$binary")
  expected=$(grep "  ${name}$" "$checksums_file" | awk '{print $1}' || true)
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
    echo "Run \`devm8 config\` to complete setup."
    return 0
  fi
  devm8 config
}

print_success() {
  echo ""
  echo "${TARGET_NAME} ${VERSION} installed successfully!"
  echo "  Binary:  $INSTALL_DIR/${TARGET_NAME}"
  if [[ "$CLIENT_MODE" != "true" ]]; then
    echo "  Service: ${SERVICE_STATUS:-registered}"
  fi
  if [[ "${PATH_MODIFIED:-false}" == "true" ]]; then
    echo "  PATH: $INSTALL_DIR added — restart your shell or run: source ~/.zshrc"
  fi
  if [[ "$CLIENT_MODE" == "true" ]]; then
    echo "  Run: devm8-client login --server <your-server-url> --code <pairing-code>"
  fi
}

register_macos_service() {
  local binary_path="$1"
  local plist_dir="$HOME/Library/LaunchAgents"
  local plist_path="$plist_dir/com.devm8.plist"
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
    <string>com.devm8</string>
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
    <string>${HOME}/Library/Logs/devm8.log</string>
    <key>StandardErrorPath</key>
    <string>${HOME}/Library/Logs/devm8.log</string>
</dict>
</plist>
EOF
  launchctl load "$plist_path"
}

install_bubblewrap() {
  if command -v bwrap &>/dev/null; then
    return 0
  fi
  echo "Installing bubblewrap for Claude process isolation..."
  local install_cmd
  if command -v apt-get &>/dev/null; then
    install_cmd="apt-get install -y bubblewrap"
  elif command -v dnf &>/dev/null; then
    install_cmd="dnf install -y bubblewrap"
  elif command -v yum &>/dev/null; then
    install_cmd="yum install -y bubblewrap"
  elif command -v pacman &>/dev/null; then
    install_cmd="pacman -S --noconfirm bubblewrap"
  elif command -v zypper &>/dev/null; then
    install_cmd="zypper install -y bubblewrap"
  else
    echo "Warning: unknown package manager — install bubblewrap manually for Claude isolation." >&2
    return 0
  fi
  if is_root; then
    $install_cmd
  elif command -v sudo &>/dev/null; then
    sudo $install_cmd
  else
    echo "Warning: need root to install bubblewrap. Run manually: $install_cmd" >&2
  fi
}

register_linux_service() {
  local binary_path="$1"

  if is_root; then
    local unit_path="/etc/systemd/system/devm8.service"
    cat > "$unit_path" <<EOF
[Unit]
Description=DevM8 Telegram Bot
After=network.target

[Service]
Type=simple
ExecStart=${binary_path} start
Restart=on-failure
RestartSec=5
StartLimitIntervalSec=300
StartLimitBurst=5

[Install]
WantedBy=multi-user.target
EOF
    systemctl daemon-reload
    systemctl enable --now devm8
  else
    local unit_dir="$HOME/.config/systemd/user"
    local unit_path="$unit_dir/devm8.service"
    mkdir -p "$unit_dir"
    cat > "$unit_path" <<EOF
[Unit]
Description=DevM8 Telegram Bot
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
    if ! systemctl --user daemon-reload 2>/dev/null; then
      echo "Warning: systemd user bus unavailable. Unit written to $unit_path." >&2
      echo "Enable manually after logging in as a regular user:" >&2
      echo "  systemctl --user daemon-reload && systemctl --user enable --now devm8" >&2
      SERVICE_STATUS="unit written (not started — no user bus)"
      return
    fi
    systemctl --user enable --now devm8
    echo
    echo "Optional: to start devm8 at boot even when not logged in, run:"
    echo "  loginctl enable-linger $(id -un)"
    echo "Note: this may require sudo on some systems."
  fi
}

start_service() {
  if [[ "$OS" == "macos" ]]; then
    if launchctl list 2>/dev/null | grep -q "com.devm8"; then
      echo "Service running (launchd)."
    else
      echo "Service not detected in launchd — check ~/Library/LaunchAgents/com.devm8.plist"
    fi
  elif is_root; then
    if systemctl is-active devm8 &>/dev/null; then
      echo "Service running (systemd system)."
    else
      echo "Service not detected — check: systemctl status devm8"
    fi
  else
    if systemctl --user is-active devm8 &>/dev/null; then
      echo "Service running (systemd user)."
    else
      echo "Service not detected — check: systemctl --user status devm8"
    fi
  fi
}

do_uninstall() {
  detect_platform
  if [[ "$CLIENT_MODE" == "true" ]]; then
    rm -f /usr/local/bin/devm8-client
    rm -f "${HOME}/.local/bin/devm8-client"
    echo "devm8-client removed. Credentials at ~/.config/devm8-client/ were left in place."
    exit 0
  fi
  stop_existing_service
  if [[ "$OS" == "macos" ]]; then
    rm -f "${HOME}/Library/LaunchAgents/com.devm8.plist"
  elif is_root; then
    systemctl disable devm8 2>/dev/null || true
    rm -f "/etc/systemd/system/devm8.service"
  else
    systemctl --user disable devm8 2>/dev/null || true
    rm -f "${HOME}/.config/systemd/user/devm8.service"
  fi
  rm -f /usr/local/bin/devm8
  rm -f "${HOME}/.local/bin/devm8"
  echo "Config files at ~/.config/devm8/ were left in place. Remove manually if desired."
  echo "PATH entries added to shell RC files must be cleaned up manually."
  exit 0
}

main() {
  PATH_MODIFIED=false
  CLIENT_MODE=false
  DO_UNINSTALL=false

  for arg in "$@"; do
    case "$arg" in
      --client) CLIENT_MODE=true ;;
      --uninstall) DO_UNINSTALL=true ;;
      --help)
        echo "Usage: install.sh [--client] [--uninstall] [--help]"
        echo "  --client                 install the devm8-client terminal client instead of the devm8 server/daemon"
        echo "  DEVM8_VERSION=vX.Y.Z     install a specific version"
        exit 0
        ;;
    esac
  done

  TARGET_NAME="devm8"
  if [[ "$CLIENT_MODE" == "true" ]]; then
    TARGET_NAME="devm8-client"
  fi

  if [[ "$DO_UNINSTALL" == "true" ]]; then
    do_uninstall
    exit 0
  fi

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

  if [[ "$CLIENT_MODE" != "true" ]]; then
    stop_existing_service
  fi
  download_with_retry "$RELEASE_URL/$BINARY" "$TMP_DIR/$BINARY"
  download_with_retry "$RELEASE_URL/checksums.txt" "$TMP_DIR/checksums.txt"
  verify_checksum "$TMP_DIR/$BINARY" "$TMP_DIR/checksums.txt"
  install_binary "$TMP_DIR/$BINARY" "$INSTALL_DIR/$TARGET_NAME"

  if [[ "$CLIENT_MODE" == "true" ]]; then
    if [[ "$OS" == "macos" ]]; then
      strip_quarantine "$INSTALL_DIR/$TARGET_NAME"
    fi
    print_success
    return 0
  fi

  if [[ "$OS" == "macos" ]]; then
    strip_quarantine "$INSTALL_DIR/devm8"
    register_macos_service "$INSTALL_DIR/devm8"
  else
    install_bubblewrap
    register_linux_service "$INSTALL_DIR/devm8"
  fi

  run_config_if_needed
  start_service
  print_success
}

if [[ "${BASH_SOURCE[0]:-$0}" == "${0}" ]]; then
  main "$@"
fi
