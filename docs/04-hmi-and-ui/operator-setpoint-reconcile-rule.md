# Operator Setpoint Reconcile Rule (draft vs committed)

**Status:** PARTIALLY DECIDED. The parts that depend on the live-telemetry slice's scoping
decision are marked **OPEN** and are deliberately not guessed. See §5.
**Date:** 2026-10-26
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

**R4 — An episode closes at an explicit boundary, or on echo — whichever comes first.**
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
a re-key destroys the surface and the draft with it, for a reason the operator did not choose. See
§5/O6 — this is treated as open rather than waved through, because the honest answer may be that a
draft needs to outlive the component or that a re-key must be forbidden while an episode is open.

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

## 5. OPEN — depends on the live-telemetry slice's scoping decision

These are marked open rather than answered, because answering them here would pre-empt a decision
that belongs to the transport slice (`qitem-20261023085422-f459118d847f001d`, `slice:hmi-live-transport`,
currently blocked with `needs-scoping-decision`).

- **O1 — Wire-level staleness.** Does a frame carry a sequence and/or an age, and is there a frame
  clock? Without it, "the value moved underneath the edit" cannot be distinguished from "no frame
  has arrived", and R6's divergence display has no bounded meaning.
- **O2 — Echo versus independent change.** Does a write return an acknowledgement or a sequence that
  identifies the resulting frame? Value equality is a weaker test: a device can legitimately land on
  the same value independently, so equality alone would misclassify a real change as an echo (and
  silently close an episode). R4's "or on echo" depends on this being answerable.
- **O3 — Central staleness resolution.** If the transport resolves staleness once, centrally (a
  frame for one tag implying others are stale), then per-field staleness rules here are redundant
  and should defer to it rather than coexist with it. Which layer owns the rule is undecided.
- **O4 — Staleness threshold policy.** If a draft is to be abandoned on drift, by what measure?
  A percentage of range is meaningless for `kp`/`tn`/`tv`. This is a plant-policy question as much
  as a technical one.
- **O5 — Do the tuning parameters ever come back from the device?** `kp`/`tn`/`tv` are stored
  configuration, not process values, and the device may never report them. If it does not, R6's
  "show the committed value beside the draft" is **not available** for those three fields, and they
  need a different honest treatment: the draft labelled uncommitted, with the device's value stated
  as unknown rather than seeded at mount. This distinction must not be flattened into the same rule
  as `inputSpeed`, which is a live process value the device does report.
- **O6 — Draft lifetime across a component re-key.** See R9. Either a draft survives a re-key, or a
  re-key while an episode is open is prevented, or a silent discard is declared acceptable and
  written down as a known limitation. Not decided here.

## 6. Test obligations when this is implemented

- The **edit-in-flight case must be proven explicitly**: open an episode, deliver a telemetry frame
  with a different committed value, and assert both that the draft survives and that the divergence
  is visible. A test that only renders with no telemetry — or only without an open draft — **passes
  against a broken implementation** and proves nothing. This is the same standard as PR #27's
  mounted-instance re-render and PR #34's class-level colour assertions.
- R7 needs its own case: quality leaves `GOOD` mid-episode, the draft is discarded, and the reason
  is on screen.
- Whatever O5 resolves to needs a case where the device reports nothing for the field at all.

## 7. Observation outside this rule (found while writing it, not fixed)

`PidControllerFaceplate` computes
`spSliderMax = spMax ?? (displayable && targetSp !== null ? Math.max(100, targetSp) : 100)`, which
invents a 100-unit setpoint span when the device reports none. That is the same fabrication the
Inspector drawer's setpoint slider was corrected for in PR #27 (invented `0..100`), on a different
surface. Nothing in this record depends on it, and it is not fixed here.
