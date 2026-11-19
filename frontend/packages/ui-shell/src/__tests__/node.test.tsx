import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { NodeInfo, Role } from '../api/client';
import { useSession } from '../data/session';
import { NodeSettings } from '../shell/settings/NodeSettings';
import { render, resetStores, typeInto, unmountAll } from './harness';

const EDGE: NodeInfo = {
  id: '7f3c2a10-5b9e-4c1d-9a2f-0e6b8d4c1a77',
  name: 'boiler-edge-1',
  role: 'edge',
  site: '',
  version: '0.1.0',
  module_protocol: 1,
  listening: '0.0.0.0:8080',
  listen_address: '0.0.0.0',
  restart_required: false,
  field_io: { on: true, from_role: true },
  announce: { on: true, from_role: true },
  field_io_off: null,
  http_on_network: false,
  https: {
    listening: '0.0.0.0:8443',
    listen_address: '0.0.0.0',
    fingerprint: 'ab'.repeat(32),
    fingerprint_spoken: 'abab abab abab abab abab abab abab abab abab abab abab abab abab abab abab abab',
  },
};

function controller(node: NodeInfo, refuse?: string) {
  const calls: { method: string; body: unknown }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const method = init?.method ?? 'GET';
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as Record<string, string>) : undefined;
      calls.push({ method, body });
      if (String(input) !== '/api/v1/node') return new Response('{}', { status: 404 });
      if (method === 'PUT') {
        if (refuse) return new Response(JSON.stringify({ error: refuse }), { status: 422, statusText: 'Unprocessable Entity' });
        // As the controller answers: a setting sent as null follows the role again.
        const { field_io, announce, ...rest } = (body ?? {}) as Record<string, unknown>;
        const role = (rest.role as NodeInfo['role'] | undefined) ?? node.role;
        const listen_address = role === 'workstation' ? '127.0.0.1' : '0.0.0.0';
        const setting = (sent: unknown, current: NodeInfo['field_io'], roleSays: boolean) =>
          sent === undefined ? current : sent === null ? { on: roleSays, from_role: true } : { on: sent as boolean, from_role: false };
        return Response.json({
          ...node,
          ...rest,
          field_io: setting(field_io, node.field_io, role !== 'server'),
          announce: setting(announce, node.announce, role === 'edge'),
          listen_address,
          restart_required: listen_address !== '0.0.0.0',
        });
      }
      return Response.json(node);
    })
  );
  return calls;
}

const signedIn = (role: Role) =>
  useSession.setState({ phase: 'signed-in', user: { username: role, display_name: role, role, via: 'session' } });

const settle = () => act(async () => { for (let i = 0; i < 5; i++) await Promise.resolve(); });

beforeEach(() => resetStores());
afterEach(() => {
  unmountAll();
  vi.unstubAllGlobals();
});

describe('Settings → Node', () => {
  it('shows what the node is, as the controller says', async () => {
    controller(EDGE);
    signedIn('viewer');
    const view = render(<NodeSettings />);
    await settle();
    expect(view.querySelector('[data-testid="node-id"]')?.textContent).toBe(EDGE.id);
    expect(view.querySelector('[data-testid="node-listening"]')?.textContent).toBe('0.0.0.0:8080');
    expect(view.querySelector('[data-testid="node-https"]')?.textContent).toBe('0.0.0.0:8443');
    expect(view.querySelector('[data-testid="node-fingerprint"]')?.textContent).toBe(EDGE.https.fingerprint_spoken);
    expect((view.querySelector('input[aria-label="Node name"]') as HTMLInputElement).value).toBe('boiler-edge-1');
    expect((view.querySelector('input[value="edge"]') as HTMLInputElement).checked).toBe(true);
    expect(view.querySelector('[data-testid="node-restart"]')).toBeNull();
  });

  it('a viewer can look but not change it', async () => {
    controller(EDGE);
    signedIn('viewer');
    const view = render(<NodeSettings />);
    await settle();
    expect((view.querySelector('input[aria-label="Node name"]') as HTMLInputElement).disabled).toBe(true);
    expect(view.querySelector('button[type="submit"]')).toBeNull();
    expect(view.textContent).toContain('Only an administrator changes the node.');
  });

  it('an administrator renames it and changes its role, and is told when that applies', async () => {
    const calls = controller(EDGE);
    signedIn('admin');
    const view = render(<NodeSettings />);
    await settle();
    const save = view.querySelector('button[type="submit"]') as HTMLButtonElement;
    expect(save.disabled).toBe(true);

    typeInto(view.querySelector('input[aria-label="Node name"]'), 'Boiler edge 1');
    act(() => (view.querySelector('input[value="workstation"]') as HTMLInputElement).click());
    expect(save.disabled).toBe(false);
    act(() => save.click());
    await settle();

    expect(calls.filter((c) => c.method === 'PUT').map((c) => c.body)).toEqual([
      { name: 'Boiler edge 1', site: '', role: 'workstation', field_io: null, announce: null, http_on_network: false },
    ]);
    const restart = view.querySelector('[data-testid="node-restart"]')?.textContent ?? '';
    expect(restart).toContain('127.0.0.1');
    expect(restart).toContain('0.0.0.0:8080');
  });

  it('says why the controller refused a change', async () => {
    controller(EDGE, "'plc' is not a role; use workstation, edge or server");
    signedIn('admin');
    const view = render(<NodeSettings />);
    await settle();
    typeInto(view.querySelector('input[aria-label="Node name"]'), 'x');
    act(() => (view.querySelector('button[type="submit"]') as HTMLButtonElement).click());
    await settle();
    expect(view.querySelector('[role="alert"]')?.textContent).toContain("'plc' is not a role");
  });

  it('sets a node to read the field whatever its role says, and back to the role', async () => {
    const server: NodeInfo = {
      ...EDGE,
      role: 'server',
      field_io: { on: false, from_role: true },
      announce: { on: false, from_role: true },
      field_io_off: 'field I/O is off on this node: its role is server, which reads no field devices',
    };
    const calls = controller(server);
    signedIn('admin');
    const view = render(<NodeSettings />);
    await settle();
    expect(view.querySelector('[data-testid="node-field-io-off"]')?.textContent).toContain('its role is server');
    const select = view.querySelector('select[aria-label="Reads field devices"]') as HTMLSelectElement;
    expect(select.value).toBe('role');
    expect(select.options[0]?.textContent).toBe('As the role says (off)');

    typeInto(select, 'on');
    act(() => (view.querySelector('button[type="submit"]') as HTMLButtonElement).click());
    await settle();
    expect(calls.filter((c) => c.method === 'PUT').map((c) => (c.body as Record<string, unknown>).field_io)).toEqual([true]);
  });
});
