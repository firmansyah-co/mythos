#!/usr/bin/env bash
# The one verification gate. If you are about to say "it passes", run this first
# and paste what it printed.
#
# Usage: ./scripts/check.sh [--rust] [--frontend] [--all]
#   default: --all
#   exit 0: every gate green; exit 1: at least one gate red or not run; exit 2: usage.
#
# When it finishes it writes target/check-summary.txt and prints the same text last:
# one line per gate, the failing test names, totals, the commit and whether the tree
# was dirty. Each gate's full output is in target/check-logs/<gate>.log.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---all}"
LOG_DIR="$ROOT/target/check-logs"
SUMMARY="$ROOT/target/check-summary.txt"
MAX_LINES=30
fail=0
declare -a failed=()
declare -a gate_lines=()   # "PASS label" / "FAIL label — reason"
declare -a gate_info=()    # per gate: totals lines (may be empty), newline-separated
declare -a gate_names=()   # per gate: failing names, newline-separated

case "$MODE" in
    --rust|--frontend|--all) ;;
    *) echo "usage: $0 [--rust|--frontend|--all]"; exit 2 ;;
esac

mkdir -p "$LOG_DIR"

# The commit and the dirty state are read before any gate runs: they describe the
# tree that was checked. Outside a git checkout they are UNKNOWN, never guessed: a
# copy unpacked under another checkout's target/ would otherwise report that
# checkout's commit, and "clean" because target/ is ignored.
toplevel="$(git -C "$ROOT" rev-parse --show-toplevel 2>/dev/null || true)"
if [ -n "$toplevel" ] && [ "$(cd "$toplevel" && pwd -P)" = "$(cd "$ROOT" && pwd -P)" ] \
    && commit="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null)"; then
    if [ -n "$(git -C "$ROOT" status --porcelain 2>/dev/null)" ]; then
        tree_state="dirty (uncommitted changes)"
    else
        tree_state="clean"
    fi
else
    commit="UNKNOWN (not a git checkout)"
    tree_state="UNKNOWN"
fi

strip_ansi() { sed -E $'s/\x1b\\[[0-9;?]*[A-Za-z]//g'; }

# Record a gate's result for the summary.
record() { # record <PASS|FAIL> <label> <reason> <info> <names>
    if [ "$1" = PASS ]; then
        gate_lines+=("PASS $2")
    else
        gate_lines+=("FAIL $2${3:+ — $3}")
    fi
    gate_info+=("$4")
    gate_names+=("$5")
}

# What a cargo test log says: totals over every test binary, and on failure the
# failing tests and the test binaries cargo names as failed.
cargo_test_details() { # <log> <rc>
    local log="$1" rc="$2" info names bins
    info="$(awk '
        /^test result: / {
            n++
            for (i = 1; i <= NF; i++) {
                if ($(i) ~ /^passed;?$/)  p += $(i-1)
                if ($(i) ~ /^failed;?$/)  f += $(i-1)
                if ($(i) ~ /^ignored;?$/) g += $(i-1)
            }
        }
        END { if (n) printf "  totals: %d passed, %d failed, %d ignored across %d test binaries", p, f, g, n }
    ' "$log")"
    [ "$rc" -eq 0 ] && { printf '%s\n\x1f\n' "$info"; return; }
    names="$(grep -E '^test .+ \.\.\. FAILED$' "$log" | awk '{print "  test " $2}' | awk '!seen[$0]++')"
    bins="$(grep -oE '`-p [^`]+`|to rerun pass `[^`]+`' "$log" | sed -E 's/^to rerun pass //' | awk '!seen[$0]++' \
        | paste -sd, - | sed 's/,/, /g')"
    [ -n "$bins" ] && info="${info:+$info$'\n'}  failed test binaries: $bins"
    if [ -z "$names" ]; then
        # No test failed by name: a build error, a crash or a timeout. Show what cargo said.
        names="$(grep -E '^error(\[E[0-9]+\])?:' "$log" | awk '!seen[$0]++' | sed 's/^/  /')"
    fi
    printf '%s\n\x1f\n%s' "$info" "$names"
}

# What a vitest log says (pnpm test): the "Tests" and "Test Files" totals, and on
# failure the FAIL lines as vitest prints them.
vitest_details() { # <log> <rc>
    local log="$1" rc="$2" info names
    info="$(strip_ansi < "$log" | grep -E '(^|[[:space:]])(Test Files|Tests)[[:space:]]+[0-9]' \
        | sed -E 's/^[[:space:]]+//; s/^/  /' | awk '!seen[$0]++')"
    [ "$rc" -eq 0 ] && { printf '%s\n\x1f\n' "$info"; return; }
    names="$(strip_ansi < "$log" | grep -E '(^|[[:space:]])(FAIL|×)[[:space:]]+[^[:space:]]' \
        | sed -E 's/^[[:space:]]+//; s/[[:space:]]+[0-9]+ms$//; s/^/  /' | awk '!seen[$0]++')"
    printf '%s\n\x1f\n%s' "$info" "$names"
}

# Any other failing gate: its first error lines, or its last line.
generic_details() { # <log>
    local names
    names="$(strip_ansi < "$1" | grep -E '^(error|Error|ERR_)' | awk '!seen[$0]++' | sed 's/^/  /')"
    [ -z "$names" ] && names="$(strip_ansi < "$1" | grep -v '^[[:space:]]*$' | tail -n 1 | sed 's/^/  /')"
    printf '\n\x1f\n%s' "$names"
}

step() { # step <label> <kind: cargo-test|vitest|plain> <cmd...>
    local label="$1" kind="$2"; shift 2
    local log="$LOG_DIR/$(printf '%s' "$label" | tr -c 'A-Za-z0-9' '-').log" rc details
    printf '\n=== %s ===\n' "$label"
    # The exit code is the command's own (PIPESTATUS), not tee's.
    "$@" 2>&1 | tee "$log"
    rc=${PIPESTATUS[0]}
    case "$kind" in
        cargo-test) details="$(cargo_test_details "$log" "$rc")" ;;
        vitest)     details="$(vitest_details "$log" "$rc")" ;;
        *)          if [ "$rc" -eq 0 ]; then details=$'\n\x1f\n'; else details="$(generic_details "$log")"; fi ;;
    esac
    local info="${details%%$'\x1f'*}" names="${details#*$'\x1f'}"
    info="${info%$'\n'}"; names="${names#$'\n'}"
    if [ "$rc" -eq 0 ]; then
        printf 'PASS %s\n' "$label"
        record PASS "$label" "" "$info" ""
    else
        printf 'FAIL %s (exit %d)\n' "$label" "$rc"
        fail=1; failed+=("$label")
        record FAIL "$label" "exit $rc, log target/check-logs/${log##*/}" "$info" "$names"
    fi
}

# A gate that could not RUN is a red, never a pass. It must record itself the same
# way `step` does, or the verdict block reports green for a check that never
# executed — the exact failure CULTURE.md §4 names: "a green summary that was never
# executed is worse than an honest red."
skip() { # skip <label> <reason>
    printf '\n=== %s ===\n%s\n' "$1" "$2"
    printf 'FAIL %s (not run)\n' "$1"
    fail=1; failed+=("$1 ($2)")
    record FAIL "$1" "not run: $2" "" ""
}

run_rust() {
    local gates=("cargo fmt --check" "cargo clippy" "cargo test") g
    if ! command -v cargo >/dev/null; then
        for g in "${gates[@]}"; do skip "$g" "cargo not installed"; done
        return 1
    fi
    step "cargo fmt --check" plain      cargo fmt --all -- --check
    step "cargo clippy"      plain      cargo clippy --all-targets --all-features -- -D warnings
    # --no-fail-fast: one failing test binary must not hide the binaries after it.
    step "cargo test"        cargo-test cargo test --workspace --no-fail-fast
}

run_frontend() {
    local fe="$ROOT/frontend" gates=("pnpm lint" "pnpm test" "pnpm build") g reason=""
    if [ ! -d "$fe" ]; then
        reason="frontend/ missing"
    elif ! command -v pnpm >/dev/null; then
        reason="pnpm not installed"
    elif [ ! -d "$fe/node_modules" ]; then
        reason="frontend/node_modules missing: run pnpm install --frozen-lockfile"
    fi
    if [ -n "$reason" ]; then
        for g in "${gates[@]}"; do skip "$g" "$reason"; done
        return 1
    fi
    step "pnpm lint"  plain  bash -c "cd '$fe' && pnpm lint"
    step "pnpm test"  vitest bash -c "cd '$fe' && pnpm test"
    step "pnpm build" plain  bash -c "cd '$fe' && pnpm build"
}

# The summary: at most about MAX_LINES lines. Gate, totals and header lines are always
# printed; failing names fill what is left, and any that do not fit are counted.
write_summary() {
    local -a out=() all_names=()
    local i n fixed avail total_names shown=0 line
    if [ "$fail" -ne 0 ]; then
        out+=("check.sh $MODE: FAIL — ${#failed[@]} gate(s) red")
    else
        out+=("check.sh $MODE: PASS — every gate green")
    fi
    out+=("commit $commit, tree $tree_state")
    fixed=${#out[@]}
    total_names=0
    for i in "${!gate_lines[@]}"; do
        fixed=$((fixed + 1))
        [ -n "${gate_info[$i]}" ] && fixed=$((fixed + $(printf '%s\n' "${gate_info[$i]}" | wc -l)))
        [ -n "${gate_names[$i]}" ] && total_names=$((total_names + $(printf '%s\n' "${gate_names[$i]}" | wc -l)))
    done
    avail=$((MAX_LINES - fixed))
    [ "$total_names" -gt "$avail" ] && avail=$((avail - 1))   # room for the "not listed" line
    [ "$avail" -lt 0 ] && avail=0
    for i in "${!gate_lines[@]}"; do
        out+=("${gate_lines[$i]}")
        if [ -n "${gate_info[$i]}" ]; then
            while IFS= read -r line; do out+=("$line"); done <<< "${gate_info[$i]}"
        fi
        if [ -n "${gate_names[$i]}" ]; then
            while IFS= read -r line; do
                if [ "$shown" -lt "$avail" ]; then out+=("$line"); fi
                shown=$((shown + 1))
            done <<< "${gate_names[$i]}"
        fi
    done
    if [ "$total_names" -gt "$avail" ]; then
        n=$((total_names - avail))
        out+=("... and $n more failure line(s) not listed: see target/check-logs/")
    fi
    printf '%s\n' "${out[@]}" > "$SUMMARY"
}

case "$MODE" in
    --rust)     run_rust ;;
    --frontend) run_frontend ;;
    --all)      run_rust; run_frontend ;;
esac

write_summary
printf '\n────────────────────────────────────────\n'
if [ "$fail" -ne 0 ]; then
    printf 'VERDICT: FAIL — %d gate(s) red: %s\n' "${#failed[@]}" "${failed[*]}"
else
    printf 'VERDICT: PASS — every gate green\n'
fi
printf '\n── summary (target/check-summary.txt) ──\n'
cat "$SUMMARY"
[ "$fail" -ne 0 ] && exit 1
exit 0
