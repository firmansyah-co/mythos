#!/usr/bin/env bash
# ==============================================================================
# MythOS Desktop Application Launcher Registration Script
# Part of MythOS Distribution Suite (TSK-214)
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ASSET_DIR="${SCRIPT_DIR}/assets"
ICON_SOURCE="${ASSET_DIR}/mythos.svg"

# Configuration defaults
ACTION="install"
DRY_RUN=0
SYSTEM_INSTALL=0
VERBOSE=0

# Formatting
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m' # No Color

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
    cat << 'EOF'
Usage: register-launcher.sh [OPTIONS]

Options:
  --install         Register desktop application launcher (default)
  --uninstall       Remove desktop application launcher and icons
  --system          Install system-wide (requires root privileges)
  --dry-run         Simulate registration without modifying filesystem
  -v, --verbose     Enable verbose diagnostic logging
  -h, --help        Show this help message
EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --install)
            ACTION="install"
            shift
            ;;
        --uninstall)
            ACTION="uninstall"
            shift
            ;;
        --system)
            SYSTEM_INSTALL=1
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        -v|--verbose)
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

# Detect OS
OS_TYPE="$(uname -s | tr '[:upper:]' '[:lower:]')"

execute_cmd() {
    if [[ "${DRY_RUN}" -eq 1 ]]; then
        printf "${YELLOW}[DRY-RUN]${NC} %s\n" "$*"
    else
        if [[ "${VERBOSE}" -eq 1 ]]; then
            printf "  -> %s\n" "$*"
        fi
        eval "$@"
    fi
}

install_linux() {
    log_info "Registering MythOS desktop launcher for Linux (FreeDesktop)..."

    local desktop_dir
    local icon_dir

    if [[ "${SYSTEM_INSTALL}" -eq 1 ]] || [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
        desktop_dir="/usr/share/applications"
        icon_dir="/usr/share/icons/hicolor/scalable/apps"
    else
        desktop_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
        icon_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
    fi

    local desktop_file="${desktop_dir}/mythos.desktop"
    local icon_dest="${icon_dir}/mythos.svg"

    if [[ "${DRY_RUN}" -eq 1 ]]; then
        log_info "[DRY-RUN] Would create directory: ${desktop_dir}"
        log_info "[DRY-RUN] Would create directory: ${icon_dir}"
        log_info "[DRY-RUN] Would copy icon: ${ICON_SOURCE} -> ${icon_dest}"
        log_info "[DRY-RUN] Would create desktop entry: ${desktop_file}"
    else
        mkdir -p "${desktop_dir}"
        mkdir -p "${icon_dir}"

        if [[ -f "${ICON_SOURCE}" ]]; then
            cp -f "${ICON_SOURCE}" "${icon_dest}"
            chmod 644 "${icon_dest}"
            log_ok "Installed icon to ${icon_dest}"
        else
            log_warn "Icon source not found at ${ICON_SOURCE}; skipping icon installation"
        fi

        cat > "${desktop_file}" << 'EOF'
[Desktop Entry]
Version=1.0
Type=Application
Name=MythOS Industrial SCADA
GenericName=Industrial Automation SCADA & Engineering Studio
Comment=Next-Generation Linux-First Industrial SCADA & Soft-Logic Microkernel
Exec=mythos-server
Icon=mythos
Terminal=false
Categories=Development;Engineering;
Keywords=scada;plc;industrial;automation;engineering;
StartupNotify=true
EOF
        chmod 644 "${desktop_file}"
        log_ok "Registered desktop entry at ${desktop_file}"

        # Update desktop database and icon caches if utilities are present
        if command -v update-desktop-database >/dev/null 2>&1; then
            update-desktop-database "${desktop_dir}" >/dev/null 2>&1 || true
        fi
        if command -v gtk-update-icon-cache >/dev/null 2>&1; then
            gtk-update-icon-cache -f -t "${icon_dir}/../../" >/dev/null 2>&1 || true
        fi
    fi

    log_ok "MythOS desktop launcher registered successfully."
}

uninstall_linux() {
    log_info "Removing MythOS desktop launcher for Linux..."

    local user_desktop="${XDG_DATA_HOME:-$HOME/.local/share}/applications/mythos.desktop"
    local user_icon="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps/mythos.svg"
    local sys_desktop="/usr/share/applications/mythos.desktop"
    local sys_icon="/usr/share/icons/hicolor/scalable/apps/mythos.svg"

    local targets=()
    if [[ "${SYSTEM_INSTALL}" -eq 1 ]] || [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
        targets+=("${sys_desktop}" "${sys_icon}")
    else
        targets+=("${user_desktop}" "${user_icon}")
    fi

    for target in "${targets[@]}"; do
        if [[ -f "${target}" ]] || [[ -L "${target}" ]]; then
            if [[ "${DRY_RUN}" -eq 1 ]]; then
                log_info "[DRY-RUN] Would remove: ${target}"
            else
                rm -f "${target}"
                log_ok "Removed ${target}"
            fi
        else
            log_info "Not found (already cleaned): ${target}"
        fi
    done

    if [[ "${DRY_RUN}" -ne 1 ]]; then
        if command -v update-desktop-database >/dev/null 2>&1; then
            update-desktop-database "${XDG_DATA_HOME:-$HOME/.local/share}/applications" >/dev/null 2>&1 || true
        fi
    fi

    log_ok "MythOS desktop launcher uninstalled successfully."
}

install_macos() {
    log_info "Registering MythOS desktop launcher for macOS..."

    local app_dir
    if [[ "${SYSTEM_INSTALL}" -eq 1 ]] || [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
        app_dir="/Applications/MythOS.app"
    else
        app_dir="${HOME}/Applications/MythOS.app"
    fi

    local macos_dir="${app_dir}/Contents/MacOS"
    local res_dir="${app_dir}/Contents/Resources"
    local plist_file="${app_dir}/Contents/Info.plist"
    local launcher_bin="${macos_dir}/mythos-launcher"

    if [[ "${DRY_RUN}" -eq 1 ]]; then
        log_info "[DRY-RUN] Would create macOS app bundle structure at: ${app_dir}"
        log_info "[DRY-RUN] Would create Info.plist at: ${plist_file}"
        log_info "[DRY-RUN] Would create launcher script at: ${launcher_bin}"
    else
        mkdir -p "${macos_dir}"
        mkdir -p "${res_dir}"

        if [[ -f "${ICON_SOURCE}" ]]; then
            cp -f "${ICON_SOURCE}" "${res_dir}/mythos.svg"
        fi

        cat > "${plist_file}" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>mythos-launcher</string>
    <key>CFBundleIconFile</key>
    <string>mythos</string>
    <key>CFBundleIdentifier</key>
    <string>co.firmansyah.mythos</string>
    <key>CFBundleName</key>
    <string>MythOS</string>
    <key>CFBundleDisplayName</key>
    <string>MythOS Industrial SCADA</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>HighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

        cat > "${launcher_bin}" << 'EOF'
#!/bin/bash
export PATH="$HOME/.mythos/bin:/usr/local/bin:$PATH"

# Check if mythos-server daemon is running; start in background if not
if ! pgrep -x "mythos-server" > /dev/null 2>&1; then
    if command -v mythos-server > /dev/null 2>&1; then
        nohup mythos-server > /dev/null 2>&1 &
        sleep 1
    fi
fi

# Open Web HMI Studio in default browser
open "http://localhost:8080"
EOF
        chmod +x "${launcher_bin}"
        log_ok "Registered macOS application bundle at ${app_dir}"
    fi

    log_ok "MythOS macOS launcher registered successfully."
}

uninstall_macos() {
    log_info "Removing MythOS desktop launcher for macOS..."

    local user_app="${HOME}/Applications/MythOS.app"
    local sys_app="/Applications/MythOS.app"

    local targets=()
    if [[ "${SYSTEM_INSTALL}" -eq 1 ]] || [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
        targets+=("${sys_app}")
    else
        targets+=("${user_app}")
    fi

    for target in "${targets[@]}"; do
        if [[ -d "${target}" ]]; then
            if [[ "${DRY_RUN}" -eq 1 ]]; then
                log_info "[DRY-RUN] Would remove bundle: ${target}"
            else
                rm -rf "${target}"
                log_ok "Removed ${target}"
            fi
        else
            log_info "Not found (already cleaned): ${target}"
        fi
    done

    log_ok "MythOS macOS launcher uninstalled successfully."
}

# Main execution dispatch
case "${OS_TYPE}" in
    linux*)
        if [[ "${ACTION}" == "install" ]]; then
            install_linux
        else
            uninstall_linux
        fi
        ;;
    darwin*)
        if [[ "${ACTION}" == "install" ]]; then
            install_macos
        else
            uninstall_macos
        fi
        ;;
    *)
        log_err "Unsupported operating system for desktop launcher registration: ${OS_TYPE}"
        exit 1
        ;;
esac
