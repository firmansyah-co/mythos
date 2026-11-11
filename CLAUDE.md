# Working in this repository

This file is committed and applies to every session, agent or human, whether or not
it is running inside the OpenRig rig. The rig's own rules live in
[`rig/culture/CULTURE.md`](rig/culture/CULTURE.md) — read it before your first edit.

## The four rules that matter most

1. **`origin/dev` is the source of truth — not the tree you are standing in.**
   Before your first edit, check where you are:

   ```bash
   git rev-parse --abbrev-ref HEAD
   git merge-base --is-ancestor HEAD origin/dev && echo "ALREADY MERGED — stale world"
   git rev-list --count HEAD..origin/dev
   ```

   If your branch is already an ancestor of `origin/dev`, the slice is finished. Stop
   and ask; do not edit. Six seats in this repo once worked in merged branches and
   produced a pull request that would have deleted 17,328 lines of `dev`.

2. **One slice = one worktree = one branch, cut from `origin/dev`.**
   The slice list is [`rig/SLICES.md`](rig/SLICES.md). Understandings come from that
   file, not memory. Inside the rig, only `dev_gatekeeper` creates or removes
   worktrees; builders and reviewers never do.

3. **No claim without pasted output.** Run the gate and paste what it printed:

   ```bash
   ./scripts/check.sh              # cargo fmt/clippy/test + pnpm lint/test/build
   ./scripts/check.sh --rust       # narrower
   ./scripts/check-slices.sh       # are any worktrees stale?
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

## The rig

`rig/mythos-org.yaml` is the team definition: 11 seats, one integration gatekeeper,
three builder/reviewer pods. `rig/agents/*/guidance/role.md` is each seat's job
description; it is delivered on fresh start and on restore, so a context reset costs
no knowledge. If you change how the team works, change those files — not a chat
message.

Launch or rebuild:

```bash
rig spec validate rig/mythos-org.yaml
rig spec preflight rig/mythos-org.yaml
rig up rig/mythos-org.yaml            # new team from this spec (archives the old one)
rig up mythos-org --existing          # resume the existing team's conversations
```
