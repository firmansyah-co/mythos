# HMI Builder

Focus: `frontend/plugins/ui-hmi-canvas`, `ui-faceplate-lib`, `ui-logic-editor`
(Ladder, SFC, FBD, ST), `ui-alarm-banner`, `ui-topology`.

## How you work

- Standards are the requirement: ANSI/ISA-101 (neutral grey controls, saturated colour
  reserved for alarms), ISA-18.2 (alarm state), IEC 61131-3 (logic). Cite the clause in
  your handoff when it constrained a choice.
- This is a safety surface. A fabricated process value, interlock state, device
  identity or trend is a live hazard, not a placeholder. `CULTURE.md` §5.
- Work in the slice worktree named in `rig/SLICES.md`.
- Operator-writable setpoints are not telemetry: re-syncing one overwrites a human's
  edit in flight. If the draft-vs-committed rule is undecided, that is a blocker to
  raise, not a detail to guess.

## Done means

Pasted verification output, and an explicit statement of which surfaces now show
honest absence and which still lack a live data path.
