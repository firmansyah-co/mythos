import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { forcedTagsOf, useController } from '../data/controller';
import type { RuntimeStatus } from '../api/client';
import { toProjectId } from '../shell/PortalView';
import { connection, ioStatus, resetStores } from './harness';

function respond(routes: Record<string, unknown>) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const path = String(input);
      if (!(path in routes)) return new Response('{"error":"no such route"}', { status: 404, statusText: 'Not Found' });
      return new Response(JSON.stringify(routes[path]), { status: 200 });
    })
  );
}

beforeEach(resetStores);
afterEach(() => vi.unstubAllGlobals());

describe('reading the controller', () => {
  it('stores what the controller reports', async () => {
    const io = ioStatus([connection('plc1', ['external.A.B.c'])]);
    respond({
      '/api/v1/projects/active': { project_id: 'plant' },
      '/api/v1/io/status': io,
      '/api/v1/tags': [{ uri: 'external.A.B.c', slot_id: 0, description: 'reg 1', engineering_units: '', access_mode: 'READ_ONLY' }],
    });

    await useController.getState().refresh();

    const state = useController.getState();
    expect(state.error).toBeNull();
    expect(state.activeProjectId).toBe('plant');
    expect(state.io).toEqual(io);
    expect(state.tagMeta.get('external.A.B.c')?.description).toBe('reg 1');
  });

  it('keeps a null active project as null, not as unknown', async () => {
    respond({
      '/api/v1/projects/active': { project_id: null },
      '/api/v1/io/status': ioStatus([]),
      '/api/v1/tags': [],
    });
    await useController.getState().refresh();
    expect(useController.getState().activeProjectId).toBeNull();
  });

  it('forgets everything it knew when the controller stops answering', async () => {
    useController.setState({
      activeProjectId: 'plant',
      io: ioStatus([connection('plc1', ['external.A.B.c'])]),
      tagMeta: new Map([['external.A.B.c', { uri: 'external.A.B.c', slot_id: 0, description: '', engineering_units: '', access_mode: 'READ_ONLY' }]]),
    });
    vi.stubGlobal('fetch', vi.fn(async () => { throw new TypeError('Failed to fetch'); }));

    await useController.getState().refresh();

    const state = useController.getState();
    expect(state.error).toContain('did not answer');
    expect(state.io).toBeNull();
    expect(state.activeProjectId).toBeUndefined();
    expect(state.tagMeta.size).toBe(0);
  });

  it('treats one failing endpoint as the controller being unreadable, not as partial truth', async () => {
    respond({
      '/api/v1/projects/active': { project_id: 'plant' },
      '/api/v1/tags': [],
      // /api/v1/io/status is missing: 404
    });
    await useController.getState().refresh();
    expect(useController.getState().error).toContain('404');
    expect(useController.getState().activeProjectId).toBeUndefined();
  });
});

const BASE = {
  '/api/v1/projects/active': { project_id: 'plant' },
  '/api/v1/io/status': ioStatus([connection('plc1', ['external.A.B.c'])]),
  '/api/v1/tags': [],
};

function runtime(id: string, over: Partial<RuntimeStatus>): RuntimeStatus {
  return {
    id,
    module: 'vm',
    program: id,
    version: '1',
    interval_ms: 100,
    mode: 'run',
    reason: null,
    scans: 1,
    last_scan_us: 10,
    max_scan_us: 10,
    overruns: 0,
    outputs_safe: false,
    outputs: [],
    pid: 1,
    restarts: 0,
    ...over,
  };
}

describe('the forced tags', () => {
  it('are every runtime’s forced tags, once each, sorted', async () => {
    respond({
      ...BASE,
      '/api/v1/runtimes': {
        runtimes: [
          runtime('a', { forced: [{ variable: 'x', value: 1 }], forced_tags: ['internal.P.b', 'external.P.a'] }),
          runtime('b', { forced: [{ variable: 'y', value: true }], forced_tags: ['external.P.a', 'external.P.c'] }),
          runtime('c', { forced: [], forced_tags: [] }),
        ],
        modules: [],
        problem: null,
      },
    });
    await useController.getState().refresh();
    expect(useController.getState().forcedTags).toEqual(['external.P.a', 'external.P.c', 'internal.P.b']);
  });

  it('are none when the controller says no runtime forces anything', async () => {
    respond({ ...BASE, '/api/v1/runtimes': { runtimes: [runtime('a', { forced: [], forced_tags: [] })], modules: [], problem: null } });
    await useController.getState().refresh();
    expect(useController.getState().forcedTags).toEqual([]);
  });

  it('are not known when the runtimes cannot be read, and the rest of the controller is kept', async () => {
    useController.setState({ forcedTags: ['external.P.a'] });
    respond(BASE); // /api/v1/runtimes answers 404
    await useController.getState().refresh();
    const state = useController.getState();
    expect(state.forcedTags).toBeNull();
    expect(state.error).toBeNull();
    expect(state.activeProjectId).toBe('plant');
    expect(state.io).not.toBeNull();
  });

  it('are not known when the runtimes answer something that is not a list of them', async () => {
    useController.setState({ forcedTags: ['external.P.a'] });
    respond({ ...BASE, '/api/v1/runtimes': { runtimes: 'none' } });
    await useController.getState().refresh();
    expect(useController.getState().forcedTags).toBeNull();
  });

  it('are not known when the controller cannot be reached', async () => {
    useController.setState({ forcedTags: ['external.P.a'] });
    vi.stubGlobal('fetch', vi.fn(async () => { throw new TypeError('Failed to fetch'); }));
    await useController.getState().refresh();
    expect(useController.getState().forcedTags).toBeNull();
  });

  it('are not known from a server that does not say them, rather than none', () => {
    const older = runtime('a', {});
    delete older.forced_tags;
    expect(forcedTagsOf([older])).toBeNull();
    expect(forcedTagsOf([runtime('b', { forced_tags: ['external.P.a'] }), older])).toBeNull();
  });

  it('take nothing from a runtime whose module does not answer, whose forces are not known', () => {
    const silent = runtime('a', { mode: 'unreachable', forced: null, forced_tags: [] });
    expect(forcedTagsOf([silent, runtime('b', { forced: [], forced_tags: ['external.P.a'] })])).toEqual(['external.P.a']);
  });
});

describe('deriving a project id from a name', () => {
  it.each([
    ['Boiler House', 'boiler_house'],
    ['  Water — Intake #2  ', 'water_intake_2'],
    ['../../etc/passwd', 'etc_passwd'],
    ['ÅÄÖ', ''],
  ])('%o becomes %o', (name, id) => {
    expect(toProjectId(name)).toBe(id);
  });
});
