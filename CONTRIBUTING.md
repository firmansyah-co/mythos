# Contributing & Engineering Process Conventions

Conventions this repository's engineering agents and contributors are held to. These are
**process**, not product requirements — they do not belong in `specs/` and do not need the
controlled-document amendment process.

Each rule below exists because something went wrong without it. The failure is recorded with
the rule, because a rule without its reason gets discarded the first time it is inconvenient.

---

## 1. Clean up the branch and worktree in the action that closes a slice

When a pull request merges, do the cleanup **in the same action that closes the slice**. Leaving
it for later means it never happens, and stale branches and worktrees accumulate until nobody
can tell which ones are safe to touch.

1. Verify GitHub actually deleted the head branch. Do not assume the auto-delete setting is on.
2. Delete the remote branch if it still exists.
3. Remove the worktree with `git worktree remove`, then delete the local branch with
   `git branch -d`.

**Safety, non-negotiable:**

- Never touch a branch whose pull request is still open.
- Never touch `dev`, `rc` or `release`.
- Use `git branch -d`, never `-D`. The lowercase form refuses to delete an unmerged branch; the
  uppercase form is how unmerged work is lost.
- Never `rm -rf` a worktree. `git worktree remove` refuses when the tree is dirty; that refusal is
  the safety feature, not an obstacle.
- Require `git status --porcelain` to be empty before removing a worktree.

### Determining whether a PR actually merged

**Pull-request merged state is the primary signal. Commit ancestry is only a cross-check.**

Do not use "the branch tip is contained in `origin/dev`" as the merge test. That test produces
**false negatives on squash merges**, because a squash creates a new commit and the original tip
is never an ancestor of `dev`.

Verified on this repository: four branches whose PRs had genuinely merged — `docs/adr-008-drift-propagation`
(#16), `feat/faceplate-designer` (#20), `feat/faceplate-propagation` (#21) and `fix/hmi-no-data-state`
(#22) — all had tips that were **not** ancestors of `dev`.

A convention that treats ancestry as proof of merge would either strand merged work or, far worse,
be used to justify deleting a branch whose work never landed.

---

## 2. A proof must report what it reverted, not only how many tests failed

Every red-on-reintroduce / mutation proof reports three things:

1. the **reverted lines**,
2. the **failing test names**,
3. a count derived from those two.

Where the suite is wide: the names that distinguish the granularities, plus a count of the rest.

**If you keep one artefact from a mutation run, keep the names.** The names are the revert's
fingerprint. A count is a property of *how much someone reverted*, not of the change, so it cannot
be checked — only believed. On one PR here the author reported "6 red" while two reviewers measured
5 and 7 at different revert granularities; nobody concealed anything, and the two tests that stayed
**green** under the narrow revert are what reconciled three numbers for identical code.

### Companion rule: a shape-based guard must name its layer

A component-level guard cannot see a literal at the call site in the parent. State which layer the
guard sits at, or it will guard the wrong place and report green.

---

## 3. Closing a queue row: `no-follow-on` for a verdict, `handoff --to` for real work

- Delivering a **verdict or report** to the seat that owns the next step → close `no-follow-on`,
  with the PR number, timestamp and what remains open in the resolution.
- Passing **real work** to another seat → `rig queue handoff --to <seat>`, which creates the
  successor row.

`--closure-reason handed_off_to` records "delivery to the next stage", which the stuck sweep reads
as intent to create a successor row. It therefore raises a successor-verification finding even when
the successor already exists — the sweep cannot verify custody from the closure alone. So
`handed_off_to` either nags or was unnecessary, and is never the right reason for "here is my
verdict, the merge is yours".

---

## 4. Verify at dispatch, not only at filing

A row that names a symptom can go stale when another pull request merges under it. Re-read the
current ref immediately before assigning work or a severity.

Corollary: **a grep hit is not a finding.** A match may be a comment above deleted code, a name in
a doc comment, or a fixture. Open the surrounding lines and the definition before labelling
anything `LIVE` — and if the severity is `LIVE` or `CRITICAL`, record in the row what you opened to
establish reachability.
