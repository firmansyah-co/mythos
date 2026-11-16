import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ModuleList, Role } from '../api/client';
import { useSession } from '../data/session';
import { ModulesSettings } from '../shell/settings/ModulesSettings';
import { MODBUS_DRIVER, render, resetStores, typeInto, unmountAll } from './harness';

interface Call {
  method: string;
  path: string;
  body: unknown;
}

const LIST: ModuleList = {
  modules: [
    {
      id: 'org.mythos.driver.modbus', name: 'Modbus TCP', version: '0.1.0', kind: 'driver', provides: 'modbus',
      description: '', path: '/opt/mythos/versions/0.1.0/bin/mythos-module-modbus', enabled: true, removable: false,
    },
    {
      id: 'org.example.notes', name: 'Notes', version: '1.0.0', kind: 'service', provides: 'notes',
      description: '', path: '/var/lib/mythos/projects/.mythos-system/modules/mythos-module-notes', enabled: true,
      removable: true,
    },
  ],
  drivers: [MODBUS_DRIVER],
  services: [
    { provides: 'notes', id: 'org.example.notes', version: '1.0.0', path: '', pid: 4242, starts: 2, last_error: null },
  ],
  problems: [{ path: '/opt/mythos/bin/mythos-module-broken', message: 'its manifest is not valid' }],
  install_dir: '/var/lib/mythos/projects/.mythos-system/modules',
  target: 'x86_64-unknown-linux-gnu',
};

function controller() {
  const calls: Call[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      const method = init?.method ?? 'GET';
      const body =
        typeof init?.body === 'string' ? (JSON.parse(init.body) as unknown) : init?.body instanceof Blob ? 'blob' : undefined;
      calls.push({ method, path, body });
      const json = (value: unknown, code = 200) =>
        new Response(JSON.stringify(value), { status: code, headers: { 'Content-Type': 'application/json' } });
      if (path === '/api/v1/modules') return json(LIST);
      if (path.endsWith('/enabled')) return json({ modules: LIST.modules });
      if (path === '/api/v1/modules/install') return json({ installed: 'org.example.pumps', version: '2.0.0', enabled: true }, 201);
      if (method === 'DELETE') return json({ removed: 'org.example.notes' });
      return json({ error: 'not in this test' }, 404);
    })
  );
  return calls;
}

const signedIn = (role: Role) =>
  useSession.setState({ phase: 'signed-in', user: { username: role, display_name: role, role, via: 'session' } });

async function settle() {
  for (let i = 0; i < 5; i++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

beforeEach(resetStores);
afterEach(() => {
  unmountAll();
  vi.unstubAllGlobals();
});

describe('Settings → Modules', () => {
  it('lists every module with its state, and what could not be used', async () => {
    controller();
    signedIn('viewer');
    const view = render(<ModulesSettings />);
    await settle();
    const rows = view.querySelectorAll('[data-testid="module-row"]');
    expect(rows).toHaveLength(2);
    expect(rows[1]!.textContent).toContain('running · pid 4242');
    expect(rows[0]!.textContent).toContain('built in');
    expect(view.textContent).toContain('its manifest is not valid');
    expect(view.textContent).toContain('x86_64-unknown-linux-gnu');
  });

  it('a viewer can look but not change anything', async () => {
    controller();
    signedIn('viewer');
    const view = render(<ModulesSettings />);
    await settle();
    expect((view.querySelector('input[aria-label="org.example.notes enabled"]') as HTMLInputElement).disabled).toBe(true);
    expect(view.querySelector('button[aria-label="Remove org.example.notes"]')).toBeNull();
    expect(view.querySelector('[aria-label="Install a module"]')).toBeNull();
  });

  it('an administrator switches a module off, removes one, and installs a package', async () => {
    const calls = controller();
    signedIn('admin');
    vi.stubGlobal('confirm', () => true);
    const view = render(<ModulesSettings />);
    await settle();

    act(() => (view.querySelector('input[aria-label="org.example.notes enabled"]') as HTMLInputElement).click());
    await settle();
    expect(calls.find((c) => c.method === 'PUT')).toMatchObject({
      path: '/api/v1/modules/org.example.notes/enabled',
      body: { enabled: false },
    });

    expect(view.querySelector('button[aria-label="Remove org.mythos.driver.modbus"]')).toBeNull();
    act(() => (view.querySelector('button[aria-label="Remove org.example.notes"]') as HTMLButtonElement).click());
    await settle();
    expect(calls.find((c) => c.method === 'DELETE')?.path).toBe('/api/v1/modules/org.example.notes');

    const input = view.querySelector('input[aria-label="Module package"]') as HTMLInputElement;
    const file = new File(['zip bytes'], 'pumps.zip', { type: 'application/zip' });
    Object.defineProperty(input, 'files', { value: [file] });
    act(() => input.dispatchEvent(new Event('change', { bubbles: true })));
    act(() => (Array.from(view.querySelectorAll('button')).find((b) => b.textContent?.includes('Install from file')) as HTMLButtonElement).click());
    await settle();
    expect(calls.find((c) => c.path === '/api/v1/modules/install')).toMatchObject({ method: 'POST', body: 'blob' });
    expect(view.textContent).toContain('org.example.pumps 2.0.0 is installed and in use');

    typeInto(view.querySelector('input[aria-label="Package URL"]'), 'https://example.invalid/m.zip');
    expect(view.querySelector('input[aria-label="Package SHA-256"]')).not.toBeNull();
  });
});
