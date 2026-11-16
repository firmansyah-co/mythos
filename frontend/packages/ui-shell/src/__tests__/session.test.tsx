import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from '../App';
import type { Identity, Role, UserAccount } from '../api/client';
import { useSession } from '../data/session';
import { useWorkbench } from '../data/workbench';
import { project, render, resetStores, typeInto, unmountAll } from './harness';

interface Call {
  method: string;
  path: string;
  body: unknown;
}

/** A controller with accounts: what `/api/v1/auth/status` says, and a few routes. */
function controller(state: { setup: boolean; user: Identity | null; users?: UserAccount[] }) {
  const calls: Call[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      const method = init?.method ?? 'GET';
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as unknown) : undefined;
      calls.push({ method, path, body });
      const json = (value: unknown, status = 200) =>
        new Response(JSON.stringify(value), { status, headers: { 'Content-Type': 'application/json' } });

      if (path === '/api/v1/auth/status') {
        return json({ auth: true, setup_required: state.setup, user: state.user });
      }
      if (path === '/api/v1/auth/login') {
        const { username, password } = body as { username: string; password: string };
        if (password !== 'right password') return json({ error: 'unknown user or wrong password' }, 401);
        state.user = { username, display_name: username, role: 'engineer', via: 'session' };
        return json(state.user);
      }
      if (path === '/api/v1/auth/setup') {
        const { username } = body as { username: string };
        state.setup = false;
        state.user = { username, display_name: username, role: 'admin', via: 'session' };
        return json(state.user, 201);
      }
      if (path === '/api/v1/auth/logout') {
        state.user = null;
        return new Response(null, { status: 204 });
      }
      if (path === '/api/v1/auth/me/tokens') return json([]);
      if (path === '/api/v1/users' && method === 'GET') {
        return json({ users: state.users ?? [], roles: ['viewer', 'operator', 'engineer', 'admin'] });
      }
      if (path.startsWith('/api/v1/users/') && method === 'PUT') {
        return json({ ...(state.users?.[0] ?? {}), ...(body as object) });
      }
      if (!state.user) return json({ error: 'sign in first' }, 401);
      if (path === '/api/v1/projects') return json([]);
      if (path === '/api/v1/projects/active') return json({ project_id: null });
      return json({ error: 'not in this test' }, 404);
    })
  );
  return calls;
}

const who = (role: Role): Identity => ({ username: role, display_name: role, role, via: 'session' });

async function settle() {
  for (let i = 0; i < 5; i++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

function click(element: Element | null) {
  if (!(element instanceof HTMLElement)) throw new Error('nothing to click');
  act(() => element.click());
}

function button(container: HTMLElement, text: string): HTMLElement | null {
  return (
    Array.from(container.querySelectorAll('button')).find((b) => b.textContent?.trim().includes(text)) ??
    null
  );
}

beforeEach(resetStores);
afterEach(() => {
  unmountAll();
  vi.unstubAllGlobals();
});

describe('signing in', () => {
  it('shows nothing of the controller until it says who the user is', async () => {
    const calls = controller({ setup: false, user: null });
    const view = render(<App />);
    await settle();
    expect(view.querySelector('[data-testid="sign-in"]')).not.toBeNull();
    expect(view.querySelector('[data-testid="portal-view"]')).toBeNull();
    expect(calls.map((c) => c.path)).toEqual(['/api/v1/auth/status']);
  });

  it('a wrong password is refused and the right one opens the workbench', async () => {
    controller({ setup: false, user: null });
    const view = render(<App />);
    await settle();
    typeInto(view.querySelector('input[autocomplete="username"]'), 'erin');
    typeInto(view.querySelector('input[type="password"]'), 'wrong');
    click(button(view, 'Sign in'));
    await settle();
    expect(view.querySelector('[role="alert"]')?.textContent).toContain('wrong password');

    typeInto(view.querySelector('input[type="password"]'), 'right password');
    click(button(view, 'Sign in'));
    await settle();
    expect(view.querySelector('[data-testid="portal-view"]')).not.toBeNull();
    expect(view.querySelector('[data-testid="signed-in-as"]')?.textContent).toContain('engineer');
  });

  it('a new controller asks for its first administrator, with the setup code', async () => {
    const calls = controller({ setup: true, user: null });
    const view = render(<App />);
    await settle();
    expect(view.textContent).toContain('Create the first administrator');
    const inputs = view.querySelectorAll('input');
    typeInto(inputs[0]!, 'a1b2c3d4');
    typeInto(inputs[1]!, 'root');
    typeInto(inputs[3]!, 'a long password');
    typeInto(inputs[4]!, 'a long password');
    click(button(view, 'Create administrator'));
    await settle();
    expect(calls.find((c) => c.path === '/api/v1/auth/setup')?.body).toMatchObject({
      setup_code: 'a1b2c3d4',
      username: 'root',
    });
    expect(view.querySelector('[data-testid="portal-view"]')).not.toBeNull();
  });

  it('a request answered 401 ends the session: back to sign-in, with the project closed', async () => {
    controller({ setup: false, user: who('engineer') });
    const view = render(<App />);
    await settle();
    act(() => useWorkbench.getState().openProject(project('plant')));
    act(() => useSession.getState().expired());
    await settle();
    expect(view.querySelector('[data-testid="sign-in"]')).not.toBeNull();
    expect(view.textContent).toContain('Your session has ended');
    expect(useWorkbench.getState().project).toBeNull();
  });
});

describe('settings', () => {
  it('a viewer sees their account, the node, the pairings, the fleet, the versions and the modules, not the accounts, and cannot create a project', async () => {
    controller({ setup: false, user: who('viewer') });
    const view = render(<App />);
    await settle();
    expect(button(view, 'Create new project')).toBeNull();
    click(button(view, 'Settings'));
    await settle();
    const nav = view.querySelector('nav[aria-label="Settings"]');
    expect(Array.from(nav?.querySelectorAll('button') ?? []).map((b) => b.textContent?.trim())).toEqual([
      'Account',
      'Node',
      'Pairing',
      'Fleet',
      'Updates',
      'Modules',
    ]);
    expect(view.querySelector('[data-testid="my-role"]')?.textContent).toBe('viewer');
  });

  it('an administrator manages accounts: changing a role sends it to the controller', async () => {
    const calls = controller({
      setup: false,
      user: who('admin'),
      users: [{ username: 'erin', display_name: 'Erin', role: 'engineer', disabled: false, created_unix_s: 0 }],
    });
    const view = render(<App />);
    await settle();
    act(() => useWorkbench.getState().showSettings('users'));
    await settle();
    expect(view.querySelectorAll('[data-testid="user-row"]')).toHaveLength(1);
    typeInto(view.querySelector('select[aria-label="Role of erin"]'), 'viewer');
    await settle();
    expect(calls.find((c) => c.method === 'PUT')).toMatchObject({
      path: '/api/v1/users/erin',
      body: { role: 'viewer' },
    });
  });

  it('signing out returns to the sign-in page', async () => {
    controller({ setup: false, user: who('engineer') });
    const view = render(<App />);
    await settle();
    act(() => useWorkbench.getState().showSettings());
    await settle();
    click(button(view, 'Sign out'));
    await settle();
    expect(view.querySelector('[data-testid="sign-in"]')).not.toBeNull();
  });
});
