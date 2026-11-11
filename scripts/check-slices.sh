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

# An enumeration that failed is not "0 stale". Outside a git repo, or if `worktree
# list` errors, the loop below simply never runs and `stale` stays 0 — which reads
# as a clean sweep when nothing was checked at all. Exit 2 so a caller can tell
# "could not look" from "looked and found nothing".
if ! git -C "$ROOT" worktree list --porcelain >/dev/null 2>&1; then
    echo "check-slices: cannot enumerate worktrees from $ROOT (not a git repo?)" >&2
    exit 2
fi
if [ "$(git -C "$ROOT" worktree list --porcelain 2>/dev/null | grep -c '^worktree ')" -eq 0 ]; then
    echo "check-slices: worktree enumeration returned nothing from $ROOT" >&2
    exit 2
fi

printf '%-42s %-38s %-8s %-9s %s\n' WORKTREE BRANCH AHEAD BEHIND VERDICT
printf '%.0s─' {1..120}; printf '\n'

while IFS= read -r wt; do
    [ "$wt" = "$ROOT" ] && continue
    total=$((total + 1))
    desc="$(git -C "$wt" status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
    branch="$(git -C "$wt" rev-parse --abbrev-ref HEAD 2>/dev/null)"
    ahead="$(git -C "$wt" rev-list --count "$BASE"..HEAD 2>/dev/null || echo '?')"
    behind="$(git -C "$wt" rev-list --count HEAD.."$BASE" 2>/dev/null || echo '?')"

    # A worktree sitting exactly on the base is healthy, not shipped. "Already merged"
    # only means a STRICT ancestor — same distinction the pre-commit hook makes.
    head_sha="$(git -C "$wt" rev-parse HEAD 2>/dev/null || echo wthead)"
    base_sha="$(git -C "$ROOT" rev-parse "$BASE" 2>/dev/null || echo base)"

    if git -C "$wt" merge-base --is-ancestor HEAD "$BASE" 2>/dev/null && [ "$head_sha" != "$base_sha" ]; then
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
