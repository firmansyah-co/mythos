/**
 * SYS-REQ-096: the network canvas may not assert a plant topology nobody discovered.
 *
 * `DEFAULT_STATIONS` / `DEFAULT_CABLES` were wired in as parameter defaults, and the shell renders
 * `<NetworkCanvas />` with no props. So the view claimed a commissioned `PLC_RACK_01` running at
 * `192.168.1.10`, with Siemens article numbers, slot layout and LED diagnostics — a description of
 * hardware that may not exist at that address, presented as discovered fact.
 *
 * Nothing discovered now renders nothing, and says so.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { NetworkCanvas } from '../src/NetworkCanvas';
import { deriveConformance, TopologyClaim } from '../src/provenance';

(globalThis as any).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

const FABRICATED_PLANT = [
  'PLC_RACK_01',
  '192.168.1.10',
  '6ES7515-2AM02-0AB0',
  '6EP1333-4BA00',
  'Siemens',
];

/** The rendered class list of a segment — what the operator's screen actually gets. */
function renderedClasses(testId: string): string {
  const element = container.querySelector(`[data-testid="${testId}"]`);
  expect(element, `no segment rendered for ${testId}`).not.toBeNull();
  return element!.className;
}

describe('SYS-REQ-096: network canvas invents no plant topology', () => {
  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    flushSync(() => {
      root.unmount();
    });
    container.remove();
  });

  it('renders an explicit empty state when nothing is discovered or configured', () => {
    const html = renderClient(<NetworkCanvas />);

    expect(html).toContain('No devices discovered or configured');
    const found = FABRICATED_PLANT.filter((token) => html.includes(token));
    expect(found, `network canvas asserted a fabricated plant: ${found.join(', ')}`).toEqual([]);
  });

  /**
   * The three views are ONE component in three modes, and this guard only ever rendered the
   * default. That is the gap this closes: the empty state is currently mode-independent by
   * structure, but nothing asserted it, so a change that made the overlay conditional on the mode
   * would have left the Device and Topology views silently fabricated with this suite still green.
   *
   * Assertions read through the resolved element, not `innerHTML.includes`: substring matching over
   * raw HTML also matches SVG path data, which is exactly what this surface is full of.
   */
  it('renders no invented station or cable in ANY of the three views', () => {
    renderClient(<NetworkCanvas />);

    const modes = ['Network View', 'Device View', 'Topology View'];
    for (const mode of modes) {
      const tab = Array.from(container.querySelectorAll('button')).find(
        (button) => (button.textContent ?? '').trim().startsWith(mode)
      ) as HTMLButtonElement | undefined;
      expect(tab, `${mode} tab not found`).toBeDefined();
      flushSync(() => {
        tab!.click();
      });

      // The explicit absence, in this mode.
      expect(
        container.querySelector('[data-testid="empty-devices-state"]'),
        `${mode} did not render the explicit no-device state`
      ).not.toBeNull();

      // And nothing fabricated anywhere on the surface. Scope the text read to the canvas root.
      const canvasText = container.querySelector('[data-testid="zone4-task-cards"]')?.textContent ?? container.textContent ?? '';
      const found = FABRICATED_PLANT.filter((token) => canvasText.includes(token));
      expect(found, `${mode} asserted a fabricated plant: ${found.join(', ')}`).toEqual([]);
    }
  });

  /**
   * VISIBLY distinct means the rendered classes differ, not that two constants differ.
   *
   * An earlier version of this guard compared `presentation.tone` — a string on the source object —
   * and stayed green when two tones were mapped to byte-identical Tailwind classes, i.e. when two
   * device standings rendered the same colour on screen. That is precisely the failure the test is
   * named for, so the assertion reads the element the operator's browser gets.
   */
  it('renders every device-standing claim in a distinct class, not merely a distinct constant', () => {
    const standing: { name: string; claim: TopologyClaim | null; testId: string }[] = [
      { name: 'UNREAD', claim: { source: 'UNREAD', health: 'UNKNOWN' }, testId: 'topology-source-unread' },
      {
        name: 'CONFIGURED',
        claim: { source: 'CONFIGURED', health: 'UNKNOWN' },
        testId: 'topology-source-configured',
      },
      {
        name: 'DISCOVERED_CANDIDATE',
        claim: { source: 'DISCOVERED', health: 'UNKNOWN', commissioning: 'CANDIDATE' },
        testId: 'topology-source-discovered-candidate',
      },
      {
        name: 'UNREACHABLE',
        claim: { source: 'CONFIGURED', health: 'UNREACHABLE' },
        testId: 'topology-health-unreachable',
      },
      {
        name: 'DEGRADED',
        claim: { source: 'CONFIGURED', health: 'DEGRADED' },
        testId: 'topology-health-degraded',
      },
    ];

    const seen: { name: string; classes: string }[] = [];
    for (const entry of standing) {
      renderClient(<NetworkCanvas claim={entry.claim} />);
      const classes = renderedClasses(entry.testId);
      for (const other of seen) {
        expect(classes, `${entry.name} renders the same classes as ${other.name}`).not.toBe(
          other.classes
        );
      }
      seen.push({ name: entry.name, classes });
    }
    expect(seen).toHaveLength(standing.length);
  });

  /**
   * THE REASON THE TWO AXES EXIST. Under the earlier single enum, a device that discovery saw and
   * that is currently silent had to be reported as either DISCOVERED_UNCOMMISSIONED — losing that
   * it is silent — or as UNREACHABLE, which was documented as "configured, but did not answer" and
   * so silently promoted an uncommissioned candidate to plant equipment. Both axes now render, so
   * the combination says both things and drops neither.
   */
  it('expresses a discovered candidate that is currently silent, without promoting it to configured', () => {
    renderClient(
      <NetworkCanvas claim={{ source: 'DISCOVERED', health: 'UNREACHABLE', commissioning: 'CANDIDATE' }} />
    );

    expect(
      container.querySelector('[data-testid="topology-source-discovered-candidate"]'),
      'the discovered source segment was dropped'
    ).not.toBeNull();
    expect(
      container.querySelector('[data-testid="topology-health-unreachable"]'),
      'the silence of the discovered candidate was dropped'
    ).not.toBeNull();

    // The defect the fusion caused: silence must not read as a claim about commissioned plant.
    expect(
      container.querySelector('[data-testid="topology-source-configured"]'),
      'a discovered candidate was presented as configured plant equipment'
    ).toBeNull();
    expect(container.textContent).not.toContain('Configured topology');

    const banner = container.querySelector('[data-testid="topology-provenance-banner"]')!;
    expect(banner.getAttribute('data-source')).toBe('DISCOVERED');
    expect(banner.getAttribute('data-health')).toBe('UNREACHABLE');
    expect(banner.getAttribute('data-commissioning')).toBe('CANDIDATE');
  });

  /**
   * A candidate and a commissioned device are both discovered, and must not look alike: the
   * candidate is not plant equipment and the commissioned device is.
   */
  it('renders a commissioned discovered device differently from a candidate', () => {
    renderClient(<NetworkCanvas claim={{ source: 'DISCOVERED', health: 'UNKNOWN', commissioning: 'CANDIDATE' }} />);
    const candidate = renderedClasses('topology-source-discovered-candidate');

    renderClient(
      <NetworkCanvas claim={{ source: 'DISCOVERED', health: 'UNKNOWN', commissioning: 'COMMISSIONED' }} />
    );
    const commissioned = renderedClasses('topology-source-discovered-commissioned');

    expect(commissioned).not.toBe(candidate);
  });

  /**
   * A measured positive adds no segment. REACHABLE and UNKNOWN-health must be indistinguishable on
   * the strip, or the absence of a measurement would quietly become a second way of saying the
   * device is healthy.
   */
  it('adds no health segment for a reachable device or an unmeasured one', () => {
    const claims: TopologyClaim[] = [
      { source: 'CONFIGURED', health: 'REACHABLE' },
      { source: 'CONFIGURED', health: 'UNKNOWN' },
    ];
    for (const claim of claims) {
      renderClient(<NetworkCanvas claim={claim} />);
      expect(container.querySelector('[data-testid="topology-health-unreachable"]')).toBeNull();
      expect(container.querySelector('[data-testid="topology-health-degraded"]')).toBeNull();
    }
  });

  /**
   * UNREAD is a true statement about an empty canvas and a FALSE one about a populated canvas: the
   * view would be drawing a device while telling the operator it makes no claim about the plant.
   * Devices with no stated source therefore render NOT_STATED, which names the wiring defect
   * instead of asserting something the screen contradicts.
   */
  it('says provenance was not stated when devices are drawn without a source', () => {
    renderClient(
      <NetworkCanvas
        initialStations={[
          {
            id: 'station-authored',
            stationName: 'Authored_Station',
            stationType: 'PLC',
            status: 'STOP',
            ipAddress: '10.0.0.5',
            articleNumber: 'ART-1',
            rackNumber: 1,
            position: { x: 0, y: 0 },
            slots: [],
            ports: [],
          } as any,
        ]}
        initialCables={[]}
      />
    );

    expect(
      container.querySelector('[data-testid="topology-source-unread"]'),
      'the banner asserted nothing had been read while drawing a device'
    ).toBeNull();
    expect(container.querySelector('[data-testid="topology-source-not-stated"]')).not.toBeNull();
    expect(container.textContent).not.toContain('Topology unknown');
  });

  it('defaults to UNREAD on an empty canvas, because nothing has been read', () => {
    renderClient(<NetworkCanvas />);
    expect(container.querySelector('[data-testid="topology-source-unread"]')).not.toBeNull();
    const banner = container.querySelector('[data-testid="topology-provenance-banner"]')!;
    expect(banner.getAttribute('data-source')).toBe('UNREAD');
    expect(banner.getAttribute('data-health')).toBe('UNKNOWN');
    expect(container.querySelector('[data-testid="topology-health-unreachable"]')).toBeNull();
  });

  /**
   * Conformance is DERIVED from nominal vs actual, never asserted as an input state. It cannot be
   * compared while only one side is readable, so the strip says so rather than leaving the slot
   * blank — and it is marked as derived so it never reads as a fourth input.
   */
  it('renders conformance as a derived claim, and never as an input state', () => {
    renderClient(<NetworkCanvas claim={{ source: 'CONFIGURED', health: 'UNKNOWN' }} conformance="MISMATCH" />);
    const conformance = container.querySelector('[data-testid="topology-conformance-mismatch"]');
    expect(conformance, 'the derived conformance claim did not render').not.toBeNull();
    expect(conformance!.getAttribute('data-derived')).toBe('true');

    // With nothing read there is nothing to compare against, so the slot stays empty rather than
    // restating "nothing has been read" in a second sentence.
    renderClient(<NetworkCanvas claim={{ source: 'UNREAD', health: 'UNKNOWN' }} conformance="MISMATCH" />);
    expect(container.querySelector('[data-testid="topology-conformance-mismatch"]')).toBeNull();
  });

  it('derives conformance from the two topologies, and answers NOT_COMPARED until both exist', () => {
    expect(deriveConformance(null, ['a'])).toBe('NOT_COMPARED');
    expect(deriveConformance(['a'], null)).toBe('NOT_COMPARED');
    expect(deriveConformance(undefined, undefined)).toBe('NOT_COMPARED');
    expect(deriveConformance(['a', 'b'], ['a', 'b'])).toBe('MATCHES');
    expect(deriveConformance(['a', 'b'], ['a', 'c'])).toBe('MISMATCH');
    expect(deriveConformance(['a'], ['a', 'b'])).toBe('MISMATCH');
  });

  it('still renders devices it is actually given', () => {
    const html = renderClient(
      <NetworkCanvas
        claim={{ source: 'CONFIGURED', health: 'REACHABLE' }}
        initialStations={[
          {
            id: 'station-authored',
            stationName: 'Authored_Station',
            stationType: 'PLC',
            status: 'STOP',
            ipAddress: '10.0.0.5',
            articleNumber: 'ART-1',
            rackNumber: 1,
            position: { x: 0, y: 0 },
            slots: [],
            ports: [],
          } as any,
        ]}
        initialCables={[]}
      />
    );

    expect(html).toContain('Authored_Station');
    expect(html).not.toContain('No devices discovered or configured');
    expect(html).not.toContain('PLC_RACK_01');
  });
});
