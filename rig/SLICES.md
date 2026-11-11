# Slice ledger

The single answer to "what is being worked on". Your memory is not this file.
`pm` owns it; `dev_gatekeeper` updates branch/worktree state as it cuts and closes.

**Rules**
- One slice = one worktree = one branch, cut by `dev_gatekeeper` from `origin/dev`.
- A builder or reviewer never creates a worktree or branch.
- A worktree whose branch is an ancestor of `origin/dev` is closed, not worked in.
- A slice is `planned → open → in-review → merged`, or `dropped` with a reason.

---

## Open

| Slice | Pod | Branch | Worktree | Base | State | Owner |
|---|---|---|---|---|---|---|
| `S-001` | uiux | `test/topology-all-view-modes` | `.claude/worktrees/topology-mode-coverage` | `origin/dev` @ `5b8ee51` | `in-review` **BLOCKED** | `pod_uiux.builder` |
| `S-002` | — | `chore/rig-harness` | repo root *(pre-existing branch, not cut)* | `origin/dev` @ `5b8ee51` | `in-review` **BLOCKED** | `dev_gatekeeper` |

### S-001 — topology view-mode claims (PR #50)

- **What:** `ui-topology` states what each network view claims, in four distinct
  states, with a guard that no view renders fabricated devices.
- **Why:** PR #33 removed fabricated stations from the render path; the guard for the
  shape was never added, so the class could come back silently.
- **Done when:** the state set is honest against the spec's axes and the guard goes red
  on an injected fabricated device.
- **Verification:** `pnpm --filter @mythos/ui-topology test` in the slice worktree.
  `./scripts/check.sh` is **not runnable there** — see blocker B2.
- **Head:** `52aa7f5`, one commit on `origin/dev`.
- **Open question carried from review — ANSWERED 2026-11-11.** `UNREACHABLE` is a
  health claim, not a provenance claim. The spec already publishes both axes, so this
  is not a new design:
  - `docs/04-hmi-and-ui/network-topology-view-spec.md:203` — *"Real-time comparison
    between **Nominal Topology** (engineered in MythOS) and **Actual Topology**
    (discovered via LLDP)"* → the source axis (CONFIGURED / DISCOVERED).
  - `:300` — `linkStatus: 'UP' | 'DOWN' | 'DEGRADED'` on NetworkPort → the health axis.
  - `:345` — `health.status: 'OPTIMAL' | 'WARNING' | 'CRITICAL' | 'OFFLINE'` on
    TopologyCableEdge → the health axis, per cable.
  - `:168` — *"Topology Status: [ OK All Links Match Nominal Design ]"* → conformance,
    a **derived** claim, not an input state.

  So the four-value enum mixes three axes, and `DISCOVERED_UNCOMMISSIONED` also fuses a
  second property (commissioned vs candidate) into a source value. Two axes plus a
  derived conformance claim is the correct shape. **This is a review finding for the
  builder to implement, not a merge waiver.** Verified against `origin/dev` by `pm`;
  sourced from `governance.scribe` `qitem-20261111052325`.
- **Remote state:** PR #50 open, base `dev`, `mergeable: MERGEABLE`,
  `mergeStateStatus: BLOCKED` (required checks unsatisfiable — blocker B1).

**Blockers — neither is a code defect**

- **B1 · CI cannot run at all.** All three checks on PR #50 are `CANCELLED` after
  queueing exactly 24h with `runner_name: ""` and `steps: []` — no job ever reached a
  runner. `gh api repos/.../actions/runners` → `total_count: 0`; org runners → `0`;
  org runner group `Default` → `0`. No workflow run has *started* since
  `2026-10-27T12:28Z` (15 days). Every workflow in this repo requires
  `runs-on: arc-runner-org`, and `dev` requires both CI checks with `strict=true`, so
  **no slice can merge on any branch until ARC is restored**. Owner: `advisor` → Adrian.
  Not diagnosable from this host: the ARC cluster is not local, `kubectl` has no
  context, no `docker`, no runner process.
- **B2 · The verification gate is not on `origin/dev`.** `scripts/check.sh`,
  `scripts/check-slices.sh`, `rig/SLICES.md` and `rig/culture/CULTURE.md` exist only on
  `chore/rig-harness` (**4** commits ahead of `origin/dev`, unmerged). Every slice worktree
  is cut from `origin/dev`, so no builder or reviewer can run the command the ledger
  names. `scripts/check.sh` itself is a good gate — it just is not in the tree the
  slices stand in. Owner: `dev_gatekeeper` (land `chore/rig-harness` as a PR to `dev`);
  tracked as slice **S-002** below.

### S-002 — land the rig harness on `dev` (fixes blocker B2)

- **What:** `chore/rig-harness` → `dev`: `rig/mythos-org.yaml`, `rig/agents/**`,
  `rig/culture/CULTURE.md`, `rig/SLICES.md`, `rig/handover/queue-rows.md`, `CLAUDE.md`,
  `scripts/check.sh`, `scripts/check-slices.sh`, `.githooks/pre-commit`, and a `.gitignore`
  block for rig runtime state.
- **Why:** every slice worktree is cut from `origin/dev`, so the culture file, the ledger
  and the gate script the ledger names are absent from the tree builders stand in. Seats
  are already improvising verification (`pod_backend.builder` runs cargo steps directly
  and says so). A gate nobody can run is not a gate.
- **Shape:** additive only — 30 files added, 1 modified (`.gitignore`), **0 deletions**.
  Verified with `git diff --numstat origin/dev...chore/rig-harness`.
- **Done when:** merged to `dev` and a freshly cut worktree contains `scripts/check.sh`,
  `rig/SLICES.md` and `CLAUDE.md`.
- **Verification:** `rig spec validate` + `rig spec preflight` clean; `bash -n` on both
  scripts; `.gitignore` change hides no tracked file; full `./scripts/check.sh` run from
  the branch (receipt in the PR body).
- **Remote state:** branch **not yet pushed**; PR opens when the gate is green.

  > `chore/rig-harness` predates this rig and is checked out in the **repo root**, not a
  > slice worktree. I am not re-cutting it: the work already exists on that branch, and
  > `git worktree add` cannot check out a branch that is already checked out. The
  > one-slice-one-worktree rule is satisfied in substance (one branch, one working tree,
  > no sharing) and the deviation is recorded here rather than hidden.

---

## Documentation rows raised by `governance.scribe` (2026-11-11)

From `qitem-20261111052325`. Each is spec drift, not code work; `scribe` owns them.
None is assigned to a pod and none blocks S-001.

| Row | Target | What |
|---|---|---|
| SCRIBE-1 | `docs/04-hmi-and-ui/network-topology-view-spec.md:201` | "Automated Device Replacement" says the neighbour "automatically assigns the configured PROFINET device name, and downloads its hardware parameters". That is only sound because the nominal config already names the slot — a precondition the spec never states. Read without it, the APPROVED sentence licenses the auto-provisioning that `NetworkCanvas.tsx:74` and `provenance.tsx:21` deliberately refuse. ADR-004 `:20`/`:74` describe discovery-for-a-human, not unattended provisioning. Fix: state the precondition, or mark the sentence unverified. |
| SCRIBE-2 | `…spec.md` §8 `:271-366` | The TypeScript contract section has no `provenance` field on `NetworkCanvasProps`, `DevicesAndNetworksState` or `HardwareStationNode`. Stale the moment #50 merges. Needs a contract addition, not a prose note. |
| SCRIBE-3 | `specs/adr/` | ADR candidate: *"Discovery yields candidates; commissioning is an explicit act."* A real architectural decision with a live alternative (auto-provision on discovery, which `:201` reads as approved), currently recorded only in source comments. Cheap now, expensive to reconstruct after the transport slice. |
| SCRIBE-4 | vocabulary | `TopologyProvenance` (`ui-topology/src/provenance.tsx:29`) collides in name with `ProcessDataQuality` (`ui-faceplate-lib/src/types.ts:28`), which this repo already describes as provenance. Two axes, so two names is defensible — but name it before a third slice imports both and infers they agree. Same class as the `InterlockCondition` smell below. Record, do not block. |

## Worktree state — 2026-11-11

`./scripts/check-slices.sh` → 5 worktrees, 2 stale:

```
ci-frontend-gate         ci/frontend-gate                  7 ahead  144 behind  STALE:behind
dev-live                 HEAD                              0        0           ok
rev-uiux-50              HEAD                              1        0           ok (+1 dirty)
topology-mode-coverage   test/topology-all-view-modes      1        0           ok (+2 dirty)
ui-isa101-consolidation  feat/ui-isa101-consolidation      3 ahead  146 behind  STALE:behind (+4 dirty)
```

`ci/frontend-gate` is now 144 commits behind and its content (the frontend gate job)
**already exists on `origin/dev`** as `frontend-tests` (`ci.yml:286`), so the branch has
nothing left to contribute — it is a delete candidate, not a rebase candidate. The ledger
line for it below ("Rebase and slice") is superseded by this. `dev-live` sits exactly on
`origin/dev` and is the autospin build worktree: **healthy, not stale.** The naive
`merge-base --is-ancestor HEAD origin/dev` check in the role description flags it falsely;
`check-slices.sh` distinguishes strict-ancestor from equal-to-base and is the correct check.

## Carried over from the previous rig (open queue rows)

`pm` decides which of these become slices. They are recorded so the work is not lost;
none of them is assigned.

| Origin qitem | Pod | State | What |
|---|---|---|---|
| `qitem-20261023085422` | uiux | blocked | HMI canvas has no live-telemetry path; every faceplate reads NOT BOUND on a real plant. **Needs a scope decision** (advisor gate). |
| `qitem-20261026053023` | hmi | blocked | Operator-writable setpoints vs telemetry re-sync: needs a draft-vs-committed rule before coding. |
| `qitem-20261026063940` | uiux | blocked | Two same-named `InterlockCondition` types in `ui-faceplate-lib` and `ui-inspector`. Naming smell, not a wrong import. |
| `qitem-20261027083546` | backend | blocked | CLI `--version` pinning and `versions --remote`; blocked on the release path producing more than one version. Rerouted from the removed `publisher` seat. |

## Removed from the ledger

| Row | Why |
|---|---|
| `qitem-20261110082629` (worktree sweep) | Executed 2026-11-11: 13 merged worktrees and 8 merged branches removed, 19 → 6 worktrees. Both "dirty" worktrees were already shipped in `origin/dev` (diffs empty), so no review finding was lost. |
| `qitem-recovery-77ae3bf3` | Synthetic stuck-sweep row for the row above; closed with it. |

## Closed

| Slice | Branch | PR | Merged at | Note |
|---|---|---|---|---|
| — | `feat/uiux-shell` | #15 | in `dev` | Superseded; worktree retained by mistake. |
| — | `feat/backend-core` | — | in `dev` | Worktree retained by mistake. |
| — | `fix/sweep-content-default-order` | #47 | in `dev` | |
| — | `test/faceplate-dirty-gate-clears` | #44 | in `dev` | |
| — | `perf/ci-wall-time` | #54 | in `dev` | |
| — | `feat/channel-identity` | #53 | in `dev` | |
| — | `ci/frontend-gate` | — | **not merged, superseded** | 7 commits, `ci.yml` +90 lines — but the frontend gate job it adds is already on `dev` as `frontend-tests` (`ci.yml:286`). Nothing left to land; **delete candidate**, do not rebase. |
| — | `docs/spi-sections-4-7-rewrite` | — | **not merged** | 2 commits on top of a base 102 commits behind `dev`. Rebase and slice. |
| — | `feat/ui-isa101-consolidation` | — | **not merged, do not merge** | Based on a stale point; merging as-is deletes 17,328 lines of `dev`. Salvage by hand or drop. |

---

## Deleted with the previous rig

14 seat conversations from the old `mythos-org` (archived on the rebuild) are not
resumed. Their open work is the table above; anything not in this file is gone.
