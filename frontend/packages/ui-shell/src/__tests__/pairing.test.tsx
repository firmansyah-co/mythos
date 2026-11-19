import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PairedNode, Role } from '../api/client';
import { useSession } from '../data/session';
import { PairingSettings } from '../shell/settings/PairingSettings';
import { render, resetStores, typeInto, unmountAll } from './harness';

const B: PairedNode = {
  id: '58e39a0a-07b3-4af1-a02e-237b41d7c5d4',
  name: 'boiler-edge-1',
  role: 'edge',
  address: '192.168.10.21:8443',
  fingerprint: 'af3d9b6f39b8f8654384887d109290c7995ce980f88238601d98cfddd8980f2e',
  granted: 'viewer',
  paired_unix_s: 1_794_800_000,
  last_seen_unix_s: null,
  call_home: false,
};

function controller(state: { peers: PairedNode[]; refuse?: string }) {
  const calls: { method: string; path: string; body: unknown }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      const method = init?.method ?? 'GET';
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as Record<string, unknown>) : undefined;
      calls.push({ method, path, body });
      const json = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status });
      if (path === '/api/v1/pairings' && method === 'GET') return json({ peers: state.peers });
      if (path === '/api/v1/pairing/code') return json({ code: 'K7QX-3MZP', expires_unix_s: 1_794_800_600, granted: body?.role });
      if (path === '/api/v1/pairings' && method === 'POST') {
        if (state.refuse) return json({ error: state.refuse }, 502);
        state.peers = [B];
        return json({ peer: B, granted_there: 'engineer' }, 201);
      }
      if (method === 'PUT' && path.startsWith('/api/v1/pairings/')) {
        state.peers = state.peers.map((p) => ({ ...p, call_home: Boolean(body?.call_home) }));
        return json({ peer: state.peers[0] });
      }
      if (path.endsWith('/status')) return json({ reachable: true, node: { name: 'boiler-edge-1', role: 'edge', version: '0.1.0' } });
      if (method === 'DELETE') {
        state.peers = [];
        return json({ removed: B.id, told: true, told_error: null });
      }
      return json({ error: 'not in this test' }, 404);
    })
  );
  return calls;
}

const signedIn = (role: Role) =>
  useSession.setState({ phase: 'signed-in', user: { username: role, display_name: role, role, via: 'session' } });
const settle = () => act(async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); });
const click = (el: Element | null | undefined) => act(() => (el as HTMLButtonElement).click());
const button = (root: HTMLElement, text: string) =>
  [...root.querySelectorAll('button')].find((b) => b.textContent?.trim() === text);

beforeEach(() => resetStores());
afterEach(() => {
  unmountAll();
  vi.unstubAllGlobals();
});

describe('Settings → Pairing', () => {
  it('makes a code that grants the role chosen', async () => {
    const calls = controller({ peers: [] });
    signedIn('admin');
    const view = render(<PairingSettings />);
    await settle();
    expect(view.querySelector('[data-testid="pairings-empty"]')).not.toBeNull();
    typeInto(view.querySelector('select[aria-label="Role granted"]'), 'operator');
    click(button(view, 'Make a pairing code'));
    await settle();
    expect(calls.find((c) => c.path === '/api/v1/pairing/code')?.body).toEqual({ role: 'operator' });
    expect(view.querySelector('[data-testid="pairing-code"]')?.textContent).toContain('K7QX-3MZP');
  });

  it('pairs with a node by address, fingerprint and code, then lists it, reaches it and unpairs it', async () => {
    const calls = controller({ peers: [] });
    signedIn('admin');
    vi.stubGlobal('confirm', () => true);
    const view = render(<PairingSettings />);
    await settle();
    typeInto(view.querySelector('input[aria-label="Address"]'), '192.168.10.21:8443');
    typeInto(view.querySelector('input[aria-label="Fingerprint"]'), 'af3d 9b6f 39b8 f865');
    typeInto(view.querySelector('input[aria-label="Pairing code"]'), 'k7qx-3mzp');
    click(button(view, 'Pair'));
    await settle();
    expect(calls.find((c) => c.method === 'POST' && c.path === '/api/v1/pairings')?.body).toEqual({
      address: '192.168.10.21:8443',
      fingerprint: 'af3d 9b6f 39b8 f865',
      code: 'k7qx-3mzp',
      grant: 'viewer',
      call_home: false,
    });
    expect(view.querySelector('[data-testid="pair-result"]')?.textContent).toContain('this node there as engineer');
    expect(view.querySelectorAll('[data-testid="paired-node"]')).toHaveLength(1);

    click(button(view, 'Reach'));
    await settle();
    expect(view.querySelector('[data-testid="paired-node-reach"]')?.textContent).toBe('reachable: boiler-edge-1 (edge)');

    click(button(view, 'Unpair'));
    await settle();
    expect(calls.some((c) => c.method === 'DELETE' && c.path === `/api/v1/pairings/${B.id}`)).toBe(true);
    expect(view.querySelector('[data-testid="pairings-empty"]')).not.toBeNull();
  });

  it('says why a pairing was refused', async () => {
    controller({ peers: [], refuse: "192.168.10.21:8443 could not be paired with: the node's certificate is not the one expected" });
    signedIn('admin');
    const view = render(<PairingSettings />);
    await settle();
    typeInto(view.querySelector('input[aria-label="Address"]'), '192.168.10.21:8443');
    typeInto(view.querySelector('input[aria-label="Fingerprint"]'), '0'.repeat(64));
    typeInto(view.querySelector('input[aria-label="Pairing code"]'), 'K7QX-3MZP');
    click(button(view, 'Pair'));
    await settle();
    expect(view.querySelector('[data-testid="pair-result"]')?.textContent).toContain('not the one expected');
  });

  it('a viewer sees the paired nodes but cannot pair or unpair', async () => {
    controller({ peers: [B] });
    signedIn('viewer');
    const view = render(<PairingSettings />);
    await settle();
    expect(view.querySelectorAll('[data-testid="paired-node"]')).toHaveLength(1);
    expect(button(view, 'Unpair')).toBeUndefined();
    expect(button(view, 'Make a pairing code')).toBeUndefined();
    expect(view.textContent).toContain('Only an administrator pairs and unpairs nodes.');
  });

  it('an administrator calls a paired node home', async () => {
    const calls = controller({ peers: [B] });
    signedIn('admin');
    const view = render(<PairingSettings />);
    await settle();
    const box = view.querySelector(`input[aria-label="Call ${B.name} home"]`) as HTMLInputElement;
    expect(box.checked).toBe(false);
    await click(box);
    await settle();
    expect(calls.find((c) => c.method === 'PUT')).toEqual({
      method: 'PUT',
      path: `/api/v1/pairings/${B.id}`,
      body: { call_home: true },
    });
    expect((view.querySelector(`input[aria-label="Call ${B.name} home"]`) as HTMLInputElement).checked).toBe(true);
  });

  it('a viewer sees whether a node is called home and cannot change it', async () => {
    controller({ peers: [{ ...B, call_home: true }] });
    signedIn('viewer');
    const view = render(<PairingSettings />);
    await settle();
    const box = view.querySelector(`input[aria-label="Call ${B.name} home"]`) as HTMLInputElement;
    expect(box.checked).toBe(true);
    expect(box.disabled).toBe(true);
  });
});
