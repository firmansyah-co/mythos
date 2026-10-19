#!/usr/bin/env bash
# ==============================================================================
# MythOS Development Daemon Background Service & Auto-Reload Supervisor
# Part of MythOS Development Tooling & Lifecycle Management
# ==============================================================================
#
# Monitors:
#   1. Binary timestamp / SHA256 (`target/debug/mythos-server`)
#   2. Active Git HEAD commit (`git rev-parse HEAD`)
#
# Automatically builds backend and frontend assets if missing or stale,
# supervises the daemon microkernel child process, and restarts it gracefully
# whenever code updates or rebuilds occur.
# ==============================================================================

set -uo pipefail

# ANSI Color Codes & Prefixes
CLR_RESET="\033[0m"
CLR_CYAN="\033[1;36m"
CLR_GREEN="\033[1;32m"
CLR_YELLOW="\033[1;33m"
CLR_RED="\033[1;31m"
CLR_BLUE="\033[1;34m"
CLR_BOLD="\033[1m"

log_info() {
    printf "${CLR_CYAN}[MYTHOS-SUPERVISOR]${CLR_RESET} ${CLR_BLUE}[INFO]${CLR_RESET} %s\n" "$*"
}

log_success() {
    printf "${CLR_CYAN}[MYTHOS-SUPERVISOR]${CLR_RESET} ${CLR_GREEN}[OK]${CLR_RESET} %s\n" "$*"
}

log_warn() {
    printf "${CLR_CYAN}[MYTHOS-SUPERVISOR]${CLR_RESET} ${CLR_YELLOW}[WARN]${CLR_RESET} %s\n" "$*"
}

log_error() {
    printf "${CLR_CYAN}[MYTHOS-SUPERVISOR]${CLR_RESET} ${CLR_RED}[ERROR]${CLR_RESET} %s\n" "$*" >&2
}

# Resolve directory roots
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

# Configuration defaults
export MYTHOS_STORAGE_ROOT="${MYTHOS_STORAGE_ROOT:-$HOME/.local/share/mythos/projects}"
export MYTHOS_STATIC_DIR="${MYTHOS_STATIC_DIR:-$REPO_ROOT/frontend/packages/ui-shell/dist}"
export MYTHOS_HTTP_PORT="${MYTHOS_HTTP_PORT:-8080}"
POLL_INTERVAL_SEC="${MYTHOS_SUPERVISOR_POLL:-2}"

SERVER_BIN="${REPO_ROOT}/target/debug/mythos-server"
UI_SHELL_INDEX="${MYTHOS_STATIC_DIR}/index.html"

# Global state
CHILD_PID=0
SHUTTING_DOWN=0

# Ensure projects storage directory exists
if [ ! -d "${MYTHOS_STORAGE_ROOT}" ]; then
    log_info "Creating project storage directory at ${MYTHOS_STORAGE_ROOT}"
    mkdir -p "${MYTHOS_STORAGE_ROOT}"
fi

# Function to check whether backend sources are newer than compiled binary
are_backend_sources_newer() {
    if [ ! -f "${SERVER_BIN}" ]; then
        return 0
    fi
    local newer_count
    newer_count=$(find "${REPO_ROOT}/crates" "${REPO_ROOT}/Cargo.toml" "${REPO_ROOT}/Cargo.lock" -type f -newer "${SERVER_BIN}" 2>/dev/null | head -n 1 | wc -l)
    if [ "${newer_count}" -gt 0 ]; then
        return 0
    fi
    return 1
}

# Build backend if missing or outdated
ensure_backend_built() {
    local force_check="${1:-0}"
    if [ ! -f "${SERVER_BIN}" ] || are_backend_sources_newer || [ "${force_check}" -eq 1 ]; then
        log_info "Compiling backend daemon (cargo build --bin mythos-server)..."
        if cargo build --bin mythos-server; then
            log_success "Backend daemon compiled successfully: ${SERVER_BIN}"
        else
            log_error "Cargo build failed! Checking if existing binary is runnable..."
            if [ ! -f "${SERVER_BIN}" ]; then
                log_error "Fatal: No binary available to execute."
                return 1
            fi
        fi
    else
        log_info "Backend daemon binary is up to date."
    fi
    return 0
}

# Build frontend if missing
ensure_frontend_built() {
    if [ ! -f "${UI_SHELL_INDEX}" ]; then
        log_info "Frontend Web HMI assets missing (${UI_SHELL_INDEX}). Building via pnpm..."
        if pnpm --filter @mythos/ui-shell build; then
            log_success "Frontend Web HMI built successfully."
        else
            log_warn "Frontend build encountered errors. Embedded fallback UI will be used if available."
        fi
    else
        log_info "Frontend Web HMI assets present at ${UI_SHELL_INDEX}."
    fi
}

# Terminate child process gracefully
stop_daemon() {
    if [ "${CHILD_PID}" -gt 0 ] && kill -0 "${CHILD_PID}" 2>/dev/null; then
        log_info "Stopping daemon process (PID ${CHILD_PID}) with SIGTERM..."
        kill -TERM "${CHILD_PID}" 2>/dev/null || true

        # Wait up to 5 seconds for clean exit
        local count=0
        while kill -0 "${CHILD_PID}" 2>/dev/null && [ "${count}" -lt 50 ]; do
            sleep 0.1
            count=$((count + 1))
        done

        if kill -0 "${CHILD_PID}" 2>/dev/null; then
            log_warn "Daemon PID ${CHILD_PID} did not terminate within 5s. Sending SIGKILL..."
            kill -KILL "${CHILD_PID}" 2>/dev/null || true
            sleep 0.2
        fi
        log_success "Daemon process (PID ${CHILD_PID}) terminated."
    fi
    CHILD_PID=0
}

# Start child daemon process
start_daemon() {
    if [ ! -x "${SERVER_BIN}" ]; then
        log_error "Binary ${SERVER_BIN} is not executable!"
        return 1
    fi

    log_info "Starting MythOS daemon on port ${MYTHOS_HTTP_PORT}..."
    log_info "  - Storage Root: ${MYTHOS_STORAGE_ROOT}"
    log_info "  - Static Dir:   ${MYTHOS_STATIC_DIR}"

    "${SERVER_BIN}" &
    CHILD_PID=$!
    log_success "Daemon launched with PID ${CHILD_PID}."
}

# Signal traps
cleanup() {
    if [ "${SHUTTING_DOWN}" -eq 1 ]; then
        return
    fi
    SHUTTING_DOWN=1
    printf "\n"
    log_warn "Received shutdown signal. Cleaning up supervisor and child daemon..."
    stop_daemon
    log_success "Supervisor exited cleanly."
    exit 0
}

trap cleanup SIGTERM SIGINT SIGHUP EXIT

# Helper to get binary signature (mtime + sha256)
get_binary_sig() {
    if [ -f "${SERVER_BIN}" ]; then
        stat -c "%Y" "${SERVER_BIN}" 2>/dev/null || stat -f "%m" "${SERVER_BIN}" 2>/dev/null || echo "0"
    else
        echo "0"
    fi
}

# Helper to get current Git HEAD
get_git_head() {
    git rev-parse HEAD 2>/dev/null || echo "none"
}

# Check command line flags
FORCE_CHECK=0
for arg in "$@"; do
    case "$arg" in
        --check|--build)
            FORCE_CHECK=1
            ;;
    esac
done

# Initial builds
ensure_backend_built "${FORCE_CHECK}"
ensure_frontend_built

# Start daemon
start_daemon

# Track initial states
LAST_BIN_SIG="$(get_binary_sig)"
LAST_GIT_HEAD="$(get_git_head)"

log_success "Supervisor active. Monitoring git commits and binary mtime (interval: ${POLL_INTERVAL_SEC}s)..."

# Main Supervisor Loop
while [ "${SHUTTING_DOWN}" -eq 0 ]; do
    sleep "${POLL_INTERVAL_SEC}"

    # 1. Check if child crashed or exited unexpectedly
    if [ "${CHILD_PID}" -gt 0 ] && ! kill -0 "${CHILD_PID}" 2>/dev/null; then
        log_error "Daemon child process (PID ${CHILD_PID}) died unexpectedly. Restarting in 2s..."
        stop_daemon
        sleep 2
        ensure_backend_built 0
        start_daemon
        LAST_BIN_SIG="$(get_binary_sig)"
        LAST_GIT_HEAD="$(get_git_head)"
        continue
    fi

    # 2. Check Git HEAD change
    CURRENT_GIT_HEAD="$(get_git_head)"
    if [ "${CURRENT_GIT_HEAD}" != "${LAST_GIT_HEAD}" ] && [ "${CURRENT_GIT_HEAD}" != "none" ]; then
        log_info "Git HEAD changed (${LAST_GIT_HEAD:0:8} -> ${CURRENT_GIT_HEAD:0:8}). Initiating rebuild and reload..."
        ensure_backend_built 1
        stop_daemon
        start_daemon
        LAST_GIT_HEAD="${CURRENT_GIT_HEAD}"
        LAST_BIN_SIG="$(get_binary_sig)"
        continue
    fi

    # 3. Check Binary modification
    CURRENT_BIN_SIG="$(get_binary_sig)"
    if [ "${CURRENT_BIN_SIG}" != "${LAST_BIN_SIG}" ] && [ "${CURRENT_BIN_SIG}" != "0" ]; then
        log_info "Binary modification detected (${SERVER_BIN}). Reloading daemon..."
        stop_daemon
        start_daemon
        LAST_BIN_SIG="${CURRENT_BIN_SIG}"
        continue
    fi
done
