import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { IoConfig } from '../api/client';
import { useController } from '../data/controller';
import { isDirty, useIoConfig } from '../data/ioConfig';
import { useLog } from '../data/log';
import { useWorkbench } from '../data/workbench';
import { ConnectionEditor } from '../editors/ConnectionEditor';
import { DevicesAndNetworks } from '../editors/DevicesAndNetworks';
import {
  connection,
  ioStatus,
  loadedIoConfig,
  modbusConnection,
  project,
  render,
  resetStores,
  unmountAll,
} from './harness';

const TAG = 'external.Plant.PT101.pressure';
const STORED: IoConfig = { connections: [modbusConnection('plc1', [TAG])] };

interface Call {
  method: string;
  path: string;
  body: unknown;
}

/** Stub `fetch`. `put` decides the answer to a save; reads answer with `stored`. */
function server(stored: IoConfig | null, put: (body: unknown) => Response) {
  const calls: Call[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      const method = init?.method ?? 'GET';
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as unknown) : undefined;
      calls.push({ method, path, body });
      if (method === 'PUT') return put(body);
      if (path.endsWith('/io/drivers')) return Response.json(['modbus']);
      if (path.endsWith('/io')) return Response.json({ config: stored });
      if (path.endsWith('/projects/active')) return Response.json({ project_id: 'plant' });
      if (path.endsWith('/io/status')) return Response.json(ioStatus([]));
      if (path.endsWith('/tags')) return Response.json([]);
      return new Response('{}', { status: 404 });
    })
  );
  return calls;
}

const puts = (calls: Call[]) => calls.filter((call) => call.method === 'PUT');
const state = () => useIoConfig.getState();
const edit = (change: (id: string) => string) =>
  state().updateConnection(state().draft[0]!.key, (c) => ({ ...c, id: change(c.id) }));

beforeEach(() => {
  resetStores();
  useWorkbench.setState({ project: project('plant'), view: 'project' });
});

afterEach(() => {
  unmountAll();
  vi.unstubAllGlobals();
});

describe('loading', () => {
  it('reads the stored configuration into an unchanged draft', async () => {
    server(STORED, () => Response.json({}));
    await state().load('plant');

    expect(state().phase).toBe('ready');
    expect(state().saved).toEqual(STORED);
    expect(state().draft.map((c) => c.id)).toEqual(['plc1']);
    expect(isDirty(state())).toBe(false);
    expect(state().drivers).toEqual(['modbus']);
  });

  it('a project with no configuration loads as an empty, unchanged draft', async () => {
    server(null, () => Response.json({}));
    await state().load('plant');
    expect(state().saved).toBeNull();
    expect(state().draft).toEqual([]);
    expect(isDirty(state())).toBe(false);
  });

  it('says the configuration is unreadable instead of showing an empty one', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"error":"disk"}', { status: 500, statusText: 'Internal Server Error' })));
    await state().load('plant');

    expect(state().phase).toBe('unreadable');
    expect(state().loadError).toContain('500');

    const container = render(<DevicesAndNetworks />);
    expect(container.querySelector('[data-testid="config-unreadable"]')).not.toBeNull();
    expect(container.querySelector('[data-testid="no-connections"]')).toBeNull();
  });
});

describe('editing', () => {
  it('is dirty after a change and clean again after discarding it', async () => {
    server(STORED, () => Response.json({}));
    await state().load('plant');

    edit(() => 'renamed');
    expect(isDirty(state())).toBe(true);

    state().discard();
    expect(isDirty(state())).toBe(false);
    expect(state().draft.map((c) => c.id)).toEqual(['plc1']);
  });

  it('changing a field back to what it was is not a change', async () => {
    server(STORED, () => Response.json({}));
    await state().load('plant');
    edit(() => 'other');
    edit(() => 'plc1');
    expect(isDirty(state())).toBe(false);
  });
});

describe('saving', () => {
  it('sends the configuration, and the draft becomes the saved state', async () => {
    const calls = server(STORED, () => Response.json({ applied: true }));
    await state().load('plant');
    edit(() => 'boiler-plc');

    expect(await state().save()).toBe(true);

    const [put] = puts(calls);
    expect(put?.path).toBe('/api/v1/projects/plant/io');
    expect(put?.body).toEqual({ connections: [{ ...STORED.connections[0], id: 'boiler-plc' }] });
    expect(isDirty(state())).toBe(false);
    expect(state().saved?.connections[0]?.id).toBe('boiler-plc');
    expect(useLog.getState().entries.at(-1)?.text).toContain('applied');
  });

  it('says so when the controller saved it without applying it', async () => {
    server(STORED, () => Response.json({ applied: false }));
    await state().load('plant');
    edit(() => 'x');
    await state().save();
    expect(useLog.getState().entries.at(-1)?.text).toContain('not applied');
  });

  it('sends nothing when a field is not valid, and reports the field', async () => {
    const calls = server(STORED, () => Response.json({ applied: true }));
    await state().load('plant');
    state().updateConnection(state().draft[0]!.key, (c) => ({ ...c, pollIntervalMs: 'fast' }));

    expect(await state().save()).toBe(false);

    expect(puts(calls)).toEqual([]);
    expect(state().fieldErrors.map((e) => e.field)).toEqual(['pollIntervalMs']);
    expect(isDirty(state())).toBe(true);
  });

  it('a field error goes as soon as its field is corrected, and the others stay', async () => {
    server(null, () => Response.json({ applied: true }));
    await state().load('plant');
    const key = state().addConnection();
    await state().save();
    expect(state().fieldErrors.map((e) => e.field).sort()).toEqual([
      'address',
      'host',
      'id',
      'pollIntervalMs',
      'uri',
    ]);

    state().updateConnection(key, (c) => ({ ...c, id: 'plc1' }));
    expect(state().fieldErrors.map((e) => e.field).sort()).toEqual([
      'address',
      'host',
      'pollIntervalMs',
      'uri',
    ]);

    // And comes back if the field is emptied again.
    state().updateConnection(key, (c) => ({ ...c, id: '' }));
    expect(state().fieldErrors.map((e) => e.field)).toContain('id');
  });

  it('shows no field errors while typing before any save was attempted', async () => {
    server(null, () => Response.json({ applied: true }));
    await state().load('plant');
    state().addConnection();
    expect(state().fieldErrors).toEqual([]);
  });

  it('the controller’s reasons are cleared once the draft is no longer what was refused', async () => {
    server(STORED, () =>
      Response.json(
        { error: 'refused', problems: [{ connection_id: 'renamed', message: 'no' }] },
        { status: 400 }
      )
    );
    await state().load('plant');
    edit(() => 'renamed');
    await state().save();
    expect(state().problems).toHaveLength(1);

    edit(() => 'renamed-again');
    expect(state().problems).toEqual([]);
  });

  it('keeps the draft and the saved state when the controller refuses it, and shows why', async () => {
    server(STORED, () =>
      Response.json(
        {
          error: 'the I/O configuration has problems and was not saved',
          problems: [{ connection_id: 'renamed', message: "tag 'external.Plant.PT101.pressure' is already provided by connection 'other'" }],
        },
        { status: 400 }
      )
    );
    await state().load('plant');
    edit(() => 'renamed');

    expect(await state().save()).toBe(false);

    expect(state().saved).toEqual(STORED);
    expect(state().draft[0]?.id).toBe('renamed');
    expect(isDirty(state())).toBe(true);
    expect(state().problems).toHaveLength(1);

    const container = render(<DevicesAndNetworks />);
    const shown = container.querySelector('[data-testid="server-problems"]')?.textContent ?? '';
    expect(shown).toContain('nothing was saved');
    expect(shown).toContain('already provided by connection');
  });

  it('reports a controller that does not answer as not saved', async () => {
    server(STORED, () => {
      throw new TypeError('Failed to fetch');
    });
    await state().load('plant');
    edit(() => 'x');

    expect(await state().save()).toBe(false);
    expect(state().saveError).toContain('did not answer');
    expect(state().saved).toEqual(STORED);
    expect(isDirty(state())).toBe(true);
  });
});

describe('devices & networks', () => {
  it('shows the controller state only for connections it has loaded', () => {
    act(() => {
      loadedIoConfig('plant', STORED);
      useController.setState({ activeProjectId: 'plant', io: ioStatus([connection('plc1', [TAG])]) });
    });
    // A second connection exists only in the draft.
    act(() => {
      const key = state().addConnection();
      state().updateConnection(key, (c) => ({ ...c, id: 'new-plc' }));
    });

    const container = render(<DevicesAndNetworks />);
    const rows = [...container.querySelectorAll('[data-testid="connection-row"]')].map((row) =>
      [...row.querySelectorAll('td')].map((cell) => cell.textContent)
    );
    expect(rows.map((cells) => [cells[0], cells[5]])).toEqual([
      ['plc1', 'RUNNING'],
      ['new-plc', 'NOT LOADED'],
    ]);
    expect(container.querySelector('[data-testid="unsaved"]')).not.toBeNull();
  });

  it('names a connection that has no name yet instead of showing a blank row', () => {
    act(() => {
      loadedIoConfig('plant', null);
      state().addConnection();
    });
    const container = render(<DevicesAndNetworks />);
    const first = container.querySelector('[data-testid="connection-row"] td')?.textContent;
    expect(first).toBe('(unnamed connection)');
  });
});

describe('the connection editor', () => {
  it('shows each field error beside its field after a refused save', async () => {
    server(null, () => Response.json({ applied: true }));
    await state().load('plant');
    let key = '';
    act(() => {
      key = state().addConnection();
    });
    await act(async () => {
      await state().save();
    });

    const container = render(<ConnectionEditor connectionKey={key} />);
    const messages = [...container.querySelectorAll('.field__error')].map((el) => el.textContent);
    expect(messages).toContain('A name is required');
    expect(messages).toContain('Poll interval is required');
    expect(messages).toContain('A host is required');
    expect(messages.some((m) => m?.includes('A tag name is required') && m.includes('Address is required'))).toBe(true);
  });

  it('shows a configuration it has no form for as stored, and does not offer to edit it', () => {
    act(() =>
      loadedIoConfig('plant', {
        connections: [{ id: 'opc', driver: 'opcua', poll_interval_ms: 1000, config: { endpoint: 'opc.tcp://plant' } }],
      })
    );
    const container = render(<ConnectionEditor connectionKey={state().draft[0]!.key} />);
    const raw = container.querySelector('[data-testid="raw-config"]');
    expect(raw?.textContent).toContain('opc.tcp://plant');
    expect(container.querySelector('[data-testid="mapping-row"]')).toBeNull();
    expect(container.querySelector('input[aria-label="Host"]')).toBeNull();
  });

  it('says the connection is gone when its row was removed', () => {
    act(() => loadedIoConfig('plant', STORED));
    const key = state().draft[0]!.key;
    const container = render(<ConnectionEditor connectionKey={key} />);
    expect(container.querySelector('[data-testid="connection-editor"]')).not.toBeNull();

    act(() => state().removeConnection(key));
    expect(container.querySelector('[data-testid="connection-editor"]')).toBeNull();
    expect(container.textContent).toContain('no longer in the configuration');
  });
});
