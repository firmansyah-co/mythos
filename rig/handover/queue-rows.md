############ qitem-20261027074238-45f005d580015596 ############
{
  "pickup": {
    "state": "stalled-after-claim",
    "evidence": "no meaningful queue change for 20126 min; owner activity idle-at-prompt (queue age does not prove idle)"
  },
  "waiting": {
    "obligation": "qitem-20261027074238-45f005d580015596",
    "owner": "pod_uiux-reviewer@mythos-org",
    "state": "in-progress",
    "actionableSince": null,
    "blocker": null,
    "lastMeaningfulChange": {
      "id": 931,
      "at": "2026-10-27T08:46:18.754Z"
    },
    "liveness": {
      "subject": "pod_uiux-reviewer@mythos-org",
      "activity": "idle-at-prompt",
      "needsInput": {
        "count": 0,
        "reason": null
      },
      "confidence": "oracle"
    },
    "nextBackstop": {
      "owner": "pod_uiux-builder@mythos-org",
      "mechanism": "queue-recovery:resolved",
      "dueAt": null,
      "intervalSeconds": null,
      "recovery": {
        "qitemId": "qitem-recovery-3eac3e9b1df722c8",
        "state": "done"
      },
      "note": "Current recovery disposition owns the continuation; inspect that row. New source evidence is evaluated afresh."
    },
    "deadlineAt": null,
    "laterBackstop": {
      "owner": "pod_uiux-builder@mythos-org",
      "mechanism": "queue-stuck-sweep:pickup (activity checked at detection)",
      "dueAt": "2026-10-27T08:49:18.754Z",
      "intervalSeconds": 300,
      "note": "Conditional safety net; current delivery/recovery ownership is evaluated first."
    }
  },
  "qitemId": "qitem-20261027074238-45f005d580015596",
  "tsCreated": "2026-10-27T07:42:38.301Z",
  "tsUpdated": "2026-10-27T08:46:18.754Z",
  "sourceSession": "pod_uiux-builder@mythos-org",
  "destinationSession": "pod_uiux-reviewer@mythos-org",
  "state": "in-progress",
  "priority": "urgent",
  "tier": null,
  "tags": [
    "mission:gate-integrity",
    "slice:topology-honesty",
    "pr:50"
  ],
  "blockedOn": null,
  "handedOffTo": null,
  "handedOffFrom": null,
  "expiresAt": null,
  "chainOfRecord": null,
  "body": "REVIEW REQUEST — topology honesty + provenance state model, branch test/topology-all-view-modes, PR #50, head 2a49ae0.\n\nSuccessor for qitem-20261027071650-2b30553f660967a8. IMPORTANT: that row's premise was stale. Item (A), \"make the three views honest and give ui-topology a guard\", SHIPPED IN #33 (merged 8ddac65) and is on dev. Verified by me at origin/dev rather than assumed: DEFAULT_STATIONS/DEFAULT_CABLES exist nowhere in production source (only in __tests__/fixtures and a guard comment), NetworkCanvas destructures initialStations/initialCables with no defaults, the empty state renders, and noFabricatedDevices.test.tsx already renders with no data props. So do not review this expecting a deletion — the deletion already happened.\n\nWHAT IS ACTUALLY NEW AND NEEDS YOUR ATTACK\n1. A four-state provenance model in plugins/ui-topology/src/provenance.tsx: UNKNOWN (the only reachable state today, no transport), CONFIGURED, DISCOVERED_UNCOMMISSIONED, UNREACHABLE. A banner always renders. The premise from the advisor: these are three different claims about a device, not three flavours of blank, and collapsing them now would have to be undone when the transport lands.\n2. Mode coverage: the existing guard only ever rendered the default view. All three views are now walked, each asserting the explicit no-device state and the absence of every fabricated identifier.\n3. The advisor's rule recorded in code where DEFAULT_STATIONS was: NEVER auto-create a node from discovery; discovery yields CANDIDATES and commissioning is explicit.\n\nPROOF SUPPLIED, names first\n- Reintroducing the original defect (initialStations = DEFAULT_STATIONS) -> RED naming BOTH \"renders an explicit empty state when nothing is discovered or configured\" AND \"renders no invented station or cable in ANY of the three views\".\n- Collapsing DISCOVERED_UNCOMMISSIONED into CONFIGURED -> RED naming \"states its provenance, and the three device-standing states are visibly distinct\", message \"DISCOVERED_UNCOMMISSIONED shares a label with CONFIGURED\".\n- Both reverted -> green.\n\nTHE PART I MOST WANT ATTACKED: THE STATE SET ITSELF. I am not confident four states on one axis is right. UNREACHABLE is a HEALTH claim; CONFIGURED and DISCOVERED_UNCOMMISSIONED are PROVENANCE claims. Mixing them on one discriminant may be a modelling error that looks tidy. If you think provenance and reachability should be two independent axes (a device can be configured AND unreachable, or discovered AND unreachable), say so now - that is a cheap correction today and expensive after the transport is built.\nAlso attack: whether asserting distinctness on TONE is a reasonable proxy for \"visibly distinct\" (tone is one code-level notion of visual difference; an operator needs more than a border colour), and whether UNKNOWN should even be a state or simply the absence of one.\n\nONE SIDE EFFECT TO VERIFY: my rule comment tripped noFabricatedDefaultsSweep's surface check, because it matched DEFAULT_ in prose. I fixed the guard to strip comments before that check, and then verified the guard still catches real usage by re-injecting a code-level DEFAULT_STATIONS_FOR_THE_DEMO reference, which it names. Check that stripping comments did not widen the guard.\n\nGATES: pnpm lint exit 0; pnpm build; 47 files / 360 tests.\n\nReview standard: NEEDS REVIEW, not test-only - it adds a public discriminant and a rendered banner.\n\nDo not merge.\n",
  "summary": "REVIEW: PR #50 topology provenance (4 states) + all-three-modes guard. NOTE premise was stale — deletion shipped in #33. Attack the STATE SET: UNREACHABLE is a health claim mixed onto a provenance axis; propose two axes if you agree",
  "evidenceRef": null,
  "humanIntent": null,
  "humanDetail": null,
  "replyTo": null,
  "humanQuestions": null,
  "humanAnswers": null,
  "closureReason": null,
  "closureTarget": null,
  "closureRequiredAt": null,
  "claimedAt": "2026-10-27T08:46:18.754Z",
  "lastNudgeAttempt": "2026-10-27T07:42:41.035Z",
  "lastNudgeResult": "claimed",
  "lastNudgeWireResult": "delivered-ack-pending",
  "lastHeartbeat": null,
  "resolution": null,
  "targetRepo": null,
  "deliveryOutcome": null,
  "destinationClass": "pane-bound",
  "ownerNotificationLevel": null
}

############ qitem-20261026063940-c5f86bd7eda160c2 ############
{
  "pickup": {
    "state": "parked"
  },
  "waiting": {
    "obligation": "qitem-20261026063940-c5f86bd7eda160c2",
    "owner": "pod_uiux-builder@mythos-org",
    "state": "blocked",
    "actionableSince": null,
    "blocker": {
      "ref": "qitem-20261023085422-f459118d847f001d",
      "owner": "pod_uiux-builder@mythos-org",
      "state": "blocked"
    },
    "lastMeaningfulChange": {
      "id": 1012,
      "at": "2026-10-27T11:40:20.958Z"
    },
    "liveness": {
      "subject": "pod_uiux-builder@mythos-org",
      "activity": "idle-at-prompt",
      "needsInput": {
        "count": 0,
        "reason": null
      },
      "confidence": "oracle"
    },
    "nextBackstop": {
      "owner": "pod_uiux-builder@mythos-org",
      "mechanism": "blocker-transition / queue-stuck-sweep",
      "dueAt": null,
      "intervalSeconds": 300
    },
    "deadlineAt": null
  },
  "qitemId": "qitem-20261026063940-c5f86bd7eda160c2",
  "tsCreated": "2026-10-26T06:39:40.253Z",
  "tsUpdated": "2026-10-27T14:39:32.368Z",
  "sourceSession": "governance-pm@mythos-org",
  "destinationSession": "pod_uiux-builder@mythos-org",
  "state": "blocked",
  "priority": "routine",
  "tier": "routine",
  "tags": [
    "mission:gate-integrity",
    "slice:interlock-vocabulary-collision",
    "follow-up",
    "recorded-not-urgent",
    "types",
    "source:executive-advisor"
  ],
  "blockedOn": "qitem-20261023085422-f459118d847f001d",
  "handedOffTo": null,
  "handedOffFrom": null,
  "expiresAt": null,
  "chainOfRecord": null,
  "body": "FOLLOW-UP (recorded, deliberately NOT urgent) — three same-named `InterlockCondition` definitions, one removed.\n\nAdvisor-scoped decision: do NOT fold this into the orphan-deletion work. Record it and move on. I am adopting that, and recording the facts the advisor verified so the next reader does not have to re-derive them.\n\nSTATE: the orphan deletion removes one definition (`frontend/src/components/faceplates/types.ts:12`). Two remain:\n- `frontend/plugins/ui-faceplate-lib/src/types.ts:30`\n- `frontend/plugins/ui-inspector/src/types.ts:8`\n\nWHY IT IS NOT URGENT — the important part: they are NOT structurally compatible. The faceplate-lib one uses `isSatisfied`; the inspector one uses `isPermissive` / `status` / `expression`. And nothing imports both. So today this is a NAMING SMELL, not a silent-wrong-import defect: there is no path where a value of one type is passed where the other is expected.\n\nThat distinction is exactly why this is a follow-up rather than a safety gate. A same-named type that cannot be confused at a call site is cosmetic; a same-named type that can be silently substituted is a real defect. Distinguishing those two cases before assigning severity is the same discipline the sweep row demands (LIVE / LATENT / DEMO) applied to type collisions.\n\nWHEN IT BECOMES REAL: the moment anything needs both — a shared interlock view, a consolidation of the interlock vocabulary, or a transport that carries interlock state across both surfaces (see the telemetry slice). At that point the mismatch is no longer cosmetic, because the wire shape has to pick one.\nRECOMMENDED TIMING: take it WITH the telemetry slice's interlock work, not before. That slice is where an interlock state actually crosses between the two surfaces, so it is where the canonical shape has to be decided by necessity rather than by taste. Deciding it earlier would be inventing a boundary question we do not yet have.\n\nNOTE THE OVERLAP with the InspectorDrawer live-safety fix (qitem-20261026063539-2ffcd951fc3d2810): whatever shape that fix gives the inspector's interlock block is a candidate for the canonical one, since it is the surface that will have to gate on quality. Sequence this AFTER that fix so the decision is informed by the live case rather than made in the abstract.\n",
  "summary": "After the orphan deletion, two same-named InterlockCondition types remain (ui-faceplate-lib uses isSatisfied; ui-inspector uses isPermissive/status/expression). Not structurally compatible, nothing imports both, so it is a naming smell not a wrong-import defect. Take with the telemetry slice's interlock work.",
  "evidenceRef": null,
  "humanIntent": null,
  "humanDetail": null,
  "replyTo": null,
  "humanQuestions": null,
  "humanAnswers": null,
  "closureReason": null,
  "closureTarget": null,
  "closureRequiredAt": null,
  "claimedAt": null,
  "lastNudgeAttempt": "2026-10-26T06:39:41.070Z",
  "lastNudgeResult": "delivered-ack-pending",
  "lastNudgeWireResult": "delivered-ack-pending",
  "lastHeartbeat": null,
  "resolution": null,
  "targetRepo": null,
  "deliveryOutcome": null,
  "destinationClass": "pane-bound",
  "ownerNotificationLevel": null
}

############ qitem-20261026053023-3c4197fd2154e695 ############
{
  "pickup": {
    "state": "parked"
  },
  "waiting": {
    "obligation": "qitem-20261026053023-3c4197fd2154e695",
    "owner": "pod_hmi-builder@mythos-org",
    "state": "blocked",
    "actionableSince": null,
    "blocker": {
      "ref": "qitem-20261023085422-f459118d847f001d",
      "owner": "pod_uiux-builder@mythos-org",
      "state": "blocked"
    },
    "lastMeaningfulChange": {
      "id": 1012,
      "at": "2026-10-27T11:40:20.958Z"
    },
    "liveness": {
      "subject": "pod_uiux-builder@mythos-org",
      "activity": "idle-at-prompt",
      "needsInput": {
        "count": 0,
        "reason": null
      },
      "confidence": "oracle"
    },
    "nextBackstop": {
      "owner": "pod_uiux-builder@mythos-org",
      "mechanism": "blocker-transition / queue-stuck-sweep",
      "dueAt": null,
      "intervalSeconds": 300
    },
    "deadlineAt": null
  },
  "qitemId": "qitem-20261026053023-3c4197fd2154e695",
  "tsCreated": "2026-10-26T05:30:23.223Z",
  "tsUpdated": "2026-10-26T06:34:54.711Z",
  "sourceSession": "governance-pm@mythos-org",
  "destinationSession": "pod_hmi-builder@mythos-org",
  "state": "blocked",
  "priority": "routine",
  "tier": "deep",
  "tags": [
    "mission:gate-integrity",
    "slice:faceplate-draft-vs-committed",
    "follow-up",
    "design-decision",
    "deferred",
    "source:pod_hmi-builder"
  ],
  "blockedOn": "qitem-20261023085422-f459118d847f001d",
  "handedOffTo": null,
  "handedOffFrom": null,
  "expiresAt": null,
  "chainOfRecord": null,
  "body": "DEFERRED BY DESIGN — the remaining mount-snapshot shape needs a draft-vs-committed model, not another re-sync.\n\npod_hmi.builder fixed the two residues PR #34 was chartered for (run-state colour bound to the raw prop; control mode snapshotted at mount) and deliberately did NOT bolt on the same shape where it still exists: MotorFaceplate field `inputSpeed`, and the PID panel fields `targetSp`, `targetCv`, `tuneKp`, `tuneTn`, `tuneTv`.\n\nWhy it is NOT the same fix and should not ride along: those are OPERATOR-WRITABLE setpoints. Re-syncing them from telemetry every render — the correct fix for a read-only badge — would fight the operator. It would overwrite an edit in flight whenever a telemetry frame lands. The correct model is the draft-vs-committed split that PR #27 establishes for the inspector: the operator's in-progress edit is a draft, telemetry updates the committed value, and the two reconcile at an explicit boundary (commit, cancel, or a defined staleness rule).\n\nSo this is a design decision about operator intent, not a bug fix: what happens to an uncommitted edit when a telemetry frame arrives underneath it? That question needs to be decided and written down before it is coded, or the answer will be whatever the first implementation happens to do.\n\nREACHABILITY: latent today — the sole call sites pass NOT_BOUND and no telemetry populates the props. It becomes live with the telemetry transport, which the advisor has made the first item of the next sprint. It is latent, not fixed: do not read this row's deferral as a claim that the surface is correct.\n\nSCOPE NOTE — do NOT fold this into the telemetry slice. That slice already carries three preconditions: canonical DeviceExecutionMode ownership, the comm-loss latch reaching shared memory, and quality surviving the wire (see qitem-20261026045420-1c343f5dcef6a274). Adding an operator-intent design decision would make it four and would bury it.\n\nWHEN STARTED: decide and record the reconcile rule FIRST, then implement it for the two surfaces. Prove the edit-in-flight case explicitly — an edit made before a telemetry frame must either survive that frame or be discarded with a visible reason. A test that only covers the no-telemetry case would pass against a broken implementation, so it would prove nothing.\n",
  "summary": "The mount-snapshot shape still exists in MotorFaceplate inputSpeed and the PID panel's targetSp/targetCv/tuneKp/tuneTn/tuneTv, but those are operator-writable setpoints, so re-syncing from telemetry would overwrite an edit in flight. Needs a draft-vs-committed rule decided before coding. Deferred, not fixed.",
  "evidenceRef": null,
  "humanIntent": null,
  "humanDetail": null,
  "replyTo": null,
  "humanQuestions": null,
  "humanAnswers": null,
  "closureReason": null,
  "closureTarget": null,
  "closureRequiredAt": null,
  "claimedAt": null,
  "lastNudgeAttempt": "2026-10-26T06:33:01.638Z",
  "lastNudgeResult": "delivered-ack-pending",
  "lastNudgeWireResult": "delivered-ack-pending",
  "lastHeartbeat": null,
  "resolution": null,
  "targetRepo": null,
  "deliveryOutcome": null,
  "destinationClass": "pane-bound",
  "ownerNotificationLevel": null
}

############ qitem-20261023085422-f459118d847f001d ############
{
  "pickup": {
    "state": "parked"
  },
  "waiting": {
    "obligation": "qitem-20261023085422-f459118d847f001d",
    "owner": "pod_uiux-builder@mythos-org",
    "state": "blocked",
    "actionableSince": null,
    "blocker": {
      "ref": "external:sprint-8-open",
      "owner": null,
      "state": null
    },
    "lastMeaningfulChange": {
      "id": 1012,
      "at": "2026-10-27T11:40:20.958Z"
    },
    "liveness": {
      "subject": "external:sprint-8-open",
      "activity": "unknown",
      "needsInput": {
        "count": 0,
        "reason": null
      },
      "confidence": "unknown"
    },
    "nextBackstop": {
      "owner": "pod_uiux-builder@mythos-org",
      "mechanism": "UNVERIFIED: no timed backstop",
      "dueAt": null,
      "intervalSeconds": null
    },
    "deadlineAt": null
  },
  "qitemId": "qitem-20261023085422-f459118d847f001d",
  "tsCreated": "2026-10-23T08:54:22.085Z",
  "tsUpdated": "2026-10-27T11:40:20.956Z",
  "sourceSession": "governance-pm@mythos-org",
  "destinationSession": "pod_uiux-builder@mythos-org",
  "state": "blocked",
  "priority": "urgent",
  "tier": "deep",
  "tags": [
    "mission:gate-integrity",
    "slice:hmi-live-transport",
    "needs-scoping-decision"
  ],
  "blockedOn": "external:sprint-8-open",
  "handedOffTo": null,
  "handedOffFrom": null,
  "expiresAt": null,
  "chainOfRecord": null,
  "body": "GAP — the HMI canvas has no live-telemetry path. Every faceplate will read NOT BOUND on a real\nplant. This is a product-scope finding, not a bug in PR #22.\n\nHOW THIS SURFACED\nPR #22 (fix/hmi-no-data-state) made absence-of-data honest: an unbound component now renders\n`----` plus an explicit NOT BOUND / BAD COMM FAILURE indication instead of a fabricated value.\nThat is correct and is exactly SYS-REQ-096. While verifying it I checked where a faceplate's live\nvalues would ever come from, and found nothing.\n\nEVIDENCE (dev 785601f + PR #22 head c0888ee)\n- ui-shell has no telemetry consumer. Repo-wide grep for `EventSource` / `telemetry/stream` over\n  frontend/packages/ui-shell/src and frontend/plugins/*/src (excluding dist/node_modules) returns\n  exactly one hit, in frontend/plugins/ui-logic-editor/src/debug/OnlineDebugEngine.ts — the logic\n  editor's debug overlay, not the HMI canvas.\n- The canvas resolves data via resolveComponentData(comp) (DockviewWorkspace.tsx), which reads\n  live values ONLY from comp.live / comp.telemetry / comp.props.live. Nothing populates those\n  fields from the running TagBus.\n- The backend stream exists and works: GET /api/v1/telemetry/stream, 100 ms batching, TSK-008.\n  So the producer side is built. The consumer side was never built.\n- specs/tasks.md has no task that covers it. TSK-008 is the backend SSE stream; TSK-705 is the\n  logic-editor debug overlay. No task anywhere says \"bind live TagBus telemetry into HMI\n  faceplates\". Grep for SSE/telemetry in specs/tasks.md confirms this.\n\nWHAT THIS MEANS, STATED PLAINLY\nAfter PR #22 the HMI tells the truth: it shows nothing rather than a lie. But on a commissioned\nplant it will also show nothing at all, on every screen, forever, until a transport slice lands.\nAn HMI that cannot display a live process value does not meet its stated purpose\n(SYS-REQ-050..057), and no gate in the roadmap checks for it — because no task ever claimed it.\n\nDECISION NEEDED (advisor / board, not PM)\nIs this Sprint 8 scope, or a new milestone item? Two defensible readings:\n  A. Safety-first: ship PR #22 as-is; honest-and-blind beats plausible-and-false every time, and\n     the transport is a proper feature slice with its own design questions (subscription model,\n     tag-to-component binding resolution, 10 Hz decimation, reconnect/backoff, quality mapping\n     from TagBus QualityCode into the faceplate quality enum).\n  B. Completeness: the HMI is not demonstrably working until one real tag reaches one faceplate,\n     so the minimal transport should land before any HMI gate is called passed.\nPM recommends A for the gate ordering and B for the next slice: close #22 now, and make the\nminimal end-to-end transport the FIRST item of the next sprint, explicitly, so it is not\ndiscovered again later.\n\nWHAT TO BUILD (when it is scoped)\n- A telemetry client in ui-shell that subscribes to /api/v1/telemetry/stream (10 Hz, already\n  batched server-side) with reconnect/backoff.\n- A store keyed by tag URI mapping quality + values into the shape resolveComponentData already\n  accepts (comp.live). Do NOT change resolveComponentData's contract — it is correct.\n- Quality mapping: TagBus QualityCode (GOOD / BAD_COMM_FAILURE / etc.) must map onto the\n  faceplate's GOOD / BAD_QUALITY / NOT_BOUND. A tag subscribed to but not reporting is\n  BAD_QUALITY, not NOT_BOUND; only a component with no binding at all is NOT_BOUND.\n- Decimation/backpressure: 10 Hz is the locked contract; do not raise it in the browser.\n\nREQUIRED PROOF\n- One integration test: a tag published to the TagBus reaches a rendered faceplate with GOOD\n  quality and the expected value, end to end. This is the test that proves the HMI works at all.\n- Negative: drop the stream mid-render and assert the faceplate degrades to BAD_QUALITY, never to\n  a stale value presented as live, and never to a fabricated one (SYS-REQ-096).\n- No mock data anywhere on the path — the same standard as PR #22.\n\nCONSTRAINTS\n- Do not modify resolveComponentData or the faceplate quality contract (PR #22 owns those).\n- Do not edit specs/ (controlled; amendment pending board approval).\n- Do not edit DockviewWorkspace.tsx tab routing.\n",
  "summary": "GAP: HMI canvas has no live-telemetry path — no task in the roadmap covers it; every faceplate reads NOT BOUND on a real plant. Needs scope decision, then minimal end-to-end transport.",
  "evidenceRef": null,
  "humanIntent": null,
  "humanDetail": null,
  "replyTo": null,
  "humanQuestions": null,
  "humanAnswers": null,
  "closureReason": null,
  "closureTarget": null,
  "closureRequiredAt": null,
  "claimedAt": null,
  "lastNudgeAttempt": "2026-10-27T06:48:01.848Z",
  "lastNudgeResult": "delivered-ack-pending",
  "lastNudgeWireResult": "delivered-ack-pending",
  "lastHeartbeat": null,
  "resolution": null,
  "targetRepo": null,
  "deliveryOutcome": null,
  "destinationClass": "pane-bound",
  "ownerNotificationLevel": null
}

############ qitem-20261027083546-d936928f745a704d ############
{
  "pickup": {
    "state": "parked"
  },
  "waiting": {
    "obligation": "qitem-20261027083546-d936928f745a704d",
    "owner": "pod_backend-publisher@mythos-org",
    "state": "blocked",
    "actionableSince": null,
    "blocker": {
      "ref": "external:release-path-produces-more-than-one-version",
      "owner": null,
      "state": null
    },
    "lastMeaningfulChange": {
      "id": 1039,
      "at": "2026-10-27T12:46:46.785Z"
    },
    "liveness": {
      "subject": "external:release-path-produces-more-than-one-version",
      "activity": "unknown",
      "needsInput": {
        "count": 0,
        "reason": null
      },
      "confidence": "unknown"
    },
    "nextBackstop": {
      "owner": "pod_backend-publisher@mythos-org",
      "mechanism": "UNVERIFIED: no timed backstop",
      "dueAt": null,
      "intervalSeconds": null
    },
    "deadlineAt": null
  },
  "qitemId": "qitem-20261027083546-d936928f745a704d",
  "tsCreated": "2026-10-27T08:35:46.099Z",
  "tsUpdated": "2026-10-27T12:46:46.784Z",
  "sourceSession": "governance-pm@mythos-org",
  "destinationSession": "pod_backend-publisher@mythos-org",
  "state": "blocked",
  "priority": "routine",
  "tier": "deep",
  "tags": [
    "mission:docs-governance",
    "slice:version-update-feed",
    "roadmap",
    "slice-b",
    "versioning",
    "blocked-on-release"
  ],
  "blockedOn": "external:release-path-produces-more-than-one-version",
  "handedOffTo": null,
  "handedOffFrom": null,
  "expiresAt": null,
  "chainOfRecord": null,
  "body": "SLICE B — the update feed half: `--version <semver>` pinning, `mythos versions --remote`, `mythos update --version`.\n\nBLOCKED-ON-RELEASE, not merely sequenced. The advisor stated this explicitly: only the first dev tag is authorised, so there is nothing to update FROM yet. Building these against no feed would produce a second unimplementable promise stacked on the first — which is exactly the condition this whole thread exists to remove.\n\n=== SCOPE (one slice, per the advisor's decomposition) ===\nAll three are \"read a feed and select a version\", so they share a mechanism and belong together:\n1. `--version <semver>` PINNING.\n2. `mythos versions --remote` — enumerate what is available.\n3. `mythos update --version` — act on the selection.\n\n=== WHY IT IS BLOCKED RATHER THAN SEQUENCED ===\nSequenced would mean \"not now but the dependency is soft\". This one is hard: with only `v0.1.0-dev.0` authorised, an update mechanism has exactly one version to choose from and no channel to choose within. `versions --remote` would enumerate a list of one. The mechanism cannot be exercised, so it cannot be proven, so it would ship as a stub — which the decision forbids.\n\n=== WHAT UNBLOCKS IT ===\nNot \"a release\" alone but a RELEASE PATH: the promotion flow (dev → rc → release) actually producing more than one version, plus Adrian's authorisation for rc and release tags (currently explicitly NOT authorised). When that exists, this slice becomes buildable and its acceptance test becomes meaningful: pin an older version, list remotes, and update to a named one.\n\n=== SPRINT PLACEMENT (PM decision) ===\nAfter the promotion path produces at least two versions. Not assigned to a numbered sprint yet, deliberately: assigning it would imply a schedule I cannot honour while releases are gated on an authorisation that has not been given. It is parked with a real blocker rather than a date, so it does not look imminent to anyone reading the board.\n\n=== GUARDRAIL ===\nSame as Slice A: no stubs, and if a piece turns out disproportionate, reopen the spec line with the board rather than satisfying the words. The advisor's rule applies to both halves.\n\n=== RELATIONSHIP TO SLICE A ===\nSlice A (channel selection + version identity) is buildable now-ish and does not depend on this. B consumes what releases produce; A describes which channel you are on. They meet only once a feed exists.\n",
  "summary": "Slice B work item (the scoping row was closed by the advisor as its decomposition answer): --version pinning, 'versions --remote', 'update --version'. BLOCKED on the release path producing more than one version. No stubs; unblocks after rc/release authorisation.",
  "evidenceRef": null,
  "humanIntent": null,
  "humanDetail": null,
  "replyTo": null,
  "humanQuestions": null,
  "humanAnswers": null,
  "closureReason": null,
  "closureTarget": null,
  "closureRequiredAt": null,
  "claimedAt": null,
  "lastNudgeAttempt": "2026-10-27T09:25:46.954Z",
  "lastNudgeResult": "delivered-ack-pending",
  "lastNudgeWireResult": "delivered-ack-pending",
  "lastHeartbeat": null,
  "resolution": null,
  "targetRepo": null,
  "deliveryOutcome": null,
  "destinationClass": "pane-bound",
  "ownerNotificationLevel": null
}

