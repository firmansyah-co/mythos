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
