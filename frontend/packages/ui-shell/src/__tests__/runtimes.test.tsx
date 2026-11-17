import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Role, RuntimeList, RuntimeModule, RuntimeStatus } from '../api/client';
import { useSession } from '../data/session';
import { useWorkbench } from '../data/workbench';
import { SettingsView } from '../shell/SettingsView';
import { RuntimesSettings } from '../shell/settings/RuntimesSettings';
import { render, resetStores, unmountAll } from './harness';

// What `GET /api/v1/runtimes` answers, field for field as `crate::runtimes::RuntimeStatus` and the
// handler in `crates/mythos-core/src/api/runtimes.rs` serialise it.
const RUNNING: RuntimeStatus = {
  id: 'tank',
  module: 'vm',
  program: 'Tank',
  version: 'v1',
  interval_ms: 20,
  mode: 'run',
  reason: null,
  scans: 1520,
  last_scan_us: 12,
  max_scan_us: 340,
  overruns: 2,
  outputs_safe: false,
  outputs: [{ tag: 'external.Plant.P1.speed', variable: 'speed', safe: 7, written: 300, error: null }],
  pid: 4242,
  restarts: 1,
};
const FAULTED: RuntimeStatus = {
  ...RUNNING,
  id: 'mixer',
  program: 'Mixer',
  mode: 'fault',
  reason: 'division by zero in MIXER at instruction 41',
  outputs_safe: true,
  outputs: [{ tag: 'external.Plant.M1.run', variable: 'run', safe: false, written: false, error: null }],
};
const STOPPED: RuntimeStatus = {
  ...RUNNING,
  mode: 'stop',
  reason: null,
  outputs_safe: true,
  outputs: [{ tag: 'external.Plant.P1.speed', variable: 'speed', safe: 7, written: 7, error: null }],
};
// Its module does not answer: the controller holds no state of the program (it sends zeros), no
// process id, and the device has not confirmed the safe value.
const UNREACHABLE: RuntimeStatus = {
  ...RUNNING,
  mode: 'unreachable',
  reason: 'the module did not answer an exchange within 1000 ms',
  scans: 0,
  last_scan_us: 0,
  max_scan_us: 0,
  overruns: 0,
  outputs_safe: false,
  outputs: [
    {
      tag: 'external.Plant.P1.speed',
      variable: 'speed',
      safe: 7,
      written: null,
      error: 'plc did not answer within 300 ms',
    },
  ],
  pid: null,
  restarts: 3,
};
const VM: RuntimeModule = {
  id: 'org.mythos.runtime.vm',
  provides: 'vm',
  name: 'Logic VM runtime',
  version: '0.1.0',
  profile: { name: 'pc', format_version: 1, max_memory: 67108864, max_code: 67108864, refuses: [] },
};

const list = (runtimes: RuntimeStatus[], modules: RuntimeModule[] = [VM], problem: string | null = null): RuntimeList => ({
  runtimes,
  modules,
  problem,
});

interface Call {
  method: string;
  path: string;
}

/** What the next read is answered with: an answer, a failure, or nothing at all. */
type Read = { body: unknown; status?: number } | 'fails' | 'hangs';

// A response as `api` reads it. A real `Response` reads its body on timers of its own, which the
// fake clock of these tests would hold still.
const answer = (body: unknown, status = 200) =>
  ({
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? 'OK' : status === 404 ? 'Not Found' : status === 403 ? 'Forbidden' : 'Error',
    json: async () => body,
  }) as unknown as Response;

function controller(first: Read) {
  const server = {
    read: first,
    calls: [] as Call[],
    /** What a run or stop is answered with. */
    command: (path: string): Response =>
      answer({
        id: path.split('/')[4],
        asked: path.endsWith('/run') ? 'run' : 'stop',
        note: "it takes effect at the runtime's next exchange; read GET /api/v1/runtimes for what it is doing",
      }),
    /** The signals of the reads that were never answered. */
    hung: [] as AbortSignal[],
    reads: () => server.calls.filter((c) => c.method === 'GET' && c.path === '/api/v1/runtimes').length,
  };
  vi.stubGlobal(
    'fetch',
    vi.fn((input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
      const path = String(input);
      const method = init?.method ?? 'GET';
      server.calls.push({ method, path });
      if (method === 'POST') return Promise.resolve(server.command(path));
      if (path !== '/api/v1/runtimes') return Promise.resolve(answer({ error: 'not in this test' }, 404));
      const read = server.read;
      if (read === 'fails') return Promise.reject(new TypeError('connection refused'));
      if (read === 'hangs') {
        return new Promise((_, reject) => {
          const signal = init?.signal;
          if (!signal) return;
          server.hung.push(signal);
          signal.addEventListener('abort', () => reject(signal.reason as Error));
        });
      }
      return Promise.resolve(answer(read.body, read.status));
    })
  );
  return server;
}

const signedIn = (role: Role) =>
  useSession.setState({ phase: 'signed-in', user: { username: role, display_name: role, role, via: 'session' } });

/** Let what is already answered reach the page. */
const settle = () =>
  act(async () => {
    for (let i = 0; i < 12; i++) await Promise.resolve();
  });

/** Let time pass: the polls due in it are made and answered. */
const pass = (ms: number) =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
    for (let i = 0; i < 12; i++) await Promise.resolve();
  });

const all = (view: ParentNode, id: string) => [...view.querySelectorAll<HTMLElement>(`[data-testid="${id}"]`)];
const one = (view: ParentNode, id: string): HTMLElement => {
  const found = all(view, id);
  if (found.length !== 1) throw new Error(`expected one ${id}, found ${found.length}`);
  return found[0]!;
};
const text = (view: ParentNode, id: string) => one(view, id).textContent?.trim();
const button = (view: ParentNode, label: string) =>
  view.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;

async function shown(first: Read, role: Role | null = 'engineer') {
  const server = controller(first);
  if (role) signedIn(role);
  const view = render(<RuntimesSettings />);
  await settle();
  return { server, view };
}

beforeEach(() => {
  resetStores();
  vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval', 'setTimeout', 'clearTimeout', 'Date'] });
});
afterEach(() => {
  unmountAll();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('Settings → Runtimes: what the controller reports', () => {
  it('shows a running runtime: its module, program and version, its scan figures and what its output was last written', async () => {
    const { view } = await shown({ body: list([RUNNING]) });
    const row = one(view, 'runtime-row');
    expect(text(row, 'runtime-id')).toBe('tank');
    expect(text(row, 'runtime-module-of')).toBe('vm');
    expect(text(row, 'runtime-program')).toBe('Tank');
    expect(text(row, 'runtime-version')).toBe('v1');
    expect(text(row, 'runtime-pid')).toBe('4242');
    expect(text(row, 'runtime-restarts')).toBe('1');

    const mode = one(row, 'runtime-mode');
    expect(mode.textContent).toBe('run');
    expect(mode.classList.contains('badge--good')).toBe(true);
    // A program that runs needs no reason, and none is made up for it.
    expect(all(row, 'runtime-reason')).toHaveLength(0);

    expect(text(row, 'runtime-interval')).toBe('20 ms');
    expect(text(row, 'runtime-scans')).toBe('1520');
    expect(text(row, 'runtime-last-scan')).toBe('12 µs');
    expect(text(row, 'runtime-max-scan')).toBe('340 µs');
    expect(text(row, 'runtime-overruns')).toBe('2');

    expect(text(row, 'runtime-output-tag')).toBe('external.Plant.P1.speed');
    expect(text(row, 'runtime-output-variable')).toBe('speed');
    expect(text(row, 'runtime-output-safe')).toBe('7');
    expect(text(row, 'runtime-output-written')).toBe('300');
    expect(all(row, 'runtime-output-error')).toHaveLength(0);
    // Running, its outputs are the program's: the page does not call that unsafe, or safe.
    const safe = one(row, 'runtime-outputs-safe');
    expect(safe.textContent).toBe('written by the program');
    expect(safe.classList.contains('badge')).toBe(false);
    expect(one(view, 'runtimes').getAttribute('data-stale')).toBe('false');
    expect(all(view, 'runtime-stale')).toHaveLength(0);
  });

  it('shows a faulted runtime with the reason the controller gives, and not in the colour of a running one', async () => {
    const { view } = await shown({ body: list([RUNNING, FAULTED]) });
    const [running, faulted] = all(view, 'runtime-row');
    const mode = one(faulted!, 'runtime-mode');
    expect(mode.textContent).toBe('fault');
    expect(mode.classList.contains('badge--bad')).toBe(true);
    expect(mode.classList.contains('badge--good')).toBe(false);
    expect(mode.className).not.toBe(one(running!, 'runtime-mode').className);
    expect(text(faulted!, 'runtime-reason')).toBe('division by zero in MIXER at instruction 41');
    expect(text(faulted!, 'runtime-outputs-safe')).toBe('AT SAFE VALUES');
    // A safe value of `false` is a value, shown as one.
    expect(text(faulted!, 'runtime-output-safe')).toBe('false');
    expect(text(faulted!, 'runtime-output-written')).toBe('false');
  });

  it('shows a stopped runtime whose outputs are at their safe values, and not in the colour of a running one', async () => {
    const { view } = await shown({ body: list([RUNNING, { ...STOPPED, id: 'pump' }]) });
    const [running, stopped] = all(view, 'runtime-row');
    const mode = one(stopped!, 'runtime-mode');
    expect(mode.textContent).toBe('stop');
    expect(mode.classList.contains('badge--good')).toBe(false);
    expect(mode.className).not.toBe(one(running!, 'runtime-mode').className);
    const safe = one(stopped!, 'runtime-outputs-safe');
    expect(safe.textContent).toBe('AT SAFE VALUES');
    expect(safe.getAttribute('data-safe')).toBe('true');
    expect(text(stopped!, 'runtime-output-written')).toBe('7');
    // The controller gave no reason for the stop, and the page does not supply one.
    const reason = one(stopped!, 'runtime-reason');
    expect(reason.textContent).toBe('no reason given');
    expect(reason.getAttribute('data-missing')).toBe('true');
  });

  it('a fault the controller gives no reason for has a reason that is UNKNOWN, not "none"', async () => {
    const { view } = await shown({ body: list([{ ...FAULTED, reason: null }]) });
    expect(text(view, 'runtime-reason')).toBe('reason UNKNOWN');
  });

  it('a stopped runtime that says why shows the reason as given', async () => {
    const { view } = await shown({ body: list([{ ...STOPPED, reason: 'loaded; no exchange yet' }]) });
    const reason = one(view, 'runtime-reason');
    expect(reason.textContent).toBe('loaded; no exchange yet');
    expect(reason.hasAttribute('data-missing')).toBe(false);
  });

  it('a runtime that is not running and whose outputs are not confirmed safe says so, in the colour of a failure', async () => {
    const { view } = await shown({ body: list([UNREACHABLE]) });
    const row = one(view, 'runtime-row');
    const mode = one(row, 'runtime-mode');
    expect(mode.textContent).toBe('unreachable');
    expect(mode.classList.contains('badge--bad')).toBe(true);
    expect(text(row, 'runtime-reason')).toBe('the module did not answer an exchange within 1000 ms');
    const safe = one(row, 'runtime-outputs-safe');
    expect(safe.textContent).toBe('NOT CONFIRMED SAFE');
    expect(safe.classList.contains('badge--bad')).toBe(true);
    expect(text(row, 'runtime-output-error')).toBe('NOT CONFIRMED plc did not answer within 300 ms');
  });

  it('the project’s runtimes not being as configured is said, with the controller’s reason', async () => {
    const { view } = await shown({ body: list([], [VM], "no runtime module provides 'plc-9000' (available: vm)") });
    expect(text(view, 'runtimes-problem')).toBe(
      "The runtimes are not as the project configures them: no runtime module provides 'plc-9000' (available: vm)"
    );
  });
});

describe('Settings → Runtimes: never an invented value', () => {
  it('an output the device has confirmed nothing for is UNKNOWN: not its safe value, not zero', async () => {
    const { view } = await shown({ body: list([UNREACHABLE]) });
    const written = one(view, 'runtime-output-written');
    expect(written.textContent).toBe('UNKNOWN');
    expect(written.getAttribute('data-missing')).toBe('true');
    expect(written.parentElement?.textContent).toContain('last confirmed UNKNOWN (the device has confirmed no write yet)');
    expect(text(view, 'runtime-output-safe')).toBe('7');
  });

  it('scan figures of a runtime whose module does not answer are NOT MEASURED, not the zeros sent in their place', async () => {
    const { view } = await shown({ body: list([UNREACHABLE]) });
    expect(text(view, 'runtime-scans')).toBe('NOT MEASURED');
    expect(text(view, 'runtime-last-scan')).toBe('NOT MEASURED');
    expect(text(view, 'runtime-max-scan')).toBe('NOT MEASURED');
    expect(text(view, 'runtime-overruns')).toBe('NOT MEASURED');
    // The controller says the module has no process: that is said, and no number stands in.
    expect(text(view, 'runtime-pid')).toBe('no process');
    expect(text(view, 'runtime-restarts')).toBe('3');
  });

  it('a program that has not scanned yet has a scan count of 0 and no scan time', async () => {
    const loaded = { ...STOPPED, reason: 'loaded; no exchange yet', scans: 0, last_scan_us: 0, max_scan_us: 0, overruns: 0 };
    const { view } = await shown({ body: list([loaded]) });
    expect(text(view, 'runtime-scans')).toBe('0');
    expect(text(view, 'runtime-last-scan')).toBe('NOT MEASURED');
    expect(text(view, 'runtime-max-scan')).toBe('NOT MEASURED');
    expect(text(view, 'runtime-overruns')).toBe('NOT MEASURED');
  });

  it('a runtime the controller says almost nothing of shows UNKNOWN and NOT MEASURED, never a default', async () => {
    // Only an id: what an answer without the other fields would be.
    const bare = { id: 'bare' } as unknown as RuntimeStatus;
    const { view } = await shown({ body: list([bare]) });
    const row = one(view, 'runtime-row');
    expect(text(row, 'runtime-id')).toBe('bare');
    const mode = one(row, 'runtime-mode');
    expect(mode.textContent).toBe('UNKNOWN');
    expect(mode.getAttribute('data-mode')).toBe('unknown');
    expect(mode.classList.contains('badge--good')).toBe(false);
    expect(text(row, 'runtime-reason')).toBe('reason UNKNOWN');
    for (const id of ['runtime-module-of', 'runtime-program', 'runtime-version', 'runtime-pid', 'runtime-restarts', 'runtime-interval', 'runtime-outputs']) {
      expect(text(row, id), id).toBe('UNKNOWN');
    }
    for (const id of ['runtime-scans', 'runtime-last-scan', 'runtime-max-scan', 'runtime-overruns']) {
      expect(text(row, id), id).toBe('NOT MEASURED');
    }
  });

  it('an output without a tag, a variable or a safe value shows UNKNOWN for each', async () => {
    const odd = { ...STOPPED, outputs_safe: undefined, outputs: [{}] } as unknown as RuntimeStatus;
    const { view } = await shown({ body: list([odd]) });
    for (const id of ['runtime-output-tag', 'runtime-output-variable', 'runtime-output-safe', 'runtime-output-written', 'runtime-outputs-safe']) {
      expect(text(view, id), id).toBe('UNKNOWN');
    }
  });

  it('a mode this page does not know is shown as reported, and not as a healthy one', async () => {
    const { view } = await shown({ body: list([RUNNING, { ...RUNNING, id: 'other', mode: 'paused' }]) });
    const [running, other] = all(view, 'runtime-row');
    const mode = one(other!, 'runtime-mode');
    expect(mode.textContent).toBe('paused');
    expect(mode.classList.contains('badge--good')).toBe(false);
    expect(mode.className).not.toBe(one(running!, 'runtime-mode').className);
    // Not running, with outputs not confirmed safe.
    expect(text(other!, 'runtime-outputs-safe')).toBe('NOT CONFIRMED SAFE');
  });

  it('a runtime with no output does not claim its outputs are safe', async () => {
    const { view } = await shown({ body: list([{ ...STOPPED, outputs: [], outputs_safe: true }]) });
    expect(text(view, 'runtime-outputs')).toBe('writes no field tag');
    expect(all(view, 'runtime-outputs-safe')).toHaveLength(0);
  });
});

describe('Settings → Runtimes: modules and empty states', () => {
  it('lists the runtime modules installed with the profile each declares', async () => {
    const bare: RuntimeModule = { id: 'org.example.board', provides: 'board', name: 'Board runtime', version: '2.0.0', profile: null };
    const { view } = await shown({ body: list([RUNNING], [VM, bare]) });
    const [vm, board] = all(view, 'runtime-module');
    expect(text(vm!, 'runtime-module-name')).toBe('Logic VM runtime');
    expect(text(vm!, 'runtime-module-id')).toBe('org.mythos.runtime.vm');
    expect(text(vm!, 'runtime-module-provides')).toBe('vm');
    expect(text(vm!, 'runtime-module-version')).toBe('0.1.0');
    expect([...one(vm!, 'runtime-module-profile').children].map((c) => c.textContent)).toEqual([
      'name pc',
      'format_version 1',
      'max_memory 67108864',
      'max_code 67108864',
      'refuses none',
    ]);
    // A module that declares no profile: what it can run is not known, and nothing stands in.
    const profile = one(board!, 'runtime-module-profile');
    expect(profile.textContent).toBe('UNKNOWN (the module declares no profile)');
    expect(profile.getAttribute('data-missing')).toBe('true');
  });

  it('with no runtime loaded, says so and names the modules that could run one', async () => {
    const { view } = await shown({ body: list([], [VM]) });
    expect(text(view, 'runtimes-empty')).toBe(
      "No runtime is loaded. A program can be loaded into: vm (a project's runtimes.json, over the API)."
    );
    expect(all(view, 'runtime-row')).toHaveLength(0);
    expect(all(view, 'runtime-module')).toHaveLength(1);
    expect(all(view, 'runtimes-no-module')).toHaveLength(0);
  });

  it('with no runtime module installed, says this node can run no program', async () => {
    const { view } = await shown({ body: list([], []) });
    expect(text(view, 'runtimes-no-module')).toBe(
      'No runtime is loaded, and no runtime module is installed, so this node can run no program.'
    );
    expect(text(view, 'runtime-modules-empty')).toBe('No runtime module is installed.');
    expect(all(view, 'runtimes-empty')).toHaveLength(0);
  });
});

describe('Settings → Runtimes: who may run and stop', () => {
  it.each(['viewer', 'operator'] as const)('a %s reads everything, has no enabled command and is told who may', async (role) => {
    const { view } = await shown({ body: list([RUNNING]) }, role);
    expect(text(view, 'runtime-id')).toBe('tank');
    expect(button(view, 'Run tank').disabled).toBe(true);
    expect(button(view, 'Stop tank').disabled).toBe(true);
    expect([...view.querySelectorAll('button')].filter((b) => !b.disabled)).toHaveLength(0);
    expect(text(view, 'runtimes-who-may')).toBe('Only an engineer or an administrator runs or stops a program.');
  });

  it.each(['engineer', 'admin'] as const)('an %s may run and stop', async (role) => {
    const { view } = await shown({ body: list([RUNNING]) }, role);
    expect(button(view, 'Run tank').disabled).toBe(false);
    expect(button(view, 'Stop tank').disabled).toBe(false);
    expect(all(view, 'runtimes-who-may')).toHaveLength(0);
  });

  it('on a controller without accounts everything is open', async () => {
    const { view } = await shown({ body: list([RUNNING]) }, null);
    expect(button(view, 'Stop tank').disabled).toBe(false);
    expect(all(view, 'runtimes-who-may')).toHaveLength(0);
  });

  it('a runtime the controller did not name cannot be commanded', async () => {
    const { view } = await shown({ body: list([{ ...RUNNING, id: undefined } as unknown as RuntimeStatus]) });
    expect(text(view, 'runtime-id')).toBe('UNKNOWN');
    expect(button(view, 'Stop UNKNOWN').disabled).toBe(true);
    expect(button(view, 'Run UNKNOWN').disabled).toBe(true);
  });

  it('the section is in Settings for a viewer', async () => {
    controller({ body: list([RUNNING]) });
    signedIn('viewer');
    useWorkbench.setState({ view: 'settings', settingsSection: 'runtimes' });
    const view = render(<SettingsView />);
    await settle();
    const entry = [...view.querySelectorAll('nav[aria-label="Settings"] button')].find((b) => b.textContent?.trim() === 'Runtimes');
    expect(entry?.getAttribute('aria-current')).toBe('true');
    expect(text(view, 'runtime-id')).toBe('tank');
  });
});

describe('Settings → Runtimes: run and stop', () => {
  it('Stop asks the controller, shows its answer and reads the list again', async () => {
    const { server, view } = await shown({ body: list([RUNNING]) });
    const before = server.reads();
    server.read = { body: list([STOPPED]) };
    act(() => button(view, 'Stop tank').click());
    await settle();
    expect(server.calls.filter((c) => c.method === 'POST')).toEqual([{ method: 'POST', path: '/api/v1/runtimes/tank/stop' }]);
    expect(text(view, 'runtime-command-answer')).toBe(
      "The controller answered for tank: asked stop; it takes effect at the runtime's next exchange; read GET /api/v1/runtimes for what it is doing"
    );
    // Read again at once, without waiting for the next poll: the row is what the controller now says.
    expect(server.reads()).toBe(before + 1);
    expect(text(view, 'runtime-mode')).toBe('stop');
    expect(all(view, 'runtime-command-error')).toHaveLength(0);
  });

  it('Run asks first, and sends nothing when declined', async () => {
    const { server, view } = await shown({ body: list([STOPPED]) });
    const asked: string[] = [];
    vi.stubGlobal('confirm', (message: string) => (asked.push(message), false));
    act(() => button(view, 'Run tank').click());
    await settle();
    expect(asked).toHaveLength(1);
    expect(asked[0]).toContain('cold start');
    expect(server.calls.filter((c) => c.method === 'POST')).toHaveLength(0);

    vi.stubGlobal('confirm', () => true);
    act(() => button(view, 'Run tank').click());
    await settle();
    expect(server.calls.filter((c) => c.method === 'POST')).toEqual([{ method: 'POST', path: '/api/v1/runtimes/tank/run' }]);
    expect(text(view, 'runtime-command-answer')).toContain('asked run');
  });

  it('a command the controller refuses shows its error, and the list is still read again', async () => {
    const { server, view } = await shown({ body: list([RUNNING]) });
    server.command = () => answer({ error: "no runtime 'tank' is loaded on this node" }, 404);
    const before = server.reads();
    act(() => button(view, 'Stop tank').click());
    await settle();
    const error = one(view, 'runtime-command-error');
    expect(error.getAttribute('role')).toBe('alert');
    expect(error.textContent).toBe("Stop was not taken for tank: 404 Not Found: no runtime 'tank' is loaded on this node");
    expect(all(view, 'runtime-command-answer')).toHaveLength(0);
    expect(server.reads()).toBe(before + 1);
    // The command did not change what the page says the runtime does.
    expect(text(view, 'runtime-mode')).toBe('run');
  });

  it('a command not yet answered is said, and does not hold back Stop', async () => {
    const { server, view } = await shown({ body: list([STOPPED]) });
    let answerRun: (response: Response) => void = () => undefined;
    const original = server.command;
    server.command = (path) => (path.endsWith('/run') ? (new Promise<Response>((resolve) => (answerRun = resolve)) as unknown as Response) : original(path));
    vi.stubGlobal('confirm', () => true);
    act(() => button(view, 'Run tank').click());
    await settle();
    expect(text(view, 'runtime-command-asking')).toBe('Asked the controller to run tank; it has not answered yet.');
    expect(all(view, 'runtime-command-answer')).toHaveLength(0);
    expect(button(view, 'Run tank').disabled).toBe(true);
    expect(button(view, 'Stop tank').disabled).toBe(false);

    act(() => button(view, 'Stop tank').click());
    await settle();
    expect(server.calls.filter((c) => c.method === 'POST').map((c) => c.path)).toEqual([
      '/api/v1/runtimes/tank/run',
      '/api/v1/runtimes/tank/stop',
    ]);
    expect(text(view, 'runtime-command-answer')).toContain('asked stop');
    expect(all(view, 'runtime-command-asking')).toHaveLength(0);
    answerRun(original('/api/v1/runtimes/tank/run'));
    await settle();
  });

  it('an answer without what was asked does not have it filled in', async () => {
    const { server, view } = await shown({ body: list([RUNNING]) });
    server.command = () => answer({});
    act(() => button(view, 'Stop tank').click());
    await settle();
    expect(text(view, 'runtime-command-answer')).toBe('The controller answered for tank: asked UNKNOWN');
  });
});

describe('Settings → Runtimes: polling, and an answer that is no longer current', () => {
  it('reads about once a second while shown, and stops when it is not', async () => {
    const { server } = await shown({ body: list([RUNNING]) });
    expect(server.reads()).toBe(1);
    await pass(3000);
    expect(server.reads()).toBe(4);
    unmountAll();
    await pass(5000);
    expect(server.reads()).toBe(4);
  });

  it('a later answer with another mode is what is shown', async () => {
    const { server, view } = await shown({ body: list([RUNNING]) });
    expect(text(view, 'runtime-mode')).toBe('run');
    expect(text(view, 'runtime-scans')).toBe('1520');
    server.read = { body: list([{ ...FAULTED, id: 'tank', scans: 1533 }]) };
    await pass(1000);
    const mode = one(view, 'runtime-mode');
    expect(mode.textContent).toBe('fault');
    expect(mode.classList.contains('badge--bad')).toBe(true);
    expect(text(view, 'runtime-reason')).toBe('division by zero in MIXER at instruction 41');
    expect(text(view, 'runtime-scans')).toBe('1533');
    expect(text(view, 'runtime-outputs-safe')).toBe('AT SAFE VALUES');
    // And a runtime that is unloaded leaves the page.
    server.read = { body: list([]) };
    await pass(1000);
    expect(all(view, 'runtime-row')).toHaveLength(0);
    expect(all(view, 'runtimes-empty')).toHaveLength(1);
  });

  it('a failed read is said, the rows become the last known state with their age, and a good read clears it', async () => {
    const { server, view } = await shown({ body: list([RUNNING]) });
    expect(all(view, 'runtimes-read-error')).toHaveLength(0);

    server.read = 'fails';
    await pass(1000);
    const error = one(view, 'runtimes-read-error');
    expect(error.getAttribute('role')).toBe('alert');
    expect(error.textContent).toBe('The runtimes could not be read: The controller did not answer (connection refused)');
    expect(text(view, 'runtimes-stale')).toBe(
      'What is shown below is the last known state, read 1 s ago. It is not what the runtimes are doing now.'
    );
    const row = one(view, 'runtime-row');
    expect(row.getAttribute('data-stale')).toBe('true');
    expect(text(row, 'runtime-stale')).toBe('LAST KNOWN · 1 s ago');
    expect(one(view, 'runtimes').getAttribute('data-stale')).toBe('true');
    // Still the last answer, and no longer in the colour of a program that runs.
    const mode = one(row, 'runtime-mode');
    expect(mode.textContent).toBe('run');
    expect(mode.classList.contains('badge--good')).toBe(false);

    await pass(2000);
    expect(text(view, 'runtime-stale')).toBe('LAST KNOWN · 3 s ago');

    server.read = { body: list([{ ...RUNNING, scans: 1700 }]) };
    await pass(1000);
    expect(all(view, 'runtimes-read-error')).toHaveLength(0);
    expect(all(view, 'runtimes-stale')).toHaveLength(0);
    expect(all(view, 'runtime-stale')).toHaveLength(0);
    expect(one(view, 'runtime-row').getAttribute('data-stale')).toBe('false');
    expect(one(view, 'runtime-mode').classList.contains('badge--good')).toBe(true);
    expect(text(view, 'runtime-scans')).toBe('1700');
  });

  it('a read that is never answered makes the rows the last known state too, and is given up so that reading resumes', async () => {
    const { server, view } = await shown({ body: list([RUNNING]) });
    server.read = 'hangs';
    await pass(3000);
    // Three seconds without a newer answer is still within what the page calls current.
    expect(one(view, 'runtime-row').getAttribute('data-stale')).toBe('false');
    await pass(1000);
    expect(one(view, 'runtime-row').getAttribute('data-stale')).toBe('true');
    expect(text(view, 'runtime-stale')).toBe('LAST KNOWN · 4 s ago');
    expect(one(view, 'runtime-mode').classList.contains('badge--good')).toBe(false);
    // One read is outstanding, not one per second.
    expect(server.hung).toHaveLength(1);
    expect(server.hung[0]!.aborted).toBe(false);

    server.read = { body: list([RUNNING]) };
    await pass(2000);
    expect(server.hung[0]!.aborted).toBe(true);
    expect(one(view, 'runtime-row').getAttribute('data-stale')).toBe('false');
    expect(all(view, 'runtime-stale')).toHaveLength(0);
    expect(all(view, 'runtimes-read-error')).toHaveLength(0);
  });

  it('when the first read fails nothing is said of the runtimes: no row, no "none loaded"', async () => {
    const { server, view } = await shown('fails');
    expect(text(view, 'runtimes-read-error')).toContain('connection refused');
    expect(all(view, 'runtimes')).toHaveLength(0);
    expect(all(view, 'runtimes-empty')).toHaveLength(0);
    expect(all(view, 'runtimes-no-module')).toHaveLength(0);
    // And once the controller answers, what it says is shown.
    server.read = { body: list([RUNNING]) };
    await pass(1000);
    expect(all(view, 'runtimes-read-error')).toHaveLength(0);
    expect(text(view, 'runtime-id')).toBe('tank');
  });

  it('an answer that is not a list of runtimes is a failed read, not an empty list', async () => {
    const { view } = await shown({ body: {} });
    expect(text(view, 'runtimes-read-error')).toBe(
      'The runtimes could not be read: The controller answered, but not with a list of runtimes'
    );
    expect(all(view, 'runtimes-empty')).toHaveLength(0);
    expect(all(view, 'runtimes-no-module')).toHaveLength(0);
  });

  it('a list with an entry that is not a runtime is a failed read too, and the page still stands', async () => {
    const { server, view } = await shown({ body: { runtimes: [RUNNING, null], modules: [VM], problem: null } });
    expect(text(view, 'runtimes-read-error')).toContain('not with a list of runtimes');
    expect(all(view, 'runtime-row')).toHaveLength(0);
    server.read = { body: list([{ ...RUNNING, outputs: [null] } as unknown as RuntimeStatus]) };
    await pass(1000);
    // An output that is not one is shown as something unknown, not left out.
    expect(text(view, 'runtime-output')).toBe('UNKNOWN');
    expect(text(view, 'runtime-id')).toBe('tank');
  });

  it('an empty list that is no longer current says when it was read, not that none is loaded now', async () => {
    const { server, view } = await shown({ body: list([], [VM]) });
    server.read = 'fails';
    await pass(1000);
    expect(text(view, 'runtimes-empty')).toContain('When last read, no runtime was loaded.');
    expect(text(view, 'runtimes-empty')).not.toContain('No runtime is loaded.');
    expect(all(view, 'runtimes-stale')).toHaveLength(1);
  });
});
