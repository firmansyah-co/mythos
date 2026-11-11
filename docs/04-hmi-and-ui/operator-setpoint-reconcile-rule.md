# Operator Setpoint Reconcile Rule (draft vs committed)

**Status:** DECIDED. §5 was rewritten from OPEN to DECIDED on 2026-11-11 after the live-telemetry
slice (S-004) landed; every question it carried is now answered or explicitly struck. See §5 for
each ruling and the evidence behind it.
**Date:** 2026-10-26 (decisions recorded 2026-11-11)
**Surfaces in scope:** `MotorFaceplate.inputSpeed`; `PidControllerFaceplate.targetSp`,
`.targetCv`, `.tuneKp`, `.tuneTn`, `.tuneTv`.
**Related:** qitem-20261026053023-3c4197fd2154e695 (the row this answers); SYS-REQ-096;
the Inspector drawer's draft handling (PR #27); the faceplate provenance work (PR #34).
**Non-goals:** this record contains no implementation, does not change the `quality` gate, and
does not decide anything about the transport.

---

## 1. The question

*What happens to an uncommitted operator edit when a telemetry frame arrives underneath it?*

The surfaces above each hold an operator-writable value in `useState`, seeded once from a prop
(`useState(speedSetpoint ?? 0)`, `useState(sp ?? null)`, …) and never reconciled afterwards. That
shape is wrong in two directions at once, and only one of them looks like the defects we have
already fixed:

1. **Stale-as-current.** If the operator has not touched the field, telemetry that arrives after
   mount never reaches it. The field keeps showing the mount-time value as though it were the
   current device value. This is the same defect as the Inspector drawer's snapshot (PR #27) and
   the mode badge (PR #34) — fixed there by reading the prop every render.
2. **Silent divergence.** Once the operator *has* edited, the edit is what stays on screen
   regardless of what the device reports. The operator may then commit against a basis that has
   already moved, with nothing on screen saying so.

The fix for (1) — read from the prop every render — is exactly wrong for (2), because it would
overwrite an edit in flight. A rule is needed that keeps both honest; that is what this record
decides.

## 2. Vocabulary

| Term | Meaning |
|---|---|
| **committed** | The value the device is reporting, read from the prop under the existing `quality` gate on every render. Not held in state. |
| **draft** | A value the operator is editing or has edited but which no telemetry has confirmed. Lives in component state. |
| **episode** | The lifetime of one draft: from the first operator input on a field to its close. Not the same as focus, and not the same as a gesture. |
| **basis** | The committed value that was on screen when the episode opened. Used only to describe drift to the operator; never used to silently resolve anything. |
| **echo** | A telemetry frame whose value reflects the operator's own command. Semantically different from an independent change — see §5/O2. |

## 3. The rule

**R1 — Two values, two provenances.** A field has at most one committed value (from props,
honesty-gated as today) and at most one draft (from state). They are never merged into a single
piece of state: merging is how a draft becomes indistinguishable from a measurement.

**R2 — A draft exists only inside an episode.** No episode, no draft: the field shows the committed
value, or the existing `----` no-data state. A field that has never been touched must not hold a
copy of anything.

**R3 — An episode opens on the first operator input**, not on focus. A focused but unmodified
field has no draft.

**R4 — An episode closes at an explicit boundary.** There is no echo path: §5/O2 strikes it,
because the transport carries no write acknowledgement and no sequence, so value equality cannot
distinguish the operator's own command returning from an independent change that happens to land on
the same value. Reconciling on equality would silently close an episode on a coincidence.
An "explicit boundary" is an affordance the operator performed deliberately:

- Sliders (`inputSpeed`, `targetSp`, `targetCv`): these stream a command on every `onChange` today.
  The boundary is **end of gesture** — pointer release, key release, blur. The draft is not required
  to persist after the operator stops moving the control.
- Tuning trio (`tuneKp`, `tuneTn`, `tuneTv`): these already have an explicit boundary, the
  **Apply Tuning** button, which calls `onTuneUpdate(kp, tn, tv)`. The episode closes on Apply,
  and also on explicit discard (Escape / abandon), not on every keystroke.

**R5 — A telemetry frame never overwrites an open draft.** No silent overwrite in either
direction: the frame does not replace the draft, and the draft does not suppress the frame's
effect on the committed value. Both are shown (§R6).

**R6 — An open draft is always visibly labelled as uncommitted.** Where the device reports a
comparable value, the field shows the draft *and* the current committed value, so the operator can
see that the device moved underneath the edit and decide. A draft that looks identical to a
reading is the failure this rule exists to prevent.

**R7 — Leaving `GOOD` quality ends every episode and discards the draft, with the reason shown.**
Every one of these controls is disabled when the device is not reporting, so a retained draft would
be hidden state behind a disabled control. This part is **decided here** and does not depend on the
transport: the gate already exists, the rule only makes its consequence visible.

**R8 — Episode grouping follows the commit boundary.**
- The tuning trio is **one** episode: three fields, one Apply, one `onTuneUpdate`. Drafts open and
  close together.
- `targetSp`, `targetCv` and `inputSpeed` are each **their own** episode: separate controls,
  separate callbacks, no shared transaction implied.

**R9 — The draft lives in the component, not in the store.** It is ephemeral UI operator intent,
meaningless without the surface showing it, and must not survive in a form that could be read back
as device state. Consequence: **unmount discards open drafts silently.** Accepted for a genuine
navigation unmount, which the operator caused. The case that is *not* obviously covered is the
canvas re-keying a faceplate on a changed component id (`DockviewWorkspace` keys on `compId`):
a re-key destroys the surface and the draft with it, for a reason the operator did not choose.
**§5/O6 settles it: a re-key discards the draft and shows a visible one-line notice.** The draft is
still not carried across the identity change — the notice is what makes the discard honest rather
than silent.

## 4. What this rule deliberately does not do

- It does not decide how long a draft may live, or when a stale draft must be abandoned rather
  than kept (§5/O4).
- It does not define what "the device changed independently" means at the wire (§5/O1, O2).
- It does not claim that a closed episode implies the device accepted the command. Applying a
  value is a request; the field returns to reporting telemetry immediately, which may differ from
  what was commanded. That is the same "request, not a state change" model as the mode toggle in
  PR #27 and the mode selector in PR #34.
- It does not seed a field from a prop. Under R2, a field with no telemetry and no episode shows
  the no-data state; the current mount-time seed is exactly defect (1) and is removed rather than
  preserved.

## 5. DECIDED — the transport questions this rule was waiting on

These were OPEN until the live-telemetry slice (S-004) landed. They are answered here against the
transport as it was built, not against an assumption about it. Each ruling states what it buys and
what it does **not**, because the second half is where this rule can go wrong quietly.

- **O1 — Wire-level staleness → ANSWERED: consumer-observed arrival age.**

  The question asked whether an age is computable. It is, but not from the device clock. The wire
  carries `TagValueUpdate.timestamp_ns` (the **slot writer's** clock, `telemetry.rs:63`) and
  `TelemetryBatch.timestamp_ms` (the **server's** clock). A subtraction between a device stamp and a
  browser's clock is only as trustworthy as their agreement, and nothing in this repository
  establishes that they are synchronised.

  **Ruling: the rule uses CONSUMER-OBSERVED ARRIVAL AGE.** The consumer stamps `performance.now()`
  when it applies a batch. That is one clock on one side, so "how long has it been since I heard
  anything about this tag" is exactly computable with no synchronisation assumption.

  What this buys: it distinguishes *no frame has arrived* (a real staleness signal) from *the value
  moved underneath the edit* (which needs the value, and we have it).

  What it does **not** buy, and what must not be claimed anywhere in the UI:
  - Nothing may display a **device-side age**. The arrival clock does not observe when the device
    wrote the value.
  - The wire carries **no sequence number**, so a dropped or reordered frame is undetectable. The
    arrival clock measures **silence, not loss**. Any wording that implies "we know we lost a frame"
    is false.

- **O2 — Echo versus independent change → STRUCK, not deferred.**

  No write acknowledgement and no sequence crosses this stream. Value equality is the only available
  signal, and a device can legitimately land on the same value independently. An equality test would
  therefore **silently close an episode on a coincidence** — the worst kind of wrong, because it
  looks exactly like the rule working.

  **Ruling: "or on echo" is struck from R4.** The rule reconciles on **commit**, **cancel**, or
  **observed drift beyond a stated bound**. If a future transport adds an acknowledgement or a
  sequence, the clause returns **with that transport** and is tested against it — it is not
  resurrected here as an assumption.

- **O3 — Central staleness resolution → ANSWERED by the implementation: the transport owns it.**

  The server enumerates the whole tag catalog on every 100 ms tick and emits an update only for
  slots it could actually read, so an absent tag means "could not read", never "unchanged". The
  consumer's cache consequently replaces per batch rather than merging.

  **Ruling: staleness is resolved centrally. This rule DEFERS to the transport and says so, rather
  than restating it per field.** Per-field staleness rules would duplicate the transport's answer
  and could disagree with it.

- **O4 — Drift threshold policy → OUT OF SCOPE of the rule; it is REQUIRED configuration.**

  A percentage of range is meaningless for `kp`/`tn`/`tv`, and a single plant-wide number is not
  something this document can choose.

  **Ruling: the drift threshold is a named, REQUIRED configuration value with NO DEFAULT in code.**
  When it is unconfigured, the surface renders `NOT CONFIGURED` and **drift detection is inactive**.
  It must never be seeded with a plausible-looking number: a carried default here is the
  fabricated-bound defect in a new costume, and an operator cannot tell a chosen bound from a
  measured one.

- **O5 — Do the tuning parameters come back from the device? → PER-FIELD, PER-PLANT, at runtime.**

  `kp` **is** in `PROCESS_KEYS` (`componentData.ts:59`), so it *can* arrive as a live value — but
  whether a given plant's catalog actually carries it is not knowable statically. `inputSpeed` is a
  live process value the device reports; `kp`/`tn`/`tv` are stored configuration the device may
  never report. **Conflating the two is the defect this question exists to prevent.**

  **Ruling: each field is treated as reported-or-not at runtime.** Where the device does not report
  the field, the tuning field shows the draft labelled uncommitted with the device value stated as
  **UNKNOWN** — never seeded at mount, and never presented as though the device had confirmed it.

- **O6 — Draft lifetime across a component re-key → DECIDED: discard, visibly.**

  Three options were open: survive the re-key, prevent the re-key, or declare a silent discard
  acceptable. Ruled against the first two and against silence:
  - Surviving a re-key means carrying a draft across an identity change, so the draft would apply to
    a **different device** than the one it was typed against.
  - Preventing the re-key is a UI lock on a navigation control: scope creep, and a new failure mode.
  - Silent discard is exactly what this rule exists to prevent.

  **Ruling: a re-key discards the draft and says so in a visible one-line notice** ("draft discarded
  when the faceplate changed device"). Visible discard is honest; silent discard is a fabricated
  success.

## 6. Test obligations when this is implemented

- The **edit-in-flight case must be proven explicitly**: open an episode, deliver a telemetry frame
  with a different committed value, and assert both that the draft survives and that the divergence
  is visible. A test that only renders with no telemetry — or only without an open draft — **passes
  against a broken implementation** and proves nothing. This is the same standard as PR #27's
  mounted-instance re-render and PR #34's class-level colour assertions.
- R7 needs its own case: quality leaves `GOOD` mid-episode, the draft is discarded, and the reason
  is on screen.
- **O5 resolved to per-field at runtime**, so this needs a case where the device reports nothing for
  the field at all: the draft is labelled uncommitted, the device value reads **UNKNOWN**, and
  nothing is seeded at mount.
- **O6 resolved to visible discard**, so this needs a case where the faceplate is re-keyed with an
  episode open: the draft is gone *and* the one-line notice is on screen. A test that only asserts
  the draft is gone would pass against the silent discard this ruling rejects.
- **O4 resolved to required configuration**, so this needs a case where no drift threshold is
  configured: the surface reads `NOT CONFIGURED` and drift detection is inactive. Asserting only
  that nothing crashed would pass against a fabricated default.

## 7. Observation outside this rule (found while writing it, not fixed)

`PidControllerFaceplate` computes
`spSliderMax = spMax ?? (displayable && targetSp !== null ? Math.max(100, targetSp) : 100)`, which
invents a 100-unit setpoint span when the device reports none. That is the same fabrication the
Inspector drawer's setpoint slider was corrected for in PR #27 (invented `0..100`), on a different
surface. Nothing in this record depends on it, and it is not fixed here.
