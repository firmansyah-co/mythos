import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useController } from '../data/controller';
import { isScreenDirty, releaseState, useScreens } from '../data/screens';
import { useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';
import { ScreenEditor } from '../editors/ScreenEditor';
import { Inspector } from '../shell/Inspector';
import {
  connection,
  ioStatus,
  loadedIoConfig,
  modbusConnection,
  project,
  render,
  resetStores,
  sample,
  typeInto,
  unmountAll,
} from './harness';

const TAG = 'external.BoilerHouse.Unit1.steam_temp';
const FIELD = { id: 'f1', type: 'io_field', x: 40, y: 80, width: 200, height: 56, tag: TAG, label: 'Steam', unit: '°C' };
const TEXT = { id: 't1', type: 'text', x: 8, y: 16, width: 160, height: 32, text: 'Boiler 1' };
const MOTOR = { id: 'm1', type: 'motor', props: { speed: 1450 } };
const canvasOf = (...components: unknown[]) => ({ width: 1280, height: 720, components });
const screenOf = (id: string, name: string, canvasData: unknown) => ({ id, name, is_root: false, canvasData });

interface Call {
  method: string;
  path: string;
  body: unknown;
}

interface Backend {
  draft: unknown[];
  active: unknown[] | 'fails';
  /** What a write answers. Defaults to success. */
  write?: (call: Call) => Response;
}

function server(backend: Backend) {
  const calls: Call[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      const method = init?.method ?? 'GET';
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as unknown) : undefined;
      const call = { method, path, body };
      calls.push(call);
      if (method !== 'GET') {
        if (backend.write) return backend.write(call);
        if (path.endsWith('/hmi/commit')) return Response.json({});
        if (method === 'POST') return Response.json(screenOf('new1', 'New', { title: 'New', components: [] }), { status: 201 });
        return new Response(null, { status: 204 });
      }
      if (path.endsWith('view=active')) {
        return backend.active === 'fails'
          ? new Response('{"error":"disk"}', { status: 500, statusText: 'Internal Server Error' })
          : Response.json(backend.active);
      }
      if (path.endsWith('view=draft')) return Response.json(backend.draft);
      return new Response('{}', { status: 404 });
    })
  );
  return calls;
}

const writes = (calls: Call[]) => calls.filter((call) => call.method !== 'GET');
const state = () => useScreens.getState();
const doc = (id = 's1') => {
  const found = state().screens.find((screen) => screen.id === id);
  if (!found) throw new Error(`no screen ${id}`);
  return found;
};

/** The controller runs `plant`, one connection provides TAG, and the workbench has it open. */
function runningProject() {
  useWorkbench.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant', io: ioStatus([connection('plc1', [TAG])]) });
  loadedIoConfig('plant', { connections: [modbusConnection('plc1', [TAG])] });
}

beforeEach(() => {
  resetStores();
  runningProject();
});

afterEach(() => {
  unmountAll();
  vi.unstubAllGlobals();
});

describe('loading screens', () => {
  it('an unedited screen is clean, and its release state is read from what was released', async () => {
    server({
      draft: [screenOf('s1', 'Overview', canvasOf(TEXT)), screenOf('s2', 'Detail', canvasOf(FIELD)), screenOf('s3', 'New', canvasOf())],
      active: [screenOf('s1', 'Overview', canvasOf(TEXT)), screenOf('s2', 'Detail', canvasOf())],
    });
    await state().load('plant');

    expect(state().screens.map(isScreenDirty)).toEqual([false, false, false]);
    expect(state().screens.map((s) => releaseState(s, state().released))).toEqual(['current', 'differs', 'never']);
  });

  it('reports the release state as unknown, not as unreleased, when it cannot be read', async () => {
    server({ draft: [screenOf('s1', 'Overview', canvasOf(TEXT))], active: 'fails' });
    await state().load('plant');

    expect(state().phase).toBe('ready');
    expect(releaseState(doc(), state().released)).toBe('unknown');
    const container = render(<ScreenEditor screenId="s1" />);
    expect(container.querySelector('[data-testid="release-state"]')?.textContent).toBe('RELEASE STATE UNKNOWN');
  });

  it('says the screens are unreadable instead of showing an empty project', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"error":"disk"}', { status: 500, statusText: 'Internal Server Error' })));
    await state().load('plant');

    expect(state().phase).toBe('unreadable');
    const container = render(<ScreenEditor screenId="s1" />);
    expect(container.querySelector('[data-testid="screens-unreadable"]')).not.toBeNull();
    expect(container.querySelector('[data-testid="screen-canvas"]')).toBeNull();
  });
});

describe('editing and saving', () => {
  it('saves exactly the canvas on screen, with what it could not draw left as it was', async () => {
    const calls = server({ draft: [screenOf('s1', 'Overview', canvasOf(TEXT, MOTOR))], active: [] });
    await state().load('plant');

    const id = state().addElement('s1', 'io_field');
    expect(isScreenDirty(doc())).toBe(true);
    state().updateElement('s1', id!, { tag: TAG });
    expect(await state().save('s1')).toBe(true);

    const [put] = writes(calls);
    expect(put?.method).toBe('PUT');
    expect(put?.path).toBe('/api/v1/projects/plant/hmi/screens/s1/draft');
    const sent = put?.body as { components: Array<Record<string, unknown>> };
    expect(sent.components.slice(0, 2)).toEqual([TEXT, MOTOR]);
    expect(sent.components[2]).toMatchObject({ type: 'io_field', tag: TAG, label: '', unit: '' });
    expect(isScreenDirty(doc())).toBe(false);
  });

  it('tells the engineer about elements it cannot show, by what they say they are', async () => {
    server({ draft: [screenOf('s1', 'Overview', canvasOf(TEXT, MOTOR))], active: [] });
    await state().load('plant');
    const container = render(<ScreenEditor screenId="s1" />);

    const notice = container.querySelector('[data-testid="unknown-elements"]')?.textContent ?? '';
    expect(notice).toContain('1 element');
    expect(notice).toContain('motor');
    expect(container.querySelectorAll('[data-testid="screen-element"]').length).toBe(1);
  });

  it('stays unsaved and says why when the controller does not take the save', async () => {
    server({
      draft: [screenOf('s1', 'Overview', canvasOf(TEXT))],
      active: [],
      write: () => new Response('{"error":"disk full"}', { status: 500, statusText: 'Internal Server Error' }),
    });
    await state().load('plant');
    state().addElement('s1', 'text');

    expect(await state().save('s1')).toBe(false);
    expect(isScreenDirty(doc())).toBe(true);
    expect(state().error).toContain('disk full');
  });

  it('discarding returns the screen to what the controller has', async () => {
    server({ draft: [screenOf('s1', 'Overview', canvasOf(TEXT, MOTOR))], active: [] });
    await state().load('plant');
    state().addElement('s1', 'text');
    state().removeElement('s1', 't1');

    state().discard('s1');
    expect(isScreenDirty(doc())).toBe(false);
    expect(doc().canvas.elements.map((e) => e.id)).toEqual(['t1', 'm1']);
  });

  it('a newly created screen is clean until it is edited', async () => {
    server({ draft: [], active: [] });
    await state().load('plant');
    const id = await state().create('New');
    expect(id).toBe('new1');
    expect(isScreenDirty(doc('new1'))).toBe(false);
  });
});

describe('releasing', () => {
  it('is refused while any screen has unsaved changes, and nothing is sent', async () => {
    const calls = server({ draft: [screenOf('s1', 'A', canvasOf(TEXT)), screenOf('s2', 'B', canvasOf())], active: [] });
    await state().load('plant');
    state().addElement('s2', 'text');

    expect(await state().release()).toBe(false);
    expect(writes(calls)).toEqual([]);
    expect(state().error).toContain('unsaved');
  });

  it('makes the saved screens the released ones', async () => {
    const calls = server({
      draft: [screenOf('s1', 'A', canvasOf(TEXT))],
      active: [],
      write: (call) => (call.path.endsWith('/hmi/commit') ? Response.json({ s1: canvasOf(TEXT) }) : new Response(null, { status: 204 })),
    });
    await state().load('plant');
    expect(releaseState(doc(), state().released)).toBe('never');

    expect(await state().release()).toBe(true);
    expect(writes(calls).map((c) => `${c.method} ${c.path}`)).toEqual(['POST /api/v1/projects/plant/hmi/commit']);
    expect(releaseState(doc(), state().released)).toBe('current');
  });

  it('recognises the released version as current whatever order its keys come back in', async () => {
    // The controller stores objects with sorted keys, so what it returns is the same canvas
    // with every key in a different place from where the workbench put it.
    const sortKeys = (value: unknown): unknown =>
      Array.isArray(value)
        ? value.map(sortKeys)
        : value && typeof value === 'object'
          ? Object.fromEntries(
              Object.entries(value as Record<string, unknown>)
                .sort(([a], [b]) => (a < b ? -1 : 1))
                .map(([key, inner]) => [key, sortKeys(inner)])
            )
          : value;
    let saved: unknown = null;
    let sentAs = '';
    server({
      draft: [screenOf('s1', 'A', canvasOf())],
      active: [],
      write: (call) => {
        if (call.method === 'PUT') {
          sentAs = JSON.stringify(call.body);
          saved = sortKeys(call.body);
          return new Response(null, { status: 204 });
        }
        return Response.json({ s1: saved });
      },
    });
    await state().load('plant');
    const id = state().addElement('s1', 'io_field');
    state().updateElement('s1', id!, { tag: TAG, label: 'Steam', unit: '°C' });
    state().addElement('s1', 'text');
    await state().save('s1');

    // The order really is different from what was sent, so this is not comparing a thing
    // with itself.
    expect(JSON.stringify(saved)).not.toBe(sentAs);

    expect(await state().release()).toBe(true);
    expect(releaseState(doc(), state().released)).toBe('current');
    expect(isScreenDirty(doc())).toBe(false);
  });

  it('an edit that is saved after a release shows the released version as different', async () => {
    server({
      draft: [screenOf('s1', 'A', canvasOf(TEXT))],
      active: [screenOf('s1', 'A', canvasOf(TEXT))],
    });
    await state().load('plant');
    expect(releaseState(doc(), state().released)).toBe('current');

    state().addElement('s1', 'text');
    // Unsaved edits are not what a release would publish, so they do not change the answer.
    expect(releaseState(doc(), state().released)).toBe('current');
    await state().save('s1');
    expect(releaseState(doc(), state().released)).toBe('differs');
  });
});

describe('an I/O field on the canvas', () => {
  async function editorWith(...components: unknown[]) {
    server({ draft: [screenOf('s1', 'Overview', canvasOf(...components))], active: [] });
    await state().load('plant');
    return render(<ScreenEditor screenId="s1" />);
  }
  const field = (container: HTMLElement) => {
    const el = container.querySelector('[data-testid="io-field"]');
    return {
      state: el?.getAttribute('data-state'),
      value: el?.querySelector('[data-testid="tag-value"]')?.textContent ?? null,
      text: el?.textContent ?? '',
    };
  };
  const online = (s = sample(TAG, { type: 'REAL', value: 56.25 })) =>
    act(() => useTelemetry.setState({ tags: new Map([[TAG, s]]), stream: 'OPEN', lastBatchAtMs: 1 }));

  it('shows a GOOD value with its unit', async () => {
    const container = await editorWith(FIELD);
    online();
    expect(field(container)).toMatchObject({ state: 'live', value: '56.25' });
    expect(field(container).text).toContain('°C');
  });

  it('marks a value that can no longer be read as stale, names the fault, and drops the unit', async () => {
    const container = await editorWith(FIELD);
    online(sample(TAG, { type: 'REAL', value: 56.25, qualityName: 'BAD_COMM_FAILURE', qualityCode: 0x18 }));
    const shown = field(container);
    expect(shown.state).toBe('stale');
    expect(shown.text).toContain('BAD COMM FAILURE');
    expect(shown.text).not.toContain('°C');
  });

  it('shows no value while offline', async () => {
    const container = await editorWith(FIELD);
    online();
    act(() => useTelemetry.setState({ stream: 'CLOSED' }));
    expect(field(container)).toMatchObject({ state: 'offline', value: '—' });
    expect(field(container).text).toContain('OFFLINE');
  });

  it('says NOT BOUND for a field with no tag, and shows no value at all', async () => {
    const container = await editorWith({ ...FIELD, tag: '' });
    online();
    expect(field(container)).toMatchObject({ state: 'not-bound', value: null });
    expect(field(container).text).toContain('NOT BOUND');
  });

  it('says NO DATA when the stream is open and does not report the tag', async () => {
    const container = await editorWith(FIELD);
    act(() => useTelemetry.setState({ tags: new Map(), stream: 'OPEN', lastBatchAtMs: 1 }));
    expect(field(container)).toMatchObject({ state: 'no-data', value: '—' });
  });

  it('shows nothing of another project’s tag when this project is not the one running', async () => {
    const container = await editorWith(FIELD);
    // The stream is open and carries a GOOD value under this very tag name — for another project.
    act(() => useController.setState({ activeProjectId: 'other_project' }));
    online();
    const shown = field(container);
    expect(shown).toMatchObject({ state: 'offline', value: '—' });
    expect(shown.text).not.toContain('56.25');
  });
});

describe('element properties', () => {
  async function selected(component: unknown, elementId: string) {
    server({ draft: [screenOf('s1', 'Overview', canvasOf(component))], active: [] });
    await state().load('plant');
    act(() => useWorkbench.getState().select({ kind: 'element', screenId: 's1', elementId }));
    return render(<Inspector collapsed={false} />);
  }
  const element = () => doc().canvas.elements[0] as unknown as Record<string, unknown>;

  it('offers the tags the project’s I/O configuration provides', async () => {
    const container = await selected({ ...FIELD, tag: '' }, 'f1');
    const options = [...container.querySelectorAll('select[aria-label="Tag"] option')].map((o) => o.textContent);
    expect(options).toEqual(['(not bound)', TAG]);

    typeInto(container.querySelector('select[aria-label="Tag"]'), TAG);
    expect(element().tag).toBe(TAG);
  });

  it('flags a field bound to a tag no connection provides', async () => {
    const container = await selected({ ...FIELD, tag: 'external.Gone.Dev.value' }, 'f1');
    expect(container.querySelector('[data-testid="tag-not-configured"]')).not.toBeNull();
  });

  it('applies a typed position only when it is a number, and says so when it is not', async () => {
    const container = await selected(TEXT, 't1');
    const x = () => container.querySelector('input[aria-label="X"]');

    typeInto(x(), 'abc');
    expect(element().x).toBe(8);
    expect(container.querySelector('.field__error')?.textContent).toBe('Must be a whole number');

    typeInto(x(), '');
    expect(element().x).toBe(8);

    typeInto(x(), '80');
    expect(element().x).toBe(80);
    expect(container.querySelector('.field__error')).toBeNull();
  });

  it('refuses a position outside the canvas', async () => {
    const container = await selected(TEXT, 't1');
    typeInto(container.querySelector('input[aria-label="X"]'), '99999');
    expect(element().x).toBe(8);
    expect(container.querySelector('.field__error')?.textContent).toContain('between');
  });
});
