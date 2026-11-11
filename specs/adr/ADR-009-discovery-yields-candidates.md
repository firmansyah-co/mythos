# ADR-009: Discovery Yields Candidates; Commissioning Is an Explicit Act

- **Status**: PROPOSED
- **Date**: 2026-11-11
- **Author**: Scribe (VP Publication)
- **Deciders (proposed — to be recorded at acceptance)**: Adrian (Board of Commissioners), Aria (CEO), Neo (VP Software)
- **Consulted**: Iris (VP Design), governance-pm

---

## 1. Context and Problem Statement

The Devices & Networks workspace shows an operator the physical plant. A device drawn on that
canvas is a claim: this equipment exists, at this address, wired this way. An operator cannot
tell a discovered device from a commissioned one by looking at it.

PR #33 (`fix(ui): stop workbench surfaces fabricating authored content`, merged 2026-10-27)
removed a fabricated station from the Network View render path: a `PLC_RACK_01` at
`192.168.1.10` carrying Siemens article numbers, wired in as `DEFAULT_STATIONS` /
`DEFAULT_CABLES` parameter defaults so that a demonstration would have a node to draw. It was
presented as the operator's own commissioned plant.

PR #50 (**open, not merged at the time of writing**) proposes to replace those defaults with
explicit provenance states and to state the rule in the render path: *never auto-create a node
from discovery; discovery yields candidates, commissioning is an explicit act.* At the time of
writing, `origin/dev` contains neither that module nor any render-path guard for the rule, so
nothing in the merged tree currently enforces it. That gap is part of why this ADR exists: the
decision should not depend on one open pull request landing.

That rule is, as of this writing, recorded only in the source comments of PR #50's branch
(`ui-topology/src/NetworkCanvas.tsx`, `ui-topology/src/provenance.tsx`). It appears nowhere in
the merged tree and nowhere in a controlled document. It has a live alternative that a
controlled document still appears to permit:
`docs/04-hmi-and-ui/network-topology-view-spec.md` §4.3 describes a neighbouring switch or
controller that "automatically assigns the configured PROFINET device name, and downloads its
hardware parameters" on discovery. Read without its precondition, that sentence licenses the
class of defect PR #33 removed.

This ADR exists so the decision outlives the comments that carry it, and so the alternative is
rejected in writing rather than by implication.

## 2. Decision Drivers

- **Operator cannot distinguish fabricated from measured.** The governing invariant is
  SYS-REQ-096 (`specs/spec.md:337`): under no circumstances shall the system generate synthetic
  values or silently substitute for a missing physical interface.
- **Discovery is a real, valuable capability.** ADR-004 establishes Layer 2 DCP discovery
  precisely so a commissioning engineer can see what is physically connected. This decision
  must not weaken it.
- **Replacement without a programming laptop is a legitimate workflow.** The spec's Automated
  Device Replacement description is correct *when the nominal topology already defines the slot*.
- **Silence is a claim.** A topology surface that renders nothing has still told the operator
  something. Provenance must be stated, not implied.

## 3. Considered Options

### Option A — Auto-provision on discovery (the unqualified reading of spec §4.3)

A device answering DCP/LLDP is added to the plant model automatically, with the configured name
and parameters applied.

**Rejected.** It makes the plant model an artifact of whatever is on the wire at that moment. An
operator cannot distinguish plant equipment from a candidate, and a device that is present but
should not be (a commissioning laptop, a contractor's switch, an attacker's device on the OT
VLAN) becomes plant equipment by the act of being plugged in. It defeats the intent of
SYS-REQ-096 at the model layer: a configured entity that no act created.

### Option B — Merge a discovered device when MAC or serial matches

A discovered device whose identity matches a known device is treated as that device.

**Rejected as a default.** Identity match is a heuristic, not an authorization. A replacement
unit would silently inherit another unit's parameters, and the operator would have no record of
the substitution. The legitimate form of this behaviour is the Automated Device Replacement
workflow in spec §4.3, which is sound only because the nominal slot already names the device and
its parameters — the precondition this ADR and the accompanying spec change make explicit.

### Option C — Discovery is read-only; the plant model is hand-entered only

The system never uses discovery to inform the plant model at all.

**Rejected.** This discards the value ADR-004 and NFR-003 exist to provide (discovery responses
from all active physical devices within 2,500 ms), and makes commissioning a manual transcription
exercise that invites the very errors discovery prevents.

### Option D — Discovery yields candidates; commissioning is an explicit act (adopted)

Discovery produces candidate devices. A candidate enters the plant model only when an authorized
operator commissions it. Commissioning supplies identity and configuration from the nominal
topology. Discovery never writes the plant model.

## 4. Decision Outcome

**Option D is adopted.**

1. **Discovery never writes the plant model.** No discovery response causes the creation of a
   station, an address, or a cable in the plant model.
2. **A discovered device is a candidate.** It is presented as such, distinctly from configured
   plant equipment, and it is never presented as commissioned.
3. **Commissioning is an explicit, authorized act.** It records operator identity, timestamp and
   target, consistent with the audit-trail requirement in SYS-REQ-096 (`specs/spec.md:346`).
4. **Every topology surface states its provenance, and derives it — it is never defaulted.**
   A surface makes a claim by existing; the claim is stated rather than implied. Stating a
   provenance that the rendered data contradicts is the same defect as stating none: a banner
   reading "nothing has been read, so this view makes no claim about the plant" above a canvas
   that is drawing a station is a false statement, not a cautious one. Where a surface has data
   it must have a provenance for that data; a caller that supplies stations without saying where
   they came from is an error, not a default. The transport is the component most likely to
   supply data and forget the provenance, which is why this is decided before it lands.
5. **Source and health are separate axes.** Where a claim came from (nominal configuration, or
   discovery) and whether a configured device is answering (reachable, unreachable, degraded) are
   distinct properties and shall not be collapsed into one enumerated value.

## 5. Consequences

### 5.1 Positive Consequences

- An operator can distinguish a commissioned device from a candidate, and can tell "nothing was
  read" from "configured but not answering".
- Provenance cannot be satisfied by a default, so a populated surface cannot announce that it
  knows nothing.
- The decision is recorded in a controlled document, so a future slice cannot reintroduce
  auto-provisioning by reading one sentence of the spec in isolation.
- The replacement workflow keeps its value without licensing node creation.

### 5.2 Negative Consequences

- Commissioning is a manual step, and a plant brought up from discovery alone will show candidates
  rather than equipment until an operator commissions them. This is the intended cost.
- Topology surfaces must carry and render provenance, which is additional state on a contract
  that did not previously have it.

## 6. References

- `docs/04-hmi-and-ui/network-topology-view-spec.md` §4.3 (Automated Device Replacement), §8
  (TypeScript Interface Contracts)
- `specs/adr/ADR-004-layer2-dcp-discovery-topology.md` (Layer 2 DCP discovery)
- `specs/spec.md` SYS-REQ-096 (zero silent fallback), NFR-003 (discovery timing)
- **Merged:** PR #33 (`fix(ui): stop workbench surfaces fabricating authored content`,
  2026-10-27) — removed the fabricated stations from the Network View render path.
- **Proposed, not merged at the time of writing:** PR #50 — provenance states and a
  render-path guard for the decision in §4. Referenced here as intent, not as shipped
  enforcement. `origin/dev` contains neither `ui-topology/src/provenance.tsx` nor any
  render-path guard for this rule.
