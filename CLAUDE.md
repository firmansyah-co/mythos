# Working in this repository

This file is committed and applies to every session, agent or human. The engineering
conventions behind these rules are in [`CONTRIBUTING.md`](CONTRIBUTING.md).

## First run on a fresh clone

Two things are **local git configuration**, not tracked files, so a fresh clone does
not have them. Run these once:

```bash
pnpm install --frozen-lockfile     # from frontend/
git config core.hooksPath .githooks  # enables the pre-commit guard
```

`.githooks/pre-commit` is committed and mode `755`, but git only looks in `.git/hooks`
unless `core.hooksPath` points elsewhere. Without the second line the hook is **inert
and silent** — it does not run, and nothing tells you. Its own header calls it a
"mechanical guard against the two failures that produced this repo's mess"; on a fresh
clone it guards neither until you opt in.

## The four rules that matter most

1. **`origin/dev` is the source of truth — not the tree you are standing in.**
   Before your first edit, check where you are:

   ```bash
   git rev-parse --abbrev-ref HEAD
   git merge-base --is-ancestor HEAD origin/dev && echo "ALREADY MERGED — stale world"
   git rev-list --count HEAD..origin/dev
   ```

   If your branch is already an ancestor of `origin/dev`, the slice is finished. Stop
   and ask; do not edit. Agents in this repo once worked in merged branches and
   produced a pull request that would have deleted 17,328 lines of `dev`.

2. **One slice = one worktree = one branch, cut from `origin/dev`.**
   What is built and what comes next is in [ADR-013](specs/adr/ADR-013-logic-compiler-and-vm.md)
   §5 and the [feature reference](docs/guides/feature-reference.md), not in memory.

3. **No claim without pasted output.** Run the gate and paste what it printed:

   ```bash
   ./scripts/check.sh              # cargo fmt/clippy/test + pnpm lint/test/build
   ./scripts/check.sh --rust       # narrower
   ```

   "Tests pass" is a guess until it appears on screen. If you did not run it, write
   "not run" and say why.

4. **Never invent data.** A missing value renders as `UNKNOWN` / `NOT BOUND` /
   `NOT MEASURED`. No mock fallbacks, no `|| 0`, no placeholder device names, IPs or
   interlock states. This is a safety HMI: an operator cannot tell a fabricated value
   from a measured one. Classify findings LIVE / LATENT / DEMO.

## Pull requests

- Base is **`dev`**. Never `rc`, never `release`.
- A PR carries one slice and a verification receipt.
- Check `git diff --stat origin/dev...HEAD` before opening one. A PR that deletes
  merged work is refused, not reviewed.
- `dev` merges through a **merge queue**: after opening the PR, enable auto-merge on it
  (the two commands are in [`CONTRIBUTING.md`](CONTRIBUTING.md) §5) and it merges by itself when
  its checks pass. Do not rebase a PR only to catch up with `dev`, and do not call the merge
  API: it is refused.
