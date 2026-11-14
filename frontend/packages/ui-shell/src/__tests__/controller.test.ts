import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useController } from '../data/controller';
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
