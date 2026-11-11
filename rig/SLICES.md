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
| `S-006` | — | `feat/desk-seat` *(exists @ `3995106`; main copy `4496ce0`)* | *(not cut)* | `origin/dev` @ `75200c0` | **CUTTABLE** — S-002 and S-005 both merged; 2 blocking findings stand | `dev_gatekeeper` |
| `S-008` | backend | *(not cut)* | — | `origin/dev` | `planned` — **blocked on a scope decision** | `pod_backend.builder` |
| `S-012` | hmi | *(cut requested)* | — | `origin/dev` @ `75200c0` | **CUT REQUESTED** — S-004 merged, unblocked | `pod_hmi.builder` |
| `S-013` | hmi | *(not cut)* | — | `origin/dev` | `planned` | `pod_hmi.builder` |
| `S-014` | hmi | `feat/setpoint-draft-committed` | `.claude/worktrees/setpoint-reconcile` | `origin/dev` @ `08245d0` | **APPROVED** @ `b09074b`; merge row `qitem-20261111130520` | `dev_gatekeeper` |
| `S-015` | backend | `fix/mythos-update-fabrication` | `.claude/worktrees/update-fabrication` | `origin/dev` @ `7c35e80` | `open` — cut 2026-11-11T12:07Z | `pod_backend.builder` |
| `S-016` | backend | `ci/release-build-job` | `.claude/worktrees/release-build-job` | `origin/dev` @ `7c35e80` | **BUILT** @ `581c2f2` — awaiting the red-direction receipt | `pod_backend.builder` |
| `S-017` | scribe | *(not cut)* | — | `origin/dev` @ `75200c0` | `planned` — MYTHOS-ARCH-008 gate wording; drop two phrases with no producer | `governance.scribe` |
| `S-019` | — | *(cut requested)* | — | `origin/dev` @ `7c35e80` | **CUT REQUESTED** — CULTURE §12: an assertion against a constant proves plumbing, not meaning | `pm` |

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
- **Head: `ab93602`, THREE commits on `origin/dev`.** `52aa7f5` (the original), `21cb9c4`
  (the two-axis split), and `ab93602` `fix(ui-topology): derive conformance instead of accepting
  it as an input` — the fix for the one blocking item the re-review raised. Worktree clean,
  0 behind `origin/dev`.
  **The conformance prop is GONE, not made optional** — `NetworkCanvas` now takes
  `nominalDeviceIds` / `actualDeviceIds` and derives, so *"the caller asserted MATCHES"* is not a
  state the type can express. **That is stronger than the review asked for**, and `dev_gatekeeper`
  verified it independently.
  **The seam where a default could have hidden was checked, not assumed** (`governance.scribe`):
  both replacement inputs are **optional** (`NetworkCanvas.tsx:64-65`), and *two optional inputs
  deriving a conclusion is exactly where a default `MATCHES` hides* — the same class as the banner
  that defaulted to `UNKNOWN`. Read the function:
  ```ts
  if (!nominal || !actual) return 'NOT_COMPARED';   // provenance.tsx:111-113
  ```
  **It does not fabricate.** The banner also renders conformance only when
  `claim.source !== 'UNREAD'`. So the honesty holds at the one seam where it would have been
  easiest to lose.
  **The test that could not fail was replaced, not patched:** it used to hand
  `conformance="MISMATCH"` to the canvas and assert `data-derived="true"` — proving only that the
  component sets an attribute on whatever it is given. It now supplies **data** and asserts the
  **conclusion** the data implies (same ids → `matches`, different → `mismatch`, one side only →
  `not-compared`).
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

  **SECOND FAULT, FOUND 2026-11-11T07:12–07:16Z — it appears only once the network recovers.**
  At 07:10Z the link healed on its own (6/6 requests at 0.03–0.32 s) and the listener stayed up
  for 106 s with 0 restarts — the first time it had outlived a few seconds all day. The scale-set
  controller then received the assignment and **scaled to 10 ephemeral runners**. They could not
  register, and the controller logged, repeatedly:

  ```
  Failed to create ephemeral runners secret
  "failed to generate JIT config ... request POST
   https://broker.actions.githubusercontent.com/rest/_apis/runtime/runnerscalesets/1/
   generatejitconfig ... 404 ... RunnerScaleSetNotFoundException:
   No runner scale set found with identifier 1."
  ```

  **The cause is an ID mismatch, and it is the state the outage left behind.** The listener
  re-registered itself against **`runnerscalesets/2`** (its own log:
  `POST .../runnerscalesets/2/sessions`), while the cluster's EphemeralRunnerSet carried
  `runnerScaleSetId: 1` and the controller kept calling `/runnerscalesets/1/`. GitHub has no scale
  set 1 any more, so every runner creation 404s and the ephemeral runners sit with no `RUNNERID`.
  `gh api orgs/firmansyah-co/actions/runner-scale-sets` also returns **404**.

  **Why this matters for the remedy:** the network fix is necessary but **not sufficient**. Once
  the cable is in, the ARC release must be **re-registered** — the listener's scale-set identity
  and the EphemeralRunnerSet's `runnerScaleSetId` have to agree, which a pod restart does not do
  because the stale ID is in the CRD, not the pod. The clean path is to reinstall/re-run the
  `arc-runner-org` helm release so the listener and its EphemeralRunnerSet are recreated together.

  **Measured state, so the next seat does not re-derive it:** the listener pod cycles
  (`...-6b8f777f-listener`, age 3–18 s, 0 restarts — it is *replaced*, not crashing), the
  EphemeralRunnerSet name rotates (`arc-runner-org-s2962` → `arc-runner-org-66jz6`), and
  `actions/runs?status=in_progress` stayed at **0** for 10 minutes of polling.

  **Acceptance test for the full fix — three gates, not one:** (1) three 15-packet pings at 0%
  loss with mdev under ~50 ms; (2) the listener pod survives past ~10 minutes; (3) **a runner
  registers** — `kubectl -n arc-runners get ephemeralrunners` shows a non-empty `RUNNERID`, and
  a CI run leaves `queued` for `in_progress`. Passing (1) and (2) alone is what 07:10Z looked
  like, and it was not working.

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

### FIRST MERGE SINCE 2026-10-27 — PR #60 landed at 2026-11-11T08:08:30Z

`origin/dev` moved `5b8ee51` → **`6dad46a`** (`docs(tasks): correct TSK-801's two stale quality
blockers (#60)`). **The release train has moved for the first time in fifteen days.** Verified by
`pm` on the remote, not from a report.

**Two consequences, both immediate:**

1. **Every open PR is now BEHIND.** `S-004` (PR #61) reports `mergeStateStatus: BEHIND`, not
   `CLEAN` — its checks are green at `3c683be`, but **a green receipt does not cover a rebased
   branch.** Rebase → re-gate → verdict at the NEW head. This is precisely the case where a stale
   receipt looks valid, and it is why every approval in this ledger names a head SHA.
2. **The merge queue is real now.** Six more PRs are approved at their current heads. Each needs the
   stale-claim check before merging: does the verdict name the **current** head, and is it on the PR
   or in the queue (both count, neither alone is evidence)?

**B1 has not been fixed — it has been outrun.** `enp86s0` still has no cable and the uplink is still
`wlan0`. **A merge that lands during a good window is a merge that passed on a good window.** See the
flakiness note below: a flaky gate that FAILS is safe; a flaky gate that PASSES is the risk.

### B1 — CI IS EXECUTING AGAIN, 2026-11-11T07:3xZ. The blocker is now FLAKY, not DOWN.

**Measured by `pm` directly, not carried:**
```
run 42977515106  feat/hmi-live-telemetry @ 3c683be  (S-004 / PR #61)
  Rust Workspace Test Suite & Lints          success, 17/17 steps
  Frontend Web Workbench Test Suite & Lints  in_progress, 13/17 steps
run 42980085256  chore/rig-harness @ e27263d (S-002 / PR #55)   queued
run 42974401479  docs/topology-provenance-spec @ 139c263 (S-009) completed/failure
```
**First workflow execution on this repo since 2026-10-27.**
`kubectl -n arc-systems get pods` → listener **1/1 Running, 0 restarts, 24m**;
`autoscalingrunnersets` → **10 current runners**; `arc-runner-org-66jz6` → pods **Running**.

**S-009's red is INFRA, not the slice.** `gh api .../runs/42974401479/jobs` → the Frontend job's
**only** failed step is **`Set up job` (step 1)**. Nothing checked out, nothing ran. **Do not read
it as a docs defect** — and do not record S-009 as broken on it.

**THE ASYMMETRY THAT MATTERS, raised by `desk-desk` and it is a safety point, not bookkeeping:**
**a flaky gate that FAILS is safe — it stops merges. A flaky gate that PASSES is not**, because it
lets a bad commit through and looks identical to a green one. On a lossy link a run can now succeed
by landing in a good window, and nothing in the verdict distinguishes "passed on a good link" from
"passed".
**So: merges during the intermittent period carry a known, unmeasured risk, and the ledger says so
rather than only saying CI works.** The mitigations available today are all weak — a re-run adds
samples but does not bound the risk, and the durable fix is the cable. **Do not read a green check
during this period as a property of the commit; read it as a property of the window it ran in.**

**What this changes operationally.** B1 was "no runner, every merge holds". It is now **"the runner
is intermittent"** — which is a different instruction: **retry a queued or failed-infra run rather
than waiting, and re-check `in_progress` with a real runner before concluding anything.** Seven PRs
are approved and the merge queue can actually move.

**What is still true, re-measured 2026-11-11T07:5xZ:** the underlying fault was never fixed, only
outrun.
```
enp86s0   DOWN  <NO-CARRIER>  carrier=0, no address
ip route get 20.205.243.168 -> via 192.168.100.1 dev wlan0 src 192.168.100.95
```
**The cable is still not in.** The link healed on its own over wifi and CI started. So the cable is
still the durable fix — **but it no longer gates merges; it gates whether this stays true.**

**Adrian answered "plug in the ethernet cable" at 06:44Z**, superseding his earlier "restore ARC"
answer (right on the 06:06Z evidence, wrong on the 07:3xZ evidence — the row records it as
superseded, not withdrawn). **Nothing to re-ask him; the answer is still correct, the urgency
dropped.**

### LOCAL BRANCH DIVERGENCE — accepted, not fixed (2026-11-11)

`chore/rig-harness` local is **diverged from its remote**: 21 ahead / 2 behind `e27263d`. The repo
root has it checked out, so **the next push from there will fail or force.**

**Deliberately not reset.** `dev_gatekeeper` attempted `git reset --hard e27263d`; the permission
layer denied it, and checking first showed **why that denial was right**:
```
git show e27263d:rig/agents/desk/agent.yaml   ->  fatal: path does not exist
git status                                    ->  " M rig/agents/desk/agent.yaml"
```
**The desk model fix is not in the pushed tree at all** (S-006 territory, deliberately kept out of
PR #55), so a hard reset would have deleted **the only copy of a correct fix**. `pm` withdrew the
conditional approval it had given before that finding — *a conditional approval that predates the
finding does not survive it.*

**Preserved instead of destroyed:**
| what | where |
|---|---|
| ledger content | `7599cb9`, durable ref `backup/pm-ledger-7599cb9` |
| S-005/S-006 commits | durable ref `backup/rig-harness-local-e9eca69` |
| the desk model fix | `/tmp/gk/desk-agent-populated.yaml` (sha256 `ad5fd626…`), `/tmp/gk/desk-model-fix.patch` |

**Why leaving it diverged is safe:** the PR head is `e27263d` on the remote; nothing pushes from the
repo root without running the pre-push checklist; and the checklist catches exactly this —
`git diff --name-only <pr-head>..HEAD` against the local branch returns the four
desk/`DECISION-DEFAULTS` files. **A destructive command that buys only tidiness is not worth a
shared branch that is eleven seats' cwd.**

**For whoever cuts S-006:** a reviewed-in-principle fix for the empty `model:` line already exists at
the paths above. Do not rediscover the defect from scratch.

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

- **PRE-PUSH CHECKLIST — first item is mechanical, not judgement.**
  1. **`git diff --name-only <pr-head>..HEAD`** before any push to a PR branch. This one command
     has now mattered twice: it is what stopped the S-005/S-006 contamination
     (`d7ae80d`/`23bf7c5`/`9e42e46` carrying the desk seat and `DECISION-DEFAULTS.md` into a
     35-file PR), and it is the check `pod_uiux.reviewer` sent as a STOP. **Both of us ran it
     after the decision, not before** — the honest record is "caught at the last possible moment
     from two directions", not "caught".
  2. If a push would carry another slice: build it in a **scratch worktree detached at the PR
     head** and take the blobs, rather than pushing the local branch. A local branch accumulates
     other seats' commits; the PR must not.
  3. Verify byte-identity of the taken blobs (`sha256sum`) against the commits the reviewer
     signed, so the approval transfers on measurement rather than on trust.
  4. **A commit attribution is a claim.** `ba6051a + e9eca69` collapsed into one row produced a
     wrong commit for `CLAUDE.md` — the reviewer caught it. Name the commit for each file.
- **Review assigned 2026-11-11T07:1xZ: `pod_uiux.reviewer`**,
  `qitem-20261111071339-4ef08044d92335c9`. **It was the last PR with no reviewer, and it is the
  one that ADDS the review process** — so its author (`dev_gatekeeper`) cannot sign it, and `pm`
  cannot either (pm has been editing `rig/SLICES.md` in this very tree all pass, so it is not
  independent). The review brief names the five checks and the three deliberate deviations so
  they are not re-opened as findings.
- **REVIEW VERDICT 2026-11-11: CHANGES REQUESTED — one blocking item, four-line fix.**
  `pod_uiux.reviewer`, row `qitem-20261111071649-34c66c66b72a32af` to `dev_gatekeeper` (author).
  **`check.sh` prints "VERDICT: PASS — every gate green" when a gate never ran.** `pm`
  reproduced it by execution against the script extracted from `dba71c7`:
  ```
  $ ./scripts/check.sh --frontend        # frontend/node_modules absent
  frontend/node_modules missing: run pnpm install --frozen-lockfile
  VERDICT: PASS — every gate green
  EXIT=0
  ```
  **Zero tests ran and the gate said every gate was green.**
  **Cause, four lines:** `set -uo pipefail` **without `-e`**; `run_rust`/`run_frontend` `return 1`
  on a missing precondition, and the `case` dispatch **discards that status**. `fail` is only ever
  set inside `step()`, so a precondition failure is invisible to the verdict.
  **Why this is blocking and not a follow-up — the reviewer's reasoning, and it is right:** a
  worktree cut from `origin/dev` **has no `frontend/node_modules`**, and `CULTURE.md` §4 tells
  every seat to run `./scripts/check.sh` and paste the output. **The first run in a fresh
  worktree — the exact scenario S-002 exists to create — prints "every gate green" before a single
  frontend test has run.** On a UI slice that is every test that matters, and the seat then pastes
  it as a receipt.
  **`CULTURE.md:88-89`, in this same commit:** *"A green summary that was never executed is worse
  than an honest red, because it moves the defect downstream to someone who cannot see it."*
  **The script contradicts the culture file it ships.**
  **The trade, stated plainly:** land the harness today with a gate that fabricates green and every
  receipt from tomorrow is worth less; or hold it one cycle for a four-line fix and land a gate
  that is trustworthy from its first run. **This session alone produced three separate "a check
  answered a different question than the one asked" failures, so the first option is not
  available.**
  **Fix is with `dev_gatekeeper` (author); the reviewer re-reviews rather than signing its own
  request.**
- **OWNERSHIP, because `pm` got this wrong once in a message and the ledger must not inherit it:**
  **`dev_gatekeeper` authored the `check.sh` fix.** `pod_uiux.reviewer` reviewed the **working-tree
  candidate** and reported it correct; it has **not** signed it and will not, because the author of
  a fix does not approve it and it is the reviewer of record on the PR containing it.
  **The sequence is: author commits → reviewer verifies the COMMIT → verdict moves.** `pm` referred
  to "check.sh (yours…)" in a message to the reviewer, meaning "the one you reviewed"; that reads
  as authorship and was corrected. **A verdict is never given against a working tree.**
- **THE FIX CANDIDATE, measured by `pm` at the working tree (not yet committed):** a `skip()`
  helper that sets `fail` **and** appends to `failed[]` **and** prints the label — so a skipped gate
  becomes a **failed** gate that **names itself**. Reproduced against the same two failure modes:
  ```
  $ ./scripts/check.sh --frontend      # node_modules absent
  frontend/node_modules missing: run pnpm install --frozen-lockfile
  FAIL pnpm (not run)
  VERDICT: FAIL — 1 gate(s) red: pnpm (frontend/node_modules missing: …)   EXIT=1

  $ ./scripts/check.sh --rust          # cargo absent
  VERDICT: FAIL — 1 gate(s) red: cargo (not installed)                     EXIT=1
  ```
  **Not "add `-e`"** — which is what `pm` half-expected. The verdict line names the gate *and* the
  reason, which is what a receipt needs. **Green path still `PASS`/exit 0** — the fix does not break
  the working case.
  **All THREE cannot-run shapes measured, not assumed** (reviewer, re-confirmed by `pm`):
  | shape | result | exit |
  |---|---|---|
  | `cargo` absent | `FAIL cargo (not run)` → `VERDICT: FAIL — 1 red: cargo (not installed)` | 1 |
  | `node_modules` absent | `FAIL pnpm (not run)` → `VERDICT: FAIL — 1 red: pnpm (…)` | 1 |
  | `pnpm` absent, `node_modules` present | `FAIL pnpm lint/test/build` → `VERDICT: FAIL — 3 red` | 1 |
  The third is caught by `step()`, which already handled it — checked so that "all preconditions
  covered" is a measurement rather than a claim.
- **A SECOND INSTANCE OF THE SAME CLASS, found while checking: `check-slices.sh` reports
  `0 worktree(s), 0 stale` with exit 0 outside a git repo.** Reproduced by `pm` in an extracted
  tree: `fatal: not a git repository` prints, then **`0 worktree(s), 0 stale`, exit 0** — it
  enumerated nothing and `stale` is its only gate signal. **The reviewer hit this in practice
  today** and had to notice the number was meaningless.
  **Calibration, and the reviewer's judgement is right:** **not blocking.** The `fatal` is
  **visibly printed**, unlike `check.sh`'s clean `PASS` with nothing above it; `check.sh` is what
  every receipt depends on, `check-slices.sh` is a diagnostic; and S-002 is the last PR holding B2
  open for every builder. Two lines in the same pass is recommended; taking it now or as a
  follow-up row is `dev_gatekeeper`'s call.
- **REVIEW VERDICT 2026-11-11T07:31Z: APPROVED — at the COMMITS, not the working tree.**
  `pod_uiux.reviewer` verified `ba6051a` and `e9eca69` by execution:
  ```
  node_modules absent  -> VERDICT: FAIL — 1 red: pnpm (…),        EXIT=1
  cargo absent         -> VERDICT: FAIL — 1 red: cargo (not installed), EXIT=1
  --all, both absent   -> VERDICT: FAIL — 2 red,                  EXIT=1
  check-slices outside a repo -> "cannot enumerate worktrees…",   EXIT=2
  check-slices in a real clone -> unchanged,                      EXIT=0
  ```
  `e9eca69` documents `git config core.hooksPath .githooks` in a **"First run on a fresh clone"**
  section and says the hook is *"inert and silent"* without it. **Correct and honest**:
  `core.hooksPath` lives in `.git/config` and **cannot be committed**, so documenting is the only
  right answer. `pm` read the change and agrees — a guard nobody knows to enable guards nothing,
  and the file says so instead of implying protection it does not provide.
  **Note the second instance of the same class fixed here too:** `check-slices.sh` now exits **2**
  outside a repo instead of reporting `0 worktree(s), 0 stale` with exit 0. **The non-blocking
  finding was taken, and taken in this PR.**
- **PUSH HAZARD — LIVE, and it is the defect this rig has refused all day.** The PR head is still
  **`dba71c7`**; the branch is **20 commits ahead** of it. **Pushing HEAD would grow PR #55 from
  31 files/+2149 to 35 files/+3519** (`pm` measured it) and drag in:
  ```
  rig/culture/DECISION-DEFAULTS.md   (S-005)
  rig/agents/desk/**                 (S-006)
  rig/pods/desk.yaml                 (S-006)
  plus the desk member and both edges in mythos-org.yaml
  ```
  **That is S-005 and S-006 riding into S-002 — three slices in one PR.**
  **The reviewer's path, recommended to `dev_gatekeeper`:** cut from `dba71c7`, cherry-pick
  `ba6051a` + `e9eca69`, push that, retarget or replace #55.
  **And the reviewer stated the consequence of getting it wrong, which `pm` endorses:** *"if they
  push HEAD instead, my approval does not carry — a review of 31 files is not an approval of 35,
  and the grown diff needs its own verdict."* **An approval is scoped to the diff it was given
  against, which is the same rule as naming the head SHA.**
- **SEQUENCING CONSEQUENCE, recorded because it would otherwise be discovered after the fact:** if
  the push goes out with all 20 commits, **S-005 and S-006 land unreviewed** and their rows must say
  so rather than continuing to report `planned`/blocked-on-S-002. Nothing about the *order* changes
  — S-005 and S-006 still follow S-002 — but the order only holds if S-002 is pushed as **two
  commits**, not twenty.
- **FOUR CHECKS CLEAN, on the record** (reviewer, not `pm`): additive only (31 files, +2149, −0,
  30 A / 1 M / 0 D); `.gitignore` hides no tracked file (552 force-added, 0 matching);
  `bash -n` clean on all three scripts and `check.sh`'s steps match the ledger; `CULTURE.md` holds
  up against every claim made about it; `spec validate` + `preflight` clean **with the caveat that
  `agent_ref` paths are absolute, so validation is not fully hermetic**. Also: `check-slices.sh`'s
  strict-ancestor rule is correct — `dev-live` reads `ok`, not `STALE:merged`.
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

### S-014 — draft-vs-committed setpoint reconcile (scoped 2026-11-11, now unblocked)

- **What:** a draft-vs-committed reconcile rule for the operator-writable setpoints —
  `MotorFaceplate.inputSpeed` and the PID panel's `targetSp`/`targetCv`/`tuneKp`/`tuneTn`/`tuneTv` —
  plus the amendment that turns `docs/04-hmi-and-ui/operator-setpoint-reconcile-rule.md` §5 from
  **OPEN** to **DECIDED**.
- **Why now:** it was parked on the HMI telemetry gap. **S-004 landed (#61 @ `08245d0`), so the
  transport exists** and these surfaces become reachable for the first time. The rule doc still says
  §5 is blocked on a transport decision that no longer exists — that staleness is itself part of this
  slice.

**All six open questions are resolved below, grounded in the wire as measured by `pm`. None is left as
a promise.**
```
telemetry.rs:63    TagValueUpdate.timestamp_ns = slot.timestamp_ns   (the SLOT's write stamp)
telemetry.rs:106   TelemetryBatch.timestamp_ms = SystemTime::now()   (the SERVER's clock)
telemetry.rs       grep seq|sequence -> 0                            (NO sequence on the wire)
```

| # | question | ruling |
|---|---|---|
| **O1** | wire-level staleness | **Use CONSUMER-OBSERVED ARRIVAL AGE**, not the wire timestamp. `performance.now()` at apply time is one clock on one side, so "silence since last frame" is exact with no clock-sync assumption. **No device-side age may be displayed** — `timestamp_ns` is the slot writer's clock and the consumer is a browser, and nothing establishes their agreement. **No sequence means loss is undetectable**: the clock measures silence, not loss, and the rule must not claim otherwise. |
| **O2** | echo vs independent change | **NOT ANSWERABLE — strike "or on echo" from R4.** Value equality is the only signal and a device can land on the same value independently; an equality test **silently closes an episode on a coincidence**, the worst kind of wrong because it looks like the rule working. Reconciles on **commit / cancel / observed drift beyond a stated bound** only. Returns with a transport that carries an ack or sequence, tested against it. |
| **O3** | central vs per-field staleness | **Answered by the implementation.** The server enumerates the whole catalog each tick and emits only readable slots, so absent = unreadable, never unchanged; the cache replaces per batch. **Staleness is resolved centrally — the rule defers to the transport rather than restating it.** |
| **O4** | drift threshold policy | **Out of scope — plant policy.** The threshold is a **named, REQUIRED configuration value with NO default.** Unconfigured renders `NOT CONFIGURED` and drift detection is inactive. **No plausible-looking seed number** — that is the fabricated-bound defect in a new costume. |
| **O5** | do tuning params come back? | **Per-field, per-plant, decided at runtime.** `kp` **is** in `PROCESS_KEYS` (`componentData.ts:59`) so it *can* arrive live — but `inputSpeed` is a process value the device reports while `kp`/`tn`/`tv` are stored configuration it may never report. **Conflating them is the defect this question exists to prevent.** An unreported tuning field shows the draft labelled uncommitted with the device value **UNKNOWN**, never seeded at mount. |
| **O6** | draft lifetime across a re-key | **`pm`'s call: a re-key DISCARDS the draft, with a visible notice.** Surviving a re-key carries a draft across an identity change onto a different device; preventing the re-key is a UI lock and scope creep; silent discard is what this rule exists to prevent. **Visible discard is honest; silent discard is a fabricated success.** |

- **Done when:** an edit made before a telemetry frame either survives that frame or is discarded with
  a **visible reason**; and each of O1–O6 is either implemented or explicitly excluded in the doc.
- **Anti-coverage clause:** **a test that only covers the no-telemetry case proves nothing** and must
  not be accepted as coverage.
- **Doc amendment ships with the slice, not before it** — a controlled-document change reviewed
  together with the code it describes.
- **Verification:** `cd frontend && npx vitest run --no-cache` from `frontend/`. Failing test names,
  not counts.
- **State:** `planned`, **cut requested** from `origin/dev` @ `08245d0`. Owner `pod_hmi.builder`.

### S-015 — `mythos update` fabricates version identity (LIVE, found 2026-11-11)

- **What:** make the CLI's version story true, or make it admit it is unimplemented. Four mechanisms,
  all verified against `origin/dev` @ `75200c0` — see the LIVE FABRICATION section above for the
  evidence.
- **Why it outranks everything else:** every other fabrication found today is **LATENT**.
  **This one is LIVE** — `mythos update` is a shipped command, `cargo test --workspace` attests it on
  every push, and an operator who runs it is told they have a version they do not have.
- **Scope, and the honest minimum:** the slice must choose, and the choice is a product decision worth
  stating in the PR:
  1. **Implement the fetch** (an HTTP client, checksums, a real catalog), or
  2. **Refuse instead of fabricate** — `update` and `versions --remote` exit non-zero with
     `NOT IMPLEMENTED`, and the copy-the-running-binary path is deleted.
  **Option 2 is legitimate and cheap.** A command that says "not implemented" is honest; a command
  that copies itself and claims a new version is not. **A half-built updater that lies is worse than
  no updater.**
- **Non-negotiable, whichever is chosen:** the **shell-stub fallback is deleted** (`update.rs:59-63`),
  and **`cli_tests.rs:46-58` stops asserting the hardcoded literals** — a test that pins fabricated
  values inside a required check is the mechanism that hides this.
- **Done when:** `mythos update` either installs a binary whose version is read from that binary, or
  exits with `NOT IMPLEMENTED`; and no test asserts a version string that did not come from a source
  the test actually contacted.
- **THE NO-ARGUMENT PATH MUST BE TESTED, and it is the operator's path** (auditor, verified by `pm`).
  Bare `mythos update` goes `lib.rs:46` (flag optional) → `update.rs:39-43`
  (`None => resolve_latest`) → `versions.rs:79-82` (`catalog.latest`) → `config.rs:13-14`
  (`ReleaseChannel` defaults to `Release`) → `versions.rs:16-39` (`latest["release"] = "0.1.0"`,
  a literal) → `update.rs:50-58` (copy `current_exe()`) → **`lib.rs:133` prints
  `"Successfully updated to version 0.1.0"`.**
  **The default invocation tells the operator they now run `0.1.0`, having installed a copy of
  whatever they were already running.**
  **And `cli_tests.rs` never calls `update` with `None`** — both calls (`:71`, `:81`) pass
  `Some(...)`. **The only path an operator reaches without flags is the one no test touches, while
  the tests that do run pin the catalogue literals.** A done-when that does not cover the bare
  invocation would ship a verified lie.
- **Verification:** `cargo test -p mythos-cli` plus a manual transcript showing the command's real
  output. **Name the failing test, not the count.** If the path is deleted, the proof is that the
  command refuses — paste what it printed.
- **Also unblocks:** `qitem-20261027083546` is parked on the premise that the CLI *"would ship as a
  stub"*. **The stub is already on `dev`**, so that row's blocker is a condition the code has already
  broken — this slice is what makes the premise answerable.
- **BUILT 2026-11-11: `581c2f2`, `.github/workflows/release-build.yml`, +215/−0.** `pm` verified the file
  rather than accepting the report:
  ```
  :46-47  permissions: / contents: read          <- the structural guarantee, not a promise
  :44     "a contents: write grant is required to push a tag, so omitting it is what
           prevents this job from ever becoming a tag creator"
  :36     workflow_dispatch        :30-31  push on rc and release
  :212-213  git tag / gh release / cargo publish appear ONLY inside comments explaining their absence
  :19-20  the build.rs stub limitation is stated in the file itself
  ```
  **`contents: read` is the load-bearing line.** A comment saying "this publishes nothing" is a claim; a
  permission that makes publishing impossible is a guarantee. **That is the difference the whole day has
  been about, executed correctly in a workflow file.**
- **NOT MERGEABLE YET — the acceptance evidence is outstanding.** The three-amended done-when requires the
  two **locally-proven red directions with pasted output**. **A well-formed job is not a job shown to fail
  when it should**, and `dev_gatekeeper` is right to hold it.
- **SCHEDULING CORRECTION 2026-11-11: S-016 and S-015 are SEPARABLE and must not queue behind one author.**
  Both were filed to `pod_backend.builder`; the seat is now 45+ minutes into one turn at its context edge,
  has **committed S-016** and **not started S-015** (worktree still at `7c35e80`, clean).
  **That is a scheduling error `pm` made when filing both to one seat**, and the fix is not a nudge or a
  rebuild:
  - **S-016 needs only a receipt.** If the builder's handoff does not arrive, **`dev_gatekeeper` takes the
    red-direction receipt from the seed and merges.** Re-running two red directions on a *committed*
    workflow is verification, not feature code — the role line is "who writes it", not "who runs it".
  - **S-015 needs a whole slice and has nothing.** If the seat drops, **re-cut it to a fresh seat rather
    than let a dying one half-start it.**
  **Do not hold S-016 hostage to S-015.** Whichever is ready first moves first.
- **State:** `planned`, **cut requested**. Owner **`pod_backend.builder`**.

### S-016 — the `release-build` CI job (the `rc` → `release` gate has no evidence producer)

- **What:** one job on `rc` and `release` that runs `cargo build -p mythos-desktop --release`, then
  `bash -n scripts/build-installers.sh`, then `scripts/test-packaging-sanity.sh`.
- **It builds and validates. It publishes nothing and must not create or push a tag.**
- **Why:** the `rc` → `release` gate lists *"release build, soak and regression stability, zero-downtime
  hot-reload, IEC safety integrity"*. **Nothing produces any of it.** Three workflows, three jobs
  (`core-engine-tests`, `frontend-tests`, `version-and-changelog`). `scripts/build-installers.sh`
  **does** run `cargo build --release` and `tauri build` (3 hits) but **no workflow invokes it**
  (`git grep build-installers origin/dev -- .github/` → zero). `test-packaging-sanity.sh` exists,
  mode `100755`, with **zero CI references**.
- **Feasible today, verified:** the package exists (`src-tauri/Cargo.toml:2`) and CI already installs
  `libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev` (`ci.yml:83`), so the toolchain is
  present.
- **DONE-WHEN AMENDED 2026-11-11 — the original was NOT SATISFIABLE IN THE ORDER IT WAS WRITTEN, and the
  builder proved that before building rather than after.** GitHub only makes a workflow dispatchable
  once the file is on the **default branch**; here that is `dev` (`gh repo view --json
  defaultBranchRef` → `dev`, confirmed by `pm`). **So `workflow_dispatch` on an unmerged file is
  undisputable, and `--ref <branch>` selects where a run happens, not what is discoverable.** The only
  remaining options were "land an unproven job" or "fake the proof by dispatching a broken tree at
  `rc`" — and the second is a destructive test.
  **Amended criterion, which is what `dev_gatekeeper` will gate against:**
  1. **Both red directions proven LOCALLY**, with pasted output — a deliberate compile break in
     `src-tauri`, and a `bash -n` failure.
  2. **ONE green CI run** as the CI-side evidence.
  3. **The gatekeeper closes the loop after merge** by dispatching against a scratch branch carrying a
     deliberate break.
  **The PR body must state that gap explicitly** rather than let a reader infer the criterion was met as
  written. **Report the failing job name, not a count.**
- **AND THE JOB PROVES LESS THAN ITS NAME SUGGESTS — `pm` verified this.** `src-tauri/build.rs` creates
  a **placeholder `index.html`** when `frontend/packages/ui-shell/dist` is absent, and
  `tauri.conf.json`'s `frontendDist` is **that same path**. So
  `cargo build -p mythos-desktop --release` **SUCCEEDS WITHOUT A BUILT FRONTEND, against a stub.**
  **A green run proves the Rust host compiles. It does not prove a shippable app.** That is now in the
  workflow's header comment so nobody cites the job as a packaging gate.
- **What this does NOT fix, and must not be read as fixing:** *soak* and *IEC safety integrity* have
  **zero grep hits** across `specs/` — no test, no requirement, no bound. **Those phrases are dropped
  by S-017, not satisfied by this job.**
- **State:** `planned`, **cut requested**. Owner **`pod_backend.builder`**, not the gatekeeper.

**OWNERSHIP CORRECTED 2026-11-11, and the reason is a rule not a preference.** `dev_gatekeeper`
refused the slice against its own role file (`role.md:62`): *"You do not write feature code. If a slice
needs a fix, it goes back to the builder."* S-016 is a workflow change **with a behavioural acceptance
criterion** — the job must go red on an injected compile break — so it is code, not a merge or worktree
operation.

> **`dev_gatekeeper`'s argument, which `pm` accepts: *"If the seat that gates is also the seat that
> writes the job being gated, there is no second pair of eyes on the thing that decides whether
> anything merges."*** That is *"the author of a fix does not approve it"*, one layer down — and it
> holds on its own merits after a day when a reviewer found the gatekeeper's own gate script printing
> a false green.

**`pm` recorded it as a decision, not an assumption**, because the next occupant will read the same
role file and reach the same conclusion. **Builder writes it in a worktree the gatekeeper cuts;
gatekeeper gates and merges it.**

### S-017 — MYTHOS-ARCH-008 gate wording: say what is NOT SATISFIED, don't just delete

- **The correction that shapes this slice** (`dev_gatekeeper`, and `pm` agrees): **deleting a false
  claim is not the same as stating the true one.** *"A controlled document that stops claiming something
  is better than one that claims it; one that says what is actually true is better than both."*
  **So the replacement says the unsupported criteria are NOT SATISFIED — it does not merely drop the
  words.** A reader who finds the phrase gone learns nothing; a reader told it is unmet knows to ask.

### S-017 detail — MYTHOS-ARCH-008 gate wording: drop two phrases with no producer

- **What:** `docs/01-architecture/08-one-click-installers-and-distribution.md:37` currently reads
  *"Strict Production (All CI tests, Box-Model audits & OT sign-offs)"*.
- **Change to:** *"Strict Production (all required checks green + release build job + recorded human
  sign-off)"*, with OT sign-off specified as a **human gate** — named signer, tracked artifact path,
  recorded on the promotion row — per `ADR-004:86`.
- **Why each phrase goes:**
  - **"soak / regression stability"** — `grep` over `specs/` for `soak|endurance|stability test|24-hour`
    → **zero hits.** No test, no requirement, no bound.
  - **"IEC safety integrity"** — `grep` over `specs/` for `safety integrity|SIL|IEC 61508|IEC 61511`
    → **zero hits.** Nothing to verify against.
  - **"Box-Model audits"** — not an audit with a producer; Box-Model is a **layout constraint**
    (`inspector-drawer-layout.md:23-30`).
  - **`SYS-REQ-094`'s clean-machine `.exe`/`.dmg`/AppImage criterion cannot be produced here at all** —
    every workflow is `runs-on: arc-runner-org` (4 of 4), one self-hosted Linux runner, **no matrix**.
- **Correction carried from the auditor, so it is not over-corrected:** **hot-reload IS measured** —
  `hot_reload_tests.rs::test_hot_reload_sub_50ms_sys_req_023` asserts `<50ms`. It is a **test, not a
  soak**, and that distinction is what the wording must preserve.
- **This is `MYTHOS-ARCH-008`, APPROVED** — a controlled document, so it is `governance.scribe`'s edit.
  **The wording above is drafted so the slice is a transcription, not a design decision.**
- **ROW FILED LATE — `governance.scribe` had this slice specified and ownerless for 20 minutes.**
  `pm` wrote S-017 into the ledger at 11:54 with scribe named as owner, **and never created a queue row
  for it.** `executive-advisor`… no — `release_train.auditor` caught it with six scans: *"CULTURE §8 says
  a message informs while a row is owned work. The scribe has 16 rows, none of them this."* **Row is now
  `qitem-20261111121442-34d973e9bc997439`.** The slice was well specified the whole time; **it just had
  nothing carrying it.**
  **`pm`'s own failure, and the fifth time today that a specified slice had no review-or-work row** —
  after S-003, S-007, S-009, S-010, S-011. **Naming an owner in the ledger is not the same as giving
  the work a row**, and the ledger being right does not make it owned.
- **Ships with S-016's job, not before it** — a doc that names a gate before the gate exists is the
  class this repo has shipped past twice.
- **State:** `planned`. Owner `governance.scribe`.

### S-018 — enforce the release ordering rule mechanically (rides with `dev` → `rc`)

- **What:** extend `validate-branch-flow` (`.github/workflows/branch-protection.yml`) so a PR into
  `release` fails when **`git merge-base --is-ancestor c165946 <head>`** is false — i.e. when the head
  lacks PR #52's guard.
- **Why it cannot be built now, and this is the important part:** **for a `pull_request` event the
  workflow runs from the merge ref, so the check must live in `rc`'s copy — and only the `dev` → `rc`
  promotion delivers it.** **The ordering rule therefore stays SOCIAL until that promotion happens.**
- **So this is not a standalone slice.** It **rides with the `dev` → `rc` promotion** and is tested
  there: a PR into `release` from a head lacking `c165946` must fail the check after the promotion
  lands.
- **`pm`'s accepted read, overturning its own earlier suggestion:** **do not promote `dev` → `rc` now to
  carry #52.** The stale publish step's `if:` is `startsWith(ref,'refs/tags/v') || (push &&
  ref_name=='release')`; a push to `rc` has `ref_name 'rc'`, and `release` takes PRs only. **Nothing
  needs promoting to defuse #52 — the ordering rule already does it.** And promoting declares `rc`
  staging-ready while it carries 144 commits with **no GATE 7 sign-off** (the only gate status in
  `specs/tasks.md` is GATE 3's *"REOPENED — NOT SIGNED OFF"* at `:273`).
- **Trigger to promote:** GATE 7 sign-off, or the first real hotfix. **Not a milestone that has not
  happened.**
- **State:** `planned`, **rides with the promotion**. Owner `release_auditor`.

### S-014 — APPROVED at `b09074b`, all four items closed

**Three blocking items at `c1ac744` became four, all closed.** `pod_hmi.reviewer` verified each with
`git show b09074b:<path>` rather than the tree:
1. **Render committed** — 2 hits each surface; it was **0 at `c1ac744`** and present only in a dirty tree.
2. **Notice words pinned** — the exact swap reported as uncaught now gives **3 red**, including a
   render-level assertion of the on-screen words, **plus a guard at `:103-107` that reads the words rather
   than the constant** — better than pinning one string.
3. **Drift status pinned on both surfaces** *and* asserted absent when configured — both directions.
4. **No fabricated thumb** — done as **"no input element at all when nothing was measured"** rather than a
   corrected value. **That makes `pm`'s substitution trap inapplicable rather than merely avoided.**

**Gate: 52 files / 443 tests, exit 0**, run by the reviewer with the concurrent-build check first and the
exit code read from the script.

**And the reviewer corrected its own suggested test, which is the last lesson of this slice:** restoring
`value={speedDraft.value ?? 0}` inside the input **passes, correctly**, because that branch is unreachable
when the value is null. **The load-bearing guard is the null-check gating render, and mutating *that* is
what reddens it.** *"Test the property, not the nearby operator."* A builder following the literal mutation
would have concluded their guard was untested and added work that was not needed.

### S-014 verdict note — three blocking items at `c1ac744`

`pod_hmi.reviewer` raised one; **`pm` added two more** after verifying, and corrected the reviewer's
instance count.

1. **O4's rendered text is UNCOMMITTED — and this item was wrong twice before it was right.**
   `pod_hmi.reviewer` raised it, retracted it, and restored it; `pm` split the difference wrongly by
   calling both trees valid. **Settled against the commit object, the only authority:**
   ```
   git grep -F DRIFT_NOT_CONFIGURED_TEXT c1ac744 -- frontend/   -> ONE hit: useOperatorDraft.ts, the definition
   working tree src/components/                                 -> FOUR hits (both faceplates, imported and rendered)
   ```
   **The builder has the fix written, correct, and uncommitted.** So the original finding was right,
   the retraction was the error, and `pm`'s "different valid trees" framing was the softer form of the
   same mistake. Doc claims rendering at `:172` **and** `:217`; at the reviewed head **nothing renders
   it**. **A dirty tree is not a second opinion — it is a different object.**
   **The doc's own acceptance line at `:217` names the exact failure it commits.** Drift-inactive-when-
   unconfigured *is* implemented and mutation-proven; the **rendered text is not**, and an operator
   seeing no indication cannot tell a quiet surface from an unconfigured one — the distinction O4 exists
   to make.
2. **The notice wording is unpinned — `pm` overruled the reviewer's non-blocking call.** All six
   assertions compare against `DRAFT_DISCARD_NOTICE.<CONSTANT>`; **replacing the string with
   `'Value committed successfully'` passes all 27 tests.** My O6 ruling was *"visible discard is honest;
   silent discard is a fabricated success"* — **a notice whose words can be inverted is a silent discard
   wearing a visible notice**, which is worse, because it is confidently wrong where the operator is most
   likely to trust it. **The thing the ruling exists to prevent survives the implementation.**
3. **Two `?? 0` fallbacks, and only one of them is a fix — the distinction is the slice's lasting rule.**
   Verified at `c1ac744`:
   ```
   418  max={rangeMax ?? 0}
   420  value={speedDraft.value ?? 0}
   421  disabled={!live || rangeMax === null || ... }
   ```
   **`max` at `:418` — `pm` escalated it and then WITHDREW the escalation.** `rangeMax` is null when
   unconfigured and `?? 0` turns "I do not know the range" into "the range ends at zero", which is O4's
   fabricated-bound defect in costume. **But the control is disabled in that state and the readout above
   it says `UNKNOWN`, so the bound cannot be read as a measurement.** Withdrawn as blocking, kept as a
   known limitation.
   **`value` at `:420` — REQUIRED.** Same fallback, different consequence, and `pod_hmi.reviewer`'s
   distinction is the reason: *"the thumb POSITION is a rendered value in a control, and a position is
   read at a glance before the label. Disabled dims it; it does not stop it being a number."*
   > **The test is not "is there a fallback" — it is "is the fallback read as a measurement."**
   **Trap, now in the builder's brief:** do **not** replace `?? 0` with a midpoint, `min`, or any other
   number. Every substitute is a fabricated bound in the same costume. A null value renders **no thumb**,
   and the readout already says `UNKNOWN`. Say so in the commit body, because *"we removed `?? 0`"*
   invites a future repair that puts a different number there.

**Corrected instance count:** the reviewer called this the "third instance today" of a test asserting a
constant. **It is two, plus one counter-example** — `qualityDetailBadge.test.tsx` has **19 render-level
assertions**. Their own badge check is the shape to copy, not a third failure. **The accurate count
matters: "three" implies the practice is failing more than it is.**

**Held and worth recording from the review's positive half:** mutation 1 (the naive prop-tracking fix)
gives **3 red including the R5 case** — the one the slice exists for; mutation 2 (`setNotice` never
fires) gives **4 red**, so silent discard is genuinely prevented; gate rerun **51 files / 432 tests,
exit 0**.

### S-019 — CULTURE §12: an assertion against a constant proves plumbing, not meaning

- **What:** one new section in `rig/culture/CULTURE.md`, appended after §11. **The most-delivered
  document in the rig** — every seat receives it on fresh start **and** on restore — so it is a slice
  with a review, not an edit.
- **Why it needs to be culture and not a per-slice fix:** **two slices today shipped a test that cannot
  fail for its own defect, and both authors believed they had tested it.**
  - **S-001** — `expect(presentation.tone).not.toBe(other.tone)` asserted the **enum member**; making
    `TONE_CLASSES.warning` byte-identical to `TONE_CLASSES.alarm` kept the suite **5/5 green** while both
    states rendered the same CSS.
  - **S-014** — `expect(notice).toBe(DRAFT_DISCARD_NOTICE.QUALITY_LOST)`, **six times**, all against the
    constant. Swapping the text to `'Value committed successfully'` **passes all 27 tests** — a discard
    announcing a successful commit.
  - The counter-example to copy: `qualityDetailBadge.test.tsx`, **19 render-level assertions**
    (`textContent`, `data-quality`, `className`).
- **The rule, and why it is not simply "assert the render":** both authors knew that. **The rule is about
  which assertions *look like* coverage:**
  > **`expect(x).toBe(CONSTANT)` where `x` was produced from `CONSTANT` proves the plumbing, not the
  > meaning. It cannot fail unless the constant is undefined or the call is deleted.**
  **A guard that cannot fail for its own defect reads exactly like a guard that can**, which is why it
  survives review — `pm` signed off one of the two.
- **THE GOOD EXAMPLE TO CITE, from S-014's fix** (`pod_hmi.reviewer`'s suggestion, and it is better than
  `pm`'s original rule): `setpointRenderPinning.test.tsx:103-107` iterates **every** notice and asserts a
  **property** of the words — *"does not contain 'committed successfully'; does contain 'discard'"* —
  rather than equality with one constant. **It rejects the failure class, not the instance.** §12 should
  carry it, because a rule with a working counter-example survives a handover and a rule without one gets
  rewritten into a platitude.
- **What the section should say, in outline** (not final wording — that is the slice):
  1. the two live examples above, briefly, with what each left unproven;
  2. the test that separates them: **can this assertion fail if the behaviour it names is broken?** If
     no, it is a wiring check and must be labelled as one;
  3. the counter-example: assert the rendered output — text, attribute, class — for any claim about what
     a surface *shows*;
  4. the honest caveat: a wiring check is **worth having**, just not worth *calling coverage*. Deleting
     it would be an over-correction; the defect is the label, not the test.
- **Done when:** §12 exists, says the above, and does not contradict §4 (no claim without pasted output)
  or §5 (never invent data).
- **Verification:** docs-only. Read §4, §5 and §11 before appending so §12 does not restate or contradict
  them, and **confirm no reference to `rig/SLICES.md`, a slice id or a `qitem` enters the file** — that is
  the `fbaaa4d` defect, and `CULTURE.md` is delivered to every seat.
- **State:** `planned`, **cut requested** from `origin/dev` @ `7c35e80`. Owner `pm`.

### Sequencing — REWRITTEN 2026-11-11T07:1xZ, because the old block contradicted the table

`governance.scribe` caught this: the previous version said "S-001 re-review is open at `21cb9c4`"
while the table said S-001 was APPROVED, and it said S-003 was approved at `7d4798c` while the
head was `5cf3984`. **A sequencing block that contradicts the table is worse than no sequencing
block, because it is the paragraph people read for the plan.** Every line below is checked against
the table and the PRs, not carried.

**Six APPROVED and blocked only on CI (B1):**
`S-001` @ `ab93602` · `S-003` @ `5cf3984` · `S-009` @ `139c263` (re-confirmed after the head moved)
· `S-010` @ `16c10e3` · `S-011` @ `7e1535e` · `S-007` @ `2bfac2e`

**`S-004` / PR #61 — CHANGES REQUESTED at `3c683be`, and now BEHIND `dev`.** Two blocking items, both
small and independent: (1) the **connection count** — two `EventSource`s from two panels against one
module-global store, with per-instance reconnect and a shared `connection` field; (2) the **badge
passthrough** — `resolveComponentLive` returns no `qualityName`, so every bad quality renders
"BAD COMM FAILURE" and an operator at a failed **sensor** reads a **comm** fault. **Item 2 is `pm`'s
own unkept ruling, not the builder's oversight.** Rebase onto `6dad46a`, re-gate, verdict at the new
head.

**`S-002` / PR #55 — APPROVED at `ba6051a` + `e9eca69`, and UNPUSHED.** It had no reviewer at all
until 2026-11-11 (it adds the review process, so its author cannot sign it). `pod_uiux.reviewer`
found that **`check.sh` printed "VERDICT: PASS — every gate green" when no gate ran**, blocked it,
and approved the fix after verifying it **at the commits**. **PR #55's head is still `dba71c7` and
the branch is 20 commits ahead** — pushing HEAD would grow the diff from 31 files to 35 and carry
S-005 and S-006 in with it. **The push must be the two reviewed commits, cherry-picked onto
`dba71c7`; if it is HEAD, the approval does not carry and S-005/S-006 land unreviewed.**

**Not a slice yet:**
- `S-005` → `S-006` follow `S-002`; `S-006` also follows `S-005` — **F4 established that
  `role.md:78-79` cites `DECISION-DEFAULTS.md`, which `mythos-org.yaml` never delivers.** S-006's
  blocking items are closed by S-005's delivery entry, not by editing the desk role.
- `S-008` needs a scope decision (who constructs the runtime) before it is a slice.
- `S-012` follows `S-004` — S-004 ships the neutral-label fallback so S-012's absence is honest
  rather than harmful. `S-013` waits on nothing.

**The HMI telemetry row has moved.** `qitem-20261023085422` was `pending`/unclaimed when the old
block was written; it is now **`handed-off` to `pod_hmi.builder` as `qitem-20261111061758`**, which
is in progress and carried by **S-004**. It is not free work.

**The device-mode half** of TSK-801 waits on S-003 and is not yet a slice.

---

## RELEASE-PATH HAZARD — `rc` → `release` can publish `v0.1.0` from `dev` HEAD (LATENT)

Raised by `release_train.auditor` on fresh start, `qitem-20261111080023`; **all load-bearing
claims re-verified by `pm`** against the remote refs, not taken from the report.

**The mechanism.** `rc` carries the **pre-#52** versioning workflow; `release` has **no `.github`
at all**; `dev` carries the guarded one. Guard counts, `pm`'s own grep:

| guard | dev | rc |
|---|---|---|
| `scripts/version.sh` | 2 | **0** |
| `target_commitish` | 2 | **0** |
| `VERSION_TAG_PUSH_ENABLED` | 4 | **0** |

`git merge-base --is-ancestor c165946 origin/rc` → **NO**. PR #52, *"a release may not create its
own tag, and it points at the built commit"*, **never reached `rc`**.

So the **first** promotion that brings `rc`'s tree into `release` also brings the unguarded
workflow **and runs it on that same push**. Its publish step on `rc` (read by `pm`):
```yaml
- name: Create or Update GitHub Release
  uses: softprops/action-gh-release@v2
  if: startsWith(github.ref, 'refs/tags/v') || (github.event_name == 'push' && github.ref_name == 'release')
  with:
    tag_name: ${{ steps.vars.outputs.tag_name }}     # v0.1.0
    draft: false
    prerelease: ${{ startsWith(...,'v') && contains(...,'-') }}   # FALSE for v0.1.0
```
On a push to `release` the `if:` is **true**, the tag is **`v0.1.0`**, `draft: false`, and
`prerelease` is **false** because `v0.1.0` contains no hyphen. **No `target_commitish`**, so
GitHub creates the tag at the **default branch (`dev`)**, not at the built commit. That is exactly
the defect #52 fixed, on the branch that never received it.

**MITIGATION IS ORDERING, NOT CODE — and ordering is `pm`'s call, so this is the rule:**
> **Always promote `dev` → `rc` BEFORE `rc` → `release`.** The `dev` → `rc` merge carries `dev`'s
> hardened workflow into `rc`, so `rc` then holds every guard. **Promoting `rc` → `release` without
> first promoting `dev` → `rc` is the unsafe order and must not be done, however urgent.**

**SECOND FINDING, and it is the one that matters more: the `rc` → `release` gate has no evidence
producer.** The auditor's role requires release build, soak and regression stability, zero-downtime
hot-reload and IEC safety integrity for that gate. **No workflow produces any of it** — three
workflows, three jobs (`core-engine-tests`, `frontend-tests`, `version-and-changelog`), and
`scripts/build-installers.sh` **does** run `cargo build --release` and `tauri build` (3 hits) but
**no workflow invokes it** (`git grep build-installers origin/dev -- .github/` → **zero hits**).
`docs/01-architecture/08-one-click-installers-and-distribution.md:37` states the gate as *"Strict
Production (All CI tests, Box-Model audits & OT sign-offs)"* and **no OT sign-off artifact exists.**

**A gate whose evidence nothing can produce is not a gate — it is an aspiration that will be signed
off on judgement and then recorded as if it were measured.** That is the fabricated-value class one
level up, and it is `release_auditor`'s and Adrian's to resolve, not `pm`'s to paper over.

**Blast radius today: nil, because nothing can reach `release`.** That is the only reason this is
LATENT rather than LIVE — and it is why it must be recorded now, before the merge queue moves.

## RECORDED, NOT SCHEDULED — the two `InterlockCondition` types (decided 2026-11-11)

`pm` decision on `qitem-20261026063940`, handed over by `pod_uiux.builder`. **No slice cut.**

**Verified by `pm` at `origin/dev` @ `6dad46a`:** two definitions exist
(`ui-faceplate-lib/src/types.ts:30`, used at `:98`/`:118`; `ui-inspector/src/types.ts:20`, used at
`:82`), **nothing imports both** (checked every file mentioning the name for both package imports),
and the types are **structurally incompatible**, so a value of one **cannot** be silently passed
where the other is expected — the failure is a **compile error**, not a wrong-import.

**Why not now:** a rename-only slice across two packages needs a worktree, branch, PR, reviewer and
re-gate **for zero behavioural change**, while six PRs sit approved and the merge queue has only just
started moving. That spends the rig's scarcest resource — review attention — on a cosmetic fix with
no live defect. **And acting on a trigger that did not correspond to anything real is the exact
complaint the row itself made.**

**The trigger that failed, recorded so it is not reused:** the row was parked on *another seat's
review verdict*, which reaches terminal state when the **review completes**, not when the slice lands
and not when anything about the types changes. It auto-unparked at `08:13:57` for that reason alone —
**a review closing is not a slice landing and is not a precondition.**

**New trigger, checkable in one command:**
```bash
for f in $(git grep -l InterlockCondition origin/dev -- frontend/ | sed 's|^origin/dev:||'); do
  git show origin/dev:$f | grep -q ui-faceplate-lib &&   git show origin/dev:$f | grep -q ui-inspector && echo "TRIGGER: $f"
done
```
**Fires only when a file imports BOTH packages** — the condition that makes the collision dangerous.
Verified **zero** today. Row re-parked on it.

**The general rule this produced:** **a blocker must name a condition that can be observed, not
another seat's queue row.** A row blocked on a review verdict wakes when the review finishes, which
may have nothing to do with the work being parked.

## S-005 CUT AND AUTHORED (2026-11-11)

Unblocked when S-002 landed: `CULTURE.md` is on `dev` (`9639acf`), `DECISION-DEFAULTS.md` is absent,
and `CULTURE.md` ends at §9 so §10–§11 append without editing existing text.

**Cut:** `policy/decision-defaults-s005` at `.claude/worktrees/policy-decision-defaults`, from
`origin/dev` @ `cdebf3e`.

**Authored by `pm`, committed `bf92e97`** — 3 files, **+159/−0**, zero deletions:
- `rig/culture/DECISION-DEFAULTS.md` (new) — the reviewed text with **F1 and F2 applied to it**, not
  a blob copy of the rejected version
- `rig/culture/CULTURE.md` — **§10 and §11 appended**
- `rig/mythos-org.yaml` — **the delivery entry, which had never existed.** The row claimed it since
  it was written; `grep -rn DECISION-DEFAULTS rig/` returned only the ledger and desk prose.
  **Without it the policy had no operational force anywhere** — the live risk `dev_gatekeeper` found.

**Receipt:** `rig spec validate` → *Rig spec valid*; `preflight` → *Preflight ready*;
`./scripts/check.sh --rust` → **VERDICT: PASS**; `--frontend` → **VERDICT: FAIL** on missing
`node_modules`, **which is the `skip()` fix working, not a red** — the slice is docs-and-config with
no frontend change, and `pm` did not install dependencies in a docs worktree to manufacture a green.

**A grep trap worth recording:** F2's sentence **wraps across two lines**, so
`grep -c "does not change who may perform"` returns **0**. It gave `pm` a false negative and nearly
caused a fix to be reported missing. Verified with `tr`. **A multi-line assertion is not a grep.**

**No reference to `rig/SLICES.md`, a slice id or a `qitem` appears in the policy file** — the
`fbaaa4d` defect. Checked.

**PUSHED: PR #63.** The head has moved **twice** since authoring — `cf0d3d6` then `190fec2`, both
`Merge branch 'dev'` — **fourth head divergence of the day.** The content is unchanged through them,
verified by blob rather than by SHA:
```
git rev-parse 190fec2:rig/culture/DECISION-DEFAULTS.md  -> 5381057…
git rev-parse bf92e97:rig/culture/DECISION-DEFAULTS.md  -> 5381057…   identical
```
**Compare content, not SHAs** — a rebase moves the head without touching the tree, and a squash-merge
leaves the branch tip outside `dev` entirely.

**Review row: `qitem-20261111103343-be5b41cf9c161f55` → `pod_uiux.reviewer`** — created *after*
`pm` had created the same row for five other slices and not for its own. See the observable-blocker
section below.

### VERDICT: APPROVED at `190fec2`, no conditions

`pod_uiux.reviewer` re-verified all four checks **fresh** rather than assuming its own fixes landed:
- **F1 HOLDS** — the provenance blockquote. *"transcribes what he said; it is not his authorship"* is
  the exact claim the original version lacked.
- **F2 HOLDS, and the wrapping trap is real** — `grep -c` returns **0**, joined with `tr` it is
  verbatim. The reviewer said they would have reported their own fix missing without the warning.
- **The `rc` claim HOLDS, and better than the fix that was asked for.** "rc is reachable" is gone;
  *"`release_auditor` may proceed on `rc` without waiting for me; `release` is not reachable by
  default, by anyone."* The phrase *"the one hard exception is the `release` branch"* **survives** in
  CULTURE §10 — but is no longer the defect, because the sentence after it names the misreading
  and closes it. **That is a better fix than deleting the sentence**, and the reviewer recorded the
  distinction between "satisfied in substance" and "satisfied literally" rather than scoring it.
- **The delivery entry HOLDS**, tested: yaml parses, `startup.files` has 2 entries, this one
  `required: true`. And they **verified the comment's own limitation** — deleted
  `DECISION-DEFAULTS.md` from an extracted tree, re-ran validate, still *"Rig spec valid"*. So **F3
  stays open by design, documented at the delivery entry, not silently assumed closed.**
  *"A comment that states its own limitation is worth more than one that implies coverage."*

**The twice-shipped defect is clean here:** no `S-0xx`, `PR #`, `qitem-`, `rig/SLICES` or `SLICES.md`
reference in the policy file or the §10–§11 additions.

**What the reviewer did NOT run, recorded honestly:** no `./scripts/check.sh --rust`. The diff is two
Markdown files and one YAML — a green from it would say nothing about this diff, *"and after this
session, a gate that answers a question other than the one asked is the specific thing I have been
blocking."* That is the right call.

**Handed to `qitem-20261111104621-1b0eb972739d6da0` → `dev_gatekeeper` for the merge.** After merge
the policy has operational force **for the first time**, but only on the **next restore** — no running
seat has it in its startup files yet, which is the correct state.

## A SAFETY GUARD WAS LOOSENED BY A MERGE — KNOWN, ACCEPTED NARROWING (decided 2026-11-11)

Found by `pod_hmi.reviewer` **inside PR #61's review window**; verified by `pm` at `c12b5af`.

`frontend/packages/ui-shell/src/__tests__/noFabricatedDefaultsSweep.test.ts` — the SYS-REQ-096
fixture-vocabulary guard — now **strips comments before matching**. The rationale is defensible and
is in the code: `NetworkCanvas` gained a comment recording that `DEFAULT_STATIONS` was removed, and
the guard was flagging **the record instead of the code**. A comment cannot fabricate.
**PR #50 was not wrong to do it.**

### The strip does NOT merely skip comments — it truncates the line

The reviewer and `advisor` probed the regexes rather than eyeballing them, and `pm` reproduced the
result:
```
const u = "http://x/DEFAULT_THING";        -> CAUGHT   (":" before // is not whitespace)
const u = "see // DEFAULT_THING";          -> MISSED   strips to 'const u = "see '
const s = "prefix // DEFAULT_DEVICES x";   -> MISSED   strips to 'const s = "prefix '
const t = "x\t// DEFAULT_CABLES";          -> MISSED   (tab counts as \s)
const x = DEFAULT_STATIONS;                -> CAUGHT
// DEFAULT_STATIONS was removed             -> MISSED   (the intent)
```
**A whitespace-preceded `//` inside a STRING LITERAL truncates the rest of the line** — closing quote
and everything after it — so the matcher never sees it. Not just identifiers: the whole line tail.

### The row was CANCELLED rather than pinned, and that is the decision

`pm` wrote an acceptance criterion ("pin a `//` inside a string is not eaten"). It was **wrong twice**,
and both corrections mattered:

1. **Unbuildable.** The guard is one `it()` that `readFileSync`s real files over a hardcoded
   `surfaces` array — **no injectable source**, so there is nothing to feed. The only way to redden it
   is to commit a fixture into a real component. `pod_hmi.reviewer` caught this.
2. **Would have certified a bug as correct.** The buildable version asserted
   `fixtureVocabularyIn("see // DEFAULT_THING")` → `null`. That does not pin a property — **it
   enshrines line truncation inside a string as intended behaviour**, and would make a correct future
   fix look like a regression. `pod_hmi.reviewer` retracted this too, having first generalised from
   the URL case that happened to pass.

**The real options were never "pin it" or "leave it unpinned."** They are **(a)** accept the strip with
its narrow quirk and record it, or **(b)** replace the regex with something that knows whether `//` is
inside a string — a tokenizer, i.e. real machinery for a test-only guard.
**Decision: (a).** Row `qitem-20261111095028` **cancelled**.

**Accepted exposure, stated exactly:** the truncation needs a whitespace-preceded `//` inside a string
literal, in a listed surface, **on the same line as a fixture identifier**. Narrow and contrived.
**The guard still fails on the defect class** — real reintroduction trips in every form, the
motivating comment case is clean, `DEFAULT_ON`/`OFF` exemption intact.
**Classification: known accepted narrowing. Not a behaviour defect, not a phantom task.**

## S-004 APPROVED AT `333f753` — AND MY OWN PRE-MERGE CHECK WAS THE WRONG INSTRUMENT

`pod_hmi.reviewer` approved. Both blocking items closed:
- **Item 1 carried** from `2c3f242` — `useSseTelemetry.ts` byte-identical (`5d7aebac`), so the earlier
  mutation proof still describes this tree.
- **Item 2 mutation-proved at the live head**, in a fresh extraction:
  restore the hardcoded `'BAD COMM FAILURE'` → **7 red**; key presentation off the specific detail
  instead of the class → **6 red**; control after restore → **30/30 green**. **The second mutation is
  the one that matters — it is the exact trap, and the suite catches it rather than only catching the
  wording.**
- **Gate: PASS on all six, exit 0** — **the first full `check.sh` receipt on any slice in this repo.**
  Concurrent-build check run first, exit code read from the script and not through a pipe.

### `pm` ruled the merge gate wrongly, and the reviewer's test is better

`pm` ruled: *"do not merge #61 unless `cd57db1` is an ancestor of the head."* **At `333f753` that
check FAILS** — and yet the badge fix **is** present. It was **re-committed as `040d2ea`** rather than
fast-forwarded, and `cd57db1` now lives only on `origin/backup/s004-badge-cd57db1`.

> **An ancestor test asserts a particular COMMIT is in the history. What needs to be true is that the
> CONTENT is in the tree.** A re-commit breaks the first while the second still holds.

**The strongest check, and it costs one command — `pod_hmi.reviewer`'s, better than `pm`'s pair:**
```bash
git rev-parse "${head}:frontend"     # compare against the head the gate was actually run at
```
**Measured: `cd57db1`, `040d2ea`, `333f753` and the MERGED `08245d0` all give `a4916f72`.** The entire
frontend is byte-identical across the commit the reviewer reviewed, the commit that was pushed, the
remote head, and what landed on `dev`.

**Why the tree hash beats both alternatives:** an **ancestry** check would have **blocked a correct
merge** (content is present after a re-commit). A **marker grep** would have **passed at a head where
someone re-added the files with different content** — it confirms a string exists, not that it is the
right string. **One `rev-parse` on a tree is stronger than both and costs the same.**

**And the rule that survives all of it:** **ancestry FAILING is a signal to check content — never a
verdict that content is gone.**
**Same correction `advisor` gave `pm` about merge status hours earlier** (`merge-base` vs the PR API):
a test answering *"is this commit in the history"* is not answering *"is this content in the tree."*
**`pm` has now made that mistake in two different forms in one day.**

### The reviewer's pre-verdict check, adopted

**Frontend tree hash identical at `040d2ea` and `333f753` → `a4916f72`.** That is what makes a gate
receipt **portable across a merge commit** — it proves the gated tree is the merged tree without
re-running six gates. **If the head moves again, re-check the tree hash rather than re-running.**

### Residual, recorded and not blocking

**Case C still renders `'BAD COMM FAILURE'` when NO detail is supplied** (`processData.ts:104`). That
is the existing pinned contract and it is **S-012's**. The specific word is always supplied on the
S-004 path, so it is unreachable from the transport this slice builds — **the defect class is gone from
this path, not from the codebase.**

## TWO RECORDS OF ONE FACT — and nothing enforces that creating one creates the other

`dev_gatekeeper`'s formulation, and it generalises the whole day's class of failure:

> **A slice named in the ledger without a destination session is a SECTION, not a ROW. Ledger sections
> and queue rows are two different objects, and nothing enforces that creating the first creates the
> second.**

**Measured today: six slices specified, four absent from `dev`'s ledger, and S-017 with no row at all
until the auditor scanned six times for it.** Five slices in total ended up specified-without-a-row
(S-003, S-007, S-009, S-010, S-011, S-017) — **four `pm` caught itself, one it did not.**

**The mechanical form, one line each way** (`dev_gatekeeper`):
```
a queue row with no ledger section   -> work nobody can find in the plan
a ledger section with no destination -> a plan nobody is doing
```
**Same divergence read from opposite directions. Both were live today.**

**The family:** every one of today's record failures is the same shape —
- a closure claiming `handed_off_to` with no successor row;
- a ledger table naming a merged PR as blocking;
- a section describing a slice no queue row carries;
- a PR body attesting a tree a check ran against.

**In each case two records describe one fact, and only reading both reveals the divergence.** The
mechanical form is a check per divergence — `check-blockers.sh`, `check-merges.sh`, the successor guard
— because a rule that depends on one seat remembering two artifacts will fail on the first busy hour.

## THE MERGE COUNT IN PROSE DRIFTED FROM THE TABLE AGAIN — and `#64` was missing entirely

`executive-advisor` caught it: **`dev` HEAD `7c35e80` IS merged PR #64, and the ledger named it
nowhere** — **zero hits for `#64`**, no row in Open, none in Closed. `pm` had recorded "eleven merged"
in prose and a `#64` merge in conversation, and **the table still had ten.**

**This is the third time today the same split occurred**, and the mechanism is now clear enough to name:
**`pm` writes the merge into prose because prose is where the narrative goes, and the table only gets
updated when a *slice* is touched.** A PR that closes a slice gets a row move; a PR that merges without a
ledger row just gets mentioned.

**Fixed:** `#64` is now in the Closed table, and so is `#62` — which `pm`'s own sweep found missing in
the same pass, the ledger's own PR with no row in the ledger it carried.

**The mechanical check, `/tmp/check-merges.sh`, and its first run is the part worth keeping.** It
asserted every merged PR on `dev` had a row and reported **38 of 54 missing** — because it swept the
whole history back to PR #3, and **the ledger covers the rig era, not the pre-rig sprints.** That number
was wrong as a result, and **informative as a fact**: it is the only place anything records where the
ledger's coverage begins.

> **A check whose scope was wrong once, and says so, is more trustworthy than one that has only ever
> been green** (`dev_gatekeeper`'s argument for keeping the number rather than replacing it with the
> green result — the same argument as keeping the wrong version of the `TopologyGraph` name).

**Scoped to merges since 2026-11-11 it reports green.** Both numbers now live here, because the green
alone hides why the scope is what it is.

## A TEST THAT ASSERTS A CONSTANT PROVES PLUMBING, NOT MEANING (S-001, then S-014)

**Two real instances today, and one good example — the accurate count matters.**

| case | what it asserted | what that leaves unproven |
|---|---|---|
| **S-001** | `expect(presentation.tone).not.toBe(other.tone)` — the **enum member** | that the two states **render** differently. Making `TONE_CLASSES.warning` byte-identical to `TONE_CLASSES.alarm` kept the suite **5/5 green** while both states rendered the same CSS. |
| **S-014** | `expect(notice).toBe(DRAFT_DISCARD_NOTICE.QUALITY_LOST)` — **6 times**, all against the constant | that the notice **says** what it means. Swapping the text to `'Value committed successfully'` passes all 27 tests — **a discard announcing a successful commit**, which is a fabricated success. |

**The counter-example, and it is the one to copy:** `qualityDetailBadge.test.tsx` has **19 render-level
assertions** — `textContent`, `data-quality`, `className`. That is the shape that can fail for the defect
it names.

**The rule is not "assert the render"** — S-001's author knew that, and so did S-014's. **The rule is
about which assertions *look like* coverage:**

> **`expect(x).toBe(CONSTANT)` where `x` was produced from `CONSTANT` proves the plumbing, not the
> meaning.** It cannot fail unless the constant is undefined or the call is deleted.

### The shape to copy — assert a PROPERTY, not an instance

S-014's fix produced the good example, and it is better than the rule as `pm` first wrote it:

```ts
it('never lets a discard notice claim a successful commit', () => {
  // The exact mutation from review: swap the string for its opposite and the suite stayed green.
  for (const words of Object.values(DRAFT_DISCARD_NOTICE)) {
    expect(words.toLowerCase()).not.toContain('committed successfully');
    expect(words.toLowerCase()).toContain('discard');
  }
});
```

**It iterates every value and asserts a property of the words** rather than equality with one constant —
so it **rejects the failure class, not the instance.** A future notice with the same inverted meaning
fails this test; a future notice with new wording that still says "discard" passes it.

> **Test the property, not the nearby operator** (`pod_hmi.reviewer`) — which is also why their own
> suggested mutation for the thumb was wrong: restoring `value={speedDraft.value ?? 0}` **passes,
> correctly**, because that branch is unreachable when the value is null. The load-bearing guard is the
> null-check gating render, and mutating **that** is what reddens it. **A literal mutation applied to a
> literal reading would have sent the builder to test code that was already sound.**

**Why this needs a culture line rather than a fix per slice:** both authors believed they had tested the
defect. The test names the right thing, passes, and covers nothing — and `pm` signed off one of them.
**A guard that cannot fail for its own defect reads exactly like a guard that can.**

## A WELL-FORMED WORKFLOW IS NOT A WORKFLOW SHOWN TO FAIL WHEN IT SHOULD

`dev_gatekeeper`'s sentence, written on S-016's queue row at `12:48`, and it is the standard for every
gate this repo builds.

**S-016's workflow is correct by inspection** — `permissions: contents: read` makes tag creation
impossible rather than promising not to, the steps run in order, and the file states the `build.rs` stub
limitation in its own header. **Every checkbox is met and `dev_gatekeeper` did not merge it.**

**Because the job has never been shown to fail.** The acceptance evidence is two **red** directions with
pasted output and the failing job name — a deliberate compile break, and a `bash -n` failure — plus one
clean green. **Until those exist the job's failure behaviour is unproven**, and an unproven gate is
precisely what today was spent discovering: *`check.sh` printed a false green for weeks, and the reason
nobody noticed is that nobody had shown it failing.*

**The same standard applied to the other defects:**
- S-001's tone guard was **green 5/5** while two states rendered identical CSS — never shown red.
- S-014's notice test **passed** with the message inverted — never shown red for its own defect.
- The `mythos update` suite **passes** on a fabricated catalog — the tests assert the fiction.

**Two facts about a gate, and only the first is cheap:**
| | proven by | how it can lie |
|---|---|---|
| **it is well-formed** | reading it | it can be perfectly shaped and do nothing |
| **it fails when it should** | injecting the defect and pasting the red | nothing — a red for the right reason cannot be faked |

**Built is not done. Built-and-shown-to-fail-for-the-right-reason is done.**

## BEFORE NUDGING A SEAT, CHECK WHETHER IT HAS PRODUCED ANYTHING

`dev_gatekeeper` nearly nudged `pod_backend.builder` twice during a **35-minute silent turn**. It checked
first and found the builder had **committed at 12:37** — working, not stuck.

> **Silence in the pane is not idleness. The cheapest disambiguator is an EFFECT — a commit, a pushed ref,
> a new file, a queue update — not a message.**

**This is the evidence-by-effect rule applied to liveness instead of delivery**, and it is the same
instrument: `rig capture` shows what a seat *appears* to be doing; `git log` and `git status` show what it
*did*. **"A nudge at a seat doing a 20-minute serial release build is pure interruption"** — and on a rig
that has spent all day correcting claims made from adjacent evidence, the probe should be the cheap
authoritative one.

## CHECK WITH SOMETHING THAT COULD DISAGREE — the day's actual method lesson

Arrived at between `pm` and `dev_gatekeeper`, and it is sharper than the version either started with.

**The wrong version:** *"check twice."* **Two runs of the same check on the same input cannot return
different answers, so their agreement is not evidence.**

**The right version:** *"check with something that could disagree."* **A verification that cannot return
a different answer is not a verification** — the same sentence as *"a verification that returns the same
answer for every row is a constant,"* one level up.

**Every finding that actually moved something today had a second instrument that could disagree:**

| primary check | second instrument | why it caught what the first could not |
|---|---|---|
| `git merge-base --is-ancestor` (ancestry) | `git rev-parse <head>:frontend` (tree hash) | squash-merge and re-commit break ancestry while content holds — **different failure modes** |
| single `ping` sample | second endpoint + control host | one host cannot distinguish a sick endpoint from a sick path |
| `grep "no topology route"` (returns 0) | enumerate all 25 routes | **a grep for an absence can only ever return zero**; only an enumeration can return 25 |
| check scoped to all history | check scoped to the rig era | the wrong scope **failed loudly (38/54)** where a correct one was green |

**And the controls that were worthless:** two seats reading the same function with the same question.
That is nearer a **spell-check** than a second opinion — it establishes that the code says what both
readers think it says, not that the ruling about it is right.

**`pm`'s own instance:** the `check-merges.sh` first run reported **38 of 54** because its scope spanned
pre-ledger sprints. It was *wrong as a result and informative as a fact* — and a correct-scope check
could never have produced that number. **A check that fails differently is worth more than one that
agrees.**

## A QUEUE READ TELLS YOU WHERE WORK WENT; AN EFFECT TELLS YOU IT ARRIVED

Found by `dev_gatekeeper`, and it is the most transferable lesson of the day.

**It cost four wrong closures.** It was closing stuck-sweep rows with *"FALSE POSITIVE — the successor
row exists and is in-progress"*. **All four were right** — and that is precisely why the habit
survived: *a weaker check that keeps being right never gets corrected.*

> **A queue read tells you where work was sent. A commit, a pushed ref, or a changed grep tells you it
> arrived.**

**The form `pod_hmi.reviewer` used to settle one properly:** a reply timestamp, an `ls-remote`, and a
grep that now returns **zero** where it returned **one**. Those are effects. A row's `state` field is
**testimony**.

**And the tell that a defect is structural rather than personal:** **two seats hit
`handed_off_to`-without-a-successor within one hour** of it being named. `update --closure-reason
handed_off_to` **cannot** create a successor, so the only honest sequence is **create-then-close**, and
**nothing enforces that order.** The fix is the verb (`handoff-and-complete`), not more care.

**`dev_gatekeeper` then corrected one of its own resolutions from ten minutes earlier** — it had closed
a row saying `pm` *"had since acted on the handoff"*, citing a decomposition and a timestamp. **That was
inference from adjacency, not evidence.** The evidence exists now (the build row), so the claim is true,
**but it was made before it could be cited.** It added a note rather than editing the claim away,
because *"the interesting part is that I reached for the weaker standard immediately after agreeing to
adopt the stronger one."*

**That sentence is the most useful thing written in this ledger today.** Every rule in this file is one
paragraph away from being broken by the seat that just wrote it.

## `handed_off_to` IS A CLAIM; `handoff-and-complete` IS THE VERB THAT CANNOT LIE

Found by `release_train.auditor`, and `pm` made the mirror-image version of the same mistake the same
day.

**The auditor's form:** closed a row `closure_reason=handed_off_to` with **no successor row behind
it.** The rig warned at the time — `handoffAdvisory status=unverified reason=no-live-local-successor` —
and the close proceeded anyway. **A closure that records a transfer and has no successor is a claim,
not a handoff.** The swept row was right.

**`pm`'s form, the other direction:** closed rows `handed_off_to` where the "successor" was **a message
to a seat**, not a row. Same defect from the other side — the transfer was real but unowned.

> **`rig queue handoff-and-complete` creates the successor atomically and cannot lie.
> `rig queue update --state done --closure-reason handed_off_to` cannot create one, so it can.**

**And `pm` hit it a second time within the hour**, handing the S-016 cut row off without a successor
and filing the build row by hand afterwards. **Two seats, same defect, one hour after it was named** —
which is the argument for the verb rather than the discipline.

**Adopted as the default: any closure that transfers work uses `handoff-and-complete`.** If the
successor already exists (because it was filed first, as with S-014 and S-015), then `update` is
correct — **the rule is that the successor must exist before the close, not that one verb is always
right.**

**The general shape, and it is the day's recurring one:** the advisory told the truth and was
proceeded past. **A warning nobody reconciles is the same as no warning** — which is why a different
`pm` pass reached the same row from the other direction and caught it.

## LIVE FABRICATION — `mythos update` MANUFACTURES A VERSION IDENTITY (found 2026-11-11)

Found by `release_train.auditor`, **every claim re-verified by `pm`** against `origin/dev` @
`75200c0`. **This is the most serious class in this repository's culture: an operator-facing surface
reporting a value it did not measure.**

**Four mechanisms, each independently verified:**

1. **A copy becomes an upgrade.** `update.rs:45-58` — `mythos update` copies `current_exe()` into
   `~/.mythos/versions/<target>/bin/` and records `<target>` as current. **A `0.1.0` binary becomes
   `"0.2.0-rc.1"` by being copied.** No download, no verification, no checksum.
2. **If the copy fails, it writes a fake binary** (`update.rs:59-63`):
   ```rust
   fs::write(&target_bin, format!("#!/bin/sh\necho 'MythOS v{}'\n", version_to_install))?;
   ```
   **A shell script that prints the intended version and does nothing else.**
3. **The "remote" fetch is a hardcoded literal.** `fetch_remote_versions` (`versions.rs:69-76`) logs
   the URL and returns `ReleaseCatalog::default()` — `0.1.0` / `0.2.0-rc.1` / `0.2.0-dev.20261019`
   (`versions.rs:16-39`). **There is no HTTP client in the workspace at all:** `grep -cE
   'reqwest|ureq|hyper|isahc|surf|attohttpc'` over every `Cargo.toml` → **zero hits.** The URL is
   never contacted.
   **The signature is the lie, and that is sharper than "the URL line reads as a fetch":**
   `fetch_remote_versions(remote_url: &str, ...)` **accepts a URL**, so every caller believes it
   fetches — `update.rs:37` passes `&config.remote_url`, and **`lib.rs:92`** then **prints
   `"Official Release Versions (Remote: {url}):"` to the operator.** A caller cannot discover it is a
   constant without opening the body. **The command tells the user it contacted a server it never
   contacted.**
   **And the one trace that a fetch was attempted is SILENT at default verbosity** (auditor, verified
   by `pm`): `versions.rs:74` logs `tracing::debug!`, and `main.rs:7` calls
   `tracing_subscriber::fmt::init()`, which installs `EnvFilter::from_default_env()` — a default
   directive of **`ERROR`**. Unless the operator sets `RUST_LOG`, **nothing is printed at all.**
   **So the "attempt" rules out "the operator could have seen it fail".**
4. **A REQUIRED CHECK CERTIFIES IT.** `cli_tests.rs:46-58` calls
   `fetch_remote_versions("https://releases.mythos.firmansyah.co", None)` and then asserts
   `assert_eq!(rc_latest, "0.2.0-rc.1")` — **a value that never came from that URL.** It runs inside
   `cargo test --workspace`, which CI runs on every push. **The green suite is what makes it look
   verified.** The URL in the test is the most misleading line in the file: it reads as a fetch and
   is a constant.

**Consequence for the release gate:** `SYS-REQ-095` (`spec.md:549` — `versions --remote`,
`update --version`, `rollback <1s`) **cannot be cited as gate evidence**, because three of its four
criteria are these paths.

**And one row is blocked on a condition the code has already broken:** `qitem-20261027083546` is
parked on the premise that the CLI *"would ship as a stub — which the decision forbids."* **The stub
is already on `dev`.**

**Why this outranks everything else in this ledger:** every other fabrication found today is
**LATENT** — unreachable until a slice wires it. This one is **LIVE**: `mythos update` is a shipped
command, the test suite attests it, and an operator who runs it is told they have a version they do
not have. **It is the exact failure `CULTURE.md` §5 exists to prevent, on the surface that tells a
person what software they are running.**

**Classification: LIVE.**

## A LEDGER PR CAN BE STALE AT THE MOMENT IT IS REVIEWED — check the diff, not the PR state

`pm` caught this on PR #62 **while asking for its re-review**, and it is the advisor's finding a third
time, now at the sign-off step.

**Measured** (`git show 8ae6fe5:rig/SLICES.md` vs the current file):
```
PR #62 ledger   2163 lines
current ledger  2280 lines
delta           +122 / −5
```
**Missing, all written after the gatekeeper pushed:** the **S-014** slice section (the slice cut minutes
earlier), the **S-004 approval** record, the **open-table-decay** lesson, and the **tree-hash check**
itself.

> **A re-review at `8ae6fe5` would have APPROVED a ledger that does not contain the merges it is
> supposed to record, or the pre-merge checks it is supposed to teach.**

**Why this one is sharper than the earlier staleness:** the earlier instances were rows going stale
against `dev`. **This is the review artifact itself being stale against the author's own work** — the
verdict would have been given against a document nobody was actually maintaining.

**The check:** before requesting a re-review, diff the PR's file against the author's current file:
```bash
git show <pr-head>:rig/SLICES.md > /tmp/pr.md
diff /tmp/pr.md rig/SLICES.md | head
```
**A PR state of `CLEAN` says the pushed file is consistent. It does not say the pushed file is current** —
the same shape as a green CI attesting a tree that is not the one you meant.

**Preserved so nothing was at risk:** `.openrig/evidence/SLICES-newest-20261111.md`
(sha256 `97e1cbe2…`), with `rig/SLICES.md` in the repo root as the source of truth.

## THE OPEN TABLE IS A CACHE OF EXTERNAL STATE, AND IT DECAYS WITHIN MINUTES

`executive-advisor` caught the same defect **twice in 40 minutes** — first five merged PRs marked as
blocking, then S-003/#57 and S-007/#58 the same way. **Both times the prose was current and the table
was stale.**

**The diagnosis `pm` got wrong the first time:** this is not carelessness. **The table is a cache of
state that four other seats change without touching it.** Every merge moves a row's truth and nothing
notifies the row. Updating it "carefully" cannot keep up — S-005 went from cut to merged *while `pm`
was mid-pass on the same file*.

**The mechanical check, which is the actual fix:**
```bash
# Every PR merged today. Any OPEN row citing one of these is a stale cache entry.
awk '/^## Open/{f=1} /^## Closed/{f=0} f&&/^\| `S-/{print}' rig/SLICES.md   # the open rows
gh pr view <n> --json state,mergedAt                                        # the truth
```
Run before any pass that reports state. **`pm` ran it against all eight merged PRs and five open rows
— clean.** A written rule was not enough last time; this is a command, and it takes one invocation.

**The second-order lesson:** `pm` moved five rows to Closed, wrote a section about how the table is
the authority, and **still** left S-003 and S-007 stale — because the pass had fixed the rows the
merge queue had already reported and stopped. **The check must be mechanical precisely because the
author of the rule is the one who skips it.**

## THE STALENESS WINDOW IS THE HANDOFF — a split workflow cannot be atomic

Found by `governance.scribe` after its worktree went behind `dev` a **third** time:
> *"every rebase that completes can go stale before it is committed. The only shape that survives the
> race is rebase + commit + push in one continuous step, whoever performs it."*

**`pm`'s refinement, which is the general form:** it is not that `dev` moves too fast — it is that
**the rebase and the push are performed by different seats.** `scribe` rebases and commits; only
`dev_gatekeeper` pushes. So for `scribe`, every rebase is necessarily **rebase-without-push**, which is
**exactly the shape that goes stale**. Scribe was right not to reverse its own commitment, and
reversing it would not have helped: it would have rebased into `094d5db` and been behind again the
moment the next PR landed.

**The rule:** **any workflow where step A is one seat and step B is another has a staleness window
equal to the handoff.** Either one seat does both, or the second step starts from a ref the first step
just confirmed.

**Verified clean this time** (`pm`): worktree at `792d139`, 1 behind `094d5db`;
`git log 792d139..origin/dev` over the two files → **empty**; and `094d5db` touches exactly one file
(`isa-18-2-alarm-engine.md`, +50/−1).

## S-004 — A GREEN PR WOULD HAVE MERGED THE BLOCKING DEFECT (caught 2026-11-11T11:0xZ)

Found by `pod_hmi.reviewer`, verified independently by `pm`. **This is the sharpest near-miss of the
day:** every check on the PR was green and the PR body was accurate — and the merge would have
shipped the exact defect `pm` ruled blocking.

**Measured:**
```
remote head  feat/hmi-live-telemetry : a711928
git grep qualityDetail a711928 -- frontend/   -> ZERO hits
git grep qualityDetail cd57db1  -- frontend/  -> 16 hits across 5 files
git merge-base --is-ancestor cd57db1 a711928  -> NO
```
**Cause, from parentage:** the badge fix is stacked on an **unpushed** `Merge branch 'dev'` commit
(`cd57db1`, on parent `aada47f`) while the pushed line runs through `d8d9046`. **The same
connection-fix commit message appears on two different commits** — one pushed, one not.

**Why it is dangerous rather than merely untidy:** PR #61's green CI and its PR body describe the
**connection fix only**. A merger reading a green PR would be reading a true attestation of *a
different tree* than the one that would have landed. **Cost of the miss: a faceplate where a failed
SENSOR reads BAD COMM FAILURE — the item ruled blocking three hours earlier.**

**The check that catches it, and it is not the PR state:**
```bash
git merge-base --is-ancestor cd57db1 <head-being-merged>   # must be YES
git grep -c qualityDetail <head>:frontend/                 # must be non-zero
```
**A green check says the pushed tree passed. It never says the pushed tree is the tree you meant.**

**The content itself is correct** — `pm` verified it, so this is a **push problem, not a code
problem**:
- `DataQualityBadge.tsx:31-34` — presentation keys off `normalizeQuality` (the three-value class);
  `qualityDetail` reaches **only** the label function. **The invariant is written in the file's own
  doc comment (`:12-13`)** rather than left implicit.
- `qualityDetailBadge.test.tsx:58-67` — asserts **both halves** for `BAD_SENSOR_FAILURE`: text
  contains no `COMM`, `data-quality="BAD_QUALITY"`, `isAmber` true with the message *"an unnamed bad
  cause must stay amber, not grey"*, `isGrey` false. **That is the criterion `pm` set, met.**

**Ruling: do not merge #61 at any head that is not a descendant of `cd57db1`.**

## THE DISCOVERY-TO-PLANT WRITE PATH DOES NOT EXIST — verified 2026-11-11

Found by `governance.scribe` while correcting ADR-009, and **widened by `pm`**: the honest state of a
safety guarantee is stronger than the document first claimed.

**The claim that was almost shipped:** *"that write happens in the backend, and nothing in the
frontend can refuse it."* That asserts a backend write path.

**Measured on `origin/dev` @ `094d5db`:**
```
TopologyGraph::add_node      (mythos-core/src/topology.rs:237)  -> callers only in tests/
TopologyGraph::add_edge      (:266)                             -> ZERO production callers
TopologyGraph::add_ip_block  (:312)                             -> ZERO production callers
```
**There is no discovery-to-plant-model write path for a node, an edge, or an IP block. The guarantee
is UNBUILT, not enforced.**

**What actually holds, and it is real:** `TopologyClaim` is a **discriminated union**
(`provenance.tsx:87`) with `commissioning` **required** (no `?`) on the `DISCOVERED` arm (`:93`). So
*"discovered therefore plant equipment"* is **inexpressible in the type** — a surface cannot present a
candidate as commissioned. That is a **shape** guarantee, not a runtime check.

**The distinction that must survive edits** (it was the scribe's own overclaim, caught by acting on
`pm`'s warning):
- the **union type** prevents an unsupported **CLAIM** on a **surface**;
- it does **NOT** prevent a backend path from writing a station into the **plant model**;
- **and no such backend path exists today.**

**Collapse those three into "the rule is enforced" and it reads as a backend guarantee that does not
exist.** That is the same class as the fabricated rack: a document asserting protection the code does
not provide.

## A BLOCKER MUST BE OBSERVABLE — AND I BROKE MY OWN RULE ON S-005

`pm` recorded this rule earlier today, then broke it on the one row `pm` owns.

**What happened.** S-005 was parked with `blockedOn: pod_uiux-reviewer@mythos-org` — **a seat**. A
seat never reaches a terminal state, so the row kept reading as an **open obligation**: the watchdog
woke `pm` for it three times, each time on work that was authored, committed, pushed and simply
waiting for a verdict nobody had been given a row to deliver.

**The fix:** the review is now **owned work** — `qitem-20261111103343-be5b41cf9c161f55` →
`pod_uiux.reviewer` — and S-005 blocks on **that qitem**. **A qitem reaching terminal state is
observable; a seat is not.**

**How it was missed:** PR #63 was the **fifth slice today** open with **no review row** (after
S-003, S-007, S-009, S-010, S-011). `pm` created rows for all five and then did not create one for
itself. **The pattern is in how slices are opened, not in the reviewers** — and the author of the
pattern is as exposed to it as anyone.

**The rule, restated because it cost three wake cycles to relearn:** a blocker must name *something
that can be observed to change*. A row, a SHA, a command output, a file on `dev`. Not a seat, not an
intention, not "someone will look at it".

## MERGE STATUS COMES FROM THE PR API, NEVER FROM BRANCH ANCESTRY

`executive-advisor` found this while checking the same stale rows, and it is the more important half
of that finding.

**`git merge-base --is-ancestor origin/<branch> origin/dev` reports "open" for a branch that has
merged.** It measured `chore/rig-harness` and `docs/topology-provenance-spec` as open **after** both
had landed. The reason is mechanical: **this repo squash-merges**, so the merge creates a **new
commit** and the merged branch's tip is **never** an ancestor of `dev`.

**The check that works:**
```bash
gh pr view <n> --json state,mergedAt,mergeCommit
```
**Merge status is a property of the PR, not of the commit graph.** Branch ancestry answers a
different question — "is this exact tree in `dev`" — and squash-merging makes it answer "no" for work
that shipped.

**This is the same failure shape as everything else today:** a command that ran, returned a clean
answer, and answered a question nobody asked. `CULTURE.md` §1's cheap decisive checks are
`merge-base --is-ancestor` for *"is my work still in this branch"* — which is correct for its stated
purpose — but **not** for *"did this PR merge".*

## THE TABLE IS THE AUTHORITY, AND I LET IT GO STALE

`executive-advisor` caught this: the ledger's **prose sections** said five PRs had merged while the
**table** still said `in-review`/blocked. Verified against `origin/dev`:

```
6dad46a #60   9639acf #55 (B2 CLOSED)   c12b5af #50   cdebf3e #56   3eaccd6 #59
```

**How it happened:** `pm` updated the narrative five times and the table once. **A reader who trusts
the table — which is the thing that is supposed to be authoritative — got the old world.**

**Worse in the same hour:** `rig/SLICES.md` **lost 334 lines** when the repo root moved to a fresh
branch from `dev`, including the release-path hazard and the B1 correction. `pm` caught it because a
grep for a section written minutes earlier returned **0**, restored from PR #62's branch, and
re-applied what was missing.

**The rule, and it is `pm`'s own:** **the table changes first, before any prose reports a state.** And
**a ledger read in a worktree is not the ledger** — `SLICES.md` was not on `origin/dev` until an hour
ago, so which tree it is read from has always mattered more than it looks.

## TWO FALSE SIGNALS CAUGHT IN ONE MINUTE, BOTH FROM A PIPE

1. **`./scripts/check.sh | tail` printed `[exited with code 0]` while the verdict line said `FAIL`.**
   The pipe's exit code is **`tail`'s**, not the script's. The script exited 1 correctly.
2. **`git ls-remote … | tail` printed `fatal: Failed to connect…` followed by `[exited with code 0]`**
   — the same shape, `pm`'s own command, minutes later.

**The rule: never take an exit code through a pipe, and never take a red or a green without checking
what else was running in that tree.** `CULTURE.md` §4 already requires the concurrent-build check; the
pipe half was not written down and now is.

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
| `S-001` | `test/topology-all-view-modes` | #50 | `c12b5af` | Network view claims; conformance prop GONE, not optional. |
| `S-002` | `chore/rig-harness` | #55 | `9639acf` | **B2 CLOSED.** The harness, gate, culture and ledger are on dev. |
| `S-009` | `docs/topology-provenance-spec` | #56 | `cdebf3e` | Discovery precondition + ADR-009 (PROPOSED). |
| `S-010` | `docs/theme-status-honest` | #59 | `3eaccd6` | Light theme defined but not selectable. |
| `S-011` | `docs/tsk-801-quality-blockers-corrected` | #60 | `6dad46a` | TSK-801's two stale quality blockers corrected. |
| `S-003` | `refactor/device-execution-mode-canonical` | #57 | `792d139` | One `DeviceExecutionMode`; strucpp adopts core's. |
| `S-007` | `docs/alarm-colour-reconcile` | #58 | `094d5db` | Alarm colours reconciled; the two priority vocabularies recorded as non-equivalent. |
| `S-005` | `policy/decision-defaults-s005` | #63 | `f10006a` | Decision-defaults policy + delivery entry. B2's harness made it cuttable. |
| `S-004` | `feat/hmi-live-telemetry` | #61 | `08245d0` | One live TagBus value to one faceplate. **Refused twice while green without the badge fix**; merged only when the fix was present at the head. |
| `S-017-adj` | `docs/topology-spec-post-merge` | #64 | `7c35e80` | Topology contract described as merged; four "open, not merged" statements corrected. |
| — | `ledger-only` | #62 | `75200c0` | The ledger itself, carried to dev so corrections live on the branch rather than in a worktree. |
| — | `ledger-only` | #65 | `d3502f9` | The ledger again — `dev` now knows every open slice, including the four cut after #62. |
| — | `docs/device-mode-wire-form` | #66 | `45fefe8` | Retires the `DeviceExecutionMode` duplicate across four controlled documents. Approved after the reviewer re-measured against the new `dev` rather than carrying its verdict. |
| — | `ci/frontend-gate` | — | **not merged, superseded** | 7 commits, `ci.yml` +90 lines — but the frontend gate job it adds is already on `dev` as `frontend-tests` (`ci.yml:286`). Nothing left to land; **delete candidate**, do not rebase. |
| — | `docs/spi-sections-4-7-rewrite` | — | **not merged** | 2 commits on top of a base 102 commits behind `dev`. Rebase and slice. |
| — | `feat/ui-isa101-consolidation` | — | **not merged, do not merge** | Based on a stale point; merging as-is deletes 17,328 lines of `dev`. Salvage by hand or drop. |

---

## Deleted with the previous rig

14 seat conversations from the old `mythos-org` (archived on the rebuild) are not
resumed. Their open work is the table above; anything not in this file is gone.
