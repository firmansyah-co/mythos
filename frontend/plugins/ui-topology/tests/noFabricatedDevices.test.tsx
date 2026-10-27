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
import {
  TOPOLOGY_PROVENANCE,
  TopologyProvenance,
} from '../src/provenance';

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
   * The three device-standing states are not three flavours of blank, and this asserts they stay
   * visibly distinct. Only UNKNOWN is reachable today because no transport exists, but the other
   * three are defined and render — shaping them now is cheap; reshaping later is not.
   */
  it('states its provenance, and the three device-standing states are visibly distinct', () => {
    const states: TopologyProvenance[] = [
      'UNKNOWN',
      'CONFIGURED',
      'DISCOVERED_UNCOMMISSIONED',
      'UNREACHABLE',
    ];
    const seen = new Map<string, { label: string; tone: string }>();

    for (const state of states) {
      renderClient(<NetworkCanvas provenance={state} />);
      const banner = container.querySelector(`[data-testid="${TOPOLOGY_PROVENANCE[state].testId}"]`);
      expect(banner, `${state} did not render its provenance banner`).not.toBeNull();
      expect(banner!.getAttribute('data-provenance')).toBe(state);

      const presentation = TOPOLOGY_PROVENANCE[state];
      // Distinctness is the property that matters, not the wording: no two states may share a
      // label or a tone, so a found-but-uncommissioned device cannot read as a configured one.
      for (const [otherState, other] of seen) {
        expect(presentation.label, `${state} shares a label with ${otherState}`).not.toBe(other.label);
        expect(presentation.tone, `${state} shares a visual tone with ${otherState}`).not.toBe(other.tone);
      }
      seen.set(state, { label: presentation.label, tone: presentation.tone });
    }
  });

  it('defaults to UNKNOWN, because nothing has been read', () => {
    renderClient(<NetworkCanvas />);
    expect(container.querySelector('[data-testid="topology-provenance-unknown"]')).not.toBeNull();
    expect(container.querySelector('[data-provenance="CONFIGURED"]')).toBeNull();
  });

  it('still renders devices it is actually given', () => {
    const html = renderClient(
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

    expect(html).toContain('Authored_Station');
    expect(html).not.toContain('No devices discovered or configured');
    expect(html).not.toContain('PLC_RACK_01');
  });
});
