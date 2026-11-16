import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { FleetMember, PairedNode } from '../api/client';
import { FleetSettings } from '../shell/settings/FleetSettings';
import { render, resetStores, unmountAll } from './harness';

const paired = (id: string, name: string, address: string | null): PairedNode => ({
  id,
  name,
  role: 'edge',
  address,
  fingerprint: 'ab'.repeat(32),
  granted: 'engineer',
  paired_unix_s: 1,
  last_seen_unix_s: null,
  call_home: false,
});

const FAR: FleetMember = {
  paired: paired('far-node', 'far-edge', null),
  via: 'call_home',
  last_poll_s: 1,
  reachable: true,
  node: { name: 'far-edge', role: 'edge', site: null, version: '0.4.2' },
  running: { project: 'plant_f--far', hash: 'b7c2a1e9f0d3c4b5a6978877665544332211' },
};
const GONE: FleetMember = {
  paired: paired('gone-node', 'boiler-edge-2', '10.0.0.9:8443'),
  via: 'direct',
  last_poll_s: null,
  reachable: false,
  error: '10.0.0.9:8443 cannot be reached: connection refused',
};

function controller(answer: { nodes: FleetMember[] } | 'fails') {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      if (String(input) !== '/api/v1/fleet') return new Response('{}', { status: 404 });
      return answer === 'fails'
        ? new Response(JSON.stringify({ error: 'the controller is down' }), { status: 500 })
        : new Response(JSON.stringify(answer));
    })
  );
}

const settle = () => act(async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); });
const cells = (view: HTMLElement, id: string) =>
  [...view.querySelectorAll(`[data-testid="${id}"]`)].map((c) => c.textContent?.trim());

beforeEach(() => resetStores());
afterEach(() => {
  unmountAll();
  vi.unstubAllGlobals();
});

describe('Settings → Fleet', () => {
  it('shows how each node is reached and what it runs; one not answering runs nothing known', async () => {
    controller({ nodes: [FAR, GONE] });
    const view = render(<FleetSettings />);
    await settle();
    expect(cells(view, 'fleet-via')).toEqual(['it calls home', 'directly']);
    expect(cells(view, 'fleet-answer')).toEqual(['edge · 0.4.2', 'NOT ANSWERING']);
    expect(cells(view, 'fleet-running')).toEqual(['plant_f--farb7c2a1e9f0d3…', 'unknown']);
    expect(view.textContent).toContain('connection refused');
  });

  it('says nothing of the fleet when it cannot be asked', async () => {
    controller('fails');
    const view = render(<FleetSettings />);
    await settle();
    expect(view.querySelector('[role="alert"]')?.textContent).toContain('the controller is down');
    expect(view.querySelectorAll('[data-testid="fleet-node"]')).toHaveLength(0);
  });

  it('says when this node is paired with no other', async () => {
    controller({ nodes: [] });
    const view = render(<FleetSettings />);
    await settle();
    expect(view.querySelector('[data-testid="fleet-empty"]')).not.toBeNull();
  });
});
