#!/usr/bin/env bash
# Staleness check for slice worktrees. Read-only.
#
# A worktree is STALE when its branch is already an ancestor of origin/dev
# (the slice shipped and nobody closed the worktree) or when it has fallen
# behind origin/dev. Prints one line per worktree and a verdict.
#
# Usage: ./scripts/check-slices.sh [--fix-report]
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE="${BASE_REF:-origin/dev}"
stale=0; total=0

git -C "$ROOT" fetch origin --quiet 2>/dev/null || echo "warning: fetch failed, comparing against local $BASE"

printf '%-42s %-38s %-8s %-9s %s\n' WORKTREE BRANCH AHEAD BEHIND VERDICT
printf '%.0s─' {1..120}; printf '\n'

while IFS= read -r wt; do
    [ "$wt" = "$ROOT" ] && continue
    total=$((total + 1))
    desc="$(git -C "$wt" status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
    branch="$(git -C "$wt" rev-parse --abbrev-ref HEAD 2>/dev/null)"
    ahead="$(git -C "$wt" rev-list --count "$BASE"..HEAD 2>/dev/null || echo '?')"
    behind="$(git -C "$wt" rev-list --count HEAD.."$BASE" 2>/dev/null || echo '?')"

    if git -C "$wt" merge-base --is-ancestor HEAD "$BASE" 2>/dev/null; then
        verdict="STALE:merged"
        stale=$((stale + 1))
    elif [ "${behind:-0}" != "0" ] && [ "${behind:-0}" != "?" ]; then
        verdict="STALE:behind"
        stale=$((stale + 1))
    else
        verdict="ok"
    fi
    [ "${desc:-0}" != "0" ] && verdict="$verdict (+${desc} dirty)"

    printf '%-42s %-38s %-8s %-9s %s\n' "${wt#$ROOT/}" "$branch" "$ahead" "$behind" "$verdict"
done < <(git -C "$ROOT" worktree list --porcelain | awk '/^worktree /{print $2}')

printf '\n%d worktree(s), %d stale\n' "$total" "$stale"
[ "$stale" -ne 0 ] && exit 1
exit 0
