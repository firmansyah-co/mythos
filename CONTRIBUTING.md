# Contributing & Engineering Process Conventions

Conventions every contributor to this repository, human or AI agent, is held to. These are
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

## 3. Verify a finding against the current `dev`, not only when it was found

A finding that names a symptom can go stale when another pull request merges under it. Re-read
the current `origin/dev` immediately before acting on it or giving it a severity.

Corollary: **a grep hit is not a finding.** A match may be a comment above deleted code, a name in
a doc comment, or a fixture. Open the surrounding lines and the definition before labelling
anything `LIVE` — and if the severity is `LIVE` or `CRITICAL`, record what you opened to
establish reachability.

---

## 4. The local gate's summary, and the mutation check

`./scripts/check.sh` (`--rust`, `--frontend` or the default `--all`) runs every gate and, at the
end, writes `target/check-summary.txt` and prints the same text last. It is about 30 lines: one
line per gate with PASS or FAIL, the failing test names and test binaries, the totals, the commit
and whether the tree was dirty. A gate that could not run (a tool or directory missing) is a FAIL
with its reason. When there are more failures than fit, the summary says how many it left out.
Each gate's full output is in `target/check-logs/`. Paste the summary; read the log only for the
failure you are fixing. The Rust tests run with `--no-fail-fast`, so one failing test binary does
not hide the ones after it.

`./scripts/check.sh --windows` lints the whole workspace for Windows from a machine that is not
one (`cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`, CI's
own command for the GNU target). It finds what only a Windows build warns of, such as an import
used only under `cfg(unix)`, before CI's Windows job does. It is not part of `--all`: it needs a
Rust that has the standard library of that target and the C compiler `x86_64-w64-mingw32-gcc`,
and without them it is a FAIL with the reason, never a pass. `MYTHOS_WINDOWS_TOOLCHAIN=<dir>`
makes it use the rustup installed in `<dir>` (`<dir>/rustup`, `<dir>/cargo`) instead of the Rust
on `PATH`. It builds into `target/windows-lint/`. Code under `cfg(target_env = "msvc")` is not
seen by it; CI's Windows job stays the check of that target.

`scripts/mutation-check.py SPEC.json` replaces the throwaway scripts written for rule 2. The spec
names the test command and, for each mutation, a file and exact text to replace (see the top of the
script and `scripts/examples/mutation-check.example.json`). It works on a copy of `HEAD` under
`target/mutation/`, never on your working tree. It runs the baseline, then each mutation, then the
baseline again, and prints a table of what went red, with the test names. It exits 0 only when both
baselines are green and every mutation is red. A mutation whose text is not found, that does not
compile, or that the tests let through is reported as such, never counted as red. Pass `--keep` to
keep the copy.

## 5. `dev` merges through a merge queue

Since 2026-11-22 (#181, #183) a pull request into `dev` is not merged by hand. The four required
checks stay; what went away is the rule that a branch must be up to date with `dev` before it may
merge, and with it the round of "rebase, wait for the whole suite again" for every pull request
that was open when another one merged.

**What to do.** Open the pull request, then ask GitHub to merge it when it is ready:

```bash
gh pr create --base dev --title "..." --body-file body.md
gh api graphql -f id="$(gh pr view --json id --jq .id)" -f query='
  mutation($id: ID!) { enablePullRequestAutoMerge(input: {pullRequestId: $id}) { clientMutationId } }'
```

When the required checks pass, the pull request enters the queue by itself. Nobody rebases it and
nobody presses merge. To take it out again: the mutation `dequeuePullRequest(input: {id: $id})`.
The direct call `PUT /repos/{owner}/{repo}/pulls/{n}/merge` is refused on `dev`.

**What the queue does.** It puts the pull request on top of `dev` and of whatever is ahead of it
in the queue, runs the CI workflow on that commit (the `merge_group` event), and moves `dev` to
that very commit when the required checks pass. The squash commit takes the pull request's title.

- A pull request that **contained `dev`** when it entered: the commit has the tree its own run
  tested, so the suites are skipped and it merges within about a minute. Measured on #183: 52 s
  from the last check to the merge; the queue's run took 11 s.
- A pull request that was **behind `dev`**, or behind another one in the queue: the whole suite
  runs on the new commit. Measured on #182: 13 min in the queue.
- A red run in the queue takes the pull request out of it. It stays open.

`scripts/ci-already-verified.sh` decides which of the two it is, from GitHub and never by
assumption; `scripts/test-ci-scripts.sh` tests it.

**The local hook is unchanged.** `.githooks/pre-commit` still refuses a commit from a branch that
is behind `origin/dev`. Rebase when you have a commit to make; do not rebase a pull request that
is only waiting.

**A required check must be a job with a fixed name.** A job with a `matrix` that is skipped by its
`if:` reports its name with the expression in it (`Rust Workspace on ${{ matrix.os }}`), so a
required check named after a matrix value never reports and the queue waits for it until its
timeout. This stopped the first pull request put into the queue (#182). The Windows and macOS
suites are therefore two jobs that share `.github/actions/rust-workspace-suite`.

**After a merge** the rule of section 1 holds as before: the pull request's merged state is the
signal. When the pull request contained `dev`, the tree of `origin/dev` equals the tree of its
head (`git rev-parse origin/dev^{tree} <head>^{tree}`); when it was behind, the commit on `dev` is
the one the queue's run tested, and there is no head tree to compare with.
