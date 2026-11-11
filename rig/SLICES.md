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
| `S-002` | — | `chore/rig-harness` | repo root *(pre-existing branch, not cut)* | `origin/dev` @ `5b8ee51` | `in-review` **BLOCKED** (PR #55) | `dev_gatekeeper` |
| `S-003` | backend | `refactor/device-execution-mode-canonical` | `.claude/worktrees/device-execution-mode` | `origin/dev` @ `5b8ee51` | `in-review` **BLOCKED** (PR #57) | `dev_gatekeeper` |
| `S-004` | uiux | *(cut after S-001)* | — | `origin/dev` | `planned` | `pod_uiux.builder` |
| `S-005` | — | `policy/decision-defaults` *(exists @ `4f2ac4b`)* | *(not cut — see note)* | `origin/dev` | `planned` **blocked on S-002** | `dev_gatekeeper` |
| `S-006` | — | `feat/desk-seat` *(exists @ `3995106`; main copy `4496ce0`)* | *(not cut — see note)* | `origin/dev` | `planned` **blocked on S-002** | `dev_gatekeeper` |

### S-001 — topology view-mode claims (PR #50)

- **What:** `ui-topology` states what each network view claims, in four distinct
  states, with a guard that no view renders fabricated devices.
- **Why:** PR #33 removed fabricated stations from the render path; the guard for the
  shape was never added, so the class could come back silently.
- **Done when:** the state set is honest against the spec's axes and the guard goes red
  on an injected fabricated device.
- **Verification:** `cd frontend && npx vitest run --no-cache` in the slice worktree.
  **Not** `pnpm --filter @mythos/ui-topology test`: a reviewer's first run of that
  reported 2 of 5 tests in `noFabricatedDevices.test.tsx` and PASSED — a stale
  transform cache, not skips. `--no-cache` runs all 5. A reviewer who trusts the
  filtered command can sign off a file where three of five tests never ran. The
  ui-shell sweep guard must also run from `frontend/`, because its ROOT resolves off
  cwd and it fails ENOENT on ui-logic-editor paths otherwise.
  `./scripts/check.sh` is **not runnable there** — see blocker B2.
- **Head:** `52aa7f5`, one commit on `origin/dev`. Builder has begun the two-axis
  split in the worktree (3 files modified, uncommitted).
- **Review verdict 2026-11-11: CHANGES REQUESTED, 4 items** — `qitem-20261111053255-6f321ed4`
  to `pod_uiux.builder`. CI recorded as UNAVAILABLE, never as passing. Local gate on
  `52aa7f5`: lint exit 0, 48 files / 369 tests passed, build exit 0.
  Two findings proven by execution, not reading:
  - **The "visibly distinct" guard cannot fail for its stated defect.** It compares the
    tone *constant* (`presentation.tone`), never the rendered class. The reviewer made
    `TONE_CLASSES.warning` byte-identical to `TONE_CLASSES.alarm`; the suite stayed
    5/5 GREEN while `DISCOVERED_UNCOMMISSIONED` and `UNREACHABLE` render the same CSS.
    Verified by `pm` at `noFabricatedDevices.test.tsx:124-125` — the assertion is
    `expect(presentation.tone).not.toBe(other.tone)`, on the enum member, not the DOM.
  - **The banner defaults to `UNKNOWN`.** Render a canvas WITH real stations and no
    provenance prop: it draws the device while the banner says "nothing has been read …
    these views make no claim about the plant." A surface contradicting itself — the
    same class as the invented rack.
- **Reviewer's addition to the two-axis case:** `NetworkCanvas` already carries a
  per-device health signal — `IndustrialStationNodeData.station.status`
  (`RUN|FAULT|STOP`), consumed by `nodeColor` (`NetworkCanvas.tsx:297`). Health is
  already modelled per device; the four-value enum puts it on the view as well. That
  strengthens the split beyond the spec's authority alone.
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
  runner. No workflow run has *started* since `2026-10-27T12:28Z` (15 days). Every
  workflow in this repo requires `runs-on: arc-runner-org`, and `dev` requires both CI
  checks with `strict=true`, so **no slice can merge on any branch until ARC is
  restored**.

  **Cause, measured by `advisor` (correcting `pm`'s first report).** ARC is neither
  remote nor gone: k3s runs on this host and the ARC controller is alive. The listener
  pod `arc-systems/arc-runner-org-6b8f777f-listener` is **restart-looping — 2,712 pod
  starts** in the k3s journal since 06:46 WIB today. *Why* it exits is **NOT MEASURED**:
  pod logs and the kubeconfig are root-only and Adrian declined a sudo probe from the
  advisor's seat. Do not retry sudo from another seat.

  **A correction to `pm`'s own evidence.** `gh api .../actions/runners` →
  `total_count: 0` is **not proof on its own**: scale-set runners are ephemeral and
  register only while a job runs. The proof is the 24h queue with empty `runner_name`
  and zero steps, plus the listener restart-loop. The earlier "no runner is registered"
  reading was an overstatement of what that call shows.
  Owner: `advisor` → Adrian (`qitem-20261111052721-b32df883eeb54ab0`; `pm`'s
  `qitem-20261111052947-4cf2c1bd2c7ae528` superseded into it, not abandoned).
  **Decided, not a gate:** no change to `dev` branch protection, no repoint of
  `runs-on`, merges hold until a run executes on `arc-runner-org`. Review verdicts,
  local gates and S-002 authoring continue. **Do not push commits to PR #50 to shake CI
  loose** — nothing is scheduling.
- **B2 · The verification gate is not on `origin/dev`.** `scripts/check.sh`,
  `scripts/check-slices.sh`, `rig/SLICES.md` and `rig/culture/CULTURE.md` exist only on
  `chore/rig-harness` (ahead of `origin/dev`, unmerged; head `238c5b8`). Every slice worktree
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
- **Remote state:** branch **not yet pushed**; PR opens when the gate is green. Head
  `238c5b8`. Deliberately **no commit count** here: it changed twice while this row was
  being written, and a number that self-stales teaches readers to distrust the rest of
  the row. Cite the SHA.

  > `chore/rig-harness` predates this rig and is checked out in the **repo root**, not a
  > slice worktree. I am not re-cutting it: the work already exists on that branch, and
  > `git worktree add` cannot check out a branch that is already checked out. The
  > one-slice-one-worktree rule is satisfied in substance (one branch, one working tree,
  > no sharing) and the deviation is recorded here rather than hidden.
  >
  > **HAZARD while this row is in review (raised by `pod_backend.builder`):** the repo
  > root is also the primary cwd of every seat in this rig. Nothing mechanical stops a
  > seat from editing a file in its cwd and thereby dirtying S-002's slice. Until S-002
  > merges, **no seat writes to the repo root**; a seat whose cwd is the repo root owns no
  > slice, and `CULTURE.md` §2 already requires it to escalate rather than edit. This
  > paragraph is that instruction, made explicit.

### S-003 — canonical `DeviceExecutionMode` ownership

- **What:** `mythos-strucpp` adopts `mythos_core::DeviceExecutionMode` and deletes its own
  duplicate definition.
- **Why:** `specs/tasks.md:225`/`:241` name canonical ownership as a precondition on TSK-801
  (Sprint 8, the pre-committed next slice). The duplication is live and the wire forms
  diverge: core's enum carries `#[serde(rename_all = "snake_case")]` + `Default`
  (`mythos-core/src/device/mode.rs:30`); strucpp's carries neither
  (`mythos-strucpp/src/quality_gate.rs:15`), so the first variant serialises as
  `"physical_hardware"` versus `"PhysicalHardware"`.
- **Verification of the citation:** `git show origin/dev:crates/mythos-strucpp/src/quality_gate.rs`
  — `:15` is `pub enum DeviceExecutionMode`; `:14` is the derive line, which has no
  `rename_all`. Checked by `dev_gatekeeper` at cut time, not carried from the report.
- **Decision:** option A, decided by `executive-advisor` 2026-11-11 on
  `qitem-20261111053553-2c552e0be9d571fd`, not escalated to Adrian — crate ownership of a
  type is a delegated technical call. Principle recorded: **for safety-relevant state, one
  definition**; agreeing only on the wire (option C) is rejected on principle, not cost.
- **Done when:**
  1. `mythos-strucpp/src/lib.rs` re-exports core's type, so the existing public path
     (`mythos_strucpp::quality_gate::DeviceExecutionMode` and the `lib.rs:22-25` re-export)
     still compiles unchanged for every consumer.
  2. strucpp's own `enum DeviceExecutionMode` is **deleted**, not left behind. No orphan
     definition, no shim alias that keeps two types alive.
  3. A wire-form test pins `physical_hardware` and `manual_simulation`. Nothing asserts the
     wire form today — repo-wide grep for both strings over `crates/` returns zero hits, so
     the test is genuinely missing rather than merely untested.
  4. A test proves it is **one type**: a `mythos_core` value passed into `QualityGate` with
     no conversion.
  5. `Default` stays `PhysicalHardware`.
- **Verification:** `cargo test -p mythos-strucpp` in the slice worktree, plus
  `cargo clippy -p mythos-strucpp --all-targets -- -D warnings`. Report **failing test
  names, not counts** — a count is a function of how much was reverted.
  `./scripts/check.sh` is not runnable from a dev-cut worktree until S-002 lands; say the
  gate script was absent rather than substituting silence.
- **Sequencing:** ahead of the device-mode half of TSK-801, as advised. **It does not
  block the telemetry half** — the tag-to-faceplate proof has no dependency on this.
- **Out of scope:** `specs/tasks.md`'s ACCEPTED DUPLICATION note. That is a controlled
  document; `scribe` retires it after this lands, so the note is retired by the change that
  makes it false rather than in advance of it.
- **State:** `in-review` — **PR #57**, head `7d4798c`, +126/−9, 2 files,
  `mergeStateStatus: BLOCKED` (no runner). Verified by `dev_gatekeeper`: one
  `enum DeviceExecutionMode` left repo-wide, no alias/newtype, 27 tests pass, forced clippy
  re-check clean, fmt clean, both mutations reproduce. Branch `refactor/device-execution-mode-canonical`, worktree
  `.claude/worktrees/device-execution-mode`, cut by `dev_gatekeeper` from `origin/dev` @
  `5b8ee51` on 2026-11-11. Owner `pod_backend.builder`.

### S-004 — HMI live-telemetry consumer (telemetry half of TSK-801)

- **What:** Prove one real TagBus value reaches one HMI faceplate. Consume the existing
  `GET /api/v1/telemetry/stream` producer, resolve one commissioned tag to one faceplate,
  map `QualityCode` to the faceplate quality enum, survive reconnect with bounded backoff,
  ship a live-path test.
- **Why:** `qitem-20261023085422-f459118d847f001d`. The HMI tells the truth today and shows
  nothing, on every screen, forever. This is the pre-committed next slice
  (`specs/tasks.md`, Sprint 8 / TSK-801) and the only thing that makes the HMI demonstrably
  work at all.
- **Scope limit, from the spec:** ONE tag to ONE faceplate. Broader subscription and binding
  architecture is follow-up. Do not expand past that proof.
- **Constraints:** do not change `resolveComponentData`'s contract or the faceplate quality
  contract; do not edit `DockviewWorkspace.tsx` tab routing; no mock data on the path.
- **Two known blockers for quality visibility, both outside the UI** (recorded in TSK-801,
  both need backend work — do not paper over them on the frontend):
  1. `QualityGate::record_comm_loss` latches in a process-local map and does not write
     `TagSlot.quality`, so the failure never reaches shared memory.
  2. `build_telemetry_event` collapses quality to `GOOD`/`BAD`/`UNCERTAIN`, so a faceplate
     cannot distinguish `BAD_COMM_FAILURE` from any other bad state unless the payload
     carries the raw quality word.
- **Done when:** one integration test shows a tag published to the TagBus arriving at a
  rendered faceplate with GOOD quality and the expected value, end to end; and a negative
  test drops the stream mid-render and asserts the faceplate degrades to BAD_QUALITY —
  never to a stale value presented as live, never to a fabricated one (SYS-REQ-096).
- **Verification:** `cd frontend && npx vitest run --no-cache`, run from `frontend/` — never
  from a worktree root (see CULTURE.md §4). Report failing test names, not counts.
- **Sequencing:** after **S-001** closes. Same pod, and S-001 is the uiux slice in review.
  S-004 does not depend on S-003; the device-mode half does.
- **State:** `planned`.

### S-005 — the operator's decision-defaults policy

- **What:** `rig/culture/DECISION-DEFAULTS.md` (112 lines), the `mythos-org.yaml` delivery
  entry that ships it to every seat, and `CULTURE.md` §10–§11.
- **Why:** Adrian's standing instruction: when a question goes unanswered, execute the recorded
  recommendation rather than stalling the team.
- **Provenance — verified, not asserted.** `dev_gatekeeper` re-read Adrian's own CLI session
  rather than accepting two seats' reports:
  `/home/adrian/.claude/projects/-home-adrian/7993c305-0d9e-493b-9811-e36ce18f0152.jsonl`,
  line 4037, ts `2026-11-11T05:36:35.779Z`, `userType: external`, `entrypoint: cli`:
  *"kalau ada pertanyaan aku mau advisor memicu question bisa? tapi kalau misalnya lama ga
  kujawab dia jalankan sesuai rekomendasinya secara otomatis"*. Recorded answers at
  `05:40:51.404Z`: **stacking several questions; 24h limit; 2h idle when no work remains;
  everything except the `release` branch, `rc` still allowed.**
- **What is his and what was drafted for him.** His words cover stacking, 24h, 2h idle,
  everything-except-release and rc-allowed. Added by the drafting agent and **not** in his
  answers: the release-tag and published-artifact exclusions; the force-push and
  deleting-another-seat's-work exclusions; sections 4–6; and CULTURE §11. Every addition narrows
  or disciplines the grant; none widens it. The PR body states this split, and the file carries a
  provenance line: substance his, drafted by his assistant, because first-person voice is not
  authorship.
- **Watchdog correction:** the three watchdogs were registered by **Adrian's own session**, at
  `05:38:25` / `05:41:50` / `05:41:56`, passing `--registered-by executive-advisor@mythos-org`.
  The commit message's "Advisor watchdogs registered against this" reads that string as
  authorship; it is a misleading audit field, not a claim of who ran the command.
- **Review:** assigned by `pm` to `pod_uiux.reviewer`,
  `qitem-20261111055057-9a6593dcbeccfcf7`, read-only, verdict as a comment. Neither `advisor`
  nor `dev_gatekeeper` reviews it: the policy widens what both may do unasked, so neither may
  approve it. Any clause that **widens** the grant rather than narrowing it is blocking.
- **Sequencing — why it is not cut yet.** `rig/culture/CULTURE.md` does not exist on
  `origin/dev`; S-002 adds it. Sections 10–11 are additions to that same file, so an S-005 cut
  from `origin/dev` today would add `CULTURE.md` in full and collide with PR #55 by
  construction. S-005 therefore follows S-002, at which point its diff is just the policy file,
  the yaml entry and §10–§11. A worktree was cut to confirm this and removed again — an empty
  worktree ahead of its slice is a defect, not a placeholder. The work is preserved on branch
  `policy/decision-defaults` at `4f2ac4b`.
- **Force:** no running seat has the file in its startup files yet, so the policy has no
  operational force anywhere until it merges and seats are restored. That is the correct state
  for it to be reviewed in.

### S-006 — the desk seat (waits for S-002)

- **What:** a twelfth seat, `rig/agents/desk/` (agent.yaml + guidance/role.md) and
  `rig/pods/desk.yaml` ("Adrian's Desk — presents every open human decision as a clickable
  question and records the answer back onto its queue row. Decides nothing"), plus the
  `mythos-org.yaml` wiring.
- **Provenance — verified, not asserted.** Adrian's own CLI session
  `7993c305-0d9e-493b-9811-e36ce18f0152`, `userType: external`, request at
  `2026-11-11T05:45:32.699Z`: *"aku suka mekanisme claude questions (pilihan ganda
  selectible)… gimana caranya biar aku bisa lihat dan jawab semua pertanyaan yang datang ke
  aku tanpa harus cari-cari lagi?"* — the desk seat is his own build answering his own
  request. Authored `adrianaryaputra`, `4496ce0`, 2026-11-11 12:52 +0700.
- **Why it is not cut yet:** it modifies `rig/mythos-org.yaml`, which does not exist on
  `origin/dev` — S-002 adds it. Same ordering constraint as S-005. It is also separable from
  S-005 even though both arrived inside the single commit `4496ce0`: one commit containing two
  changes does not make them one slice.
- **Three things for the review, found by `dev_gatekeeper` reading the files:**
  1. **`rig/agents/desk/agent.yaml:7` declares `model:` with an EMPTY value.** Every one of
     the eleven existing seats declares a model (`claude-opus-5.5`, or `claude-fable-5.2` for
     `advisor` and `release-auditor`). A seat with no model is not launchable as written.
     `rig spec validate` accepted the spec, so this is the gap between "well-formed" and
     "correct".
  2. **`rig/pods/desk.yaml` has `edges: []`** — the pod is wired to nothing. A seat that
     collects `gate:human` rows but is not on a delivery edge would collect nothing at all.
     Confirm that is intentional before gating.
  3. **Is a twelfth seat wanted?** `rig spec validate` and preflight check form, not desire.
     Adding a seat and a pod is a topology decision and should be the review's actual question.
- **State:** `planned`, blocked on S-002 for the same reason as S-005. Work preserved at
  `4496ce0` (on local `chore/rig-harness`) and `3995106` (`feat/desk-seat`, cherry-picked onto
  `origin/dev`). Neither is pushed.

### Sequencing (decided by `pm`, 2026-11-11)

1. **S-001** (uiux, in review) closes first — same pod as S-004.
2. **S-004** is cut after S-001.
3. **S-003** (backend) is independent and is cut now.
4. The **device-mode half** of TSK-801 waits on S-003 and is not yet a slice.

The HMI telemetry row `qitem-20261023085422` is `pending`, unclaimed, `blockedOn: null` —
its block released when the DeviceExecutionMode decision landed. It is carried by **S-004**;
it is not free work.

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
| `qitem-20261023085422` | uiux | blocked — **premises corrected** | HMI canvas has no live-telemetry path; every faceplate reads NOT BOUND on a real plant. **The gap is real; two premises in the row were stale.** See below. |
| `qitem-20261026053023` | hmi | blocked | Operator-writable setpoints vs telemetry re-sync: needs a draft-vs-committed rule before coding. |
| `qitem-20261026063940` | uiux | blocked | Two same-named `InterlockCondition` types in `ui-faceplate-lib` and `ui-inspector`. Naming smell, not a wrong import. |
| `qitem-20261027083546` | backend | blocked | CLI `--version` pinning and `versions --remote`; blocked on the release path producing more than one version. Rerouted from the removed `publisher` seat. |

### The HMI telemetry gap — corrected 2026-11-11

**Still true:** no live process values reach an HMI faceplate. At `origin/dev` the only
`EventSource` in `ui-shell` is `useFaceplateStore.ts:277`, and it listens for
`FACEPLATE_PROPAGATED` only — a library re-read, carrying no process values.
`resolveComponentData` is called at `DockviewWorkspace.tsx:423` but nothing populates
`comp.live` from the TagBus. There is no telemetry value store (`stores/` is
`useFaceplateStore` and `useHmiStore`).

**No longer true — "no task in the roadmap covers it".** `TSK-801` exists in
`specs/tasks.md` under *Sprint 8: Live Telemetry Cutover & Device-Mode Completion*,
marked PRE-COMMITTED PREREQUISITE, and scopes exactly this — including two code-verified
quality blockers: `QualityGate::record_comm_loss` does not write `TagSlot.quality`
(`mythos-strucpp/src/quality_gate.rs`), and `build_telemetry_event` collapses quality to
`GOOD`/`BAD`/`UNCERTAIN` (`mythos-core/src/api/telemetry.rs`).

**No longer true — "needs a scope decision".** It is scoped. The scope limit is explicit:
prove **one** real tag-to-faceplate path; broader subscription and binding architecture is
follow-up.

**The real blocker, replacing `external:sprint-8-open`:** the canonical ownership of
`DeviceExecutionMode`, which `specs/tasks.md:225` and `:241` name as a precondition on
TSK-801 and explicitly decline to resolve. It was **untracked until 2026-11-11** — raised
as `qitem-20261111053553-2c552e0be9d571fd` to `advisor` → Adrian, recommendation A.
**Resolved 2026-11-11:** `executive-advisor` decided **option A** — `mythos-strucpp` adopts
`mythos_core::DeviceExecutionMode` and deletes its own. Not escalated to Adrian; crate
ownership of a type is a delegated technical call. Principle recorded: **for safety-relevant
state, one definition**; agreeing only on the wire (option C) is rejected on principle, not
on cost. The work is slice **S-003**, cut from `origin/dev`.

The duplication is live and the wire forms genuinely diverge:
`mythos-core/src/device/mode.rs:30` carries `#[serde(rename_all = "snake_case")]` +
`Default`; `mythos-strucpp/src/quality_gate.rs:15` carries neither, so the first variant
serialises as `"physical_hardware"` versus `"PhysicalHardware"`. `pm` checked whether the
migration cost the spec warns about is real today: it is **not** — strucpp's enum is used
only in memory (`RwLock` field, equality, two `match` arms) and nothing repo-wide
serialises it. So the decision is cheap now and gets more expensive the moment something
starts serialising it.

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
