# MythOS rig culture

This file is delivered to every seat on fresh start **and** on restore. It is the
part of your job description that survives a context reset. If something here is
wrong or missing, say so to `pm` — do not quietly work around it, because the next
occupant of your seat will inherit the workaround and not the reason.

Read `rig/SLICES.md` for the current slice, its worktree, branch and owner. That
file, not your memory, is the answer to "what am I working on".

---

## 1. The source of truth is `origin/dev`

Every claim about "the current code" means `origin/dev` at the moment you check it,
not the tree you happen to be standing in.

Before your first edit in a session, and again before you claim anything is done:

```bash
git rev-parse --abbrev-ref HEAD
git rev-parse --short HEAD
git merge-base --is-ancestor HEAD origin/dev && echo "ALREADY MERGED — your world is stale"
git rev-list --count HEAD..origin/dev
```

- **Already merged** → stop. Your branch is history. Do not edit. Tell `pm`.
- **Behind** → stop and tell `pm`. Do not rebase your way into a different slice.
- **Ahead by a sane slice-sized amount** → proceed.

A branch that is an ancestor of `origin/dev` is a finished slice, not a workspace.
Six seats in this rig once worked on merged branches and produced a 17,328-line
deletion. That is the failure this rule exists for.

## 2. Worktrees belong to `dev_gatekeeper`

- **Only `dev_gatekeeper` runs `git worktree add` or `git worktree remove`.**
- Builders and reviewers never create a worktree, never create a branch, never
  `git checkout -b`. If you think you need one, ask `dev_gatekeeper`.
- One slice = one worktree = one branch. Three seats sharing one tree is how PRs
  that nobody can explain get made.
- When a slice is closed, its worktree is removed. A worktree older than its slice
  is a defect, not a convenience.

If your `cwd` no longer matches the slice in `rig/SLICES.md`, the rig is wrong and
that is a `pm` escalation, not something to fix by editing in the wrong place.

## 3. Closing a slice hands over the seat

A slice that ships is a slice whose builder and reviewer have left. Their remaining
context is about a world that no longer exists, and every hour it lives costs the
next slice accuracy.

- At slice close, `dev_gatekeeper` runs:
  `rig seat handover <seat> --source rebuild --reason "slice <id> closed"`
- Never hand over a seat that owns open work. Close or hand off the qitem first.
- `rebuild` is the default. `--fresh` only when the role itself changed.
- A seat's running context is measured, not guessed. If it is near the wall,
  close the slice you are on rather than starting another one.

## 4. No claim without pasted output

"Tests pass" is not a claim, it is a guess until it appears on the screen.

```bash
./scripts/check.sh          # or the slice's narrower judge, named in SLICES.md
```

Paste the command and its real output into your handoff. If you did not run it,
say "not run" and say why. A green summary that was never executed is worse than
an honest red, because it moves the defect downstream to someone who cannot see it.

**A receipt is invalid if another build was live in the same tree.** `dist/` and
`*.tsbuildinfo` are shared state, not per-run state: two concurrent `pnpm build` runs
collide and produce `TS6305` ("Output file … has not been built from source file …") on
a tree that is byte-identical to `origin/dev`. Observed 2026-11-11, when a full gate and
a `--frontend` run overlapped in the repo root and the full gate reported a red the
branch could not have caused. Before you run a gate, check whether someone else is
building there. If a red looks impossible, compare tree hashes against the base before
believing it:

```bash
git rev-parse <sha>:frontend origin/dev:frontend   # identical → not your branch
```

The repo root is one tree every seat can see, but only `dev_gatekeeper` builds in it.

## 5. Never invent data

An operator looking at a fabricated value cannot tell it is fabricated. This is a
safety system; the failure is not cosmetic.

- A missing measurement renders as `UNKNOWN`, `NOT BOUND` or `NOT MEASURED`.
- No mock fallbacks, no sample values, no `|| 0`, no placeholder identifiers, no
  invented device names or IP addresses.
- If a surface has no live data path, that is the finding. Report it; do not paper
  over it so the screen looks complete.
- Classify every fabricated-data finding as **LIVE** (reachable by an operator now),
  **LATENT** (reachable only on a path not yet wired) or **DEMO** (a declared demo
  dataset). A planted demo must not be reported at the severity of a live E-stop.

## 6. Pull requests and merging

| Action | Only these seats |
|---|---|
| `git push` a slice branch | `dev_gatekeeper` |
| `gh pr create` | `dev_gatekeeper` |
| `gh pr merge` | `dev_gatekeeper` |
| merge `dev` → `rc` → `release` | `release_auditor` |

- PR base is **`dev`**. Never `rc`, never `release`.
- A PR with a commit that deletes other people's merged work is refused, not
  reviewed. Check `git diff --stat origin/dev...HEAD` before opening one.
- A PR is not a slice. It carries one slice, with a stated verification receipt.

## 7. Scope: the dog and the doghouse

Before shaping work, know who wants the outcome, what it is for, and what counts as
done. Two failure shapes, both of which lose the plot:

- Over-building: every step locally defensible, and an hour later there is a moon
  base while the dog waits outside. Ask: **does the thing actually need this?**
- Under-asking: exactly the doghouse requested, tidy and on time, with a door the
  dog cannot fit through. Ask: **how big is the dog?**

If the answer is not in the evidence, it is a question for Adrian — routed through
`advisor` as a `--gate human` qitem, with the options, the trade-offs and your
recommendation. A question the evidence settles is not a gate; decide it and say
what evidence settled it.

## 8. Work moves only through the queue

A message informs. A queue row is owned work. Do not start work you were told about
in passing until someone owns it.

- Every qitem has one owner and one closure reason.
- `handed_off_to` means delivered to the next stage, not accepted by it.
- Do not leave a row claimed while you wait on someone. Block it with a continuation.
- Do not close a row you did not verify.

## 9. Say when you are wrong

If you find that an earlier claim of yours was false — a test that could not fail, a
verdict issued without running the probe, a fabricated value you shipped — say so
plainly and fix the record. The queue and the docs are the team's memory; a clean
memory that is wrong costs more than a noisy one that is right.
