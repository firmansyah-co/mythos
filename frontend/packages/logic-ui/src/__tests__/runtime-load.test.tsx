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
  emit,
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

  it("the project's runtimes saved by someone else while the question was open are not overwritten", async () => {
    const spare = { id: 'Spare', module: 'vm', run: true, program: { name: 'Spare', built_from: 'Spare', version: 'v1' } };
    const backend = plant({ config: { instances: [spare] } });
    const { calls, container } = await opened(backend);
    const agent = { id: 'Mixer', module: 'vm', run: true, program: { name: 'Mixer', built_from: 'Mixer', version: 'v-agent' } };
    const asked = answering(calls, () => {
      // While the engineer reads the question, an agent loads Mixer into the project's runtimes.
      backend.config = { instances: [spare, agent] };
      return true;
    });
    await load(container);

    expect(asked).toHaveLength(1);
    expect(asked[0]?.question).not.toContain('Mixer');
    expect(coreSaves(calls)).toEqual([]);
    expect(backend.config).toEqual({ instances: [spare, agent] });
    expect(byTestId(container, 'load-refused')?.querySelector('p')?.textContent).toBe(
      "The project's runtimes were changed by someone else while the question was open, so nothing was loaded: what was asked is no longer what would be sent. Load again to be asked about what is there now."
    );
    expect(byTestId(container, 'runtime-loaded')).toBeNull();
  });

  it("the project's runtimes that cannot be read again after the question are not overwritten", async () => {
    const backend = plant();
    const { calls, container } = await opened(backend);
    let reads = 0;
    backend.intercept = (call) => {
      if (call.method !== 'GET' || !/\/projects\/plant\/runtimes$/.test(call.path)) return undefined;
      reads += 1;
      // The read before the question answers; the one after it does not.
      return reads === 2 ? Response.json({ error: 'the storage is busy' }, { status: 500, statusText: 'Internal Server Error' }) : undefined;
    };
    answering(calls, true);
    await load(container);

    expect(reads).toBe(2);
    expect(coreSaves(calls)).toEqual([]);
    expect(byTestId(container, 'load-refused')?.querySelector('p')?.textContent).toBe(
      "The project's runtimes could not be read again after the question, so nothing was loaded."
    );
    expect(byTestId(container, 'load-refused-error')?.textContent).toBe('the storage is busy');
  });

  it('a program whose source declares another PROGRAM name is found where it runs, under that name', async () => {
    // Line runs Tank alone; Tank's source declares PROGRAM Main, and the controller lists it so.
    const backend = plant({
      programs: [{ entry: entryOf('Tank'), source: 'PROGRAM Main\nEND_PROGRAM\n' }, program('Pump'), program('Mixer'), program('Spare')],
      configured: [{ id: 'Line', programs: ['Tank'] }],
      runtimes: listing(runtimeOf('Line', { program: 'Main', version: 'v-old', programs: [{ name: 'Main', version: 'v-old' }] })),
    });
    const { calls, container } = await opened(backend);
    const asked = answering(calls, true);
    await load(container);

    expect(asked[0]?.question).toContain('- Tank: version v-Tank to load; runs now: version v-old in runtime Line');
    // The module names the instance's program by the PROGRAM declared, and its task's too.
    const sent = (coreSaves(calls)[0]?.body as { instances: Array<{ program?: { name: string; built_from: string }; tasks?: Array<{ programs: string[] }> }> })
      .instances[0];
    expect(sent?.program).toMatchObject({ name: 'Main', built_from: 'Tank' });
    expect(sent?.tasks?.[0]?.programs).toEqual(['Main']);

    // Once the controller runs the build made here, it is called the same.
    backend.runtimes = listing(runtimeOf('Line', { program: 'Main', version: 'v-Tank', programs: [{ name: 'Main', version: 'v-Tank' }] }));
    await doing(() => usePrograms.getState().readRuntimes());
    expect(texts(card(container), 'runtime-program-compare')).toEqual(['the same as the one built here']);
  });

  it('the question says whether each runtime it starts or restarts is put in RUN or loaded in STOP', async () => {
    // Old is in STOP (an operator stopped it); its configuration says it runs when loaded.
    const old = { id: 'Old', module: 'vm', run: true, programs: [{ name: 'Mixer', built_from: 'Mixer' }, { name: 'Tank', built_from: 'Tank' }] };
    const backend = plant({
      configured: [{ ...LINE, run: false }, { id: 'Old', programs: ['Mixer'] }],
      config: { instances: [old] },
      runtimes: listing(runtimeOf('Old', { program: 'Mixer, Tank', mode: 'stop', programs: [{ name: 'Mixer', version: 'a' }, { name: 'Tank', version: 'b' }] })),
    });
    const { calls, container } = await opened(backend);
    const asked = answering(calls, true);
    await load(container);

    const question = asked[0]?.question ?? '';
    expect(question).toContain(
      'Runtime Line: It is loaded in STOP (the controller does not list it now), and writes no output until it is put in RUN.'
    );
    expect(question).toContain(
      "Runtime Old gives up the moved program: it is built again from the project's saved programs and restarts warm (not the same programs). It ran Mixer, Tank; it will run Mixer (version v-Mixer). It is put in RUN (it is in STOP now), and its programs write their outputs."
    );
  });

  it('a rebuilt runtime that now runs a program running alone takes that one out, and one built before 0.8.0 is said', async () => {
    const old = { id: 'Old', module: 'vm', run: true, programs: [{ name: 'Mixer', built_from: 'Mixer' }, { name: 'Tank', built_from: 'Tank' }] };
    // Spare runs alone under its own id; Legacy, built before 0.8.0, runs a PROGRAM Spare under another id.
    const spare = { id: 'Spare', module: 'vm', run: true, program: { name: 'Spare', built_from: 'Spare' } };
    const legacy = { id: 'Legacy', module: 'vm', run: true, program: { name: 'Spare' } };
    const backend = plant({ configured: [LINE, { id: 'Old', programs: ['Mixer', 'Spare'] }], config: { instances: [old, spare, legacy] } });
    const { calls, container } = await opened(backend);
    const asked = answering(calls, true);
    await load(container);

    expect(asked).toHaveLength(1);
    const question = asked[0]?.question ?? '';
    expect(question).toContain('Taken out, replaced by runtime Old: Spare (ran Spare).');
    expect(question).toContain(
      'Runtime Legacy may also run Spare, which runtime Old will run too (built before 0.8.0, not compared by the core). It is kept.'
    );
    // One save the core takes: Spare alone out, the rebuilt Old with Spare in.
    expect(sentIds(coreSaves(calls)[0])).toEqual(['Legacy', 'Line', 'Old']);
    expect(byTestId(container, 'load-refused')).toBeNull();
    expect(sentIds({ method: 'PUT', path: '', body: backend.config })).toEqual(['Legacy', 'Line', 'Old']);
  });

  it('a runtime built again whose program another configured runtime holds builds that one too, once', async () => {
    // Line takes Tank from Old; Old now runs Mixer, which Third still holds; Third now runs Spare.
    const old = { id: 'Old', module: 'vm', run: true, programs: [{ name: 'Tank', built_from: 'Tank' }] };
    const third = { id: 'Third', module: 'vm', run: true, programs: [{ name: 'Mixer', built_from: 'Mixer' }, { name: 'Spare', built_from: 'Spare' }] };
    const backend = plant({
      configured: [LINE, { id: 'Old', programs: ['Mixer'] }, { id: 'Third', programs: ['Spare'] }],
      config: { instances: [old, third] },
    });
    const { calls, container } = await opened(backend);
    const asked = answering(calls, true);
    calls.length = 0;
    await load(container);

    expect(writes(calls)).toEqual([
      'POST /projects/plant/logic/runtimes/Line/check',
      'POST /projects/plant/logic/runtimes/Line/build',
      'POST /projects/plant/logic/runtimes/Old/build',
      'POST /projects/plant/logic/runtimes/Third/build',
      'PUT /projects/plant/runtimes',
    ]);
    expect(asked[0]?.question).toContain('Runtime Third gives up the moved program');
    expect(sentIds(coreSaves(calls)[0])).toEqual(['Line', 'Old', 'Third']);
    expect(byTestId(container, 'load-refused')).toBeNull();
  });

  it('Load cold asks first, sends the save with start=cold, and says each rebuilt runtime restarts cold', async () => {
    const old = { id: 'Old', module: 'vm', run: true, programs: [{ name: 'Mixer', built_from: 'Mixer' }, { name: 'Tank', built_from: 'Tank' }] };
    const backend = plant({ configured: [LINE, { id: 'Old', programs: ['Mixer'] }], config: { instances: [old] } });
    const { calls, container } = await opened(backend);
    const cold = async () => {
      click(byTestId(container, 'runtime-load-cold'));
      await settle();
      await until('the load ends', () => useRuntimes.getState().docs.Line?.busy === null);
      await settle();
    };

    // Cancelled at the first question: nothing is built or sent.
    let asked = answering(calls, false);
    calls.length = 0;
    await cold();
    expect(asked.map((a) => a.question)).toEqual([
      'Load runtime Line cold? Every runtime this load starts or replaces starts from its initial values, RETAIN variables included, and nothing they hold is carried over. The draft is saved first if it has changes, and what the load will change is asked before anything is loaded.',
    ]);
    expect(writes(calls)).toEqual([]);

    asked = answering(calls, true);
    calls.length = 0;
    await cold();
    expect(asked).toHaveLength(2);
    expect(asked[1]?.savesBefore).toBe(0);
    const question = asked[1]?.question ?? '';
    expect(question).toContain('Load runtime Line cold? Every runtime this save starts or replaces starts from its initial values, RETAIN variables included.');
    expect(question).toContain('Runtime Old gives up the moved program: it is built again from the project\'s saved programs and restarts cold (not the same programs).');
    expect(coreSaves(calls).map((c) => c.path)).toEqual(['/api/v1/projects/plant/runtimes?start=cold']);
    expect(sentIds(coreSaves(calls)[0])).toEqual(['Line', 'Old']);
  });

  it('an applied save the controller says nothing more of does not say how any runtime started', async () => {
    const backend = plant({ fieldIoOff: true });
    const { calls, container } = await opened(backend);
    answering(calls, true);
    await load(container);

    expect(byTestId(container, 'runtime-loaded-said')?.textContent).toBe(
      "Saved into the project's runtimes. The controller said nothing of what it started or stopped, as it answers while its field I/O is not running, so nothing is said here of how any runtime started. Whether each runtime runs is on its card, as the controller lists it."
    );
    expect(texts(container, 'runtime-start-said')).toEqual([]);
  });

  it('the fake core answers as the core does: commands of an unlisted runtime, and what validate refuses', async () => {
    const backend = plant({ runtimes: listing(runtimeOf('Line')) });
    server(backend);
    const post = (path: string, body?: unknown) =>
      fetch(path, { method: body === undefined ? 'POST' : 'PUT', ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
    const stopped = await post('/api/v1/runtimes/Gone/stop');
    expect([stopped.status, ((await stopped.json()) as { error: string }).error]).toEqual([404, "no runtime 'Gone' is loaded on this node"]);
    expect((await post('/api/v1/runtimes/Line/stop')).status).toBe(200);
    const refusedWith = async (instances: unknown[]) => ((await (await post('/api/v1/projects/plant/runtimes', { instances })).json()) as { error?: string }).error;
    const a = { id: 'A', module: 'vm', run: true, program: { name: 'A', outputs: [{ tag: 'plc1.valve' }] } };
    expect(await refusedWith([a, { ...a }])).toBe("runtime id 'A' is used more than once");
    expect(await refusedWith([{ ...a, programs: [{ name: 'B' }] }])).toBe("runtime 'A': give program or programs, not both");
    expect(await refusedWith([a, { ...a, id: 'B' }])).toBe("plc1.valve is an output of both 'A' and 'B': one tag has one writer");
    expect(backend.config).toEqual({ instances: [] });
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

  it("another project's runtime of the same id is not shown as this one, and the question says this project runs nowhere", async () => {
    const backend = plant({
      active: 'other',
      runtimes: listing(runtimeOf('Line', { program: 'Tank, Pump', mode: 'run', programs: [{ name: 'Tank', version: 'x' }, { name: 'Pump', version: 'y' }] })),
    });
    const { calls, container } = await opened(backend);
    const at = card(container);
    expect(mode(container)).toBe('NOT LOADED');
    expect(byTestId(at, 'runtime-status-why')?.textContent).toBe(
      "Runtime Line of this project is not loaded: the controller runs another project (other), and a runtime Line it lists is that project's, not this one."
    );
    expect(byTestId(at, 'runtime-status-version')).toBeNull();
    expect(texts(at, 'runtime-program')).toEqual([]);
    expect(byTestId(at, 'runtime-status-project')).toBeNull();
    for (const command of ['runtime-card-run', 'runtime-card-stop', 'runtime-card-unload']) {
      expect((byTestId(at, command) as HTMLButtonElement | null)?.disabled).toBe(true);
    }

    const asked = answering(calls, true);
    await load(container);
    expect(asked[0]?.question).toContain('- Tank: version v-Tank to load; runs now: none: the controller runs another project (other)');
    expect(byTestId(container, 'runtime-loaded-said')?.textContent).toBe(
      "Saved into the project's runtimes. The controller runs another project, so nothing was started."
    );
  });

  it('Stop stays on the card when the module can no longer be read', async () => {
    const backend = plant({ runtimes: listing(runtimeOf('Line', { program: 'Tank, Pump', mode: 'run' })) });
    const { calls, container } = await opened(backend);
    expect(byTestId(card(container), 'runtime-card-stop')).not.toBeNull();
    backend.configured = 'fails';
    emit('LOGIC_RUNTIMES_CHANGED', { project: 'plant', runtime: 'Line', what: 'saved' });
    await until('the read fails', () => byTestId(container, 'runtime-read-error') !== null);
    await settle();

    expect(mode(container)).toBe('RUN');
    // Which project the controller runs was never read here: said, not assumed.
    expect(byTestId(card(container), 'runtime-status-project')?.textContent).toBe(
      "Which project the controller runs is not known yet: what it lists is shown as this project's."
    );
    const stop = byTestId(card(container), 'runtime-card-stop') as HTMLButtonElement | null;
    expect(stop?.disabled).toBe(false);
    calls.length = 0;
    click(stop);
    await settle();
    expect(writes(calls)).toEqual(['POST /runtimes/Line/stop']);

    // Not listed: what is not known of the module is not said ("configured").
    backend.runtimes = listing();
    await reread();
    expect(byTestId(card(container), 'runtime-status-why')?.textContent).toBe('The controller lists no runtime Line: it is not loaded.');
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
