#!/usr/bin/env bash
# The one verification gate. If you are about to say "it passes", run this first
# and paste what it printed. Scoped narrower slices name their judge in rig/SLICES.md.
#
# Usage: ./scripts/check.sh [--rust] [--frontend] [--all]
#   default: --all
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---all}"
fail=0
declare -a failed=()

step() { # step <label> <cmd...>
    local label="$1"; shift
    printf '\n=== %s ===\n' "$label"
    if "$@"; then
        printf 'PASS %s\n' "$label"
    else
        printf 'FAIL %s\n' "$label"
        fail=1; failed+=("$label")
    fi
}

run_rust() {
    command -v cargo >/dev/null || { echo "cargo not found"; return 1; }
    step "cargo fmt --check"  cargo fmt --all -- --check
    step "cargo clippy"       cargo clippy --all-targets --all-features -- -D warnings
    step "cargo test"         cargo test --all
}

run_frontend() {
    local fe="$ROOT/frontend"
    [ -d "$fe/node_modules" ] || { echo "frontend/node_modules missing: run pnpm install --frozen-lockfile"; return 1; }
    step "pnpm lint"  bash -c "cd '$fe' && pnpm lint"
    step "pnpm test"  bash -c "cd '$fe' && pnpm test"
    step "pnpm build" bash -c "cd '$fe' && pnpm build"
}

case "$MODE" in
    --rust)     run_rust ;;
    --frontend) run_frontend ;;
    --all)      run_rust; run_frontend ;;
    *) echo "usage: $0 [--rust|--frontend|--all]"; exit 2 ;;
esac

printf '\n────────────────────────────────────────\n'
if [ "$fail" -ne 0 ]; then
    printf 'VERDICT: FAIL — %d gate(s) red: %s\n' "${#failed[@]}" "${failed[*]}"
    exit 1
fi
printf 'VERDICT: PASS — every gate green\n'
