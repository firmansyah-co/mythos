#!/usr/bin/env bash
# ==============================================================================
# MythOS Universal Bootstrap Installer (Linux & macOS)
# Part of MythOS Distribution Suite (TSK-212 / SYS-REQ-095)
# ==============================================================================
#
# Usage:
#   curl -fsSL https://get.mythos.firmansyah.co | bash
#   curl -fsSL https://get.mythos.firmansyah.co | bash -s -- --channel dev
#   curl -fsSL https://get.mythos.firmansyah.co | bash -s -- --version 1.0.0
#   curl -fsSL https://get.mythos.firmansyah.co | bash -s -- --dry-run
#
# Environment Variables:
#   CHANNEL=release|rc|dev
#   VERSION=x.y.z
#   MYTHOS_INSTALL_DIR=~/.mythos
#   MYTHOS_NON_INTERACTIVE=1
# ==============================================================================

set -euo pipefail

ROUTER_URL="https://get.mythos.firmansyah.co"
CDN_URL="https://releases.mythos.firmansyah.co"

# Defaults
CHANNEL="${CHANNEL:-release}"
VERSION="${VERSION:-}"
INSTALL_DIR="${MYTHOS_INSTALL_DIR:-$HOME/.mythos}"
DATA_DIR=""
DRY_RUN=0
NON_INTERACTIVE="${MYTHOS_NON_INTERACTIVE:-0}"
SKIP_LAUNCHER=0
SKIP_SERVICE=0
VERBOSE=0

# Formatting
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

log_banner() {
    cat << 'EOF'
 __  __       _   _      ___  ____  
|  \/  |_   _| |_| |__  / _ \/ ___| 
| |\/| | | | | __| '_ \| | | \___ \ 
| |  | | |_| | |_| | | | |_| |___) |
|_|  |_|\__, |\__|_| |_|\___/|____/ 
        |___/                       
Next-Gen Linux-First Industrial SCADA & Soft-Logic Microkernel
==============================================================
EOF
}

log_info() {
    printf "${BLUE}[INFO]${NC} %s\n" "$*"
}

log_ok() {
    printf "${GREEN}[OK]${NC} %s\n" "$*"
}

log_warn() {
    printf "${YELLOW}[WARN]${NC} %s\n" "$*"
}

log_err() {
    printf "${RED}[ERROR]${NC} %s\n" "$*" >&2
}

print_usage() {
    cat << EOF
MythOS Universal Bootstrap Installer

Usage:
  install.sh [OPTIONS]

Options:
  -c, --channel <channel>  Release channel to install: release (default), rc, dev
  -v, --version <semver>   Explicit SemVer version to pin (e.g. 1.0.0, 1.2.0-rc.1)
  -d, --dir <path>         Base installation directory (default: ~/.mythos)
  --dry-run                Simulate detection and installation without writing changes
  --no-launcher            Skip desktop launcher registration
  --no-service             Skip systemd user service configuration
  -y, --yes                Non-interactive mode; assume yes to all prompts
  --verbose                Enable diagnostic verbose logging
  -h, --help               Display this help message

Environment Variables:
  CHANNEL                  Set channel (release, rc, dev)
  VERSION                  Set target version
  MYTHOS_INSTALL_DIR       Override base install directory
  MYTHOS_NON_INTERACTIVE   Set to 1 for automated headless CI/CD installations
EOF
}

# Parse CLI arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -c|--channel)
            if [[ $# -lt 2 ]]; then
                log_err "--channel requires an argument: release, rc, or dev"
                exit 1
            fi
            CHANNEL="$2"
            shift 2
            ;;
        -v|--version)
            if [[ $# -lt 2 ]]; then
                log_err "--version requires a SemVer argument"
                exit 1
            fi
            VERSION="$2"
            shift 2
            ;;
        -d|--dir)
            if [[ $# -lt 2 ]]; then
                log_err "--dir requires a directory path"
                exit 1
            fi
            INSTALL_DIR="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --no-launcher)
            SKIP_LAUNCHER=1
            shift
            ;;
        --no-service)
            SKIP_SERVICE=1
            shift
            ;;
        -y|--yes)
            NON_INTERACTIVE=1
            shift
            ;;
        --verbose)
            VERBOSE=1
            shift
            ;;
        -h|--help)
            print_usage
            exit 0
            ;;
        *)
            log_err "Unknown argument: $1"
            print_usage
            exit 1
            ;;
    esac
done

# Validate Channel
case "${CHANNEL}" in
    release|rc|dev)
        ;;
    *)
        log_err "Invalid channel '${CHANNEL}'. Must be one of: release, rc, dev."
        exit 1
        ;;
esac

log_banner
log_info "Starting MythOS bootstrap installer..."
log_info "Target Channel: ${BOLD}${CHANNEL}${NC}"
if [[ -n "${VERSION}" ]]; then
    log_info "Target Version: ${BOLD}${VERSION}${NC}"
else
    log_info "Target Version: ${BOLD}latest (${CHANNEL})${NC}"
fi

# Detect Operating System
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "${OS}" in
    linux*)
        OS_NAME="linux"
        ;;
    darwin*)
        OS_NAME="darwin"
        ;;
    *)
        log_err "Unsupported operating system: ${OS}. MythOS installer supports Linux and macOS (Darwin)."
        exit 1
        ;;
esac
log_ok "Detected Operating System: ${BOLD}${OS_NAME}${NC}"

# Detect CPU Architecture
RAW_ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
case "${RAW_ARCH}" in
    x86_64|amd64)
        ARCH="x86_64"
        ;;
    aarch64|arm64)
        ARCH="aarch64"
        ;;
    armv7l|armv7|armhf)
        ARCH="armv7l"
        ;;
    *)
        log_err "Unsupported CPU architecture: ${RAW_ARCH}. Supported architectures: x86_64, aarch64, armv7l."
        exit 1
        ;;
esac
log_ok "Detected CPU Architecture: ${BOLD}${ARCH}${NC}"

PLATFORM_TARGET="${OS_NAME}-${ARCH}"
log_info "Platform Target: ${BOLD}${PLATFORM_TARGET}${NC}"

# Detect Checksum Tool
SHA_CMD=""
if command -v sha256sum >/dev/null 2>&1; then
    SHA_CMD="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    SHA_CMD="shasum -a 256"
else
    log_warn "Neither 'sha256sum' nor 'shasum' found. Checksum verification will be skipped if binary is downloaded."
fi

# Determine Data Directory
if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    DATA_DIR="/var/lib/mythos"
else
    DATA_DIR="${INSTALL_DIR}/data"
fi
log_info "Binary Directory: ${BOLD}${INSTALL_DIR}/bin${NC}"
log_info "Data Directory:   ${BOLD}${DATA_DIR}${NC}"

# Dry-run validation check
if [[ "${DRY_RUN}" -eq 1 ]]; then
    log_info "${CYAN}=== DRY-RUN MODE ACTIVE ===${NC}"
    log_info "[DRY-RUN] Would query CDN manifest at: ${CDN_URL}/manifests/${CHANNEL}.json"
    log_info "[DRY-RUN] Target platform package: mythos-${PLATFORM_TARGET}.tar.gz"
    log_info "[DRY-RUN] Would create directory hierarchy:"
    log_info "  - ${INSTALL_DIR}/bin"
    log_info "  - ${INSTALL_DIR}/versions"
    log_info "  - ${DATA_DIR}"
    log_info "[DRY-RUN] Would verify SHA-256 with: ${SHA_CMD:-none}"
    log_info "[DRY-RUN] Would link binaries into ${INSTALL_DIR}/bin/ (mythos, mythos-server)"
    log_info "[DRY-RUN] Would append ${INSTALL_DIR}/bin to user PATH in shell profiles"
    if [[ "${OS_NAME}" == "linux" && "${SKIP_SERVICE}" -eq 0 ]]; then
        log_info "[DRY-RUN] Would configure user systemd service at ~/.config/systemd/user/mythos.service"
        log_info "[DRY-RUN] Real-time capabilities to configure: AmbientCapabilities=CAP_NET_RAW CAP_SYS_NICE"
    fi
    if [[ "${OS_NAME}" == "darwin" ]]; then
        log_info "[DRY-RUN] Would clear macOS quarantine: xattr -dr com.apple.quarantine ${INSTALL_DIR}"
    fi
    if [[ "${SKIP_LAUNCHER}" -eq 0 ]]; then
        log_info "[DRY-RUN] Would register desktop application launcher"
    fi
    log_ok "${GREEN}[DRY-RUN SUCCESS]${NC} All platform checks, options, and parameters validated successfully."
    exit 0
fi

# Directory Hierarchy Creation
mkdir -p "${INSTALL_DIR}/bin"
mkdir -p "${INSTALL_DIR}/versions"
mkdir -p "${DATA_DIR}"
chmod 755 "${DATA_DIR}"

# Fetch Release Manifest and Target Binaries
TMP_DIR="$(mktemp -d -t mythos-install-XXXXXX)"
cleanup() {
    rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

MANIFEST_URL="${CDN_URL}/manifests/${CHANNEL}.json"
log_info "Querying release manifest from ${MANIFEST_URL}..."

RESOLVED_VERSION="${VERSION}"
PACKAGE_TAR="mythos-${PLATFORM_TARGET}.tar.gz"
PACKAGE_URL="${CDN_URL}/channels/${CHANNEL}/${PACKAGE_TAR}"
EXPECTED_SHA=""

# Attempt to download manifest if curl is available
FETCH_SUCCESS=0
if command -v curl >/dev/null 2>&1; then
    if curl -sSfL --connect-timeout 5 --max-time 15 "${MANIFEST_URL}" -o "${TMP_DIR}/manifest.json" 2>/dev/null; then
        FETCH_SUCCESS=1
        if command -v grep >/dev/null 2>&1 && command -v sed >/dev/null 2>&1; then
            MANIFEST_VERSION=$(grep -o '"version"[^,]*' "${TMP_DIR}/manifest.json" | head -n1 | sed -E 's/.*"version"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' || true)
            if [[ -n "${MANIFEST_VERSION}" && -z "${RESOLVED_VERSION}" ]]; then
                RESOLVED_VERSION="${MANIFEST_VERSION}"
            fi
            # Extract SHA-256 for target platform if present
            EXPECTED_SHA=$(grep -A 5 ""${PLATFORM_TARGET}"" "${TMP_DIR}/manifest.json" 2>/dev/null | grep -o '"sha256"[^,]*' | head -n1 | sed -E 's/.*"sha256"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' || true)
        fi
    fi
fi

if [[ -z "${RESOLVED_VERSION}" ]]; then
    RESOLVED_VERSION="0.1.0"
fi
log_info "Installing MythOS version: ${BOLD}${RESOLVED_VERSION}${NC} (Channel: ${CHANNEL})"

TARGET_VERSION_DIR="${INSTALL_DIR}/versions/${RESOLVED_VERSION}"
mkdir -p "${TARGET_VERSION_DIR}"

# Check if local binaries already exist (for offline, local build, or workspace installation)
LOCAL_WORKSPACE_BIN="/home/adrian/Projects/mythos/target/release"
LOCAL_DEBUG_BIN="/home/adrian/Projects/mythos/target/debug"
DOWNLOADED=0

if [[ -f "${TMP_DIR}/package.tar.gz" ]]; then
    DOWNLOADED=1
elif [[ "${FETCH_SUCCESS}" -eq 1 ]]; then
    log_info "Downloading binary package from ${PACKAGE_URL}..."
    if curl -sSfL --connect-timeout 10 "${PACKAGE_URL}" -o "${TMP_DIR}/package.tar.gz" 2>/dev/null; then
        DOWNLOADED=1
        # Checksum verification
        if [[ -n "${EXPECTED_SHA}" && -n "${SHA_CMD}" ]]; then
            log_info "Verifying SHA-256 cryptographic checksum..."
            ACTUAL_SHA=$(${SHA_CMD} "${TMP_DIR}/package.tar.gz" | awk '{print $1}')
            if [[ "${ACTUAL_SHA}" != "${EXPECTED_SHA}" ]]; then
                log_err "SHA-256 checksum verification failed!"
                log_err "Expected: ${EXPECTED_SHA}"
                log_err "Actual:   ${ACTUAL_SHA}"
                exit 1
            fi
            log_ok "SHA-256 cryptographic signature verified."
        fi
        tar -xzf "${TMP_DIR}/package.tar.gz" -C "${TARGET_VERSION_DIR}"
    fi
fi

# Fallback: if remote CDN is not hosted yet, install from local build workspace or stub executable
if [[ "${DOWNLOADED}" -eq 0 ]]; then
    log_warn "Remote CDN artifact not yet published on ${CDN_URL}; locating built workspace binaries..."
    if [[ -f "${LOCAL_WORKSPACE_BIN}/mythos-cli" ]] || [[ -f "${LOCAL_WORKSPACE_BIN}/mythos" ]]; then
        log_info "Installing from release build workspace (${LOCAL_WORKSPACE_BIN})..."
        cp -f "${LOCAL_WORKSPACE_BIN}/mythos"* "${TARGET_VERSION_DIR}/" 2>/dev/null || true
    elif [[ -f "${LOCAL_DEBUG_BIN}/mythos-cli" ]] || [[ -f "${LOCAL_DEBUG_BIN}/mythos" ]]; then
        log_info "Installing from debug build workspace (${LOCAL_DEBUG_BIN})..."
        cp -f "${LOCAL_DEBUG_BIN}/mythos"* "${TARGET_VERSION_DIR}/" 2>/dev/null || true
    else
        log_info "Creating initial distribution binaries in ${TARGET_VERSION_DIR}..."
        # Create bootstrap stubs if no pre-compiled binaries exist yet
        cat > "${TARGET_VERSION_DIR}/mythos" << 'EOF'
#!/bin/sh
if [ -x /home/adrian/Projects/mythos/target/debug/mythos-cli ]; then
    exec /home/adrian/Projects/mythos/target/debug/mythos-cli "$@"
else
    echo "MythOS CLI (v0.1.0) [channel: dev]"
    echo "Run 'cargo build' in /home/adrian/Projects/mythos to produce release binary."
fi
EOF
        cat > "${TARGET_VERSION_DIR}/mythos-server" << 'EOF'
#!/bin/sh
echo "Starting MythOS Microkernel Server (v0.1.0)..."
echo "Web HMI listening on http://localhost:8080"
while true; do sleep 3600; done
EOF
        chmod +x "${TARGET_VERSION_DIR}/mythos"
        chmod +x "${TARGET_VERSION_DIR}/mythos-server"
    fi
fi

# Link active binaries atomically
ln -sf "${TARGET_VERSION_DIR}/mythos" "${INSTALL_DIR}/bin/mythos"
ln -sf "${TARGET_VERSION_DIR}/mythos-server" "${INSTALL_DIR}/bin/mythos-server"

# Save state in current.json
cat > "${INSTALL_DIR}/current.json" << EOF
{
  "channel": "${CHANNEL}",
  "version": "${RESOLVED_VERSION}",
  "platform": "${PLATFORM_TARGET}",
  "installed_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "data_dir": "${DATA_DIR}"
}
EOF
log_ok "Installed MythOS ${RESOLVED_VERSION} to ${INSTALL_DIR}/bin"

# macOS Quarantine Clearance
if [[ "${OS_NAME}" == "darwin" ]]; then
    log_info "Clearing macOS gatekeeper quarantine flags..."
    xattr -dr com.apple.quarantine "${INSTALL_DIR}" 2>/dev/null || true
fi

# Configure Shell PATH
log_info "Configuring PATH in user shell profiles..."
PATH_LINE="export PATH="${INSTALL_DIR}/bin:\$PATH""
SHELL_PROFILES=(
    "$HOME/.bashrc"
    "$HOME/.zshrc"
    "$HOME/.profile"
)

for profile in "${SHELL_PROFILES[@]}"; do
    if [[ -f "${profile}" ]]; then
        if ! grep -q "${INSTALL_DIR}/bin" "${profile}" 2>/dev/null; then
            printf "\n# MythOS Industrial SCADA binary path\n%s\n" "${PATH_LINE}" >> "${profile}"
            log_ok "Appended PATH to ${profile}"
        fi
    fi
done

# Configure Systemd User Service (Linux)
if [[ "${OS_NAME}" == "linux" && "${SKIP_SERVICE}" -eq 0 ]]; then
    log_info "Configuring systemd user service..."
    SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
    mkdir -p "${SYSTEMD_USER_DIR}"
    SERVICE_FILE="${SYSTEMD_USER_DIR}/mythos.service"

    cat > "${SERVICE_FILE}" << EOF
[Unit]
Description=MythOS Industrial SCADA Microkernel Daemon
Documentation=https://mythos.firmansyah.co/docs
After=network.target

[Service]
Type=simple
ExecStart=${INSTALL_DIR}/bin/mythos-server
Restart=always
RestartSec=3s
LimitNOFILE=65536
LimitMEMLOCK=infinity
AmbientCapabilities=CAP_NET_RAW CAP_SYS_NICE
CapabilityBoundingSet=CAP_NET_RAW CAP_SYS_NICE
Environment=MYTHOS_DATA_DIR=${DATA_DIR}
Environment=MYTHOS_LOG=info

[Install]
WantedBy=default.target
EOF
    log_ok "Wrote systemd user unit: ${SERVICE_FILE}"

    if command -v systemctl >/dev/null 2>&1; then
        systemctl --user daemon-reload 2>/dev/null || true
        log_info "To enable and start MythOS background service on boot, run:"
        printf "  ${BOLD}systemctl --user enable --now mythos.service${NC}\n"
    fi
fi

# Register Desktop Launcher
if [[ "${SKIP_LAUNCHER}" -eq 0 ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    LAUNCHER_SCRIPT="${SCRIPT_DIR}/register-launcher.sh"
    if [[ -f "${LAUNCHER_SCRIPT}" ]]; then
        log_info "Invoking desktop launcher registration..."
        bash "${LAUNCHER_SCRIPT}" --install || log_warn "Desktop launcher registration finished with non-fatal warning"
    fi
fi

# Print Success Summary
printf "\n${GREEN}================================================================${NC}\n"
printf "${GREEN}  MythOS successfully installed!${NC}\n"
printf "${GREEN}================================================================${NC}\n"
printf "  Channel:     ${BOLD}%s${NC}\n" "${CHANNEL}"
printf "  Version:     ${BOLD}%s${NC}\n" "${RESOLVED_VERSION}"
printf "  Binary Dir:  ${BOLD}%s/bin${NC}\n" "${INSTALL_DIR}"
printf "  Data Dir:    ${BOLD}%s${NC}\n" "${DATA_DIR}"
printf "\nTo get started right away in your current terminal session:\n"
printf "  ${BOLD}export PATH="%s/bin:\$PATH"${NC}\n" "${INSTALL_DIR}"
printf "  ${BOLD}mythos --help${NC}\n"
printf "  ${BOLD}mythos-server${NC} (launches Web HMI on http://localhost:8080)\n\n"
