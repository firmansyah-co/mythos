/**
 * Topology provenance — what the network views are claiming, and on what basis.
 *
 * WHY THIS EXISTS, and why it is not just a blank. The three views (Network, Device, Topology)
 * previously rendered a hardcoded sample plant: a `PLC_RACK_01` at `192.168.1.10` with Siemens
 * article numbers, applied as parameter defaults, presented as the operator's own network.
 *
 * TWO AXES, NOT ONE ENUM. The spec already publishes them
 * (`docs/04-hmi-and-ui/network-topology-view-spec.md`), so this is not a new design:
 *
 *   AXIS A — SOURCE (:203, Nominal vs Actual Topology)
 *       UNREAD      nothing has been read
 *       CONFIGURED  nominal: engineered in MythOS
 *       DISCOVERED  actual: seen by LLDP/DCP
 *
 *   AXIS B — HEALTH (:300 `linkStatus`, :345 `health.status`)
 *       UNKNOWN / REACHABLE / UNREACHABLE / DEGRADED
 *
 * Two further facts are NOT input states, and are modelled as what they are:
 *
 *   COMMISSIONING  a property of a DISCOVERED device — a candidate, or commissioned. Not a fourth
 *                  source value: the source of both is discovery.
 *   CONFORMANCE    (:168, "All Links Match Nominal Design") DERIVED from nominal vs actual. It is
 *                  a function, `deriveConformance`, and cannot be passed in as a claim.
 *
 * WHY THE AXES ARE SEPARATE. A single four-value enum could not express a device that is both
 * discovered and currently silent: the only ways to report it were to drop that it is silent, or to
 * report it as UNREACHABLE, which was documented as "configured, but the device did not answer" and
 * so silently promoted an uncommissioned candidate to plant equipment. That is the defect class
 * this module exists to prevent. Here the product says both things and drops neither.
 *
 * The impossible pairing is a compile error rather than a comment: source UNREAD means nothing has
 * been read, so no measurement can exist for it.
 *
 * THE RULE THAT PREVENTS THE ORIGINAL DEFECT (advisor's third answer), stated here because this is
 * where the invented rack lived: NEVER auto-create a node from discovery. Discovery yields
 * CANDIDATES; commissioning is an explicit act. The phantom rack appeared because a node was
 * needed and one was invented — a default that "kept the demo working". Nothing in this module or
 * the canvas may synthesise a station, an address or a cable.
 *
 * AND THE BANNER MAY NOT OUTRUN THE SCREEN. UNREAD is a true statement about an empty canvas and a
 * false one about a populated canvas, so the canvas never defaults it: devices with no stated
 * source render NOT_STATED, which says exactly what is wrong instead of asserting that no claim is
 * being made while one visibly is.
 */

import React from 'react';

/** AXIS A — on what basis do we assert this device exists? */
export type TopologySource =
  /** Nothing has been read: no transport exists, so no claim can be made. */
  | 'UNREAD'
  /** Nominal topology: read from the project's configured topology model. */
  | 'CONFIGURED'
  /** Actual topology: seen by discovery (LLDP / DCP). */
  | 'DISCOVERED';

/** AXIS B — did it answer, and how well? A measurement, not project data. */
export type TopologyHealth =
  /** Not measured. */
  | 'UNKNOWN'
  /** Answered. */
  | 'REACHABLE'
  /** Measured, and it did not answer. Its state is unknown, not healthy. */
  | 'UNREACHABLE'
  /** It answered, but not cleanly. */
  | 'DEGRADED';

/**
 * A property of a DISCOVERED device, not a fourth source value.
 *
 * Both are discovered; the difference is whether a person accepted the device into the plant. A
 * candidate is not plant equipment, and must never read as though it were.
 */
export type TopologyCommissioning = 'CANDIDATE' | 'COMMISSIONED';

/** DERIVED from nominal vs actual. Never an input state — see `deriveConformance`. */
export type TopologyConformance = 'NOT_COMPARED' | 'MATCHES' | 'MISMATCH';

/**
 * What a view is claiming: the product of the two axes, with commissioning carried alongside
 * discovery.
 *
 * The union encodes the impossible pairing — nothing read, yet something measured — so a consumer
 * cannot construct a claim its own data does not support.
 */
export type TopologyClaim =
  | { readonly source: 'UNREAD'; readonly health: 'UNKNOWN' }
  | { readonly source: 'CONFIGURED'; readonly health: TopologyHealth }
  | {
      readonly source: 'DISCOVERED';
      readonly health: TopologyHealth;
      readonly commissioning: TopologyCommissioning;
    };

/** Nothing read and nothing measured — true for an empty canvas while no transport exists. */
export const TOPOLOGY_CLAIM_UNREAD: TopologyClaim = {
  source: 'UNREAD',
  health: 'UNKNOWN',
};

/**
 * DERIVED conformance: do the two topologies describe the same plant?
 *
 * A function, not a value, because conformance is a conclusion drawn from nominal and actual — not
 * a third thing an operator or a caller can assert. It answers NOT_COMPARED until both sides have
 * actually been read, which is the honest answer today: neither is readable.
 */
export function deriveConformance(
  nominal: readonly string[] | null | undefined,
  actual: readonly string[] | null | undefined
): TopologyConformance {
  if (!nominal || !actual) return 'NOT_COMPARED';
  const nominalIds = new Set(nominal);
  const actualIds = new Set(actual);
  if (nominalIds.size !== actualIds.size) return 'MISMATCH';
  for (const id of nominalIds) {
    if (!actualIds.has(id)) return 'MISMATCH';
  }
  return 'MATCHES';
}

/**
 * Tones are the ISA-18.2 semantic alarm tokens, not a local palette: this strip sits on the same
 * surface as the rest of the alarm system, so it must mean the same thing there. `--alarm-critical`
 * is #DC2626 in both theme blocks, and a hardcoded `rose-950` would not track it.
 */
export type ProvenanceTone = 'neutral' | 'info' | 'warning' | 'major' | 'alarm' | 'defect';

const TONE_CLASSES: Record<ProvenanceTone, string> = {
  neutral: 'border-[var(--alarm-normal)]',
  info: 'border-[var(--alarm-diagnostic)]',
  warning: 'border-[var(--alarm-minor)]',
  major: 'border-[var(--alarm-major)]',
  alarm: 'border-[var(--alarm-critical)]',
  /** A defect in the app, not a plant condition — dashed, so it cannot be mistaken for one. */
  defect: 'border-dashed border-[var(--alarm-minor)]',
};

export interface ProvenancePresentation {
  readonly label: string;
  readonly detail: string;
  readonly tone: ProvenanceTone;
  readonly testId: string;
}

/**
 * The source segment's presentation. Keyed by what the operator reads, not by the bare source
 * value, because a discovered device reads differently depending on whether it is plant equipment.
 */
export type SourceDisplayKey =
  | 'UNREAD'
  | 'CONFIGURED'
  | 'DISCOVERED_CANDIDATE'
  | 'DISCOVERED_COMMISSIONED'
  | 'NOT_STATED';

export const TOPOLOGY_SOURCE: Record<SourceDisplayKey, ProvenancePresentation> = {
  UNREAD: {
    label: 'Topology unknown',
    detail:
      'Nothing has been read from the project or from discovery, so these views make no claim about the plant.',
    tone: 'neutral',
    testId: 'topology-source-unread',
  },
  CONFIGURED: {
    label: 'Configured topology',
    detail: "Nominal: read from the project's configured topology model.",
    tone: 'info',
    testId: 'topology-source-configured',
  },
  DISCOVERED_CANDIDATE: {
    label: 'Discovered — not commissioned',
    detail:
      'Actual: found by discovery. A discovered device is a candidate, not plant equipment; it becomes plant equipment only when commissioned.',
    tone: 'warning',
    testId: 'topology-source-discovered-candidate',
  },
  DISCOVERED_COMMISSIONED: {
    label: 'Discovered — commissioned',
    detail: 'Actual: found by discovery, and accepted into the plant by an explicit commissioning act.',
    tone: 'info',
    testId: 'topology-source-discovered-commissioned',
  },
  /**
   * Devices are on screen and nobody said where they came from. This is not a claim about the
   * plant, it is a defect in the wiring — and it exists so that UNREAD can never be rendered over a
   * populated canvas, which would tell the operator that no claim is being made while one is.
   */
  NOT_STATED: {
    label: 'Provenance not stated',
    detail:
      'These views are drawing devices without saying where they came from. The topology below is not vouched for.',
    tone: 'defect',
    testId: 'topology-source-not-stated',
  },
};

/**
 * The health segment's presentation, for the states that have something to say beside the source.
 *
 * UNREACHABLE and DEGRADED contradict the healthy assumption, so they always get their own segment.
 * REACHABLE is a measured positive that changes nothing an operator may assume, and UNKNOWN is the
 * absence of a measurement; neither adds a segment, so neither becomes a second, quieter way of
 * saying "fine".
 */
export const TOPOLOGY_HEALTH: Partial<Record<TopologyHealth, ProvenancePresentation>> = {
  UNREACHABLE: {
    label: 'Device unreachable',
    detail: 'The device did not answer. Its state is unknown, not healthy.',
    tone: 'alarm',
    testId: 'topology-health-unreachable',
  },
  DEGRADED: {
    label: 'Device degraded',
    detail: 'The device answered, but not cleanly. Links or error counters are outside nominal.',
    tone: 'major',
    testId: 'topology-health-degraded',
  },
};

/**
 * The derived conformance claim, rendered as derived. It never carries an alarm tone: a mismatch is
 * a statement about the project's design versus the plant, which an operator investigates, not an
 * active alarm on the plant.
 */
export const TOPOLOGY_CONFORMANCE: Record<TopologyConformance, ProvenancePresentation> = {
  NOT_COMPARED: {
    label: 'Conformance not compared',
    detail: 'Only one topology can be read today, so nominal and actual have not been compared.',
    tone: 'neutral',
    testId: 'topology-conformance-not-compared',
  },
  MATCHES: {
    label: 'All links match nominal design',
    detail: 'The discovered topology matches the engineered one.',
    tone: 'neutral',
    testId: 'topology-conformance-matches',
  },
  MISMATCH: {
    label: 'Topology differs from nominal design',
    detail: 'The discovered topology does not match the engineered one. Investigate the discrepancy.',
    tone: 'warning',
    testId: 'topology-conformance-mismatch',
  },
};

function sourceKeyOf(claim: TopologyClaim): SourceDisplayKey {
  if (claim.source === 'UNREAD') return 'UNREAD';
  if (claim.source === 'CONFIGURED') return 'CONFIGURED';
  return claim.commissioning === 'COMMISSIONED'
    ? 'DISCOVERED_COMMISSIONED'
    : 'DISCOVERED_CANDIDATE';
}

const Segment: React.FC<{ presentation: ProvenancePresentation; derived?: boolean }> = ({
  presentation,
  derived = false,
}) => (
  <div
    data-testid={presentation.testId}
    data-tone={presentation.tone}
    data-derived={derived ? 'true' : undefined}
    className={`flex flex-col gap-0.5 border-b border-l-4 px-3 py-1.5 text-neutral-200 ${TONE_CLASSES[presentation.tone]}`}
  >
    <span className="font-mono font-semibold uppercase tracking-wider">{presentation.label}</span>
    <span className="opacity-80">{presentation.detail}</span>
  </div>
);

/**
 * The provenance strip: one segment per axis that has something to say, so a claim is never
 * compressed back into one value that has to mean two things.
 *
 * Always rendered, because the views always make a claim — the question is only which one, and
 * silence would be a claim of its own. A `null` claim means the caller supplied devices without a
 * source, and renders NOT_STATED rather than a claim it cannot support.
 *
 * The conformance segment appears only once something has actually been read, because "not
 * compared" beside "nothing has been read" is the same sentence twice.
 */
export const TopologyProvenanceBanner: React.FC<{
  claim: TopologyClaim | null;
  /**
   * The two id sets conformance is DERIVED from. The banner does not accept a conformance value:
   * `data-derived` marks the segment as a conclusion rather than one of the two input axes, and a
   * conclusion handed in by a caller would make that marker a lie. Both absent derives
   * NOT_COMPARED.
   */
  nominalDeviceIds?: readonly string[];
  actualDeviceIds?: readonly string[];
  className?: string;
}> = ({ claim, nominalDeviceIds, actualDeviceIds, className = '' }) => {
  const key: SourceDisplayKey = claim ? sourceKeyOf(claim) : 'NOT_STATED';
  const health = claim ? TOPOLOGY_HEALTH[claim.health] : undefined;
  const conformance = deriveConformance(nominalDeviceIds, actualDeviceIds);
  const conformancePresentation =
    claim && claim.source !== 'UNREAD' ? TOPOLOGY_CONFORMANCE[conformance] : undefined;

  return (
    <div
      data-testid="topology-provenance-banner"
      data-source={claim?.source ?? 'NOT_STATED'}
      data-health={claim?.health ?? 'UNKNOWN'}
      data-commissioning={claim && claim.source === 'DISCOVERED' ? claim.commissioning : undefined}
      data-conformance={conformancePresentation ? conformance : undefined}
      role="status"
      className={`flex flex-col text-[11px] ${className}`}
    >
      <Segment presentation={TOPOLOGY_SOURCE[key]} />
      {health && <Segment presentation={health} />}
      {conformancePresentation && <Segment presentation={conformancePresentation} derived />}
    </div>
  );
};
