// Loading a configured runtime from its editor (C8d-1, R12–R16): saved, checked and built by the
// logic module; the instance of this id, the ids it replaces and every holder of one of its
// programs taken out, a holder that is a configured runtime built again and put into the same
// save; nothing sent to the core's configuration before the engineer agreed to a question that
// names all of it; the core's answer shown as it gave it; and the "Runtime" card, which says what
// the controller lists and never more. Every assertion reads a rendered element by its test id,
// or the requests sent.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { RuntimesAnswer, RuntimeStatus } from '../bridge';
import { usePrograms } from '../programs';
import { RUNTIME_EDITOR, type RuntimeEntry } from '../runtimes';
import { RUNTIME_STATUS_OWNER, RuntimeEditor } from '../RuntimeEditor';
import { useRuntimes } from '../runtimestore';
import {
  byTestId,
  click,
  doing,
  endTest,
  entryOf,
  paneOf,
  project,
  render,
  resetStores,
  runtimeOf,
  server,
  settle,
  texts,
  timeScale,
  typeInto,
  until,
  useShell,
  type Backend,
  type Call,
} from './harness';

vi.setConfig({ testTimeout: 20_000 * timeScale });

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
  vi.restoreAllMocks();
});

const program = (name: string) => ({ entry: entryOf(name), source: `PROGRAM ${name}\nEND_PROGRAM\n` });

/** Runtime Line: one cyclic task `fast` running Tank and Pump. */
const LINE: RuntimeEntry = {
  id: 'Line',
  module: 'vm',
  target: 'pc',
  run: true,
  tasks: [{ name: 'fast', kind: { type: 'cyclic', interval_ms: 10, phase_ms: 0 }, priority: 1, programs: ['Tank', 'Pump'] }],
};

/** What the controller lists, with module vm installed. */
const listing = (...runtimes: RuntimeStatus[]): RuntimesAnswer => ({
  runtimes,
  modules: [{ provides: 'vm', profile: { name: 'pc', tasks: { kinds: ['cycle', 'cyclic', 'startup', 'event'] } } }],
  problem: null,
});

const plant = (over: Partial<Backend> = {}): Backend => ({
  programs: [program('Tank'), program('Pump'), program('Mixer'), program('Spare')],
  configured: [LINE],
  runtimes: listing(),
  config: { instances: [] },
  ...over,
});

/** The workbench has `plant` open and an editor on runtime `id`. */
async function opened(backend: Backend, id = 'Line') {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  await settle();
  useShell.getState().openEditor(RUNTIME_EDITOR, id);
  const container = render(<RuntimeEditor id={id} onClose={() => undefined} />);
  await until('the runtime is read', () => byTestId(container, 'runtime-editor') !== null);
  await settle();
  return { calls, container };
}

/** What was asked to change something, in order. */
const writes = (calls: Call[]) =>
  calls
    .filter((call) => call.method !== 'GET')
    .filter((call) => !(call.path.endsWith('/logic/library/check') && JSON.stringify(call.body) === '{}'))
    .map((call) => `${call.method} ${call.path.replace(/^.*\/api\/v1/, '')}`);
/** The saves of the core's runtimes configuration sent. */
const coreSaves = (calls: Call[]) => calls.filter((call) => call.method === 'PUT' && /\/projects\/[^/]+\/runtimes(\?|$)/.test(call.path));
const sentIds = (call: Call | undefined) => ((call?.body as { instances: Array<{ id: string }> } | undefined)?.instances ?? []).map((i) => i.id);

/** Agree (or not) to every question, keeping each one asked and how many core saves were sent before it. */
function answering(calls: Call[], agree: boolean | (() => boolean)) {
  const asked: Array<{ question: string; savesBefore: number }> = [];
  vi.stubGlobal('confirm', (question: string) => {
    asked.push({ question, savesBefore: coreSaves(calls).length });
    return typeof agree === 'function' ? agree() : agree;
  });
  return asked;
}

async function load(container: HTMLElement) {
  click(byTestId(container, 'runtime-load'));
  await until('the load ends', () => useRuntimes.getState().docs.Line?.busy === null);
  await settle();
}

const card = (container: HTMLElement) => {
  const pane = paneOf(container, 'task-cards', RUNTIME_STATUS_OWNER);
  if (!pane) throw new Error('no runtime card');
  return pane;
};

describe('loading a configured runtime', () => {
  it('Load checks, builds and saves the instance in place of its id and the ids it replaces', async () => {
    const spare = { id: 'Spare', module: 'vm', run: true, program: { name: 'Spare', built_from: 'Spare', version: 'v-old' } };
    const tankAlone = { id: 'Tank', module: 'vm', run: true, program: { name: 'Tank', built_from: 'Tank', version: 'v-old' } };
    const oldLine = { id: 'Line', module: 'vm', run: true, program: { name: 'Pump', built_from: 'Pump' } };
    const mixer = { id: 'Mixer', module: 'vm', run: false, program: { name: 'Mixer', built_from: 'Mixer' } };
    const backend = plant({
      config: { instances: [spare, tankAlone, oldLine, mixer] },
      runtimes: listing(runtimeOf('Tank', { version: 'v-old' })),
    });
    const { calls, container } = await opened(backend);
    // A change not saved yet: Load saves it first.
    typeInto(byTestId(container, 'task-priority'), '2');
    await settle();
    const asked = answering(calls, true);
    calls.length = 0;
    await load(container);

    expect(writes(calls)).toEqual([
      'PUT /projects/plant/logic/runtimes/Line',
      'POST /projects/plant/logic/runtimes/Line/check',
      'POST /projects/plant/logic/runtimes/Line/build',
      'PUT /projects/plant/runtimes',
    ]);
    // The stored runtime is checked, not a draft.
    expect(calls.find((c) => c.path.endsWith('/Line/check'))?.body).toEqual({});
    // Asked once, before anything was sent to the core's configuration.
    expect(asked).toHaveLength(1);
    expect(asked[0]?.savesBefore).toBe(0);
    const question = asked[0]?.question ?? '';
    expect(question).toContain('Load runtime Line?');
    expect(question).toContain('- Tank: version v-Tank to load; runs now: version v-old in runtime Tank');
    expect(question).toContain('- Pump: version v-Pump to load; runs now: none: the controller runs no build of it');
    expect(question).toContain('Taken out, replaced by runtime Line: Tank (ran Tank).');
    expect(question).toContain('Taken out, replaced by runtime Line: Line (ran Pump).');
    expect(question).not.toContain('Taken out whole');
    // Every other instance sent back untouched and in its order, then the new one.
    const put = coreSaves(calls)[0];
    expect(sentIds(put)).toEqual(['Spare', 'Mixer', 'Line']);
    const sent = (put?.body as { instances: unknown[] }).instances;
    expect(sent[0]).toEqual(spare);
    expect(sent[1]).toEqual(mixer);
    expect(sent[2]).toMatchObject({ id: 'Line', programs: [{ name: 'Tank', built_from: 'Tank' }, { name: 'Pump', built_from: 'Pump' }] });
    expect(byTestId(container, 'runtime-loaded-said')?.textContent).toBe(
      "Saved into the project's runtimes and applied by the controller. Whether each runtime runs is on its card, as the controller lists it."
    );
    expect(byTestId(container, 'load-refused')).toBeNull();
  });

  it('a program moved from another configured runtime is taken out of it by building that runtime in the same save', async () => {
    // Old still runs Tank (said in another case), though its configuration gave it up.
    const old = { id: 'Old', module: 'vm', run: true, programs: [{ name: 'Mixer', built_from: 'Mixer' }, { name: 'Tank', built_from: 'TANK' }] };
    const other = { id: 'Spare', module: 'vm', run: true, program: { name: 'Spare', built_from: 'Spare' } };
    const backend = plant({
      configured: [LINE, { id: 'Old', programs: ['Mixer'] }],
      config: { instances: [old, other] },
    });
    const { calls, container } = await opened(backend);
    const asked = answering(calls, true);
    calls.length = 0;
    await load(container);

    expect(writes(calls)).toEqual([
      'POST /projects/plant/logic/runtimes/Line/check',
      'POST /projects/plant/logic/runtimes/Line/build',
      'POST /projects/plant/logic/runtimes/Old/build',
      'PUT /projects/plant/runtimes',
    ]);
    const question = asked[0]?.question ?? '';
    expect(question).toContain(
      "Runtime Old gives up the moved program: it is built again from the project's saved programs and restarts warm (not the same programs). It ran Mixer, Tank; it will run Mixer (version v-Mixer)."
    );
    // Both in one save: the rebuilt Old without Tank, the old Old gone.
    const put = coreSaves(calls)[0];
    expect(sentIds(put)).toEqual(['Spare', 'Line', 'Old']);
    expect((put?.body as { instances: unknown[] }).instances[2]).toMatchObject({ id: 'Old', program: { name: 'Mixer', built_from: 'Mixer' } });
    expect(sentIds({ method: 'PUT', path: '', body: backend.config })).toEqual(['Spare', 'Line', 'Old']);
    expect(byTestId(container, 'load-refused')).toBeNull();
  });

  it('an instance of a runtime no longer configured that holds the program is taken out whole, and the question names its programs', async () => {
    const gone = { id: 'Gone', module: 'vm', run: true, programs: [{ name: 'Tank', built_from: 'Tank' }, { name: 'Mixer', built_from: 'Mixer' }] };
    const backend = plant({ config: { instances: [gone] } });
    const { calls, container } = await opened(backend);
    const asked = answering(calls, true);
    calls.length = 0;
    await load(container);

    // Not a configured runtime: not built, taken out.
    expect(writes(calls)).not.toContain('POST /projects/plant/logic/runtimes/Gone/build');
    expect(asked[0]?.question).toContain(
      'Taken out whole, as it is not a configured runtime: Gone (ran Tank, Mixer). Its outputs go to their safe values.'
    );
    expect(sentIds(coreSaves(calls)[0])).toEqual(['Line']);
    expect(texts(container, 'load-taken-out')).toEqual(["Taken out of the project's runtimes: Gone."]);
  });

  it('an instance built before 0.8.0 is said to may also run the program and is kept', async () => {
    const legacy = { id: 'Legacy', module: 'vm', run: true, program: { name: 'Tank' } };
    // Another project program whose source declares PROGRAM Tank: it says what it is a build of.
    const copy = { id: 'Copy', module: 'vm', run: true, program: { name: 'Tank', built_from: 'Copy' } };
    const backend = plant({ config: { instances: [legacy, copy] } });
    const { calls, container } = await opened(backend);
    const asked = answering(calls, true);
    calls.length = 0;
    await load(container);

    const question = asked[0]?.question ?? '';
    expect(question).toContain('Runtime Legacy may also run Tank (built before 0.8.0, not compared by the core). It is kept.');
    expect(question).not.toContain('Runtime Copy may also run');
    const put = coreSaves(calls)[0];
    expect(sentIds(put)).toEqual(['Legacy', 'Copy', 'Line']);
    expect((put?.body as { instances: unknown[] }).instances[0]).toEqual(legacy);
  });

  it('a cancelled Load sends no save', async () => {
    const before = { id: 'Spare', module: 'vm', run: true, program: { name: 'Spare', built_from: 'Spare' } };
    const backend = plant({ config: { instances: [before] } });
    const { calls, container } = await opened(backend);
    const asked = answering(calls, false);
    calls.length = 0;
    await load(container);

    expect(asked).toHaveLength(1);
    expect(coreSaves(calls)).toEqual([]);
    expect(backend.config).toEqual({ instances: [before] });
    expect(byTestId(container, 'runtime-loaded')).toBeNull();
  });

  it('a check that is not ok stops the Load before the build', async () => {
    const backend = plant({
      checkRuntimeProgram: (name) =>
        name === 'Pump'
          ? { ok: false, diagnostics: [{ line: 2, col: 3, end_line: 2, end_col: 6, message: 'x is not declared' }], binding_problems: [] }
          : { ok: true, diagnostics: [], binding_problems: [], version: `v-${name}` },
    });
    const { calls, container } = await opened(backend);
    const asked = answering(calls, true);
    calls.length = 0;
    await load(container);

    expect(writes(calls)).toEqual(['POST /projects/plant/logic/runtimes/Line/check']);
    expect(asked).toEqual([]);
    expect(byTestId(container, 'load-refused')?.querySelector('p')?.textContent).toBe(
      'Runtime Line does not check as saved, so nothing was built or loaded. What the module found is in the Diagnostics tab.'
    );
    expect(byTestId(container, 'runtime-check-state')?.textContent).toBe('DOES NOT CHECK');
  });

  it('a refused build of the other runtime stops the Load and saves nothing', async () => {
    const old = { id: 'Old', module: 'vm', run: true, programs: [{ name: 'Tank', built_from: 'Tank' }, { name: 'Mixer', built_from: 'Mixer' }] };
    // Old's configuration gave up Tank and now runs nothing: the module refuses to build it.
    const backend = plant({ configured: [LINE, { id: 'Old', programs: [] }], config: { instances: [old] } });
    const { calls, container } = await opened(backend);
    const asked = answering(calls, true);
    calls.length = 0;
    await load(container);

    expect(writes(calls)).toEqual([
      'POST /projects/plant/logic/runtimes/Line/check',
      'POST /projects/plant/logic/runtimes/Line/build',
      'POST /projects/plant/logic/runtimes/Old/build',
    ]);
    expect(asked).toEqual([]);
    expect(backend.config).toEqual({ instances: [old] });
    expect(byTestId(container, 'load-refused')?.querySelector('p')?.textContent).toBe(
      'Runtime Old holds a program of Line and was not built again without it, so nothing was loaded.'
    );
    expect(byTestId(container, 'load-refused-error')?.textContent).toBe("runtime 'Old': task 't' runs no program");
  });

  it("the core's refusal shows its error and every problem", async () => {
    const backend = plant({
      configProblems: () => [
        { connection_id: 'runtime:Line', message: "module 'vm' is not running" },
        { connection_id: 'plc1', message: 'tag external.Plant.LT1.level is written by two programs' },
      ],
    });
    const { calls, container } = await opened(backend);
    answering(calls, true);
    await load(container);

    expect(byTestId(container, 'load-refused-error')?.textContent).toBe('the runtimes configuration has problems and was not saved');
    expect(texts(container, 'load-refused-problem')).toEqual([
      "runtime:Line: module 'vm' is not running",
      'plc1: tag external.Plant.LT1.level is written by two programs',
    ]);
    expect(backend.config).toEqual({ instances: [] });
    expect(byTestId(container, 'runtime-loaded')).toBeNull();
  });

  it('not given values are listed with why', async () => {
    const backend = plant({
      starts: () => [
        {
          runtime: 'Line',
          start: 'warm',
          why: 'it runs other programs now',
          carried: null,
          dropped: [],
          initialized: [],
          not_given: [
            { variable: 'Tank.total', why: 'its program was made again under its name' },
            { variable: 'Pump.hours', why: 'it is of another type now' },
          ],
        },
      ],
      warnings: ['Tank.total is RETAIN and was kept for another build of Tank'],
    });
    const { calls, container } = await opened(backend);
    answering(calls, true);
    await load(container);

    const start = container.querySelector('[data-testid="runtime-start"][data-runtime="Line"]');
    expect(start?.querySelector('[data-testid="runtime-start-said"]')?.textContent).toBe(
      'Not changed online: it runs other programs now. Its RETAIN variables start from the values kept for them, except those listed as not given, every other variable from its initial value.'
    );
    expect(texts(container, 'runtime-not-given')).toEqual([
      'Tank.total (its program was made again under its name)',
      '; Pump.hours (it is of another type now)',
    ]);
    expect(texts(container, 'build-warning')).toEqual(['Tank.total is RETAIN and was kept for another build of Tank']);
  });

  it('what did not start is said in the core’s words, for this runtime and not another', async () => {
    const backend = plant({ failedToStart: [['runtime:Line', 'the module exited at start'], ['runtime:Other', 'no such module']] });
    const { calls, container } = await opened(backend);
    answering(calls, true);
    await load(container);

    expect(texts(container, 'load-failed')).toEqual(['runtime:Line: the module exited at start']);
    expect(byTestId(container, 'runtime-loaded-said')?.textContent).toBe(
      "Saved into the project's runtimes and applied, but the controller did not start everything it was given:"
    );
    expect(container.querySelector('[data-testid="runtime-start"][data-runtime="Line"]')).toBeNull();
  });

  it('a load into a project the controller does not run says nothing was started', async () => {
    const backend = plant({ applied: false });
    const { calls, container } = await opened(backend);
    answering(calls, true);
    await load(container);

    expect(byTestId(container, 'runtime-loaded-said')?.textContent).toBe(
      "Saved into the project's runtimes. The controller runs another project, so nothing was started."
    );
    expect(texts(container, 'runtime-start')).toEqual([]);
    expect(sentIds({ method: 'PUT', path: '', body: backend.config })).toEqual(['Line']);
  });
});

describe('the runtime card', () => {
  const mode = (container: HTMLElement) => card(container).querySelector('[data-testid="runtime-status-mode"]')?.textContent;
  const reread = () => doing(() => usePrograms.getState().readRuntimes());

  it('the card says NOT LOADED only after a good read, and UNKNOWN when the runtimes cannot be read', async () => {
    const backend = plant({ runtimes: 'fails' });
    const { container } = await opened(backend);
    expect(mode(container)).toBe('UNKNOWN');
    expect(byTestId(card(container), 'runtime-status-why')?.textContent).toBe(
      'Whether the controller runs runtime Line is not known: 500 Internal Server Error: the runtimes cannot be read.'
    );

    backend.runtimes = listing();
    await reread();
    expect(mode(container)).toBe('NOT LOADED');
    expect(byTestId(card(container), 'runtime-status-why')?.textContent).toBe('The controller lists no runtime Line: it is configured, not loaded.');

    // Read once as not loaded, then not read: not known, never "not loaded" on the old reading.
    backend.runtimes = 'fails';
    await reread();
    expect(mode(container)).toBe('UNKNOWN');

    backend.runtimes = listing(runtimeOf('Line', { program: 'Tank, Pump', version: 'v-Tank, v-Pump' }));
    await reread();
    expect(mode(container)).toBe('RUN');
    expect(byTestId(card(container), 'runtime-status-stale')).toBeNull();

    backend.runtimes = 'fails';
    await reread();
    expect(mode(container)).toBe('RUN');
    expect(byTestId(card(container), 'runtime-status-stale')?.textContent).toBe('LAST KNOWN');
  });

  it('each program’s version that runs is beside the one built, and is not called the same without a build', async () => {
    const backend = plant({
      runtimes: listing(
        runtimeOf('Line', { program: 'Tank, Pump', version: 'v-Tank, v-Pump', programs: [{ name: 'Tank', version: 'v-Tank' }, { name: 'Pump', version: 'old' }] })
      ),
    });
    const { calls, container } = await opened(backend);
    await until('the card lists the programs', () => texts(card(container), 'runtime-program-compare').length === 2);
    expect(texts(card(container), 'runtime-program-compare')).toEqual([
      'not known: nothing of it was built or checked here',
      'not known: nothing of it was built or checked here',
    ]);

    answering(calls, true);
    await load(container);
    expect(texts(card(container), 'runtime-program-compare')).toEqual([
      'the same as the one built here',
      'another version than the one built here (v-Pump)',
    ]);
  });

  it('Stop is offered whenever the runtime is listed; Run asks and names its programs', async () => {
    const backend = plant({
      runtimes: listing(runtimeOf('Line', { program: 'Tank, Pump', programs: [{ name: 'Tank', version: 'a' }, { name: 'Pump', version: 'b' }] })),
    });
    const { calls, container } = await opened(backend);
    const at = card(container);
    const enabled = (id: string) => !(byTestId(at, id) as HTMLButtonElement | null)?.disabled;
    expect(enabled('runtime-card-stop')).toBe(true);
    expect(enabled('runtime-card-run')).toBe(true);
    expect(enabled('runtime-card-unload')).toBe(true);

    const asked = answering(calls, false);
    calls.length = 0;
    click(byTestId(at, 'runtime-card-run'));
    await settle();
    expect(asked.map((a) => a.question)).toEqual([
      'Put runtime Line in RUN? It runs Tank, Pump. From STOP or FAULT that is a warm restart: their RETAIN variables keep the values they had, the others take their initial values, and the programs write their outputs.',
    ]);
    expect(writes(calls)).toEqual([]);

    // Stop is not asked about.
    click(byTestId(at, 'runtime-card-stop'));
    await settle();
    expect(asked).toHaveLength(1);
    expect(writes(calls)).toEqual(['POST /runtimes/Line/stop']);
    expect(byTestId(at, 'runtime-commanded')?.textContent).toBe('Runtime Line was asked to stop.');

    // The runtimes can no longer be read: Stop stays; Run and Unload go.
    backend.runtimes = 'fails';
    await doing(() => usePrograms.getState().readRuntimes());
    expect(enabled('runtime-card-stop')).toBe(true);
    expect(enabled('runtime-card-run')).toBe(false);
    expect(enabled('runtime-card-unload')).toBe(false);

    // Not listed after a good read: none is offered.
    backend.runtimes = listing();
    await doing(() => usePrograms.getState().readRuntimes());
    expect(enabled('runtime-card-stop')).toBe(false);
    expect(enabled('runtime-card-run')).toBe(false);
  });

  it('Unload sends the configuration without the runtime', async () => {
    const a = { id: 'Spare', module: 'vm', run: true, program: { name: 'Spare', built_from: 'Spare' } };
    const line = { id: 'Line', module: 'vm', run: true, programs: [{ name: 'Tank', built_from: 'Tank' }, { name: 'Pump', built_from: 'Pump' }] };
    const b = { id: 'Mixer', module: 'vm', run: true, program: { name: 'Mixer', built_from: 'Mixer' } };
    const backend = plant({ config: { instances: [a, line, b] }, runtimes: listing(runtimeOf('Line', { program: 'Tank, Pump' })) });
    const { calls, container } = await opened(backend);
    let agree = false;
    const asked = answering(calls, () => agree);
    calls.length = 0;
    click(byTestId(card(container), 'runtime-card-unload'));
    await settle();
    expect(asked[0]?.question).toBe(
      "Take runtime Line out of the project's runtimes? It stops running Tank, Pump, and their outputs go to their safe values. Its configuration in the logic module is kept."
    );
    expect(coreSaves(calls)).toEqual([]);

    agree = true;
    click(byTestId(card(container), 'runtime-card-unload'));
    await until('unloaded', () => coreSaves(calls).length === 1);
    await settle();
    expect((coreSaves(calls)[0]?.body as { instances: unknown[] }).instances).toEqual([a, b]);
    expect(byTestId(card(container), 'runtime-commanded')?.textContent).toBe("Runtime Line was taken out of the project's runtimes.");
  });
});
