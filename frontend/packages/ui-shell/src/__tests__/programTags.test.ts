import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useProgramTags } from '../data/programTags';
import { controllerView } from '../modules/host';
import { loadedIoConfig, modbusConnection, resetStores } from './harness';

const TAG = 'external.Plant.LT1.level';
const SETPOINT = 'internal.Plant.Tank.setpoint';
const SPEED = 'internal.Plant.Tank.speed';

function runtimes(answer: () => Response) {
  const calls: string[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      calls.push(String(input));
      return answer();
    })
  );
  return calls;
}

const config = (...published: string[][]) =>
  Response.json({
    config: {
      instances: published.map((tags, i) => ({
        id: `p${i}`,
        program: { published: tags.map((tag) => ({ variable: 'x', tag })) },
      })),
    },
  });

beforeEach(() => resetStores());
afterEach(() => vi.unstubAllGlobals());

describe("the tags a project's programs publish", () => {
  it('are read from the project’s runtimes configuration, once each, sorted', async () => {
    const calls = runtimes(() => config([SPEED, SETPOINT], [SETPOINT]));
    await useProgramTags.getState().load('plant');
    expect(calls).toEqual(['/api/v1/projects/plant/runtimes']);
    expect(useProgramTags.getState().published).toEqual([SETPOINT, SPEED]);
  });

  it('the tags of every program of a runtime of several programs are offered', async () => {
    const LEVEL_SET = 'internal.Plant.Pump.level';
    runtimes(() =>
      Response.json({
        config: {
          instances: [
            { id: 'alone', program: { published: [{ variable: 'x', tag: SETPOINT }] } },
            {
              id: 'Line',
              programs: [
                { name: 'Tank', published: [{ variable: 'speed', tag: SPEED }] },
                { name: 'Pump', published: [{ variable: 'level', tag: LEVEL_SET }] },
              ],
            },
          ],
        },
      })
    );
    await useProgramTags.getState().load('plant');
    expect(useProgramTags.getState().published).toEqual([LEVEL_SET, SETPOINT, SPEED]);
  });

  it('are none when the project has no runtimes', async () => {
    runtimes(() => Response.json({ config: null }));
    await useProgramTags.getState().load('plant');
    expect(useProgramTags.getState().published).toEqual([]);
  });

  it('stay as last read when a read fails, and say why', async () => {
    let fail = false;
    runtimes(() => (fail ? new Response('{"error":"disk"}', { status: 500, statusText: 'Internal Server Error' }) : config([SETPOINT])));
    await useProgramTags.getState().load('plant');
    fail = true;
    await useProgramTags.getState().load('plant');
    expect(useProgramTags.getState().published).toEqual([SETPOINT]);
    expect(useProgramTags.getState().error).toContain('disk');
  });

  it('of another project are not kept when a project opens', async () => {
    runtimes(() => config([SETPOINT]));
    await useProgramTags.getState().load('plant');
    runtimes(() => new Response('{"error":"down"}', { status: 500, statusText: 'Internal Server Error' }));
    await useProgramTags.getState().load('other');
    expect(useProgramTags.getState().published).toEqual([]);
  });

  it('are what a binding may name, beside the I/O configuration’s, for every module', async () => {
    loadedIoConfig('plant', { connections: [modbusConnection('plc1', [TAG])] });
    runtimes(() => config([SETPOINT]));
    await useProgramTags.getState().load('plant');
    expect(controllerView().configuredTags).toEqual([TAG, SETPOINT].sort((a, b) => a.localeCompare(b)));
  });
});
