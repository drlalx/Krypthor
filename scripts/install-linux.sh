#!/usr/bin/env bash
# ==============================================================================
# Krypthor Vault - Linux Installation Script (Syncthing-Style)
#
# Features:
# - Builds optimized release binary (with FUSE auto-detection)
# - Installs binary to ~/.local/bin/krypthor
# - Installs application vector & raster icons
# - Creates Desktop application starters (GUI & Web Dashboard)
# - Configures & starts systemd user daemon (auto-start on login)
# - Supports clean uninstall (--uninstall)
# ==============================================================================

set -euo pipefail

# ANSI color codes
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

PREFIX="${HOME}/.local"
BIN_DIR="${PREFIX}/bin"
ICON_DIR_SVG="${PREFIX}/share/icons/hicolor/scalable/apps"
ICON_DIR_PNG="${PREFIX}/share/icons/hicolor/128x128/apps"
APPS_DIR="${PREFIX}/share/applications"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
SERVICE_NAME="krypthor.service"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

print_info() { echo -e "${BLUE}==>${NC} $1"; }
print_success() { echo -e "${GREEN}==>${NC} $1"; }
print_warn() { echo -e "${YELLOW}==>${NC} $1"; }
print_error() { echo -e "${RED}==>${NC} $1"; }

# ------------------------------------------------------------------------------
# Uninstall Mode
# ------------------------------------------------------------------------------
uninstall() {
    print_info "Uninstalling Krypthor Vault (Linux)..."

    # Stop and disable systemd daemon
    if systemctl --user is-active --quiet "${SERVICE_NAME}" 2>/dev/null; then
        print_info "Stopping systemd service ${SERVICE_NAME}..."
        systemctl --user stop "${SERVICE_NAME}" || true
    fi

    if systemctl --user is-enabled --quiet "${SERVICE_NAME}" 2>/dev/null; then
        print_info "Disabling systemd service ${SERVICE_NAME}..."
        systemctl --user disable "${SERVICE_NAME}" || true
    fi

    # Remove files
    rm -f "${SYSTEMD_USER_DIR}/${SERVICE_NAME}"
    systemctl --user daemon-reload || true

    rm -f "${BIN_DIR}/krypthor"
    rm -f "${ICON_DIR_SVG}/krypthor.svg"
    rm -f "${ICON_DIR_PNG}/krypthor.png"
    rm -f "${APPS_DIR}/krypthor.desktop"
    rm -f "${APPS_DIR}/krypthor-web.desktop"

    # Update desktop database
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "${APPS_DIR}" 2>/dev/null || true
    fi

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
    print_error "Rust & Cargo are required to build Krypthor. Please install via https://rustup.rs"
    exit 1
fi

BUILD_FEATURES=""
if command -v fusermount3 >/dev/null 2>&1 || command -v fusermount >/dev/null 2>&1; then
    print_success "FUSE subsystem detected. Enabling optional 'fuse' feature for virtual FS mounts."
    BUILD_FEATURES="--features fuse"
else
    print_warn "FUSE (fusermount3) not found. Building with portable WebDAV-only mount backend."
fi

# ------------------------------------------------------------------------------
# Build Release Binary
# ------------------------------------------------------------------------------
print_info "Building Krypthor Vault in release mode..."
cd "${REPO_ROOT}"

# shellcheck disable=SC2086
cargo build --release ${BUILD_FEATURES}

TARGET_BIN="${REPO_ROOT}/target/release/krypthor"
if [[ ! -f "${TARGET_BIN}" ]]; then
    print_error "Build failed: ${TARGET_BIN} does not exist."
    exit 1
fi

# ------------------------------------------------------------------------------
# Install Binary & Assets
# ------------------------------------------------------------------------------
print_info "Installing binary and icons to ${PREFIX}..."
mkdir -p "${BIN_DIR}"
install -m 755 "${TARGET_BIN}" "${BIN_DIR}/krypthor"

# Install Icons
mkdir -p "${ICON_DIR_SVG}" "${ICON_DIR_PNG}"
if [[ -f "${REPO_ROOT}/resources/images/krypthor.svg" ]]; then
    install -m 644 "${REPO_ROOT}/resources/images/krypthor.svg" "${ICON_DIR_SVG}/krypthor.svg"
fi
if [[ -f "${REPO_ROOT}/resources/images/krypthor.png" ]]; then
    install -m 644 "${REPO_ROOT}/resources/images/krypthor.png" "${ICON_DIR_PNG}/krypthor.png"
fi

# Ensure ~/.local/bin is in PATH
if [[ ":$PATH:" != *":${BIN_DIR}:"* ]]; then
    print_warn "${BIN_DIR} is not in your current PATH. Adding hint to shell profile."
    PROFILE_FILE="${HOME}/.bashrc"
    if [[ -f "${HOME}/.zshrc" ]]; then
        PROFILE_FILE="${HOME}/.zshrc"
    fi
    if ! grep -q 'export PATH="$HOME/.local/bin:$PATH"' "${PROFILE_FILE}" 2>/dev/null; then
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "${PROFILE_FILE}"
        print_info "Appended PATH export to ${PROFILE_FILE}."
    fi
fi

# ------------------------------------------------------------------------------
# Desktop Integration
# ------------------------------------------------------------------------------
print_info "Creating Desktop application starters..."
mkdir -p "${APPS_DIR}"

cat <<EOF > "${APPS_DIR}/krypthor.desktop"
[Desktop Entry]
Version=1.0
Type=Application
Name=Krypthor Vault
GenericName=Encrypted Zero-Knowledge Vault
Comment=Modern zero-knowledge encrypted vaults in pure Rust
Exec=${BIN_DIR}/krypthor
Icon=krypthor
Terminal=false
Categories=Utility;Security;FileManager;
StartupWMClass=krypthor
Keywords=encryption;vault;security;cryptomator;privacy;
EOF
chmod 644 "${APPS_DIR}/krypthor.desktop"

cat <<EOF > "${APPS_DIR}/krypthor-web.desktop"
[Desktop Entry]
Version=1.0
Type=Application
Name=Krypthor Web Dashboard
GenericName=Encrypted Vault Web Interface
Comment=Open the local Krypthor Vault Syncthing-style web interface
Exec=xdg-open http://127.0.0.1:8080
Icon=krypthor
Terminal=false
Categories=Utility;Security;Network;
EOF
chmod 644 "${APPS_DIR}/krypthor-web.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${APPS_DIR}" 2>/dev/null || true
fi

# ------------------------------------------------------------------------------
# Syncthing-Style Background Daemon (systemd --user)
# ------------------------------------------------------------------------------
print_info "Configuring Syncthing-style systemd user daemon..."
mkdir -p "${SYSTEMD_USER_DIR}"

cat <<EOF > "${SYSTEMD_USER_DIR}/${SERVICE_NAME}"
[Unit]
Description=Krypthor Vault Background Daemon (Syncthing-style)
Documentation=https://github.com/adrichel/krypthor
After=network.target

[Service]
Type=simple
ExecStart=${BIN_DIR}/krypthor serve --host 127.0.0.1 --port 8080
Restart=on-failure
RestartSec=5s
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now "${SERVICE_NAME}"

print_success "Krypthor background service enabled and started."

# ------------------------------------------------------------------------------
# Verification & Summary
# ------------------------------------------------------------------------------
echo ""
echo -e "${GREEN}===================================================================${NC}"
echo -e "${GREEN}       Krypthor Vault installed successfully (Syncthing-Style)     ${NC}"
echo -e "${GREEN}===================================================================${NC}"
echo ""
echo -e "  • ${BLUE}Web Dashboard:${NC}   http://127.0.0.1:8080"
echo -e "  • ${BLUE}WebDAV Mount:${NC}    http://127.0.0.1:8080/webdav/<VaultName>/"
echo -e "  • ${BLUE}Binary:${NC}          ${BIN_DIR}/krypthor"
echo -e "  • ${BLUE}Desktop Menu:${NC}    Krypthor Vault & Krypthor Web Dashboard"
echo -e "  • ${BLUE}Service Status:${NC}  systemctl --user status ${SERVICE_NAME}"
echo -e "  • ${BLUE}Service Logs:${NC}    journalctl --user -u ${SERVICE_NAME} -f"
echo ""
echo -e "Commands:"
echo -e "  krypthor               # Start native desktop GUI"
echo -e "  krypthor tui           # Start terminal user interface"
echo -e "  krypthor --help        # View all CLI commands"
echo ""
echo -e "To uninstall anytime: ${YELLOW}${BASH_SOURCE[0]} --uninstall${NC}"
echo ""
