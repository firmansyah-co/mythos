import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useController } from '../data/controller';
import { formatLocation, parseLocation } from '../data/location';
import { screenToShow, useRuntime } from '../data/runtime';
import { goOffline, useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';
import { RuntimeView } from '../shell/RuntimeView';
import { connection, ioStatus, project, render, resetStores, sample, unmountAll } from './harness';

const TAG = 'external.BoilerHouse.Unit1.steam_temp';
const FIELD = { id: 'f1', type: 'io_field', x: 40, y: 80, width: 200, height: 56, tag: TAG, label: 'Steam', unit: '°C' };
const text = (id: string, value: string) => ({ id, type: 'text', x: 8, y: 16, width: 160, height: 32, text: value });
const screenOf = (id: string, name: string, components: unknown[], isRoot = false) => ({
  id,
  name,
  is_root: isRoot,
  canvasData: { width: 1280, height: 720, components },
});

/** A stand-in for the browser's EventSource that the test drives by hand. */
class FakeEventSource {
  static instances: FakeEventSource[] = [];
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  private listeners = new Map<string, Array<(event: { data: string }) => void>>();
  constructor(readonly url: string) {
    FakeEventSource.instances.push(this);
  }
  addEventListener(name: string, listener: (event: { data: string }) => void) {
    this.listeners.set(name, [...(this.listeners.get(name) ?? []), listener]);
  }
  close() {}
  emit(name: string, data: unknown) {
    for (const listener of this.listeners.get(name) ?? []) listener({ data: JSON.stringify(data) });
  }
  static last(): FakeEventSource {
    const instance = FakeEventSource.instances.at(-1);
    if (!instance) throw new Error('no EventSource was opened');
    return instance;
  }
}

interface Backend {
  active: unknown[] | 'fails';
  draft: unknown[];
}

function server(backend: Backend) {
  const reads: string[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const path = String(input);
      reads.push(path);
      if (path.endsWith('view=active')) {
        return backend.active === 'fails'
          ? new Response('{"error":"disk"}', { status: 500, statusText: 'Internal Server Error' })
          : Response.json(backend.active);
      }
      if (path.endsWith('view=draft')) return Response.json(backend.draft);
      return new Response('{}', { status: 404 });
    })
  );
  return { reads, backend };
}

const releasedReads = (reads: string[]) => reads.filter((path) => path.endsWith('view=active')).length;
const flush = () => act(async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); });

async function openRuntime(running = true) {
  useWorkbench.setState({ project: project('plant', 'Boiler House'), view: 'runtime' });
  useController.setState({
    activeProjectId: running ? 'plant' : 'other_project',
    io: ioStatus([connection('plc1', [TAG])]),
  });
  const container = render(<RuntimeView />);
  await flush();
  return container;
}

const texts = (container: HTMLElement) =>
  [...container.querySelectorAll('.textfield')].map((el) => el.textContent);

beforeEach(() => {
  FakeEventSource.instances = [];
  vi.stubGlobal('EventSource', FakeEventSource);
  resetStores();
});

afterEach(() => {
  unmountAll();
  goOffline();
  vi.unstubAllGlobals();
});

describe('what the runtime shows', () => {
  it('shows what was released, not what is being edited', async () => {
    const { reads } = server({
      active: [screenOf('s1', 'Overview', [text('t', 'released text')])],
      draft: [screenOf('s1', 'Overview', [text('t', 'draft text')])],
    });
    const container = await openRuntime();

    expect(texts(container)).toEqual(['released text']);
    expect(reads.some((path) => path.endsWith('view=draft'))).toBe(false);
  });

  it('opens on the root screen and switches when asked', async () => {
    server({
      active: [
        screenOf('s1', 'Detail', [text('t', 'detail')]),
        screenOf('s2', 'Overview', [text('t', 'overview')], true),
      ],
      draft: [],
    });
    const container = await openRuntime();
    expect(texts(container)).toEqual(['overview']);

    const detail = [...container.querySelectorAll('.rt__screen')].find((b) => b.textContent === 'Detail');
    act(() => (detail as HTMLButtonElement).click());
    expect(texts(container)).toEqual(['detail']);
  });

  it('says that nothing has been released instead of showing a blank screen', async () => {
    server({ active: [], draft: [screenOf('s1', 'Overview', [text('t', 'draft text')])] });
    const container = await openRuntime();

    expect(container.querySelector('[data-testid="runtime-nothing-released"]')).not.toBeNull();
    expect(container.querySelector('[data-testid="runtime-canvas"]')).toBeNull();
    expect(container.textContent).not.toContain('draft text');
  });

  it('says the released screens are unreadable instead of showing nothing released', async () => {
    server({ active: 'fails', draft: [] });
    const container = await openRuntime();

    expect(container.querySelector('[data-testid="runtime-unreadable"]')).not.toBeNull();
    expect(container.querySelector('[data-testid="runtime-nothing-released"]')).toBeNull();
  });

  it('tells the operator when part of the screen cannot be drawn', async () => {
    server({
      active: [screenOf('s1', 'Overview', [text('t', 'shown'), { id: 'm', type: 'motor' }, { id: 'v', type: 'valve' }])],
      draft: [],
    });
    const container = await openRuntime();

    expect(container.querySelector('[data-testid="runtime-incomplete"]')?.textContent).toContain('2 elements');
    expect(container.querySelectorAll('[data-testid="runtime-element"]').length).toBe(1);
  });
});

describe('a new release', () => {
  it('is taken up when the controller announces it for this project', async () => {
    const { reads, backend } = server({ active: [screenOf('s1', 'Overview', [text('t', 'first release')])], draft: [] });
    const container = await openRuntime();
    expect(texts(container)).toEqual(['first release']);
    const before = releasedReads(reads);

    backend.active = [screenOf('s1', 'Overview', [text('t', 'second release')])];
    act(() => FakeEventSource.last().emit('SCREENS_RELEASED', { project_id: 'plant', screens: 1 }));
    await flush();

    expect(releasedReads(reads)).toBe(before + 1);
    expect(texts(container)).toEqual(['second release']);
  });

  it('is ignored when it is another project’s', async () => {
    const { reads } = server({ active: [screenOf('s1', 'Overview', [text('t', 'first release')])], draft: [] });
    await openRuntime();
    const before = releasedReads(reads);

    act(() => FakeEventSource.last().emit('SCREENS_RELEASED', { project_id: 'elsewhere', screens: 3 }));
    await flush();
    expect(releasedReads(reads)).toBe(before);
  });

  it('is looked for again after the stream comes back, in case one was announced meanwhile', async () => {
    const { reads, backend } = server({ active: [screenOf('s1', 'Overview', [text('t', 'first release')])], draft: [] });
    const container = await openRuntime();
    act(() => FakeEventSource.last().onopen?.());
    await flush();
    const before = releasedReads(reads);

    act(() => useTelemetry.setState({ stream: 'CLOSED' }));
    backend.active = [screenOf('s1', 'Overview', [text('t', 'released while away')])];
    act(() => useTelemetry.setState({ stream: 'OPEN' }));
    await flush();

    expect(releasedReads(reads)).toBe(before + 1);
    expect(texts(container)).toEqual(['released while away']);
  });

  it('takes the screens off the display when they can no longer be read', async () => {
    const { backend } = server({ active: [screenOf('s1', 'Overview', [text('t', 'first release')])], draft: [] });
    const container = await openRuntime();
    expect(texts(container)).toEqual(['first release']);

    backend.active = 'fails';
    act(() => FakeEventSource.last().emit('SCREENS_RELEASED', { project_id: 'plant', screens: 1 }));
    await flush();

    expect(container.querySelector('[data-testid="runtime-unreadable"]')).not.toBeNull();
    expect(texts(container)).toEqual([]);
    // Not merely hidden by this view: nothing that reads the store can still draw them.
    expect(useRuntime.getState().screens).toEqual([]);
  });
});

describe('what stands between the operator and a current value', () => {
  const field = (container: HTMLElement) => {
    const el = container.querySelector('[data-testid="io-field"]');
    return { state: el?.getAttribute('data-state'), text: el?.textContent ?? '' };
  };
  const good = () =>
    act(() =>
      useTelemetry.setState({
        tags: new Map([[TAG, sample(TAG, { type: 'REAL', value: 56.25 })]]),
        stream: 'OPEN',
        lastBatchAtMs: 1_700_000_000_000,
      })
    );

  it('shows a live value, and when it was last updated, for the running project online', async () => {
    server({ active: [screenOf('s1', 'Overview', [FIELD])], draft: [] });
    const container = await openRuntime();
    good();

    expect(field(container).state).toBe('live');
    expect(field(container).text).toContain('56.25');
    expect(container.querySelector('[data-testid="runtime-stream"]')?.textContent).toBe('ONLINE');
    expect(container.querySelector('[data-testid="runtime-updated"]')?.textContent).toMatch(/^updated \d\d:\d\d:\d\d/);
  });

  it('says the connection is lost, and shows no value and no update time', async () => {
    server({ active: [screenOf('s1', 'Overview', [FIELD])], draft: [] });
    const container = await openRuntime();
    good();
    act(() => useTelemetry.setState({ stream: 'CLOSED', tags: new Map(), lastBatchAtMs: null }));

    expect(container.querySelector('[data-testid="runtime-stream"]')?.textContent).toContain('CONNECTION LOST');
    expect(field(container).state).toBe('offline');
    expect(field(container).text).not.toContain('56.25');
    expect(container.querySelector('[data-testid="runtime-updated"]')?.textContent).toBe('');
  });

  it('says the controller is running another project, and shows none of its values', async () => {
    server({ active: [screenOf('s1', 'Overview', [FIELD])], draft: [] });
    const container = await openRuntime(false);
    good();

    expect(container.querySelector('[data-testid="runtime-not-active"]')?.textContent).toContain('other_project');
    expect(field(container).state).toBe('offline');
    expect(field(container).text).not.toContain('56.25');
  });

  it('says the controller cannot be reached', async () => {
    server({ active: [screenOf('s1', 'Overview', [FIELD])], draft: [] });
    const container = await openRuntime();
    act(() =>
      useController.setState({ activeProjectId: undefined, io: null, error: 'The controller did not answer' })
    );
    expect(container.querySelector('[data-testid="runtime-unreachable"]')?.textContent).toContain('did not answer');
    expect(field(container).state).toBe('offline');
  });
});

describe('addresses and choosing a screen', () => {
  it.each([
    { projectId: 'plant', runtime: {} },
    { projectId: 'plant', runtime: { screenId: 'screen_1' } },
  ])('round-trips the runtime address %o', (location) => {
    expect(parseLocation(formatLocation(location))).toEqual(location);
  });

  it('falls back from a screen that is gone to the root, then to the first', () => {
    const [a, b] = [
      { id: 'a', name: 'A', isRoot: false, canvas: useRuntime.getState().screens as never },
      { id: 'b', name: 'B', isRoot: true, canvas: useRuntime.getState().screens as never },
    ];
    expect(screenToShow([a!, b!], 'a')?.id).toBe('a');
    expect(screenToShow([a!, b!], 'gone')?.id).toBe('b');
    expect(screenToShow([a!], 'gone')?.id).toBe('a');
    expect(screenToShow([], 'gone')).toBeUndefined();
  });
});
