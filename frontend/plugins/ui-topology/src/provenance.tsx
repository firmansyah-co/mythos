/**
 * Topology provenance — what the network views are claiming, and on what basis.
 *
 * WHY THIS EXISTS, and why it is not just a blank. The three views (Network, Device, Topology)
 * previously rendered a hardcoded sample plant: a `PLC_RACK_01` at `192.168.1.10` with Siemens
 * article numbers, applied as parameter defaults, presented as the operator's own network.
 *
 * The obvious repair is one generic empty state — "no topology data" — and that is correct today
 * because no transport exists. It is also wrong tomorrow: the advisor's transport slice has to
 * distinguish THREE claims that are not three flavours of blank.
 *
 *   CONFIGURED                 read from the project's configured topology model
 *   DISCOVERED_UNCOMMISSIONED  found by discovery, but NOT commissioned into the plant
 *   UNREACHABLE                configured, but the device did not answer
 *
 * A found-but-uncommissioned device must never look configured, and an unreachable configured
 * device must never look like either. Those are different statements about a device's standing,
 * and collapsing them into one blank would have to be undone the moment the transport lands.
 *
 * THE RULE THAT PREVENTS THE ORIGINAL DEFECT (advisor's third answer), stated here because this is
 * where the invented rack lived: NEVER auto-create a node from discovery. Discovery yields
 * CANDIDATES; commissioning is an explicit act. The phantom rack appeared because a node was
 * needed and one was invented — a default that "kept the demo working". Nothing in this module or
 * the canvas below may synthesise a station, an address or a cable.
 */

import React from 'react';

export type TopologyProvenance =
  /** Nothing has been read: no transport exists, so no claim can be made. The reachable state today. */
  | 'UNKNOWN'
  /** Read from the project's configured topology model. */
  | 'CONFIGURED'
  /** Found by discovery and not yet commissioned. A candidate, not plant equipment. */
  | 'DISCOVERED_UNCOMMISSIONED'
  /** Configured, but the device did not answer. */
  | 'UNREACHABLE';

export type ProvenanceTone = 'neutral' | 'info' | 'warning' | 'alarm';

export interface ProvenancePresentation {
  readonly label: string;
  readonly detail: string;
  readonly tone: ProvenanceTone;
  readonly testId: string;
}

/**
 * Presentation per state. The LABELS and TONES are the mechanism that keeps the three
 * device-standing claims visibly distinct; tests assert distinctness rather than exact wording,
 * so the copy can change without weakening the guarantee.
 */
export const TOPOLOGY_PROVENANCE: Record<TopologyProvenance, ProvenancePresentation> = {
  UNKNOWN: {
    label: 'Topology unknown',
    detail:
      'Nothing has been read from the project or from discovery, so these views make no claim about the plant.',
    tone: 'neutral',
    testId: 'topology-provenance-unknown',
  },
  CONFIGURED: {
    label: 'Configured topology',
    detail: "Read from the project's configured topology model.",
    tone: 'info',
    testId: 'topology-provenance-configured',
  },
  DISCOVERED_UNCOMMISSIONED: {
    label: 'Discovered — not commissioned',
    detail:
      'Found by discovery. A discovered device is a candidate, not plant equipment; it becomes configured only when commissioned.',
    tone: 'warning',
    testId: 'topology-provenance-discovered-uncommissioned',
  },
  UNREACHABLE: {
    label: 'Device unreachable',
    detail: 'Configured, but the device did not answer. Its state is unknown, not healthy.',
    tone: 'alarm',
    testId: 'topology-provenance-unreachable',
  },
};

const TONE_CLASSES: Record<ProvenanceTone, string> = {
  neutral: 'border-neutral-600 bg-neutral-800/80 text-neutral-300',
  info: 'border-sky-800 bg-sky-950/40 text-sky-300',
  warning: 'border-amber-800 bg-amber-950/40 text-amber-300',
  alarm: 'border-rose-800 bg-rose-950/40 text-rose-300',
};

/**
 * The provenance strip. Always rendered, because the views always make a claim — the question is
 * only which one, and silence would be a claim of its own.
 */
export const TopologyProvenanceBanner: React.FC<{
  provenance: TopologyProvenance;
  className?: string;
}> = ({ provenance, className = '' }) => {
  const presentation = TOPOLOGY_PROVENANCE[provenance];
  return (
    <div
      data-testid={presentation.testId}
      data-provenance={provenance}
      role="status"
      className={`flex flex-col gap-0.5 border-b px-3 py-1.5 text-[11px] ${TONE_CLASSES[presentation.tone]} ${className}`}
    >
      <span className="font-mono font-semibold uppercase tracking-wider">{presentation.label}</span>
      <span className="opacity-80">{presentation.detail}</span>
    </div>
  );
};
