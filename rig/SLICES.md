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
| `S-001` | uiux | `test/topology-all-view-modes` | `.claude/worktrees/topology-mode-coverage` | `origin/dev` @ `5b8ee51` | `in-review` **APPROVED** @ `ab93602`; BLOCKED on CI only (PR #50) | `pod_uiux.builder` |
| `S-002` | — | `chore/rig-harness` | repo root *(pre-existing branch, not cut)* | `origin/dev` @ `5b8ee51` | `in-review` **BLOCKED** (PR #55 @ `dba71c7`; local ahead, unpushed — see S-002) | `dev_gatekeeper` |
| `S-003` | backend | `refactor/device-execution-mode-canonical` | `.claude/worktrees/device-execution-mode` | `origin/dev` @ `5b8ee51` | `in-review` **APPROVED** @ `5cf3984`; BLOCKED on CI (PR #57) | `pod_backend.reviewer` |
| `S-004` | hmi | `feat/hmi-live-telemetry` | `.claude/worktrees/hmi-live-telemetry` | `origin/dev` @ `5b8ee51` | `in-review` @ `3c683be` (+846/−1, 5 files); not pushed | `pod_hmi.builder` |
| `S-005` | — | `policy/decision-defaults` *(exists @ `4f2ac4b`)* | *(not cut — see note)* | `origin/dev` | `planned` **blocked on S-002**; review CHANGES REQUESTED | `dev_gatekeeper` |
| `S-006` | — | `feat/desk-seat` *(exists @ `3995106`; main copy `4496ce0`)* | *(not cut — see note)* | `origin/dev` | `planned` **CHANGES REQUESTED**; blocked on S-002 + S-005 | `dev_gatekeeper` |
| `S-007` | — | `docs/alarm-colour-reconcile` | `.claude/worktrees/docs-alarm-colour` | `origin/dev` @ `5b8ee51` | `in-review` **APPROVED** @ `2bfac2e`; BLOCKED on CI only (PR #58) | `governance.scribe` |
| `S-008` | backend | *(not cut)* | — | `origin/dev` | `planned` — **blocked on a scope decision** | `pod_backend.builder` |
| `S-009` | scribe | `docs/topology-provenance-spec` | `.claude/worktrees/docs-topology-provenance` | `origin/dev` @ `5b8ee51` | `in-review` **APPROVED** @ `139c263` (carried forward from `72630ad`); BLOCKED on CI only (PR #56) | `governance.scribe` |
| `S-010` | scribe | `docs/theme-status-honest` | `.claude/worktrees/docs-theme-status` | `origin/dev` @ `5b8ee51` | `in-review` **APPROVED** @ `16c10e3`; BLOCKED on CI only (PR #59) | `governance.scribe` |
| `S-011` | scribe | `docs/tsk-801-quality-blockers-corrected` | `.claude/worktrees/docs-tsk801-correction` | `origin/dev` @ `5b8ee51` | `in-review` **APPROVED** @ `7e1535e`; BLOCKED on CI (PR #60) | `governance.scribe` |
| `S-012` | hmi | *(not cut)* | — | `origin/dev` | `planned` — **blocked on S-004** | `pod_hmi.builder` |
| `S-013` | hmi | *(not cut)* | — | `origin/dev` | `planned` | `pod_hmi.builder` |

**Re-verified against the live world 2026-11-11T06:1xZ, by `pm`, not carried from the
previous pass.** Corrections this pass: S-004 moved to `pod_hmi` (evidence in its section);
S-003's review got an owner (it had none — `qitem-20261111060931`); **S-009 was missing
entirely** — PR #56 is open with 3 commits and had no row here, which is the same defect
S-003 had in the other direction; S-001's head is `21cb9c4`, not `52aa7f5`; S-002's PR
exists at `dba71c7` while the row still said "not yet pushed" at head `238c5b8`.

### S-001 — topology view-mode claims (PR #50)

- **CURRENT HEAD `ab93602` — APPROVED, and the only non-infrastructure blocker is gone.**
  Verdict `qitem-20261111064330-f9a58a093310708c`: F1 fixed and proven in both directions
  (TS2769 rejects an asserted `conformance` prop; two mutations go red on the named test,
  including the same-caller-different-data case that a happy-path-only test would pass).
  Diff `origin/dev...HEAD`: 5 files, **+632/−2**, **zero deletions**, all under
  `frontend/plugins/ui-topology/` except one guard file in `ui-shell`.
  **Gate run by `dev_gatekeeper` from `frontend/` in the slice worktree** (not the worktree
  root, per CULTURE §4): `npx vitest run --no-cache` → **48 files, 375 tests, all passed**,
  exit 0. Worktree clean at `ab93602`, 0 behind `origin/dev`, remote head confirmed by
  `git ls-remote`.
  **What still blocks it is B1 only** — no runner, so `mergeStateStatus: BLOCKED`.
- **The one guard change worth a reviewer's attention, checked by `dev_gatekeeper`:** the
  `ui-shell` sweep now strips comments before scanning for fixture vocabulary
  (`DEFAULT_/SAMPLE_/FIXTURE_`). That is a guard weakening, so I mutation-tested it: injecting
  a real non-comment `const DEFAULT_STATIONS = []` into `NetworkCanvas.tsx` still went red
  (`plugins/ui-topology/src/NetworkCanvas.tsx: references DEFAULT_STATIONS`). The probe was
  reverted; the tree is clean at `ab93602`. The weakening is justified — a comment cannot
  fabricate, and the comment it was tripping over records why `DEFAULT_STATIONS` went.
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
- **Head:** `21cb9c4`, **two** commits on `origin/dev` (`52aa7f5` + `21cb9c4`
  `refactor(ui-topology): split provenance and health into the spec's two axes`). The
  worktree is clean. The two-axis split described below is now committed, so the
  `52aa7f5` verdict no longer describes the head.
- **Re-review landed 2026-11-11: CHANGES REQUESTED at `21cb9c4`, 1 blocking.**
  Builder row `qitem-20261111062745-7a4f2ed200b8fcf0`, gatekeeper row
  `qitem-20261111062755-2d7a2e58e9184508`. All four earlier items were re-verified **fresh by
  execution**, none carried from `52aa7f5`: the mutation goes red on the named test at `:141`;
  impossible pairings are `tsc` errors; the banner renders `NOT_STATED` over a populated canvas;
  no hardcoded palette remains. The reviewer also mutation-tested the sweep guard this diff
  weakened by stripping comments — a real `DEFAULT_STATIONS` still goes red.
  **The blocking finding:** `NetworkCanvasProps.conformance` (`NetworkCanvas.tsx:60`) accepts
  conformance as an **input**, while `NetworkCanvas.tsx:59` and `provenance.tsx:24` say it is
  **derived-only**. The test named for the rule (`noFabricatedDevices.test.tsx:267`) passes it in
  and asserts `data-derived="true"` — **a test that cannot fail for its defect.** Classified
  **LATENT, not LIVE**: the only production caller (`DockviewWorkspace.tsx:643`) passes no
  conformance. It matters because ADR-009 (S-009) is about to become the reference rule and says
  provenance "is never defaulted".
  Gate at `21cb9c4`: `cd <worktree>/frontend && npx vitest run --no-cache` → 48 files, 375 tests,
  all passed. CI **UNAVAILABLE**, never recorded as passing.
- **Head `52aa7f5` — the superseded verdict, kept because it is what the split answers.**
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
  starts** in the k3s journal since 06:46 WIB today.

  **ROOT CAUSE — MEASURED 2026-11-11T06:35Z by `dev_gatekeeper`. It is NOT ARC.** The
  listener is a victim. `kubectl -n arc-systems logs` shows it dying on:
  `Post "https://api.github.com/orgs/firmansyah-co/actions/runners/registration-token":
  dial tcp 20.205.243.168:443: i/o timeout`, retrying with `timeout=1s remaining=4`.
  It cannot mint a registration token, so the pod is recreated — which is why every
  restart has a **fresh pod UID** (a container crash-loop keeps its UID).

  **The fault is the WIRELESS LINK, not one GitHub endpoint.** Corrected 2026-11-11T06:46Z —
  my first reading ("GitHub-API-specific") did not survive a control, and `desk-desk` caught
  it. Measured from a normal shell, 20 requests to `https://api.github.com/rate_limit`:
  **13 ok, 7 outright failures, 7 of the 13 slow (>2s)** — roughly a 35% failure rate.
  Connect times on success ranged `0.03s … 7.2s`; some returned `000` with
  `time_connect 0.000000` (never completed).

  **The control that overturns "one endpoint":** `github.com` is *as bad as* `api.github.com`
  from this host — `desk-desk` measured 14 of 20 connects over 1s with 2 outright failures,
  against 13 of 20 and 4 failures for `api.github.com`. A single sick endpoint would have left
  `github.com` near 0.73 s. `pm` independently measured `github.com` at 3.8 s / 0.58 s / 4.8 s
  with one 10 s timeout, and `gh api` at 11.8 s. So the whole path to GitHub is degraded.

  **Packet loss and carrier, the direct evidence:** `ping` to `20.205.243.168` across three
  15-packet runs gave **13% / 0% / 0% loss with RTT 28 ms → 520 ms (max 1036 ms, mdev 277)**;
  `desk-desk` measured **50% loss** on a 10-packet run. `ping 1.1.1.1` is equally bad
  (46–234 ms avg, max 821 ms), so it is not GitHub-specific at all. **`ip link show enp86s0`
  → `NO-CARRIER`, `carrier=0`, no cable; `wlan0` is the only real uplink** and its carrier is
  up. The host is on WiFi to a lossy access point.

  **TODAY'S METHOD LESSON, and it is the sharpest one: one sample is not a measurement, including
  when the sample supports you.** `dev_gatekeeper`'s first diagnosis rested on a **single**
  `github.com` sample (0.73 s) that happened to look healthy, and used it as evidence that the
  endpoint was fine. `desk-desk` measured the same host **twenty** times and got **14 of 20
  connects over 1 s**. The error was **nested inside the control**, not absent from it — worse
  than having no control, because it produces confidence. Recorded next to the S-001 guard, the
  fail-open table and the desk's deadline: **four seats, four variants of one failure, every one
  caught by a cheap second check.**

  **Consequence that changes the remedy: there is a wired NIC sitting unused on this machine.**
  Plugging a cable into `enp86s0` is more likely to fix CI than anything done in k3s, and it is
  a ten-second physical action. This also means **"restore the ARC listener" cannot work** on a
  link losing this many packets — Adrian's recorded answer to that question needs re-scoping.

  **Why "restore the ARC listener" will not fix it:** the controller is alive and the
  listener starts cleanly; it dies at the first GitHub API call. Any fix that restarts
  pods loops forever. **Why it fails 100% of the time despite ~65% of requests eventually
  succeeding:** the listener's retry timeout is `1s`, so a 4–7 s connection is
  indistinguishable from a dead one.

  **How the cluster was read without sudo (corrects "the kubeconfig is root-only").**
  `/etc/rancher/k3s/k3s.yaml` is `0600 root`, but **`/home/adrian/.kube/config` is
  `0640 root:adrian`** — group-readable. `export KUBECONFIG=/home/adrian/.kube/config`
  gives read-only `kubectl -n arc-systems get pods|logs` and `helm list -A`. No sudo was
  used and no cluster state was changed. **Do not retry sudo from another seat** — it is
  not needed for diagnosis.

  **What would actually fix it** (host/network changes; Adrian's call, in order of likely
  effect): (A) **plug an ethernet cable into `enp86s0`** — the NIC is present and
  `NO-CARRIER`; this is the highest-probability fix and costs ten seconds; (B) if WiFi must
  stay, move closer to / change the access point, since 50% loss at 238 ms avg is a radio
  problem, not a routing one; (C) raising the listener's HTTP timeout is a **mitigation only**
  — it converts "always fails" into "sometimes works" and leaves CI flaky. **Acceptance test
  for any fix:** three 15-packet pings to `20.205.243.168` at 0% loss with mdev under ~50 ms,
  the 20-request loop shows ~0 failures, the listener pod survives past a few minutes, and a
  CI run leaves `queued`.

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
  `chore/rig-harness` (unmerged; **PR #55 open at `dba71c7`** — see S-002 for the measured
  remote state, and do not take a head SHA from this file). Every slice worktree
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
- **Remote state — measured 2026-11-11T06:1xZ, not carried.** **PR #55 is OPEN**, base `dev`,
  pushed head **`dba71c7`**, `mergeStateStatus: BLOCKED` (no runner — blocker B1), 31 files,
  **+2149/−0**, 30 added + 1 modified + **0 deleted**, `mergeable: MERGEABLE`.
  `dev_gatekeeper` re-verified that shape at the pushed SHA rather than at the receipt's SHA,
  and proved the receipt still applies to it by tree hash, not by trust:
  `git rev-parse 080ba94:crates dba71c7:crates` and the same for `frontend/` are identical —
  the later commits touch only `rig/`, which the cargo+pnpm gate does not read.

  **This paragraph previously said "branch not yet pushed; PR opens when the gate is green.
  Head `238c5b8`."** That was true when written and was stale by the time anyone read it; the
  table row above it had been corrected and this paragraph had not, so the file contradicted
  itself and `pod_hmi.reviewer` reasoned from the stale half. Reconciled here.

  **Three commits on the local branch are NOT in the PR and must not be pushed into it:**
  `d7ae80d` (advisor + release-auditor model), `23bf7c5` (desk seat + `DECISION-DEFAULTS.md`),
  `9e42e46` (ledger line). They carry S-005 and S-006 content. Pushing them would put three
  slices inside PR #55 — `CULTURE.md` §6, "a PR is not a slice". `dev_gatekeeper` left them
  local deliberately; that is the correct call and is recorded here so nobody "helpfully"
  pushes them.

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
- **State:** `in-review`, **review APPROVED**, awaiting CI only — **PR #57**, head
  **`5cf3984`**, +135/−9, 2 files, `mergeStateStatus: BLOCKED` (blocker B1, no runner).
  Head moved `7d4798c` → `5cf3984` when the builder added one **docstring-only** commit to
  close the reviewer's F3; the reviewer re-verified at the new head rather than letting the
  approval drift. Branch `refactor/device-execution-mode-canonical`, worktree
  `.claude/worktrees/device-execution-mode`, cut by `dev_gatekeeper` from `origin/dev` @
  `5b8ee51` on 2026-11-11.
- **Two verdicts, both `pod_backend.reviewer`, neither on a row until 2026-11-11:**
  APPROVED at `7d4798c` (PR comment `6788558556`), re-confirmed APPROVED at `5cf3984`
  (comment `6788672119`), row `qitem-20261111061824-ee64027c3fbea057` closed
  `no-follow-on`. Evidence at `5cf3984` in an isolated `git archive` copy — **never the slice
  worktree**, which had mutation residue: fmt exit 0, clippy exit 0,
  `cargo test -p mythos-strucpp` 27 passed / 0 failed, one `enum DeviceExecutionMode`
  repo-wide, four mutations each red for its named defect, the pure `pub type` alias green and
  correct.
- **Worktree is clean at `5cf3984`** (verified by `pm` 2026-11-11T06:2xZ). The two dirty files
  seen earlier were the reviewer's own mutation run overlapping the builder's — `pm`'s warning
  about them was correct when written and is resolved.
- **The process gap this slice exposed, worth keeping:** the builder closed its build row with
  `handed_off_to` → `dev_gatekeeper`, who cut, self-verified and opened the PR. **No reviewer
  was ever in the chain**, so the slice sat `in-review` with zero reviews and no owning row.
  The reviewer reviewed it unprompted and posted a comment nobody was accountable for. Fixed
  by creating the review row and naming `pod_backend.reviewer` as S-003's owner in the table.
  **Closure was `no-follow-on`, not `handed_off_to`, deliberately** — the only follow-on (F1/F2
  doc drift) already has its own owned row, and a handoff would have manufactured a successor
  for work already owned.

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
- **Two "known blockers" that were carried from `specs/tasks.md` — BOTH CORRECTED
  2026-11-11.** The old text said "both need backend work — do not paper over them on the
  frontend". That text would have misdirected the builder of this slice, and it is the exact
  class this repo keeps producing: a statement read once and then reasoned from. Found by
  `pod_hmi.reviewer`, verified independently by `pm` against `origin/dev` @ `5b8ee51`:
  1. **FIXED, not a blocker.** `build_telemetry_event` emits three fields, not one:
     `quality` (class), `quality_code` (raw word) and `quality_name` (the specific state) —
     `mythos-core/src/api/telemetry.rs:187-189`. Pinned end-to-end through the **real SSE
     route**, not a struct: `crates/mythos-core/tests/telemetry_quality_fidelity_tests.rs`
     GETs `/api/v1/telemetry/stream` and asserts `quality_name == "BAD_COMM_FAILURE"` versus
     `"BAD_SENSOR_FAILURE"`. Fixed by `aa6cce0`, confirmed an ancestor of `origin/dev`.
     **`QualityCode` → faceplate quality enum is mappable today.** The builder must not
     design around a limitation that no longer exists.
  2. **MIS-STATED, and the real gap is bigger.** `record_comm_loss` *does* write
     `TagSlot::quality` when the gate has a publisher (`quality_gate.rs:134`,
     `with_shm_publication`), proven across a real fork in
     `crates/mythos-strucpp/tests/quality_gate_shm_publication.rs`. The real gap is that
     **nothing constructs the gate in production**: `with_shm_publication` has zero
     production call sites and `QualityGate::new` appears only under
     `crates/mythos-strucpp/tests/`. `pm` widened the check and found it is not just the
     gate — **`StrucppRuntime::new` has zero call sites repo-wide**, and `mythos-mcp` is the
     only crate depending on `mythos-strucpp` (`Cargo.toml:17`) with no reference to it in
     `src/`. The whole ST runtime engine is uninstantiated. That is **S-008**, a backend
     slice, not this one.
  **Consequence the builder must state in the handoff:** because no `QualityGate` exists in
  production, the plant cannot emit `BAD_COMM_FAILURE` at all today. The drop-the-stream
  negative test asserts a **frontend** property (SSE ends → faceplate degrades to
  BAD_QUALITY, never a stale value as live, SYS-REQ-096). It does not and cannot prove the
  backend latch. `pod_hmi.builder` volunteered exactly this framing and it is the right one.
- **BLOCKING FINDING, proven by execution before the cut — the naive wiring passes its own
  test while violating its acceptance criterion.** Found by `pod_hmi.reviewer`, re-proved
  independently by `pm` on an extracted copy of `componentData.ts` from `origin/dev`
  (sha256 `2035ed12…`, 179 lines), run against the real exported function:
  | payload | resolved quality | values rendered |
  |---|---|---|
  | `quality: 'GOOD'`, value | `GOOD` | `{"actualSpeed":1450}` |
  | `quality: 'BAD'`, value | **`GOOD`** | **`{"actualSpeed":1450}`** |
  | `quality: 'UNCERTAIN'`, value | **`GOOD`** | **`{"actualSpeed":1450}`** |
  | `quality_name: 'BAD_COMM_FAILURE'`, value | **`GOOD`** | **`{"actualSpeed":1450}`** |
  **Mechanism:** the wire sends `"GOOD"`/`"BAD"`/`"UNCERTAIN"` (`telemetry.rs:187`); the
  faceplate enum is `'GOOD' | 'BAD_QUALITY' | 'NOT_BOUND'` (`types.ts:33`). `asQuality()`
  (`componentData.ts:99`) returns `null` for anything outside those three, so `"BAD"` and
  `"UNCERTAIN"` both fall through to `:160`
  (`return Object.keys(toProcessValues(live)).length > 0 ? 'GOOD' : 'BAD_QUALITY'`) — which
  cannot tell *"no quality declared"* from *"a quality declared in a vocabulary I do not
  know"*, and **fails open** on the second.
  **Why this is the crux:** the slice's negative test is "drop the stream mid-render and assert
  BAD_QUALITY". Dropping the stream leaves no payload, so `!live` returns `BAD_QUALITY` at
  `:155` and **the test goes green** — while a payload that IS arriving and says `BAD` still
  renders its value as live. The test and the defect do not overlap. Same class as the S-001
  "visibly distinct" guard that could not fail for its own defect.
- **SCOPE DECISION on the above — `pm`, 2026-11-11. Both halves are IN S-004.** Reasoning, so
  the builder does not have to re-litigate it:
  1. **Required, plainly in scope:** the consumer must map explicitly from
     `quality_name`/`quality_code` to the faceplate enum and pass the result as the **declared**
     `quality` prop. Declared quality already wins at `:149`, so this needs **no contract
     change**. This *is* the "map `QualityCode` to the faceplate quality enum" criterion done
     honestly.
  2. **Also in scope, narrowly:** when a payload **carries a quality key whose value the module
     does not recognise**, `resolveComponentQuality` must return `BAD_QUALITY`, not fall through
     to `GOOD`. Failing open on an unrecognised quality is the mechanism of the defect, and
     leaving it in the function S-004 is required to go through would hand the next consumer the
     same trap.
     **Deliberately NOT changed:** `componentData.test.ts:41` pins `live: { actualSpeed: 1450 }`
     — a payload with **no quality key at all** — as `GOOD`. That stays. The wire producer
     always sends `quality`, so that shape does not occur on this path, and widening the change
     to it would alter the shared contract beyond the defect. **Keep that test green; do not
     "fix" it.** Residual risk, stated so it is on the record and not assumed away: that path
     stays fail-open, so **any future transport that sends values without a quality key is
     declared GOOD**. Revisit when a second transport lands — not now, and not by the builder
     silently widening it.
  **Why Decision 2 is a consistency fix, not a scope expansion** (argument from
  `pod_hmi.reviewer`, verified by `pm` at `componentData.test.ts:51-53`): the module **already
  fails closed on an unrecognised declared quality** —
  `resolveComponentQuality({ quality: 'PROBABLY_FINE' })` → `NOT_BOUND`, under the test name
  *"ignores an unrecognised quality value rather than trusting it"*. So the module already holds
  the principle *"a quality I do not recognise is unsafe"*. Failing **open** on an unrecognised
  **payload** quality is the anomaly. Decision 2 applies the rule the module already has.
  The reviewer also ran the proposed rule against every existing expectation in
  `componentData.test.ts`: **0 broken** — `:32`, `:34`, `:41` all preserved, and only the two
  defect cases change.
  **Blast radius, measured:** `resolveComponentData` has exactly **one** production caller,
  `DockviewWorkspace.tsx:423`, and **no production code supplies `live:` today** (repo-wide
  grep outside tests: zero hits). So the module change has no live consumer to disturb — it is
  LATENT now and becomes LIVE when S-004 wires it. That is the whole argument for fixing it
  here rather than later.
- **Required proof for the module change:** a test using the wire's real vocabulary
  (`quality: 'BAD'`, `quality: 'UNCERTAIN'`, `quality_name: 'BAD_COMM_FAILURE'`, each with a
  value present) asserting `BAD_QUALITY` and **empty values**. Mutation-prove it: revert `:160`
  and the test must go red **on the stale-value case specifically**, not merely somewhere.
- **Reconnect criterion — the trap, recorded so the review applies it.** Found by
  `pod_hmi.reviewer`, confirmed by `pm`:
  - "Survive reconnect with bounded backoff" is **genuinely new work**:
    `git grep -E 'reconnect|backoff|retry'` over `ui-shell/src` and `ui-faceplate-lib/src`
    returns only unrelated UI retry buttons. Nothing to reuse.
  - **`EventSource` reconnects by itself, with no backoff.** The one existing consumer
    (`packages/ui-shell/src/stores/useFaceplateStore.ts:277`) relies on that and does not
    handle `onerror` at all. So "survive reconnect" is free — a builder can satisfy it by
    doing nothing, while shipping an **unbounded retry storm** against a down backend.
  - **Worse:** on disconnect `EventSource` fires `onerror` and keeps the last values in
    memory. Unless the consumer explicitly degrades quality on error/close, a dropped stream
    leaves the faceplate rendering the last good value **as live**. That is SYS-REQ-096, and
    it is the one failure a "does it reconnect?" test cannot catch — after reconnect it looks
    healthy.
  - **Criterion the review will apply:** a test that closes the `EventSource` mid-render and
    asserts the faceplate shows `BAD_QUALITY`/`NOT_BOUND` — **not the previous value** —
    *before any reconnect happens*. Bounded backoff means the delay is capped and
    **observable**, not "we call `close()` and reopen immediately".
- **NEW BLOCKING HAZARD for S-004 — the faceplate would name the WRONG CAUSE.** Found by
  `pod_hmi.reviewer` while reviewing PR #60; verified by `pm` on `origin/dev` @ `5b8ee51`.
  - `ProcessDataQuality` has **three** values: `'GOOD' | 'BAD_QUALITY' | 'NOT_BOUND'`
    (`types.ts:33`). Both `BAD_COMM_FAILURE` and `BAD_SENSOR_FAILURE` collapse into
    `BAD_QUALITY`.
  - `BAD_QUALITY_TEXT = 'BAD COMM FAILURE'` (`processData.ts:22`) is returned for **every**
    bad quality by `dataQualityLabel` (`:53-63`), which has one hardcoded string for the whole
    case. `DataQualityBadge.tsx:16` renders it directly.
  - `git grep quality_name|quality_code origin/dev -- frontend/` → **zero hits.** The frontend
    never reads the fields that carry the distinction.
  **So an operator looking at a failed SENSOR would read "BAD COMM FAILURE".** That is not a
  missing diagnosis, it is a **wrong** one — and on a safety HMI a confident wrong cause is
  worse than an honest absence. **It is LATENT today** (nothing populates `comp.live`, so the
  badge never reaches a live path) and **S-004 makes it LIVE the moment it wires the stream.**
  This is the same failure class as the fabricated rack and the fail-open quality: a surface
  asserting a cause it did not measure.
- **WIP ARCHITECTURAL FINDING — one connection per panel against one module-global store.**
  Found by `pod_hmi.reviewer` reading the in-progress diff; verified by `pm` at the dirty tree.
  - `DockviewWorkspace.tsx:64` calls `useSseTelemetry()` **inside** `HmiScreenCanvasPanel`
    (`:47`), and `useTagStore` is a **module-global** zustand store (`useTagStore.ts:163`).
    Panel ids are per-screen (`:700`), so **two open screens = two instances = two
    `EventSource`s** to the same endpoint.
  - The hook's cleanup calls `useTagStore.getState().clear()` **unconditionally**
    (`useSseTelemetry.ts:109,128`). **Close one screen tab while another is open and the
    surviving panel's store is wiped** — its faceplates resolve to `BAD_QUALITY` while its
    stream is healthy. Safe direction (values suppressed, never fabricated), but an operator
    watches a live screen go dark for no reason.
  - Mirror case: two feeds, and `applyBatch` replaces the whole `Map` from whichever fired last,
    so they overwrite each other's snapshot.
  **Fix, either is cheap now and awkward after tests pin per-instance behaviour:** (a) refcount
  subscribers and clear only at zero; or (b) **hoist the hook above `DockviewReact`** so one
  panel set shares one connection — cleaner, matches "one stream per station", and makes the
  negative test simpler because there is exactly one connection to drop.
  **Not a fourth blocking item** — the failure is in the safe direction. But it is **in scope**:
  S-004 is what makes multi-panel telemetry reachable at all, so the first slice that opens the
  stream is where the connection count gets decided.
  **Why "one tag to one faceplate" does not exclude it:** that constraint is about **binding
  architecture breadth** — do not build a general subscription/registration system. **Connection
  count is not that.** It is the difference between a stream lifecycle that is right and one
  known wrong at the moment it first becomes reachable.
  **THE CRITERION — TWO PROPERTIES, NOT ONE, and a test for only the first passes on a broken
  fix.** The builder's own harness already supports both: `liveTelemetryPath.test.tsx:41` defines
  `static instances: FakeEventSource[]` pushed in the constructor, so counting connections is an
  assertion on an array that already exists.
  | property | test | mutation that must redden it |
  |---|---|---|
  | **(a) teardown isolation** | mount two panels, unmount one, assert the survivor's tags remain and its faceplates still resolve `GOOD` | remove the refcount/hoist → survivor goes `BAD_QUALITY` |
  | **(b) connection count** | mount two panels, assert `FakeEventSource.instances.length === 1` (or whatever the chosen design states) | revert to per-instance `useSseTelemetry` → length becomes 2 |
  **They fail independently.** A fix for (a) alone — e.g. refcounting the `clear()` — **leaves the
  double connection intact and looks green.** Refcounting is the smaller change and the
  temptation will be to stop there, which is exactly why the row states both.
  **Not acceptable as coverage: a single-panel test.** Every existing test in that file mounts
  one panel, so all of them pass on both defects. Only the two shapes above can fail for these
  bugs.
  **Either fix shape passes** — (b) hoist makes both properties trivial, and a correct refcount
  also passes. The review is against the **properties**, not the shape.
- **BUILT 2026-11-11T07:04Z — `feat/hmi-live-telemetry` @ `3c683be`**, one commit on
  `origin/dev` @ `5b8ee51`, **+846/−1, 5 files**. Not pushed (`dev_gatekeeper`'s). Gate from
  `frontend/` in the worktree: `pnpm lint` exit 0, `pnpm build` all 6 packages,
  `npx vitest run --no-cache` → **49 files / 382 tests passed** against a **48/366 baseline**, so
  the 16 new tests are additive and nothing regressed. Mutation-proved with each injection
  asserted to have landed and reverted.
  **The commit body is the standard I want on every slice:** it states the fail-open table it
  measured, why the mapping keys on the specific name rather than two class strings, and a
  **"WHAT THIS DOES NOT PROVE"** section naming the frontend-only negative test, the
  `UNCERTAIN_*` → `BAD COMM FAILURE` label limit, and that no staleness deadline exists because
  none is published. **A slice that says what it does not prove is worth more than one that
  proves more.**
- **Three things `pm` verified rather than took on trust:**
  1. **The fail-open is unreachable from this path** — `resolveComponentLive` returns a
     `ProcessDataQuality`, so `asQuality()` accepts it, `:149` returns it, `:160` never runs.
     **Deferral accepted; S-013 opened for the module hardening.** `pm`'s ruling that it belonged
     in S-004 assumed the fix was needed to stop the defect going live; it is provably
     unreachable here, so the ruling's purpose is met without the change.
  2. **The badge names a wrong cause, and this is S-012's justification arriving live.** The
     consumer maps to the class and does **not** pass the specific quality, so the badge is in
     case C and shows `BAD COMM FAILURE` — **an operator at a failed sensor reads "BAD COMM
     FAILURE"**. Values are suppressed, so this is a wrong label and not a fabricated reading.
     **Correct per `pm`'s own case C ruling, wrong in outcome, and recorded in the commit body
     rather than hidden.**
  3. **Connection count NOT met.** `DockviewWorkspace.tsx:64` still calls `useSseTelemetry()`
     **inside** `HmiScreenCanvasPanel`; `useSseTelemetry.ts` has no refcount and no module-level
     singleton; `liveTelemetryPath.test.tsx` has **no** `instances.length === 1` assertion and no
     two-panel teardown test. So two open screens still open two `EventSource`s against one
     module-global store. **`pm` sent this at 06:56:41 and the commit is 07:04:15**, so it is an
     absorbed-too-late rather than a refusal — recorded as `pm`'s timing, not the builder's error.
     **Raised to the reviewer as the one open question for the verdict.**
- **PROCESS NOTE — a permission prompt held this slice for ~7 minutes and nothing caught it but
  a reviewer watching.** `pod_hmi.builder` was gated on `Read(/tmp/verify-fallthrough.mjs)`;
  four urgent inbox items queued behind it, worktree unmoved at `5b8ee51`. `rig send` refuses at
  a prompt by design, so the seat could not be nudged. **Cleared by `pm` at 06:50Z** with
  `rig send --dangerously-interact`, choosing the **session-scoped** option ("always allow /tmp
  for this session") over the one-shot, because the builder generates its own probe scripts under
  `/tmp` and would otherwise stall once per script.
  **Two escalations fired on this stall** (`qitem-20261111062609`, `qitem-20261111065111`) and
  both were closed as **resolved-at-source**, not as lost wakes.
  **The prompts were measured, and they were not risky operations.** Every prompt cleared today
  was **`/tmp` path access** — `Read(/tmp/verify-fallthrough.mjs)` and an overwrite of the same
  file: the builder writing and re-reading its own probe scripts, which is correct practice and
  what the slice's verification needs. **Not one was a genuinely risky operation.** So the narrow
  fix is a permission **allowlist**, not a mode change — raised to `advisor` as
  `qitem-20261111065512`, with a member-level custom policy (`surface: config`, an `/tmp` allow)
  recommended over `builtin:auto`: it widens nothing, and because config-surface rules are picked
  up from settings rather than at launch it needs **no relaunch**.
  **Two constraints on that decision, both from `dev_gatekeeper`:** the seat that benefits must
  not be the seat that approves it (the S-005 rule), and `pm` is a builder's proxy this pass, so
  neither decides it.

  **THE RATE IS NOW FOUR SEATS**: `pod_hmi.builder` twice, and `executive-advisor` once
  (`Do you want to create note-arc.md?` — the same `/tmp` scratch-file class, cleared by `pm` at
  07:1xZ). Every one was a path prompt, none was a risky operation, and every one stopped a seat
  mid-work. **This is now a rig-level defect, not a slice-level annoyance.**

  **Why this is worth recording rather than fixing silently:** the builder had no way to clear its
  own gate, so a slice can stall indefinitely with the work already queued and correct. The
  reviewer's call **not** to answer another seat's prompt was right; clearing a routing block is
  an orchestrator act, not a reviewer one. If this recurs, the question for `advisor` is whether
  builders should be launched with a `/tmp` allowance so their own verification scripts do not
  gate them.
- **SCOPE DECISION on the above — `pm`, 2026-11-11. Split, with a hard constraint on S-004.**
  - **S-012 (new, own slice):** let the faceplate **name the specific bad quality**. This needs
    the faceplate quality contract to carry more than three values — a field or prop beyond the
    enum — so it touches `types.ts`, `processData.ts`, `DataQualityBadge` and every faceplate.
    That is **wider than "one tag to one faceplate"** and is explicitly outside S-004's
    constraints, so it cannot ride along. Not cut; `planned`.
  - **S-004, required:** it **must not make the wrong cause reachable.** The consumer passes the
    specific quality through, and the badge distinguishes **supplied-but-unrecognised** from
    **not supplied at all**. **Do not widen this into redesigning the enum in S-004.**
  - **THE THREE-CASE RULE — `pm`'s first wording was self-contradictory and the reviewer caught
    it. This is the rule to build to.** `pm` wrote "falls back to a neutral label when the cause
    is unknown" *and* "`noData.test.tsx:126` / `honestCanvas.test.tsx:131` stay green". **Both
    cannot hold**: those two tests render with **no specific quality supplied at all**
    (`noData.test.tsx:119` passes `quality: 'BAD_QUALITY'` and nothing else;
    `honestCanvas.test.tsx:118` renders a bound component with bindings and no payload), so under
    the naive rule the cause is "unknown" in both, both fall back to neutral, both stop rendering
    `BAD COMM FAILURE`, and **both go red** — the opposite of what `pm` said. Verified by `pm` at
    `origin/dev`.
    | case | detail | badge |
    |---|---|---|
    | **A** | supplied **and** recognised | name it — `BAD COMM FAILURE`, `SENSOR FAILURE`, … |
    | **B** | supplied but **not** recognised | neutral wording (`BAD QUALITY`) — **never names a cause** |
    | **C** | **not** supplied at all | today's behaviour, `BAD COMM FAILURE`, **unchanged** |
    **Case B is the defect S-004 makes reachable and must never name a cause. Case C is the
    existing pinned contract and stays green untouched.** Case C is also **not reachable from
    S-004's live path**, because the consumer always supplies `quality_name` — so the honest path
    is fully honest without changing the legacy contract.
    **`pm` considered making case C neutral too and rejected it for S-004 — recorded with the
    trade-off, because it is defensible:** a bound device with no payload genuinely has no known
    cause, so `BAD COMM FAILURE` there is itself a guess. But it changes the contract for all
    four faceplates and **reds two existing tests**, which is **S-012's blast radius, not
    S-004's**. If that is wanted, it belongs in S-012, not here.
  - **The enum is not touched, and that is verified mechanics, not an opinion:** `DataQualityBadge`'s
    props are **inline and already optional** (`DataQualityBadge.tsx:12`), `BaseFaceplateProps.quality`
    stays `ProcessDataQuality` (`types.ts:58`), and adding one optional passthrough
    (`qualityDetail?: string | null`) is additive — the faceplates that do not thread it keep
    compiling and keep their behaviour. So S-004 touches the badge, one label function, and the
    **one** faceplate it wires. "One tag to one faceplate" is satisfiable.
  - **THE WORD "NEUTRAL" WAS AMBIGUOUS AND THE REVIEWER PINNED IT — this is the exact criterion
    to build to.** `pm` wrote "neutral label"; `pod_hmi.reviewer` pointed out that in
    `DataQualityBadge.tsx` **the tone and tooltip key off the three-value enum, not the label**
    (verified by `pm` at `origin/dev`):
    ```tsx
    const isBadQuality = resolved === 'BAD_QUALITY';
    const tone = isBadQuality ? 'bg-amber-950/70 …' : 'bg-neutral-800/80 …';  // amber : grey
    title={ isBadQuality ? 'The bound device is not reporting usable data.' : 'No device binding.' }
    ```
    The **grey** branch is the `NOT_BOUND` **configuration** presentation. If the builder keys
    the fallback off the specific quality instead of the class, an unrecognised **bad** cause
    falls into it and an operator at a genuinely failing bound device gets **grey badge +
    "No device binding" + `data-quality="NOT_BOUND"`** — **two false statements at once**: it
    denies the binding exists *and* files a live equipment failure as a configuration note.
    **That is worse than the bug being fixed**, because "BAD COMM FAILURE" at least says the
    device is failing.
    **The rule, verbatim from the reviewer:** a bound device with an unrecognised bad quality
    must keep the **ABNORMAL** presentation — `data-quality="BAD_QUALITY"`, amber tone, the
    "not reporting usable data" tooltip — and change **only the wording** (e.g. `BAD QUALITY`),
    never naming a cause it does not know. **The class still comes from the three-value enum;
    the specific quality supplies only the noun.**
  - **Test that must exist, and it must not pass on a technicality:** a payload with
    `quality_name: 'BAD_SENSOR_FAILURE'` renders (a) text that does **not** claim a communication
    failure, **and** (b) `data-quality="BAD_QUALITY"` with the abnormal tone — **not**
    `NOT_BOUND`. **A test that only asserts the text would go green while the badge quietly
    became a configuration state.** Mutation-prove it: restore the hardcoded string and the test
    must go red.
  - **The test that must NOT be written:** "bad quality renders BAD COMM FAILURE". That asserts
    the defect, and it is exactly the shape an existing suite tempts a builder to copy —
    `noData.test.tsx:126` and `honestCanvas.test.tsx:131` already do it for the **generic** bad
    case. Those assertions are **correct for an UNNAMED bad quality and stay**; they must not be
    reused for the named-sensor case.
- **Pod: `pod_hmi`, decided 2026-11-11 — not `pod_uiux`.** Evidence, not preference:
  `hmi-builder/guidance/role.md:3` puts `ui-faceplate-lib` in hmi's focus; `ui-builder`'s
  focus is `ui-shell/`, tokens, layout, UX flows and does **not** include `ui-faceplate-lib`.
  The acceptance criterion is an honesty property (never stale-as-live, never fabricated),
  which is hmi's stated mandate. `pod_uiux.builder` is also carrying S-001 with CHANGES
  REQUESTED outstanding. **Named exception:** the consumer needs a store under
  `ui-shell/src/stores/`, which is uiux's focus — `pod_hmi.builder` may add it;
  `pod_hmi.reviewer` checks it against uiux conventions.
- **Done when:** one integration test shows a tag published to the TagBus arriving at a
  rendered faceplate with GOOD quality and the expected value, end to end; and a negative
  test drops the stream mid-render and asserts the faceplate degrades to BAD_QUALITY —
  never to a stale value presented as live, never to a fabricated one (SYS-REQ-096).
- **Verification:** `cd frontend && npx vitest run --no-cache`, run from `frontend/` — never
  from a worktree root (see CULTURE.md §4). Report failing test names, not counts.
- **Sequencing — the S-001 dependency is DROPPED, 2026-11-11.** The old rationale was "same
  pod", and the pod changed. File overlap with S-001 is nil: S-001's one `ui-shell` file is
  `src/__tests__/noFabricatedDefaultsSweep.test.ts`; this slice adds `src/stores/`. S-004
  does not depend on S-003 or S-008. **Cut** from `origin/dev` @ `5b8ee51` as
  `feat/hmi-live-telemetry` at `.claude/worktrees/hmi-live-telemetry`.
- **State:** `open`. Worktree **cut 2026-11-11T06:1xZ** by `dev_gatekeeper`:
  `feat/hmi-live-telemetry` at `.claude/worktrees/hmi-live-telemetry`, from `origin/dev` @
  `5b8ee51`, 0 behind, 0 modified. Builder notified and working.
  **A fresh worktree has no `node_modules`** — `cd frontend && npx vitest run --no-cache`
  fails on a missing binary until dependencies are installed *in the worktree*. Install
  there, never from the repo root (S-002's tree, currently dirty).
  **The queue row `qitem-20261023085422` still points at `pod_uiux-builder`** and is
  unclaimed. That row is the last place the old ownership survives and `pm` reassigns it.

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
  **That provenance line is the F1 fix, and it is NOT in the file on this branch** — see the
  live risk below. The sentence above describes the intended file, not the current one.
- **LIVE RISK, found by `dev_gatekeeper` 2026-11-11T06:1xZ — read this before cutting S-005.**
  Two facts, both measured:
  1. **The copy on this branch is the REJECTED text.** `git rev-parse
     23bf7c5:rig/culture/DECISION-DEFAULTS.md` → `345e5be…`, identical to
     `git rev-parse 4f2ac4b:…`. Commit `23bf7c5` restored it "verbatim from `feat/desk-seat`",
     which is the version the review rejected. **F1 and F2 are both still absent** — grep on
     the restored file returns zero hits for either fix.
  2. **Nothing delivers it to any seat.** The row's own "What" says the slice includes "the
     `mythos-org.yaml` delivery entry that ships it to every seat". **That entry does not
     exist.** `grep -rn 'DECISION-DEFAULTS' rig/` returns only `rig/SLICES.md` and desk role
     prose; `mythos-org.yaml`'s `startup.files` lists `SLICES.md` only, and `culture_file`
     points at `culture/CULTURE.md`. So the file sits on disk with no delivery path at all.
  **Consequence: if S-005 is cut from this branch as it stands, the rejected text ships.** The
  cut must start from the fixed text, not from `4f2ac4b`/`23bf7c5`. F3 (silent `required: true`)
  also means nothing would have caught it.
- **Watchdog correction:** the three watchdogs were registered by **Adrian's own session**, at
  `05:38:25` / `05:41:50` / `05:41:56`, passing `--registered-by executive-advisor@mythos-org`.
  The commit message's "Advisor watchdogs registered against this" reads that string as
  authorship; it is a misleading audit field, not a claim of who ran the command.
- **Review verdict 2026-11-11 — CHANGES REQUESTED, 3 blocking + 1 question**
  (`qitem-20261111055506-94d7ef5f`, closed against `qitem-20261111055057-9a6593dcbeccfcf7`):
  - **F1 — no provenance line.** The file opens "Adrian, 2026-11-11." in his first person and is
    delivered to every seat with `required: true`. Add, near the top, that the substance is his
    instruction of 2026-11-11 and the drafting is his assistant's. *"The difference between
    'Adrian wrote this' and 'this transcribes what Adrian said' is the difference between
    authority and attribution."*
  - **F2 — the scope statement grants more than CULTURE §6 allows.** `DECISION-DEFAULTS.md:57`,
    `:68` and `CULTURE.md:174` assert the branch is the whole boundary ("Everything except the
    `release` branch", "rc is reachable", "the one hard exception"). Read with §5's "then do the
    work" and §3's allowed-actions table, a builder can take that as merge authority over `dev`
    and `rc`. Fix, verbatim: *"This file says whether an act may proceed without an answer; it
    does not change who may perform it (§6) or whether it must be reviewed (§6)."* Keep "rc is
    still allowed" only as "`release_auditor` may proceed on `rc` without waiting".
  - **F3 — `required: true` fails SILENTLY.** From a git archive of `4f2ac4b`, `rig spec
    validate` / `preflight` / `audit` all stay green with `DECISION-DEFAULTS.md` **or**
    `CULTURE.md` deleted. `validate` checks the schema, not the tree, so the three commands
    `CLAUDE.md` tells every seat to run prove nothing about delivery. Keep `required: true` (the
    intent is right), but establish what happens at **restore** when a required startup file is
    missing before the ledger calls delivery safe — the reviewer did not test it and said so.
    The only enforcement path found is `bundle-check.js` `startupFiles()`, reachable through
    bundle operations, not `validate`.
  - **Q1 — answered by `pm`, not a gate.** Mark the busy-clock exclusions
    (`DECISION-DEFAULTS.md:43-50`) as a drafting decision Adrian can overrule: he said "kecuali
    udah gaada kerjaan lagi" (unless there is no work left) and the file redefines "no work" as
    "no work that can proceed". Plausibly what he meant, but it is disguised as a definition
    inside a section that otherwise quotes him closely.
  - **Re-review:** `pod_uiux.reviewer` re-reads the result. The author of a fix does not approve
    its own fix.
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
- **Row state, 2026-11-11T06:2xZ — `pm` owns this row and it is BLOCKED, not idle.**
  `qitem-20261111055506-94d7ef5f` is blocked on S-002 landing. It cannot be fixed before then:
  F2's fix is a sentence in `rig/culture/CULTURE.md`, and that file is **not on `origin/dev`** —
  S-002 adds it. Editing F1/F2 now would mean either touching the repo root (forbidden while
  S-002 is under review) or creating `CULTURE.md` in a fresh worktree and colliding with PR #55
  by construction. **F3** (required-file delivery unverified) is a `rig`-level question already
  raised to `advisor` as `qitem-20261111055550-394fc3bf87db337b` and is not ours to answer here.
  So: three fixes, all correctly waiting on one dependency, none of them forgotten.
  **`pm` does not sit on a claimed row while waiting** — this paragraph is the block reason.

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
- **Six findings for the review, from `advisor` (`qitem-20261111055622-6ac42a42`), each
  verified at `4496ce0` or live — with `pm`'s corrections where a claim did not survive:**
  1. **`rig/agents/desk/agent.yaml:7` declares `model:` with an EMPTY value**, while the pod
     label says "Desk (Opus 5.5)" and `rig ps` reports "configured not-declared". **A label
     asserting a model that is not configured.**
     **CORRECTED TWICE — and the second correction is the important one.** This text first said
     "a seat with no model is not launchable as written". `pm` then called that **falsified by
     observation**, because `desk.desk` is running. **That was wrong, and `dev_gatekeeper`
     withdrew it:** the running seat was launched from the **populated working tree**, so it
     proves nothing about `9e42e46`. The observation does not distinguish the two trees.
     **Correct state: the claim is UNPROVEN, not falsified.** What survives, measured at the
     commit, is the **label/configuration mismatch** — the pod label asserts "Desk (Opus 5.5)"
     and the committed spec declares no model. `rig spec validate` accepts it: the gap between
     "well-formed" and "correct".
     **Reading hazard:** the model-id value is stripped from agent tool output in this rig;
     `sed`/`grep`/`git diff` render the line with the value silently gone. Use
     `sed -n '7p' f | od -An -c`.
  2. **The desk IS wired; the pod file alone reads as isolated.** `rig/pods/desk.yaml` carries
     `edges: []`; the wiring lives at org level in `rig/mythos-org.yaml`:
     `executive.advisor -> desk.desk` and `desk.desk -> governance.pm`, both `delegates_to`. The
     seat is connected in both directions and would receive `gate:human` rows. **Fix by
     cross-referencing the org-level edges from the pod file — not by adding edges.**
  3. **Is a twelfth seat wanted?** `rig spec validate` and preflight check form, not desire.
     Adding a seat and a pod is a topology decision and should be the review's actual question.
  4. **LIVE presentation defect — the desk stated a deadline it had not read.**
     It told Adrian the CI row's *"deadline (05:42Z) has already passed — 23 minutes overdue"*,
     and repeated it to `advisor`. **Measured, and it was wrong twice over:**
     - `05:42:43.622Z` is `closureRequiredAt`, which equals `claimedAt` (`05:27:43.622Z`)
       **+ exactly 15 minutes** — the owner's closure SLA for `tier: critical`. It is not a
       human decision deadline. `waiting.deadlineAt` mirrors it, which is why it read as one.
     - **The real deadline was already on the row**, in `advisor`'s own transition note at
       `05:50:06Z`, verbatim: *"Clock: 24h busy clock, team has other actionable work. Arrived
       05:27Z, so deadline 2026-11-12 05:27Z."* `tsCreated` is `05:27:21.504Z`; +24h =
       `2026-11-12T05:27:21Z`. **23h21m remained.** The decision was not overdue.
     **Why this is worse than a cosmetic error:** presenting a decision as overdue can make the
     operator rush a root action on their own machine. The outcome was unaffected — he chose
     the recommended option — but it was a fabricated measurement on a safety-adjacent surface.
     **`pm`'s first advice was also wrong and the desk corrected it.** `pm` said *"when a row has
     no deadline field, say so"*. That would have been **false here**: this row *did* state its
     deadline, in a note. The role fix is therefore **not** "be silent when the field is absent"
     — it is **name where the deadline comes from, in priority order**:

     > The deadline is NOT `closureRequiredAt` or `waiting.deadlineAt`. Those are the owner's
     > closure SLA (`claimedAt` + tier allowance) and say nothing about how long Adrian has. The
     > human deadline is arrival + the clock in `DECISION-DEFAULTS.md` — 24h busy, 2h idle —
     > computed from `tsCreated`. **Take it from the row's transition notes if `advisor` stated
     > one; the stated value wins over your own arithmetic.** If neither exists, say "no deadline
     > recorded on this row" and compute and **label** your own as inferred.

     Proposed by the desk itself (`desk-desk`), for the reviewer to accept or reject — not for
     the desk to write, since `role.md` is under review and S-006 is blocked on S-002.
     **The desk corrected its own record on the row** (`06:32:39Z`) rather than leaving it for a
     successor to find. That is the behaviour `CULTURE.md` §9 asks for.
  4b. **"Stack the questions" vs "present exactly one at a time" — a reading hazard, not a
     contradiction, and the distinction is load-bearing.** Found by the desk (`desk-desk`).
     - `rig/agents/desk/guidance/role.md:28` — *"Present exactly one at a time, as a clickable
       question."* and `:95` — *"Never batch-answer. One decision per question."*
     - `rig/culture/DECISION-DEFAULTS.md:20` — *"Stack the questions. Several open decisions is
       normal and expected... Keep one running list... and present it when I ask."*
     **`pm`'s reading, checked against both files:** these are **not** contradictory — they
     describe **two different acts**. *Stacking* is maintaining a running **list**; *presenting
     one at a time* is issuing one **clickable question**. You keep the stack and ask from it one
     item at a time. The desk already resolves it this way (one question per message) and that
     satisfies both.
     **Why it still needs fixing, and why a careless fix would be worse:** neither file says that
     these are two different acts, so a future reader can see a contradiction and "resolve" it by
     deleting the stacking language. That would remove the thing Adrian actually asked for — he
     wants to see **every** open question without hunting, which is the desk's whole reason to
     exist. **The fix is one clause in each file naming the two acts, not the removal of either.**
     Owner: the desk role text is under S-006 review; `DECISION-DEFAULTS.md` is S-005. Neither is
     the desk's to edit while both are blocked on S-002.
  5. **Ownership conflict with `advisor`.** The desk role records answers onto `gate` rows it
     owns (`rig queue resolve`/`update`/`handoff`). `rig/agents/advisor/guidance/role.md` still
     says the **advisor** presents the list. **Update the advisor role in the same slice** so
     the two files agree on who presents and who recommends.
  6. **Push hazard — `dev_gatekeeper`'s to handle.** Local `chore/rig-harness` is ahead of PR
     #55's pushed head; a push from the repo root would put the desk seat into PR #55 — three
     slices in one PR. Already recorded in S-002.
- **Review verdict 2026-11-11: CHANGES REQUESTED — 2 blocking, 1 withdrawn-then-restored.**
  `pod_uiux.reviewer`, row to `dev_gatekeeper` `qitem-20261111063542-0a821de156ab6bb9`.
  - **F1 — RESTORED by `pm` after the reviewer withdrew it. The withdrawal measured the wrong
    tree.** The reviewer withdrew on the grounds that `agent.yaml:7` is populated. It is
    populated in the **working tree** and **empty at the commit under review**:
    `git show 9e42e46:rig/agents/desk/agent.yaml | sed -n '7p' | wc -c` → **10 bytes**
    (`  model:` + space + newline, no value); the working file → **25 bytes**; another seat's
    committed spec → **25 bytes**. The populated value is an **unclaimed uncommitted edit** in
    the repo root. What the withdrawal got right, and must be kept: **"not launchable" was
    always false** — the seat is running. The surviving finding is the **label/config mismatch
    in the committed spec**.
  - **F3 (was finding 4) — CONFIRMED LIVE**, and the desk's own correction is the accurate
    wording: `closureRequiredAt − claimedAt = 0:15:00` exactly, so it is the owner's closure SLA,
    not Adrian's deadline.
  - **F4 — NEW, BLOCKING, and it is the CAUSE of F3, not a sibling.** `role.md:78-79` cites
    `DECISION-DEFAULTS.md`, which **appears nowhere in `mythos-org.yaml`** — only
    `culture/CULTURE.md` is delivered. **The desk invented a deadline because it was told to
    follow a clock it cannot read.** That reframes the whole finding: F3 is a symptom.
  - **F5 — the one-at-a-time vs stack conflict is THREE-way**, not two: `advisor` role, desk
    role, and `DECISION-DEFAULTS.md` all speak to it. Recorded as 4b above. **Do not resolve it
    by deleting the stacking language.**
  - **Not decided by the reviewer, correctly:** whether a twelfth seat is wanted. That is `pm`
    and `advisor`'s call, and it is still the slice's real question.
- **Sequencing consequence of F4, recorded 2026-11-11:** S-006's blocking items are closed by
  **S-005's delivery entry**, not by editing the desk role. So S-006 waits on **S-002** (which
  adds `rig/`) **and S-005** (which delivers the policy the desk cites) before the desk presents
  another deadline. **The seat is running now with neither**, which is why F3 happened — and why
  this is urgent rather than tidy.
- **Recusal:** `advisor` does not review this — the desk sits on its outgoing edge and changes
  its job. The review belongs to a pod reviewer.
- **State:** `planned`, blocked on **S-002** (it modifies `rig/mythos-org.yaml`, which is not
  on `origin/dev`) and on **S-005** (the desk role cites `DECISION-DEFAULTS.md`). Work
  preserved at `4496ce0` (on local `chore/rig-harness`) and `3995106` (`feat/desk-seat`,
  cherry-picked onto `origin/dev`). Neither is pushed.
- **Review assigned 2026-11-11: `pod_uiux.reviewer`**, row
  `qitem-20261111063016-576619bdb0521c32`. `advisor` recused itself (the desk sits on its
  outgoing edge and changes its job), which is correct. The reviewer reads and prepares now;
  the worktree comes after S-002 lands.

### S-007 — alarm colour and priority reconciliation (PR #58)

- **What:** `docs/05-enterprise-governance/isa-18-2-alarm-engine.md` §7 relabelled so its colour
  column reads "Illustrative Color", a note naming the two authorities (DTCG as controlled
  definition, `frontend/src/tokens.css` as implementation), and a new §7.1 recording that the two
  priority vocabularies do not agree. +45/−1, docs only.
- **Why:** the engine doc's table contradicts the rendered palette on all four values
  (`#D32F2F`/`#DC2626`, `#F57C00`/`#EA580C`+`#D97706`, `#FBC02D`/`#CA8A04`, `#0288D1`/`#2563EB`)
  and its four hexes appear in **no code**. The DTCG file and the token file agree on every alarm
  value, so the engine doc is the lone outlier.
- **The real finding, §7.1:** the vocabularies are not positionally equivalent. Engine doc
  Medium = "quality deviation or minor equipment fault"; DTCG Priority 3 = "low warning threshold
  excursion". Engine doc Low = "advisory maintenance or efficiency notice"; DTCG Priority 4 =
  "sensor fault, wire break, communication packet drop". The engine doc has **no class for a
  sensor or communication fault**. A positional mapping files sensor faults as advisory
  maintenance — the quietest place a failed sensor can land. Recorded, not resolved: choosing a
  taxonomy is not a documentation call.
- **What it does not claim, deliberately:** that either scheme is wrong against ANSI/ISA-18.2.
  Neither document cites the clause that fixes the values, so the note says which scheme renders
  and that which is normative is "not established in this repository".
- **Verification:** gate **not run and not runnable** — no `scripts/check.sh` on `origin/dev`
  (S-002). Docs-only. Executed instead: diff touches only `docs/`, no deletions, the relative
  DTCG link resolves, §7.1 numbered under §7, and all four quoted DTCG priority descriptions
  match the token file verbatim. `dev_gatekeeper` additionally checked all six cited token names
  against `frontend/src/tokens.css`.
- **Related hygiene finding, recorded not fixed:** there are **three** tracked copies of the token
  file and only one is imported — `frontend/src/tokens.css` (172 lines, imported by
  `ui-shell/src/index.css:1`), `frontend/src/styles/tokens.css` (154, zero references) and
  `frontend/plugins/ui-faceplate-lib/src/tokens.css` (154, declared export, nothing imports it).
  The two unimported copies are byte-identical. `git grep "tokens.css" origin/dev -- frontend/`
  returns exactly one import in the whole tree, so editing the wrong copy silently changes
  nothing. Needs its own repo-hygiene row.
  **Two failure modes, not one** (found by `governance.scribe`): every alarm token is defined
  **twice** in the live file, once per theme block. Editing the wrong *copy* does nothing;
  editing the wrong *theme block in the right copy* changes one theme and leaves the other stale
  — worse, because it looks like it worked. The counts below are per-file lines, not per-token.
- **State:** `in-review`, PR #58, `mergeStateStatus: BLOCKED` (no runner).

### S-008 — the ST runtime engine is never instantiated (found 2026-11-11)

- **What:** Give `mythos-strucpp` a production construction path. Today the whole engine is
  library-only: `StrucppRuntime::new` has **zero call sites repo-wide** and `QualityGate::new`
  appears only under `crates/mythos-strucpp/tests/`. `mythos-mcp` is the sole crate depending
  on it (`crates/mythos-mcp/Cargo.toml:17`) and does not reference it in `src/`.
- **Why:** this is the *real* form of what the ledger used to call "blocker (1)" on S-004 —
  "`record_comm_loss` never reaches shared memory". The method is correct and tested; nothing
  calls it. Consequence an operator can see: **the plant cannot produce `BAD_COMM_FAILURE` at
  all**, because no `QualityGate` exists to latch it. That is a live honesty gap, not a
  refactor.
- **Found by:** `pod_hmi.reviewer`, widened and confirmed by `pm` (the reviewer found the gate;
  `pm` found the runtime). Verified on `origin/dev` @ `5b8ee51`:
  `git grep "StrucppRuntime::new" origin/dev -- crates/` → zero hits.
- **Not HMI work and not sequenced against S-004.** S-004's negative test asserts a frontend
  property and does not need this. Do not let S-004 absorb it.
- **Done when:** named, not yet specified — this needs a scope decision before it is a slice:
  who constructs the runtime (a binary? `mythos-mcp`? the core API?), and whether the gate is
  wired with `with_shm_publication` on that path. **A slice whose "done when" cannot name its
  verification command is not a slice** — so this row stays `planned` until that is answered.
- **State:** `planned`. Owner `pod_backend.builder`. Not cut.

### S-009 — topology provenance docs (PR #56)

- **What:** `docs/04-hmi-and-ui/network-topology-view-spec.md` states the discovery
  precondition on the "automatically assigns the configured PROFINET device name" sentence;
  new `specs/adr/ADR-009-discovery-yields-candidates.md` (Status PROPOSED) records *"Discovery
  yields candidates; commissioning is an explicit act."*; `docs/README.md` index entry.
  3 files, **+147/−0**, docs only.
- **Why:** `:199-201` is an APPROVED controlled document. Read without the precondition, it
  licenses exactly the auto-provisioning that `NetworkCanvas.tsx` and `provenance.tsx`
  deliberately refuse and that PR #33 removed from the render path. This is ledger row
  **SCRIBE-1** and **SCRIBE-3** made concrete.
- **Branch/worktree:** `docs/topology-provenance-spec` at
  `.claude/worktrees/docs-topology-provenance`, `origin/dev` @ `5b8ee51`, head `72630ad`,
  3 commits, clean.
- **This row was MISSING until 2026-11-11T06:1xZ.** PR #56 was open, BLOCKED, with three
  commits and no ledger entry — a PR with no slice, which `CULTURE.md` §6 refuses. Found
  independently by `dev_gatekeeper` (from the PR side) and `pm` (from the ledger side). The
  slice content itself is sound; the defect was that nothing owned it.
- **Not to be confused with** the SCRIBE queue rows: SCRIBE-1 (`qitem-…53040-6c7ac449`) and
  SCRIBE-3 (`qitem-…53047-1585f8fe`) are still `blocked` in the queue while this PR
  implements them. Closing them is `scribe`'s call, on merge — not in advance of it.
- **VERDICT IS STALE — the head moved past it.** The approval says "Reviewed at head
  `72630ad`"; the current head is **`139c263`** (the ADR-header commit `scribe` made in response
  to the reviewer's non-blocking note). **The approval does not cover the diff as it stands.**
  Filed by `scribe` as `qitem-20261111070221` with two resolutions and a recommendation
  (**A: the S-009 reviewer re-reads the one changed line at `139c263`**). `pm` accepts A —
  it is one line, it keeps the approval honest, and B (revert the commit, new slice) would throw
  away a correct fix to avoid a two-minute re-read.
  **This is the pattern the merge rule above exists for:** an `APPROVED` that names a head SHA
  must be read against the current head, not taken as a property of the PR.
- **State:** `in-review`, **review APPROVED at `72630ad`** — but the branch has moved since.
  **The branch moved after the verdict: `139c263`** — a one-line ADR header change
  (`Deciders (proposed — to be recorded at acceptance)`) made in response to the reviewer's
  non-blocking note. **The verdict does not automatically cover `139c263`.** That is the same
  approval drift `pod_backend.reviewer` avoided by re-verifying S-003 at its new head; here the
  reviewer should confirm in one line, which is cheap and keeps "APPROVED" honest.
  **`scribe` deliberately did NOT promote ADR-009 to ACCEPTED**, and the reason is worth
  keeping: the ADR records a decision that is right from the code, not that Adrian, Aria or Neo
  have read it, and `scribe` has no evidence they have. Promoting it would assert an acceptance
  it cannot show — the same defect one level up from the one being fixed. **The qualifier flips
  to a plain "Deciders" by the edit that makes it true**, which is the rule already applied to
  the TSK-801 cell.
  PR #56, `mergeStateStatus: BLOCKED` (no runner). Owner `governance.scribe`.
  **Reviewer: `pod_hmi.reviewer`**, row `qitem-20261111061830-6355b9695e358d3b`, closed
  `no-follow-on`. Verdict at PR comment `6788706671`. All four checks pass: the precondition is
  a hard "valid only when", not a hedge, plus a normative "shall not create a station, an
  address, or a cable"; `fbaaa4d` correctly marks #50 open/not-merged and states the rule is
  normative whether or not #50 lands; all nine citations check out against the trees.
  **`scribe` cannot review its own slice** — that is why it went to hmi.
  **SCRIBE-1 and SCRIBE-3 queue rows were deliberately left open** — they close on merge, not
  on approval.
- **One non-blocking note for `scribe`, raised by the reviewer:** ADR-009 is the **only**
  `PROPOSED` ADR in the repo — all eight others are `ACCEPTED` — and it reuses the accepted
  template, so `Deciders: Adrian (Board of Commissioners), Aria (CEO), Neo (VP Software)`
  imports the meaning *"who accepted it"* onto a decision nobody has accepted. Suggested
  one-line fix **at acceptance**: `Deciders (proposed):`. Not blocking; the status is
  unambiguous in two places and the doc is not in force.
- **A docs PR does not merge on "docs only" without a named verdict** — that is the gap this
  row closed when it was created.

### S-010 — the light theme is defined but not selectable (cut 06:17Z)

- **What:** four controlled documents say Light Clean Slate is a supported mode; nothing in the
  repo applies `data-theme` or `.theme-light`, so the light block is dead CSS. One sentence in
  each doc saying the theme is defined and not currently selectable. 3 files, docs only, no
  deletions. Implements ledger row **SCRIBE-8** (`qitem-20261111060911-94d5c7fb3056b390`).
- **Claim verified by `dev_gatekeeper` at cut time, not taken from the request:**
  `git grep -nE "data-theme|theme-light" origin/dev -- frontend/` → 9 hits, **all of them CSS
  definitions**, in the three tracked copies of the token file
  (`src/tokens.css:7,117,118`; `src/styles/tokens.css:7,99,100`;
  `plugins/ui-faceplate-lib/src/tokens.css:7,99,100`). Zero hits anywhere that *applies* either
  selector. "Defined but not selectable" is measured, not inferred.
- **Branch/worktree:** `docs/theme-status-honest` at `.claude/worktrees/docs-theme-status`,
  `origin/dev` @ `5b8ee51`, 0 behind, 0 dirty at cut. Owner `governance.scribe`.
- **A third blocked docs PR was considered and accepted.** S-009 (PR #56) and S-007 (PR #58)
  are both docs-only and BLOCKED on the same infrastructure fault. Cutting anyway is deliberate:
  all three are independently reviewable now, they touch disjoint files so there is no ordering
  hazard, and withholding decided work does not restore a runner. It trades a durable record for
  a tidier queue.
- **Review assigned 2026-11-11: `pod_uiux.reviewer`**, row
  `qitem-20261111065743-b96efc4fec2251ae`. **PR #59 was open with no slice row and no reviewer —
  the fourth time today** (S-003, S-009, S-011, S-010). That is a pattern, not four accidents:
  a branch gets pushed and nobody creates the row that makes its verdict owned. `pm`'s own
  check for the next pass: **for every open PR, is there a slice row AND a reviewer row?**
- **Review verdict 2026-11-11: APPROVED** at `16c10e3`, `pod_uiux.reviewer`, row to
  `dev_gatekeeper` `qitem-20261111070021-e427c68e130901c2`. All four criteria re-measured rather
  than taken from the row: 9 grep hits all CSS definitions with **zero apply sites**; no
  unmerged-work references (`S-0xx`/`PR #`/`qitem-`/`rig/SLICES`/`DECISION-DEFAULTS` all absent);
  docs only, 4 files; and the **token-file trap was avoided** — none of the three `tokens.css`
  copies is touched. "Not selectable", never "unsupported".
- **The diff carries a SECOND claim `pm`'s row did not describe, and the reviewer verified it
  rather than waving it through:** an implementation-status note in `access-mode-matrix.md` §6.3
  about the simulation path, with all five assertions checked (fn at `:395`, both match arms, no
  `/api/v1/simulation/` route, `ErrSimulationModeDisabled` absent, no task schedules an inject
  endpoint). It correctly tells implementers that `POST /api/v1/simulation/inject` is **design
  intent, not a shipped API**.
- **Non-blocking finding, deliberately not blocked — and the judgement is the point:** the note
  says the lockout *"is implemented"* and `validate_forcing_permitted` *"is the enforcement
  point"*. True of the code, but **the gate is never constructed in production**:
  `QualityGate::new` 11 hits all under `tests/`, `StrucppRuntime::new` 0 hits anywhere,
  `validate_forcing_permitted` called only from `tests/sync_and_gating_tests.rs:90,116` — **verified
  by `pm`**. So the lockout is **defined, tested and UNWIRED**. The reviewer did not block,
  reasoning that (a) the function is genuinely where enforcement would live, so an implementer is
  pointed at the right place, and (b) the note itself says there is no HTTP endpoint, so nothing
  can force today either way. **"Blocking a net-honesty docs PR over one word is the over-rigour
  failure, not the care one."** Recommended one-clause fix: *"is implemented"* → *"is defined and
  tested"*, or add *"the gate is not yet constructed by any runtime"*.
  **This is the same fact behind S-011/TSK-801's blocker (1)** — the sharper truth is that the
  gate never runs at all. **S-008** is that gap as a slice.
- **Two traps recorded for the builder:** `./scripts/check.sh` is absent at this base (blocker
  B2), so the receipt says "not run and not runnable" rather than substituting silence. And the
  three token-file copies are a trap for a reviewer who asks "also fix the light block" —
  editing the two unimported copies changes nothing, because only `frontend/src/tokens.css` is
  imported.
- **Reviewer not yet assigned.** `scribe` cannot review its own slice; needs a named verdict
  before merge, same rule as S-009.
- **State:** `in-review`, **PR #59**, head `16c10e3`, base `dev`, 4 files +21/−3, zero deletions,
  `mergeStateStatus: BLOCKED` (no runner). Pushed and opened by `dev_gatekeeper` after
  re-running the diffstat, the no-deletions check, the no-non-docs-paths check, the JSON parse
  and the selector grep — the author's receipt was verified, not taken.

### S-011 — TSK-801's two stale quality blockers corrected (PR #60)

- **What:** `specs/tasks.md`, the TSK-801 acceptance cell: replaces the "Two known blockers for
  quality visibility" text with what is actually true. One file, **+1/−1**, docs only.
- **Why:** both claims were false against `origin/dev` @ `5b8ee51`, and this cell is what the
  S-004 builder reads. Blocker (1) sent a builder hunting a payload field that already ships;
  blocker (2) pointed at the wrong layer entirely. Raised by `pm` as SCRIBE-10
  (`qitem-20261111061844-4cccf2c08fa3f25d`) after `pod_hmi.reviewer` found it; both claims
  re-verified by `pm` and again by `scribe` at the amendment tree.
- **The constraint `scribe` caught, and it is the important part:** `pm`'s row said "replace it,
  because it is now tracked as slice S-008". **That would have written a pointer to an unmerged
  artifact into a controlled document on `dev`** — the exact defect PR #56 had to fix in
  `fbaaa4d`. The amendment states the gap in code terms that exist on `dev` and leaves the
  ledger pointer for after S-002 lands. `pm`'s row was wrong; `scribe` was right to refuse the
  wording while keeping the substance.
- **`scribe` chose to correct it NOW rather than after S-008 is scoped**, against `pm`'s
  suggested sequencing, and gave the better reason: S-004 is next and this cell is what its
  builder reads, and the replacement asserts no unmerged work so the sequencing rule does not
  apply to it. Correct call.
- **Branch/worktree:** `docs/tsk-801-quality-blockers-corrected`, pushed, head `7e1535e`.
  **Not cut as a worktree** — `scribe` worked the branch directly, which is the same deviation
  S-002 records for the repo root. Recorded rather than hidden.
- **State:** `in-review`, PR #60, `mergeStateStatus: BLOCKED` (no runner).
  **No reviewer assigned yet** — this row exists because PR #60 was open with no slice row at
  all, which is the defect S-009 and S-003 both had. Assignment is `pm`'s next action.
  **A controlled-document amendment does not merge on "docs only" without a named verdict.**

### S-013 — the resolver fails open on an unrecognised quality (deferred from S-004)

- **What:** `resolveComponentQuality` returns `GOOD` for **any** unrecognised quality string when
  the payload carries a value. One line at `componentData.ts:160` plus a test.
- **Why it is deferred rather than in S-004:** `pm` ruled it in scope; `pod_hmi.builder` showed it
  is **provably unreachable from S-004's path**, and the reasoning is right:
  `resolveComponentLive` returns a `ProcessDataQuality`, so `{...comp, quality:
  liveResolution.quality}` is a value `asQuality()` accepts, `:149` returns it, and `:160` never
  runs. **`pm` verified this and accepted the deferral** — the ruling's purpose (stop the defect
  becoming live) is met without the change.
- **Why it is still a slice:** the defect is real and the next caller that passes a **raw wire
  quality string** through will hit it. Measured against the resolver extracted verbatim from
  `origin/dev`: `quality: "BAD"` + value → `GOOD`, `quality: "UNCERTAIN"` + value → `GOOD`,
  `quality: "BAD_COMM_FAILURE"` + value → `GOOD`. **A comm failure with a stale value in the slot
  renders as a live reading.**
- **The rule to implement:** a payload that **carries** a quality key whose value the module does
  not recognise must resolve to `BAD_QUALITY`. **A payload with no quality key stays `GOOD`** —
  `componentData.test.ts:41` pins that and it stays green.
- **Done when:** a test using the wire's real vocabulary (`quality: 'BAD'`, `'UNCERTAIN'`,
  `quality_name: 'BAD_COMM_FAILURE'`, each **with a value present**) asserts `BAD_QUALITY` and
  **empty values**; mutation-prove by reverting `:160` and the stale-value case goes red.
  **Test with a bound component** — my first probe omitted `bindings`, got `NOT_BOUND` for every
  case, and would have "proved" the path honest.
- **Verification:** `cd frontend && npx vitest run --no-cache` from `frontend/`. Failing test names,
  not counts.
- **State:** `planned`. Owner `pod_hmi.builder`. Not cut — it is small and waits on nothing.

### S-012 — the faceplate cannot name the specific bad quality (found 2026-11-11)

- **What:** let a faceplate say *which* bad quality it is showing. Today it can only say "bad".
- **Why — and it is a wrong-diagnosis hazard, not a missing feature.** `ProcessDataQuality` has
  three values (`types.ts:33`), so `BAD_COMM_FAILURE` and `BAD_SENSOR_FAILURE` both become
  `BAD_QUALITY`. `BAD_QUALITY_TEXT = 'BAD COMM FAILURE'` (`processData.ts:22`) is returned for
  **every** bad quality by `dataQualityLabel` (`:53-63`) and rendered by
  `DataQualityBadge.tsx:16`. **An operator at a failed sensor reads "BAD COMM FAILURE".** A
  confident wrong cause on a safety HMI is worse than an honest absence.
- **Why it is not S-004:** it needs the quality contract to carry more than three values — a
  field or prop beyond the enum — so it touches `types.ts`, `processData.ts`,
  `DataQualityBadge` and every faceplate. That is wider than "one tag to one faceplate", which is
  S-004's explicit scope limit. Found by `pod_hmi.reviewer` while reviewing PR #60, verified by
  `pm`; `git grep quality_name|quality_code origin/dev -- frontend/` → **zero hits**, so the
  frontend never reads the fields that carry the distinction.
- **The interim obligation sits in S-004, not here:** S-004 must not make the wrong cause
  reachable — it passes the specific quality through and the badge falls back to a **neutral**
  label, never to `'BAD COMM FAILURE'`, when the cause is unknown to it. See S-004.
- **Done when:** a faceplate driven by `quality_name: 'BAD_SENSOR_FAILURE'` names a sensor fault
  and a faceplate driven by `'BAD_COMM_FAILURE'` names a communication failure, neither
  asserting the other; and a faceplate with no specific quality renders a neutral label rather
  than a guess.
- **Verification:** `cd frontend && npx vitest run --no-cache` from `frontend/` (never a worktree
  root). Failing test names, not counts. Mutation-prove the fallback: restore the hardcoded
  string and the sensor-fault test must go red.
- **Sequencing:** after **S-004** lands — it is S-004 that makes the wrong diagnosis live, and
  S-004's neutral fallback is what makes this slice's absence honest rather than harmful.
- **State:** `planned`. Owner `pod_hmi.builder`. Not cut.

### Merge readiness — 2026-11-11T07:05Z

**Every open PR now has a verdict at its current head, or a stated reason it does not.**
That is the first time today that sentence has been true.

| PR | Slice | Head | Verdict | Blocked by |
|---|---|---|---|---|
| #50 | S-001 | `ab93602` | **APPROVED** at head | B1 only |
| #56 | S-009 | `139c263` | **APPROVED** at head (carried from `72630ad`) | B1 only |
| #57 | S-003 | `5cf3984` | **APPROVED** at head (carried from `7d4798c`) | B1 only |
| #58 | S-007 | `2bfac2e` | **APPROVED** at head | B1 only |
| #59 | S-010 | `16c10e3` | **APPROVED** at head | B1 only |
| #60 | S-011 | `7e1535e` | **APPROVED** at head | B1 only |
| #55 | S-002 | `dba71c7` | no reviewer — it is the harness that adds the review process | B1, plus its own review |

**B1 is the only blocker for six of the seven.** When the link is fixed they can merge in any
order; they touch disjoint files.

**Two process findings from the hour, both worth carrying forward:**

1. **A reviewer row should name the HEAD it is against.** Two of the four PRs reviewed in this
   hour had moved since their ledger row. A reviewer row without a SHA is the same defect as a
   verdict without one — found by `pod_uiux.reviewer`, who hit both ends of it.
2. **A verdict is not on one surface.** It lives in the union of the PR comment log and the queue
   rows, and **neither alone is evidence of absence**. Measured over one hour:
   - `gh pr view --json reviews` returns `[]` **in every case** — `gh` here is the PR author, so
     approve/request-changes are refused and verdicts land as issue comments or queue rows.
   - #50 and #58 have **zero** PR comments; both verdicts are queue-only. A comment-only check
     reports them unapproved. Two false negatives.
   - #59's verdict is a queue row alone.
   - #57's verdict list has three entries and only the **last** one names the current head.
   So the check is: **the last SHA-naming entry in the PR comment log AND the queue rows**, and the
   verdict must name the CURRENT head. `scribe` hit three instances of this in one hour from its own
   seat — comment-only said "no verdict" on #59, queue-only said "stale" on #57, comment-only said
   "unapproved" on #50/#58. All three were wrong in the same direction: one surface read as absence.
3. **The structural cause of four unreviewed PRs:** opening a PR is one action a builder takes;
   creating the reviewer row is a second action nobody owns. Making the ledger row the artifact
   that creates the reviewer row would close it. Raised by `pod_uiux.reviewer` and `scribe`
   independently; it is `pm`'s to decide.

### Sequencing (decided by `pm`, 2026-11-11 — revised 06:1xZ)

1. **S-003** (backend) is independent; review is **APPROVED** at `7d4798c`, awaiting CI only.
2. **S-001** (uiux) re-review is open at `21cb9c4`. Closes on that verdict.
3. **S-004** moved to **pod_hmi** and is **cut now** — the S-001 dependency is dropped, because
   its only rationale was "same pod" and the pod changed. File overlap with S-001 is nil.
4. **S-009, S-010 and S-011 are all APPROVED** and waiting on CI only. **Every one of them had
   no reviewer at all until 2026-11-11** — the same gap, three times, plus S-003. That is a
   pattern in how slices are opened, not four accidents.
5. **S-008** needs a scope decision before it is a slice.
6. **S-012** follows **S-004** — S-004 must ship the neutral-label fallback so S-012's absence is
   honest rather than harmful.
7. The **device-mode half** of TSK-801 waits on S-003 and is not yet a slice.
8. **S-005 → S-006** still follow S-002, and S-006 also follows S-005 — **F4 established that
   `role.md:78-79` cites `DECISION-DEFAULTS.md`, which `mythos-org.yaml` never delivers.** S-006's
   blocking items are closed by S-005's delivery entry, not by editing the desk role.

The HMI telemetry row `qitem-20261023085422` is `pending`, unclaimed, `blockedOn: null` —
its block released when the DeviceExecutionMode decision landed. It is carried by **S-004**;
it is not free work.

---

## MERGE RULE — "does this PR have a verdict?" is not answerable from the obvious command

Found independently by `governance.scribe` and `pm` on 2026-11-11, after both ran a PR sweep and
got the wrong answer.

**`gh pr view <n> --json reviews` returns `[]` even when a verdict exists.** Verdicts in this repo
are posted as **issue comments**, not PR reviews — because the `gh` account is the PR author, so
`request-changes` and `approve` are rejected. Measured: PR #56 → 1 issue comment, 0 reviews;
PR #57 → 2 issue comments, 0 reviews.

**And a verdict may not be on the PR at all.** S-010 (PR #59) is APPROVED, and that verdict exists
**only** in its review queue row (`qitem-20261111070021`, closed `done` to `dev_gatekeeper`).
`gh api …/issues/59/comments` → **0**. So a merger checking the PR alone sees an unreviewed slice.

**The rule — check both, in this order:**
```bash
gh api repos/:owner/:repo/issues/<n>/comments --jq '[.[]|select(.body|test("APPROVED|CHANGES REQUESTED"))]'
rig queue list --json   # find the slice's review row; is it closed with a verdict?
```
**A verdict counts if it is in EITHER place, at the CURRENT head.** Three distinct states a merger
must not confuse:

| state | example | what it looks like | what it is |
|---|---|---|---|
| verdict on the PR, current head | #57 @ `5cf3984`, #60 @ `7e1535e` | comment present | **clean** |
| verdict on the PR, STALE head | #56 — approved @ `72630ad`, head now `139c263` | comment present | **stale** — does not cover the diff |
| verdict only in the queue | #59 (S-010) | **no PR comment at all** | **approved, but invisible on the PR** |

**Why this matters more than it looks:** `mergeable: MERGEABLE` plus "docs only" plus no visible
review is exactly the shape a merger waves through. Three of four docs PRs were in one of the two
bad states when this was found. **The ledger's `APPROVED` must name the head SHA it was given at**,
which is why every approval recorded today carries one.

## Documentation rows raised by `governance.scribe` (2026-11-11)

From `qitem-20261111052325`. Each is spec drift, not code work; `scribe` owns them.
None is assigned to a pod and none blocks S-001.

| Row | Target | What |
|---|---|---|
| SCRIBE-1 | `docs/04-hmi-and-ui/network-topology-view-spec.md:201` | "Automated Device Replacement" says the neighbour "automatically assigns the configured PROFINET device name, and downloads its hardware parameters". That is only sound because the nominal config already names the slot — a precondition the spec never states. Read without it, the APPROVED sentence licenses the auto-provisioning that `NetworkCanvas.tsx:74` and `provenance.tsx:21` deliberately refuse. ADR-004 `:20`/`:74` describe discovery-for-a-human, not unattended provisioning. Fix: state the precondition, or mark the sentence unverified. |
| SCRIBE-2 | `…spec.md` §8 `:271-366` | The TypeScript contract section has no `provenance` field on `NetworkCanvasProps`, `DevicesAndNetworksState` or `HardwareStationNode`. Stale the moment #50 merges. Needs a contract addition, not a prose note. |
| SCRIBE-3 | `specs/adr/` | ADR candidate: *"Discovery yields candidates; commissioning is an explicit act."* A real architectural decision with a live alternative (auto-provision on discovery, which `:201` reads as approved), currently recorded only in source comments. Cheap now, expensive to reconstruct after the transport slice. |
| SCRIBE-4 | vocabulary | `TopologyProvenance` (`ui-topology/src/provenance.tsx:29`) collides in name with `ProcessDataQuality` (`ui-faceplate-lib/src/types.ts:28`), which this repo already describes as provenance. Two axes, so two names is defensible — but name it before a third slice imports both and infers they agree. Same class as the `InterlockCondition` smell below. Record, do not block. |

## Worktree state — re-measured 2026-11-11T06:50Z, after the `/tmp` sweep

`./scripts/check-slices.sh` → **11 worktrees, 2 stale**:

```
ci-frontend-gate          ci/frontend-gate                          7 ahead  144 behind  STALE:behind
dev-live                  HEAD                                      0        0           ok
device-execution-mode     refactor/device-execution-mode-canonical  2        0           ok
docs-alarm-colour         docs/alarm-colour-reconcile               2        0           ok
docs-theme-status         docs/theme-status-honest                  1        0           ok
docs-topology-provenance  docs/topology-provenance-spec             4        0           ok
docs-tsk801-correction    docs/tsk-801-quality-blockers-corrected   1        0           ok
hmi-live-telemetry        feat/hmi-live-telemetry                   0        0           ok
rev-uiux-50               HEAD                                      1        0           ok
topology-mode-coverage    test/topology-all-view-modes              3        0           ok
ui-isa101-consolidation   feat/ui-isa101-consolidation              3 ahead  146 behind  STALE:behind (+4 dirty)
```

**Six `/tmp` scratch worktrees removed 2026-11-11T06:50Z by `dev_gatekeeper`.** They had
accumulated in the shared registry at `bd30ec6`, 144 behind, and inflated the count to 17/8 —
which buries the two genuinely stale trees in noise. **Nothing unique was lost, and that was
proved rather than assumed:** the two that carried 157 staged entries each had
`git write-tree` produce `21a3730…`, **byte-identical to `origin/dev^{tree}`**; the other four
were clean. They were extracted trees where someone ran `git add -A`, not unique work.

**Rule recorded:** an isolated tree for a gate run comes from `git archive` into a scratch
directory, **never `git worktree add`** — a worktree registers itself in the repo's shared
registry and outlives the run.

`ci/frontend-gate` is now 144 commits behind and its content (the frontend gate job)
**already exists on `origin/dev`** as `frontend-tests` (`ci.yml:286`), so the branch has
nothing left to contribute — it is a delete candidate, not a rebase candidate. The ledger
line for it below ("Rebase and slice") is superseded by this. `dev-live` sits exactly on
`origin/dev` and is the autospin build worktree: **healthy, not stale.** The naive
`merge-base --is-ancestor HEAD origin/dev` check in the role description flags it falsely;
`check-slices.sh` distinguishes strict-ancestor from equal-to-base and is the correct check.

**`device-execution-mode` is now CLEAN at `5cf3984`** (verified 2026-11-11T06:5xZ). It carried
2 uncommitted mutation-test files earlier in the pass, which is why a gate run there measured a
mutated tree; that is resolved and the PR head moved to `5cf3984` with the docstring commit.
**The lesson survives the resolution:** a review that mutation-tests **in the slice worktree**
makes every subsequent gate result there ambiguous, and two seats doing it concurrently made it
worse. **Mutate in an isolated `git archive` copy** — both reviewers now do, and it is why their
receipts are trustworthy.

### Repo root is DIRTY — an unclaimed edit, not a PR-integrity problem

`git status` in the repo root, 2026-11-11T06:1xZ:

```
 M rig/agents/desk/agent.yaml
```

Committed at `9e42e46`: line 7 has `model:` with an **empty value**. Working tree: the same
line with a model value. Uncommitted, and unclaimed by any seat.

**Corrected, twice, before this paragraph was written.** My first draft called this "S-002's
slice tree, so PR #55 no longer matches what is under review". That was wrong, and
`dev_gatekeeper` caught it with a measurement:

```
git cat-file -e ${c}:rig/agents/desk/agent.yaml
  dba71c7 (PR #55's pushed head)  absent
  d7ae80d                         absent
  23bf7c5 (unpushed)              PRESENT
  9e42e46 (local HEAD, unpushed)  PRESENT
```

`gh pr view 55 --json files` lists 31 files and **none** under `rig/agents/desk/`. The desk
seat enters the branch only in the three unpushed commits, so the dirty file **cannot**
compromise PR #55 — it is not part of what is under review. `pod_hmi.reviewer`'s inference
was sound in shape and wrong in fact; so was my repeat of it. What is true is narrower: an
uncommitted edit sits on top of the unpushed part of the branch, in S-005/S-006 territory,
claimed by nobody.

**A reading hazard worth recording, because three seats got three answers from one file.**
The model-id string in these yaml files is stripped from agent tool output in this rig.
`sed`, `grep`, `awk` and `git diff` all render the line as `model:` with the value silently
gone; `awk '{print NF}'` even reports 2 fields. Reported readings of the same bytes across
seats included "empty", "empty" and `fable`→`opus`. Only byte-level tools show the truth:

```bash
sed -n '7p' rig/agents/<seat>/agent.yaml | od -An -c    # or xxd, or rev
```

A check that silently answers "empty" for every seat is the same failure class `CULTURE.md`
§4 is about: the command ran, so the answer looked measured. Any claim about a seat's model
id must come from `od`, not from `grep`.

Reported independently by `pod_hmi.reviewer` and `dev_gatekeeper`; neither edited it, both
correct. **`pm` does not fix it and does not commit it.** `dev_gatekeeper` owns the repo root
and decides whether the edit belongs in S-005/S-006 or reverts.

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
marked PRE-COMMITTED PREREQUISITE, and scopes exactly this.

**No longer true — "two code-verified quality blockers" (corrected 2026-11-11T06:1xZ).**
This paragraph used to repeat `specs/tasks.md:230`'s two blockers as if both were live. Both
statements are wrong against `origin/dev` @ `5b8ee51`, and this copy is the one that misleads
because it is the copy builders read:
- **"`build_telemetry_event` collapses quality to `GOOD`/`BAD`/`UNCERTAIN`"** — FALSE now.
  It emits `quality`, `quality_code` and `quality_name` (`telemetry.rs:187-189`), pinned
  through the real SSE route by `crates/mythos-core/tests/telemetry_quality_fidelity_tests.rs`.
  Fixed by `aa6cce0`, an ancestor of `origin/dev`. **S-004's builder is unblocked on this.**
- **"`record_comm_loss` does not write `TagSlot.quality`"** — FALSE as stated. It does, when
  the gate has a publisher (`quality_gate.rs:134`). The true gap is that no production path
  constructs the gate at all, and `StrucppRuntime::new` has zero call sites — that is **S-008**,
  a backend slice, not a frontend workaround.
`specs/tasks.md:230` still carries the stale text. That is a controlled document and the row
belongs to `governance.scribe`; raised as a scribe row on 2026-11-11, **not** edited here.

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
