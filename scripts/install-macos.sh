#!/usr/bin/env bash
# ==============================================================================
# Krypthor Vault - macOS Installation Script (Syncthing-Style)
#
# Features:
# - Builds optimized release binary
# - Installs binary to ~/.local/bin/krypthor (or /usr/local/bin)
# - Configures & loads launchd agent (~/Library/LaunchAgents/com.krypthor.vault.plist)
# - Creates macOS Web Dashboard launcher in ~/Applications
# - Supports clean uninstall (--uninstall)
# ==============================================================================

set -euo pipefail

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

PREFIX="${HOME}/.local"
BIN_DIR="${PREFIX}/bin"
LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"
PLIST_FILE="${LAUNCH_AGENTS_DIR}/com.krypthor.vault.plist"
LOG_DIR="${HOME}/Library/Logs"
APPS_DIR="${HOME}/Applications"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

print_info() { echo -e "${BLUE}==>${NC} $1"; }
print_success() { echo -e "${GREEN}==>${NC} $1"; }
print_warn() { echo -e "${YELLOW}==>${NC} $1"; }
print_error() { echo -e "${RED}==>${NC} $1"; }

# ------------------------------------------------------------------------------
# Uninstall Mode
# ------------------------------------------------------------------------------
uninstall() {
    print_info "Uninstalling Krypthor Vault (macOS)..."

    # Unload launchd agent
    if [[ -f "${PLIST_FILE}" ]]; then
        print_info "Unloading launchd service com.krypthor.vault..."
        launchctl unload "${PLIST_FILE}" 2>/dev/null || true
        rm -f "${PLIST_FILE}"
    fi

    rm -f "${BIN_DIR}/krypthor"
    rm -rf "${APPS_DIR}/Krypthor Web Dashboard.app"

    print_success "Krypthor Vault uninstalled successfully."
    exit 0
}

if [[ "${1:-}" == "--uninstall" || "${1:-}" == "-u" ]]; then
    uninstall
fi

# ------------------------------------------------------------------------------
# Prerequisites Check
# ------------------------------------------------------------------------------
print_info "Checking prerequisites..."

if ! command -v cargo >/dev/null 2>&1; then
    print_error "Rust & Cargo are required. Install via https://rustup.rs"
    exit 1
fi

# ------------------------------------------------------------------------------
# Build Release Binary
# ------------------------------------------------------------------------------
print_info "Building Krypthor Vault in release mode..."
cd "${REPO_ROOT}"
cargo build --release

TARGET_BIN="${REPO_ROOT}/target/release/krypthor"
if [[ ! -f "${TARGET_BIN}" ]]; then
    print_error "Build failed: ${TARGET_BIN} does not exist."
    exit 1
fi

# ------------------------------------------------------------------------------
# Install Binary
# ------------------------------------------------------------------------------
print_info "Installing binary to ${BIN_DIR}..."
mkdir -p "${BIN_DIR}"
install -m 755 "${TARGET_BIN}" "${BIN_DIR}/krypthor"

# Ensure ~/.local/bin is in PATH for zsh (default macOS shell)
ZSHRC="${HOME}/.zshrc"
if [[ ":$PATH:" != *":${BIN_DIR}:"* ]]; then
    if ! grep -q 'export PATH="$HOME/.local/bin:$PATH"' "${ZSHRC}" 2>/dev/null; then
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "${ZSHRC}"
        print_info "Appended PATH export to ${ZSHRC}."
    fi
fi

# ------------------------------------------------------------------------------
# Syncthing-Style Background Daemon (launchd LaunchAgent)
# ------------------------------------------------------------------------------
print_info "Configuring Syncthing-style launchd agent..."
mkdir -p "${LAUNCH_AGENTS_DIR}" "${LOG_DIR}"

# Unload previous instance if running
if [[ -f "${PLIST_FILE}" ]]; then
    launchctl unload "${PLIST_FILE}" 2>/dev/null || true
fi

cat <<EOF > "${PLIST_FILE}"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.krypthor.vault</string>
    <key>ProgramArguments</key>
    <array>
        <string>${BIN_DIR}/krypthor</string>
        <string>serve</string>
        <string>--host</string>
        <string>127.0.0.1</string>
        <string>--port</string>
        <string>8080</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>${LOG_DIR}/krypthor.log</string>
    <key>StandardErrorPath</key>
    <string>${LOG_DIR}/krypthor-err.log</string>
    <key>ProcessType</key>
    <string>Background</string>
</dict>
</plist>
EOF

chmod 644 "${PLIST_FILE}"
launchctl load "${PLIST_FILE}"

print_success "launchd agent loaded and running in background."

# ------------------------------------------------------------------------------
# Web Dashboard App Shortcut for macOS
# ------------------------------------------------------------------------------
if command -v osacompile >/dev/null 2>&1; then
    mkdir -p "${APPS_DIR}"
    APP_PATH="${APPS_DIR}/Krypthor Web Dashboard.app"
    rm -rf "${APP_PATH}"
    osacompile -e 'open location "http://127.0.0.1:8080"' -o "${APP_PATH}" 2>/dev/null || true
    print_info "Created '${APP_PATH}'."
fi

# ------------------------------------------------------------------------------
# Summary
# ------------------------------------------------------------------------------
echo ""
echo -e "${GREEN}===================================================================${NC}"
echo -e "${GREEN}       Krypthor Vault installed successfully (Syncthing-Style)     ${NC}"
echo -e "${GREEN}===================================================================${NC}"
echo ""
echo -e "  • ${BLUE}Web Dashboard:${NC}   http://127.0.0.1:8080"
echo -e "  • ${BLUE}WebDAV Mount:${NC}    http://127.0.0.1:8080/webdav/<VaultName>/"
echo -e "  • ${BLUE}Binary:${NC}          ${BIN_DIR}/krypthor"
echo -e "  • ${BLUE}Daemon Status:${NC}   launchctl list | grep krypthor"
echo -e "  • ${BLUE}Daemon Logs:${NC}     tail -f ~/Library/Logs/krypthor.log"
echo ""
echo -e "Native Mount in macOS Finder:"
echo -e "  Drücke ⌘K im Finder und gib ein: ${BLUE}http://localhost:8080/webdav/<VaultName>/${NC}"
echo ""
echo -e "To uninstall anytime: ${YELLOW}${BASH_SOURCE[0]} --uninstall${NC}"
echo ""
