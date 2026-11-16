import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Identity, ReleaseList, Role, UpdateStatus } from '../api/client';
import { useSession } from '../data/session';
import { UpdatesSettings } from '../shell/settings/UpdatesSettings';
import { render, resetStores, typeInto, unmountAll } from './harness';

interface Call {
  method: string;
  path: string;
  body: unknown;
}

const installed: UpdateStatus = {
  running: { version: '0.1.0', installed_version: '0.4.0', target: 'x86_64-unknown-linux-gnu' },
  installation: {
    home: '/opt/mythos',
    current_version: '0.4.0',
    previous_version: null,
    installed: ['0.4.0'],
    channel: 'release',
    pinned_version: null,
    feed: 'https://feed.example/releases',
  },
  note: null,
};

const RELEASES: ReleaseList = {
  feed: 'https://feed.example/releases',
  fetched_unix_s: 1_794_800_000,
  target_version: '0.5.0',
  releases: [
    {
      version: '0.6.0-rc.1', tag: 'v0.6.0-rc.1', channel: 'rc', name: 'MythOS 0.6.0 RC 1',
      notes: 'Logic engine preview.', published_at: '2026-11-15T10:00:00Z', url: null,
      has_bundle: true, installed: false, current: false,
    },
    {
      version: '0.5.0', tag: 'v0.5.0', channel: 'release', name: 'MythOS 0.5.0',
      notes: '- Accounts and roles\n- Settings', published_at: '2026-11-10T10:00:00Z', url: 'https://feed.example/v0.5.0',
      has_bundle: true, installed: false, current: false,
    },
    {
      version: '0.4.0', tag: 'v0.4.0', channel: 'release', name: null, notes: null,
      published_at: null, url: null, has_bundle: true, installed: true, current: true,
    },
  ],
};

function controller(status: UpdateStatus, releases: ReleaseList | 'feed down') {
  const calls: Call[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      const method = init?.method ?? 'GET';
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as unknown) : undefined;
      calls.push({ method, path, body });
      const json = (value: unknown, code = 200) =>
        new Response(JSON.stringify(value), { status: code, headers: { 'Content-Type': 'application/json' } });
      if (path === '/api/v1/updates') return json(status);
      if (path.startsWith('/api/v1/updates/releases')) {
        return releases === 'feed down'
          ? json({ error: 'the release feed https://feed.example could not be read: connection refused' }, 502)
          : json(releases);
      }
      if (path === '/api/v1/updates/install') {
        const version = (body as { version?: string }).version ?? '0.5.0';
        return json({ version, reported: `mythos ${version}`, restart_required: true, status });
      }
      if (path === '/api/v1/updates/follow') {
        const { channel, pinned_version } = body as { channel: 'release'; pinned_version: string | null };
        return json({ ...status, installation: { ...status.installation!, channel, pinned_version } });
      }
      return json({ error: 'not in this test' }, 404);
    })
  );
  return calls;
}

const signedIn = (role: Role) =>
  useSession.setState({
    phase: 'signed-in',
    user: { username: role, display_name: role, role, via: 'session' } satisfies Identity,
  });

async function settle() {
  for (let i = 0; i < 5; i++) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

function button(container: HTMLElement, text: string) {
  return Array.from(container.querySelectorAll('button')).find((b) => b.textContent?.includes(text)) ?? null;
}

beforeEach(resetStores);
afterEach(() => {
  unmountAll();
  vi.unstubAllGlobals();
});

describe('Settings → Updates', () => {
  it('lists every published version, and shows what one contains when opened', async () => {
    controller(installed, RELEASES);
    signedIn('viewer');
    const view = render(<UpdatesSettings />);
    await settle();
    const rows = view.querySelectorAll('[data-testid="release"]');
    expect(Array.from(rows).map((r) => r.querySelector('.settings__release-version')?.textContent)).toEqual([
      '0.6.0-rc.1',
      '0.5.0',
      '0.4.0',
    ]);
    expect(rows[2]!.textContent).toContain('current');
    act(() => (rows[1]!.querySelector('button[aria-expanded]') as HTMLButtonElement).click());
    expect(view.querySelector('[data-testid="release-notes"]')?.textContent).toContain('Accounts and roles');
  });

  it('a viewer sees the versions but has nothing to install with', async () => {
    controller(installed, RELEASES);
    signedIn('viewer');
    const view = render(<UpdatesSettings />);
    await settle();
    expect(view.textContent).toContain('Only an administrator changes the version');
    expect(button(view, 'Install')).toBeNull();
    expect(view.querySelector('select[aria-label="Channel"]')).toBeNull();
  });

  it('an administrator installs what the installation follows, and is told to restart', async () => {
    const calls = controller(installed, RELEASES);
    signedIn('admin');
    const view = render(<UpdatesSettings />);
    await settle();
    act(() => button(view, 'Install 0.5.0')!.click());
    await settle();
    expect(calls.find((c) => c.path === '/api/v1/updates/install')?.body).toEqual({});
    const done = view.querySelector('[data-testid="update-done"]')?.textContent ?? '';
    expect(done).toContain('0.5.0 is now current');
    expect(done).toContain('restart it from /opt/mythos/current/bin');
  });

  it('a specific version is installed from its row, or pinned', async () => {
    const calls = controller(installed, RELEASES);
    signedIn('admin');
    const view = render(<UpdatesSettings />);
    await settle();
    const rc = view.querySelectorAll('[data-testid="release"]')[0]!;
    act(() => (Array.from(rc.querySelectorAll('button')).find((b) => b.textContent?.includes('Install')) as HTMLButtonElement).click());
    await settle();
    expect(calls.find((c) => c.path === '/api/v1/updates/install')?.body).toEqual({ version: '0.6.0-rc.1' });

    act(() => (view.querySelectorAll('input[name="follow"]')[1] as HTMLInputElement).click());
    typeInto(view.querySelector('select[aria-label="Pinned version"]'), '0.5.0');
    act(() => button(view, 'Save')!.click());
    await settle();
    expect(calls.find((c) => c.path === '/api/v1/updates/follow')?.body).toEqual({
      channel: 'release',
      pinned_version: '0.5.0',
    });
  });

  it('a feed that cannot be read is said so, not shown as an empty list', async () => {
    controller(installed, 'feed down');
    signedIn('admin');
    const view = render(<UpdatesSettings />);
    await settle();
    expect(view.querySelector('[data-testid="feed-error"]')?.textContent).toContain('could not be read');
    expect(view.textContent).not.toContain('Nothing has been published');
  });

  it('a server that runs from no installation says why, and offers no install', async () => {
    controller(
      { ...installed, installation: null, note: 'This server does not run from an installation (/src/target/debug/mythos-server)' },
      RELEASES
    );
    signedIn('admin');
    const view = render(<UpdatesSettings />);
    await settle();
    expect(view.querySelector('[data-testid="no-installation"]')?.textContent).toContain('does not run from an installation');
    expect(button(view, 'Install')).toBeNull();
    expect(view.querySelectorAll('[data-testid="release"]')).toHaveLength(3);
  });
});
