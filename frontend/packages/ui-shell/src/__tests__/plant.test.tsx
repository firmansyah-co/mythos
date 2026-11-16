import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Role } from '../api/client';
import { useSession } from '../data/session';
import { useWorkbench } from '../data/workbench';
import { PlantEditor } from '../editors/PlantEditor';
import { project, render, resetStores, typeInto, unmountAll } from './harness';

const THIS = 'aaaa1111-0000-4000-8000-000000000001';
const EDGE = 'bbbb2222-0000-4000-8000-000000000002';

function controller(states: Record<string, string>) {
  const calls: { method: string; path: string; body: unknown }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      const method = init?.method ?? 'GET';
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as unknown) : undefined;
      calls.push({ method, path, body });
      const json = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status });
      if (path === '/api/v1/projects/plant_a/plant') {
        return json({
          implicit: false,
          devices: [
            { id: 'this', name: 'This controller', project: 'plant_a', node: { id: THIS, name: 'laptop', this: true } },
            { id: 'edge1', name: 'Boiler edge 1', project: 'plant_a--edge1', node: { id: EDGE, name: 'boiler-edge-1', paired: true } },
          ],
        });
      }
      const compared = path.match(/plant\/devices\/([^/]+)\/compare$/);
      if (compared) return json({ device: compared[1], state: states[compared[1]!], error: 'connection refused' });
      if (path === '/api/v1/pairings') return json({ peers: [{ id: EDGE, name: 'boiler-edge-1', role: 'edge', address: '10.0.0.5:8443', fingerprint: 'ab', granted: 'viewer', paired_unix_s: 1, last_seen_unix_s: null }] });
      if (path === '/api/v1/node') return json({ id: THIS, name: 'laptop' });
      if (path.endsWith('/download') && path.includes('/devices/')) return json({ device: 'edge1', state: 'done' });
      if (path.endsWith('/plant/download')) return json({ complete: false, devices: [{ device: 'this', state: 'done' }, { device: 'edge1', state: 'unreachable', error: 'no route' }] });
      if (method === 'PUT') return json({ id: 'edge1' });
      return json({ error: 'not in this test' }, 404);
    })
  );
  return calls;
}

const signedIn = (role: Role) =>
  useSession.setState({ phase: 'signed-in', user: { username: role, display_name: role, role, via: 'session' } });
const settle = () => act(async () => { for (let i = 0; i < 10; i++) await Promise.resolve(); });
const button = (root: HTMLElement, text: string) =>
  [...root.querySelectorAll('button')].find((b) => b.textContent?.trim() === text);

beforeEach(() => {
  resetStores();
  useWorkbench.getState().openProject(project('plant_a', 'Boiler House'));
});
afterEach(() => {
  unmountAll();
  vi.unstubAllGlobals();
});

describe('the plant', () => {
  it('shows each device, its node, and what comparing it says; unreachable is never the same', async () => {
    controller({ this: 'same', edge1: 'unreachable' });
    signedIn('engineer');
    const view = render(<PlantEditor />);
    await settle();
    const rows = [...view.querySelectorAll('[data-testid="plant-device"]')];
    expect(rows.map((r) => r.querySelector('strong')?.textContent)).toEqual(['This controller', 'Boiler edge 1']);
    expect([...view.querySelectorAll('[data-testid="plant-device-state"]')].map((b) => b.textContent)).toEqual([
      'SAME AS PROJECT',
      'UNREACHABLE',
    ]);
    expect((view.querySelector('select[aria-label="Node of Boiler edge 1"]') as HTMLSelectElement).value).toBe(EDGE);
  });

  it('downloads only when it is confirmed, and says what each device of a download-all did', async () => {
    const calls = controller({ this: 'same', edge1: 'differs' });
    signedIn('engineer');
    const confirm = vi.fn(() => false);
    vi.stubGlobal('confirm', confirm);
    const view = render(<PlantEditor />);
    await settle();
    const rowButtons = (name: string) =>
      [...view.querySelectorAll('[data-testid="plant-device"]')].find((r) => r.textContent?.includes(name))!;
    act(() => (button(rowButtons('Boiler edge 1') as HTMLElement, 'Download') as HTMLButtonElement).click());
    await settle();
    expect(confirm).toHaveBeenCalled();
    expect(calls.some((c) => c.path.endsWith('/download'))).toBe(false);

    confirm.mockReturnValue(true);
    act(() => (button(view, 'Download all') as HTMLButtonElement).click());
    await settle();
    expect(calls.find((c) => c.path.endsWith('/plant/download'))?.body).toEqual({ confirm: true });
    expect(view.querySelector('[data-testid="plant-notice"]')?.textContent).toContain('edge1: unreachable (no route)');
  });

  it('assigns a device to a paired node', async () => {
    const calls = controller({ this: 'same', edge1: 'not_assigned' });
    signedIn('engineer');
    const view = render(<PlantEditor />);
    await settle();
    typeInto(view.querySelector('select[aria-label="Node of Boiler edge 1"]'), '');
    await settle();
    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ name: 'Boiler edge 1', node: null });
  });

  it('a viewer sees the plant but downloads nothing', async () => {
    controller({ this: 'same', edge1: 'same' });
    signedIn('viewer');
    const view = render(<PlantEditor />);
    await settle();
    expect(button(view, 'Download all')).toBeUndefined();
    expect(button(view, 'Download')).toBeUndefined();
  });
});
