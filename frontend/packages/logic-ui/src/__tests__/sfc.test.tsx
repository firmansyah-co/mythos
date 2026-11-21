// A Sequential Function Chart program in the workbench (ADR-013 L3d-2): its language in the tree,
// its chart drawn from the TC6 document (read-only for a viewer; an engineer's editor, L3d-3, draws
// it with this view, and these tests run as an engineer unless they say otherwise) with IEC 61131-3's forms (§6.7), each
// diagnostic at its element, and its live state, drawn only from watched values and only while the
// runtime runs the version on screen in RUN: each active step marked with its time, a transition's
// condition only where it was evaluated in the last scan (not evaluated otherwise, never an older
// value), each action's Q, UNKNOWN where a value cannot be read, FORCED kept. The charts drawn are
// the compiler's own corpus (`crates/mythos-logic-graphic/tests/sfc/`). Every assertion reads a
// rendered element by its data-testid.

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ChartProbe, CheckResult, Variable, WatchedValue } from '../bridge';
import { ProgramEditor } from '../ProgramEditor';
import { ProgramsTree } from '../ProgramsTree';
import { followsSavedSource, usePrograms } from '../programs';
import { readSfc } from '../sfc';
import { place } from '../SfcView';
import {
  byLabel,
  byTestId,
  click,
  endTest,
  entryOf,
  project,
  render,
  resetStores,
  runtimeOf,
  runtimesOf,
  server,
  settle,
  texts,
  typeInto,
  until,
  doing,
  useController,
  useSession,
  useShell,
  watchOf,
  timeScale,
  type Backend,
} from './harness';

const CORPUS = resolve(__dirname, '../../../../../crates/mythos-logic-graphic/tests/sfc');
/**
 * A corpus document with each CDATA section written as escaped text, which is the same text to an
 * XML reader: the test DOM's (happy-dom's) DOMParser refuses CDATA as not well-formed, where a
 * browser's reads it (the real-browser run, scripts/e2e.sh, draws a chart written with CDATA).
 */
const corpus = (name: string) =>
  readFileSync(resolve(CORPUS, name), 'utf8').replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, (_, text: string) =>
    text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  );
/** The bottling line: every element the front-end reads, in one chart (see its header). */
const BOTTLING = corpus('bottling.xml');
/** A chart in a function block, called twice by an FBD program of the same document. */
const DOORS = corpus('doors.xml');
/** A mixing tank whose actions and conditions are written in LD and FBD (L3d-1b; see its header). */
const MIXER = corpus('mixer.xml');

const VERSION = 'aaaaaaaaaaaa';

const STEPS: Record<number, string> = { 1: 'Idle', 3: 'Convey', 10: 'Reject', 16: 'Fill', 22: 'FillDone', 23: 'Cap', 27: 'CapDone', 30: 'Release' };
const TRANSITIONS = [2, 8, 9, 14, 21, 26, 29, 34];
/** Each association's own localId, and the variable the logic module says keeps its Q (§2.17). */
const ACTIONS: Record<number, string> = {
  5: 'conveyor',
  6: 'CountBottle.Q',
  12: 'alarm',
  13: 'Discard.Q',
  18: 'valve',
  19: 'label_ok',
  20: '__SFC_20_Q',
  25: 'capper',
  32: 'valve',
  33: 'label_ok',
};

/** Where the check of the bottling chart says each state is kept, as the logic module gives `chart`. */
const CHART: ChartProbe[] = [
  ...Object.entries(STEPS).flatMap(([id, name]) => [
    { local_id: Number(id), step: 'X' as const, variable: `${name}.X`, negated: false },
    { local_id: Number(id), step: 'T' as const, variable: `${name}.T`, negated: false },
  ]),
  ...TRANSITIONS.map((id) => ({ local_id: id, transition: true, variable: `__SFC_${id}`, negated: false, ran: `__SFC_${id}_EN` })),
  ...Object.entries(ACTIONS).map(([id, variable]) => ({ local_id: Number(id), action: 'Q' as const, variable, negated: false })),
];

const variable = (path: string, type: string, hidden = false): Variable => ({ path, type, section: 'local', bindable: false, hidden });
const compiles = (chart: ChartProbe[] = CHART) => (): CheckResult => ({
  ok: true,
  diagnostics: [],
  program: 'Bottling',
  version: VERSION,
  variables: [
    ...['conveyor', 'valve', 'capper', 'alarm', 'label_ok'].map((p) => variable(p, 'BOOL')),
    ...['bottles', 'rejected'].map((p) => variable(p, 'INT')),
    variable('fill_time', 'TIME'),
    ...Object.values(STEPS).flatMap((name) => [variable(`${name}.X`, 'BOOL'), variable(`${name}.T`, 'TIME')]),
    ...TRANSITIONS.flatMap((id) => [variable(`__SFC_${id}`, 'BOOL', true), variable(`__SFC_${id}_EN`, 'BOOL', true)]),
  ],
  code_size: 2603,
  memory_size: 269,
  binding_problems: [],
  chart,
});

const bool = (value: boolean, forced = false): Partial<WatchedValue> => ({ type: 'BOOL', value, ...(forced ? { forced } : {}) });
const time = (ms: number): Partial<WatchedValue> => ({ type: 'TIME', value: ms * 1e6 });

/**
 * A scan with Convey active for 45 ms: the selection after it was enabled (8 and 9 evaluated, 8
 * TRUE, 9 FALSE); Idle's transition not enabled, its condition holding TRUE from an earlier scan;
 * the conveyor on, CountBottle's pulse over.
 */
const RUNNING: Record<string, Partial<WatchedValue>> = {
  ...Object.fromEntries(Object.values(STEPS).flatMap((name) => [[`${name}.X`, bool(false)], [`${name}.T`, time(0)]])),
  'Convey.X': bool(true),
  'Convey.T': time(45),
  'Idle.T': time(61),
  ...Object.fromEntries(TRANSITIONS.flatMap((id) => [[`__SFC_${id}`, bool(false)], [`__SFC_${id}_EN`, bool(false)]])),
  __SFC_2: bool(true),
  __SFC_8_EN: bool(true),
  __SFC_8: bool(true),
  __SFC_9_EN: bool(true),
  __SFC_9: bool(false),
  conveyor: bool(true),
  'CountBottle.Q': bool(false),
  alarm: bool(false),
  'Discard.Q': bool(false),
  valve: bool(false),
  label_ok: bool(false),
  __SFC_20_Q: bool(false),
  capper: bool(false),
  bottles: { type: 'INT', value: 3 },
  rejected: { type: 'INT', value: 0 },
  fill_time: time(0),
};

const bottling = (over: Partial<Backend> = {}): Backend => ({
  programs: [{ entry: entryOf('Bottling', { language: 'sfc' }), source: BOTTLING }],
  check: compiles(),
  runtimes: runtimesOf(runtimeOf('Bottling', { version: VERSION })),
  watch: watchOf(RUNNING),
  ...over,
});

async function opened(backend: Backend, name = 'Bottling') {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant' });
  await settle();
  const container = render(<ProgramEditor name={name} />);
  await settle();
  // An engineer gets the chart editor (L3d-3), which draws the chart with the view.
  await until('the chart is shown', () => byTestId(container, 'sfc-view') ?? byTestId(container, 'sfc-editor') ?? byTestId(container, 'sfc-unreadable'));
  return { calls, container };
}

const element = (container: HTMLElement, id: number) => {
  const found = container.querySelector(`[data-testid="sfc-element"][data-local-id="${id}"]`);
  if (!found) throw new Error(`no element ${id}`);
  return found;
};
const step = (container: HTMLElement, id: number) => {
  const e = element(container, id);
  return {
    state: e.getAttribute('data-state'),
    time: e.querySelector('[data-testid="sfc-step-time"]')?.textContent ?? null,
    status: e.querySelector('[data-testid="sfc-step-status"]')?.textContent ?? null,
    forced: e.getAttribute('data-forced'),
  };
};
const condition = (container: HTMLElement, id: number) => {
  const e = element(container, id);
  const value = e.querySelector('[data-testid="sfc-condition-value"]');
  return { state: e.getAttribute('data-state'), text: value?.textContent ?? null };
};
const action = (container: HTMLElement, id: number) => {
  const row = container.querySelector(`[data-testid="sfc-action"][data-local-id="${id}"]`);
  if (!row) throw new Error(`no action ${id}`);
  return {
    state: row.getAttribute('data-state'),
    text: row.querySelector('[data-testid="sfc-action-value"]')?.textContent ?? null,
    forced: row.getAttribute('data-forced'),
  };
};
const liveness = (container: HTMLElement) => byTestId(container, 'sfc-live')?.getAttribute('data-live');
const live = async (container: HTMLElement) => until('the chart is live', () => (liveness(container) === 'live' ? true : null));

vi.setConfig({ testTimeout: 20_000 * timeScale });

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
});

describe('a Sequential Function Chart in the tree', () => {
  it("names the program's language, and adds one in SFC", async () => {
    const backend = bottling();
    backend.programs.unshift({ entry: entryOf('Tank'), source: 'PROGRAM Tank\nEND_PROGRAM\n' });
    const { calls } = await opened(backend);
    const tree = render(<ProgramsTree openEditor={() => undefined} />);
    expect(texts(tree, 'tree-program-language')).toEqual(['ST', 'SFC']);

    click(byTestId(tree, 'tree-add-program'));
    typeInto(byLabel(tree, 'Program name'), 'Filler');
    const language = byLabel(tree, 'Program language');
    if (!(language instanceof HTMLSelectElement)) throw new Error('no language');
    expect([...language.options].map((o) => o.value)).toEqual(['st', 'ld', 'fbd', 'sfc']);
    typeInto(language, 'sfc');
    await doing(() => byLabel(tree, 'Program name')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
    const made = calls.filter((call) => call.method === 'POST' && call.path.endsWith('/logic/programs'));
    expect(made.map((call) => call.body)).toEqual([{ name: 'Filler', language: 'sfc' }]);
    expect(texts(tree, 'tree-program-language')).toEqual(['ST', 'SFC', 'SFC']);
  });
});

describe('drawing a Sequential Function Chart', () => {
  it('draws every element of the corpus where the document puts it, read-only for a viewer', async () => {
    // A viewer gets the view (an engineer the editor, L3d-3: sfc-editor.test.tsx): no text area to type in.
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { container } = await opened(bottling({ check: () => ({ ...compiles()(), version: 'other' }) }));
    expect(byTestId(container, 'sfc-read-only')?.textContent).toBe('Read-only: only an engineer changes a program.');
    expect(byTestId(container, 'ld-editor')).toBeNull();
    expect(byTestId(container, 'fbd-editor')).toBeNull();
    // The document's text is folded under the chart, and not drawn until it is opened.
    expect((byTestId(container, 'sfc-document') as HTMLDetailsElement | null)?.open).toBe(false);
    expect(container.querySelector('textarea')).toBeNull();
    const kinds = [...container.querySelectorAll('[data-testid="sfc-element"]')].map(
      (e) => `${e.getAttribute('data-kind')}:${e.getAttribute('data-local-id')}`
    );
    expect(kinds.sort()).toEqual(
      [
        ...Object.keys(STEPS).map((id) => `step:${id}`),
        ...TRANSITIONS.map((id) => `transition:${id}`),
        'actionBlock:4',
        'actionBlock:11',
        'actionBlock:17',
        'actionBlock:24',
        'actionBlock:31',
        'selectionDivergence:7',
        'selectionConvergence:35',
        'simultaneousDivergence:15',
        'simultaneousConvergence:28',
        'jumpStep:36',
        'comment:90',
      ].sort()
    );
    expect(byTestId(container, 'sfc-not-drawn')).toBeNull();
    // A step is a box with its name; the initial step, alone, has the second frame.
    expect(element(container, 3).textContent).toContain('Convey');
    expect(
      [...container.querySelectorAll('[data-testid="sfc-initial-frame"]')].map((f) => f.closest('[data-testid="sfc-element"]')?.getAttribute('data-local-id'))
    ).toEqual(['1']);
    // A transition is a bar with its condition, inline or a transition of the POU by its name.
    expect(element(container, 2).querySelector('[data-testid="sfc-transition-bar"]')).not.toBeNull();
    expect(element(container, 2).querySelector('[data-testid="sfc-condition"]')?.textContent).toBe('Idle.T >= T#60ms');
    expect(element(container, 21).querySelector('[data-testid="sfc-condition"]')?.textContent).toBe('Filled');
    // A condition of 40 characters is written whole; it is in the element's title as well.
    expect(element(container, 8).querySelector('[data-testid="sfc-condition"]')?.textContent).toBe('bottles MOD 3 = 0 AND Convey.T >= T#40ms');
    expect(element(container, 8).querySelector('title')?.textContent).toBe('transition 8: bottles MOD 3 = 0 AND Convey.T >= T#40ms');
    // A selection is one line, a simultaneous divergence or convergence two.
    expect(element(container, 7).querySelectorAll('line')).toHaveLength(1);
    expect(element(container, 35).querySelectorAll('line')).toHaveLength(1);
    expect(element(container, 15).querySelectorAll('line')).toHaveLength(2);
    expect(element(container, 28).querySelectorAll('line')).toHaveLength(2);
    // A jump names the step it goes to.
    expect(element(container, 36).getAttribute('data-target')).toBe('Idle');
    expect(element(container, 36).textContent).toContain('Idle');
    // Each association of a block with its qualifier (and duration) and what it runs.
    const rows = [...container.querySelectorAll('[data-testid="sfc-action"]')].map(
      (r) =>
        `${r.getAttribute('data-local-id')}:${r.querySelector('[data-testid="sfc-qualifier"]')?.textContent}:${r.querySelector('[data-testid="sfc-action-text"]')?.textContent}`
    );
    expect(rows.sort()).toEqual(
      [
        '5:N:conveyor',
        '6:P:CountBottle',
        '12:D T#20ms:alarm',
        '13:P:Discard',
        '18:S:valve',
        '19:DS T#40ms:label_ok',
        '20:N:fill_time := Fill.T;',
        '25:L T#60ms:capper',
        '32:R:valve',
        '33:R:label_ok',
      ].sort()
    );
    expect(element(container, 90).textContent).toContain('Every element of the chart');
    // Every link of the net, each action block to its step.
    const links = [...container.querySelectorAll('[data-testid="sfc-link"]')].map((l) => `${l.getAttribute('data-from')}>${l.getAttribute('data-to')}`);
    expect(links.sort()).toEqual(
      [
        '1>2', '2>3', '3>4', '3>7', '7>8', '7>9', '8>10', '10>11', '10>14', '9>15', '15>16', '16>17', '16>21', '21>22',
        '15>23', '23>24', '23>26', '26>27', '22>28', '27>28', '28>29', '29>30', '30>31', '30>34', '14>35', '34>35', '35>36',
      ].sort()
    );
    // The POU's own actions and transitions, with their text.
    expect([...container.querySelectorAll('[data-testid="sfc-pou-action"]')].map((a) => a.getAttribute('data-name'))).toEqual(['CountBottle', 'Discard']);
    expect(container.querySelector('[data-testid="sfc-pou-transition"][data-name="Filled"]')?.textContent).toContain('Fill.T >= T#100ms');
    // Not the running version: nothing is drawn live, and the view says why.
    expect(liveness(container)).toBe('none');
    expect(byTestId(container, 'sfc-live')?.textContent).toContain('the runtime runs version aaaaaaaaaaaa, and this text builds version other');
    expect(container.querySelectorAll('[data-testid="sfc-element"]:not([data-state="none"]):is([data-kind="step"], [data-kind="transition"])')).toHaveLength(0);
    expect(container.querySelectorAll('[data-testid="sfc-condition-value"], [data-testid="sfc-action-value"], [data-testid="sfc-step-status"]')).toHaveLength(0);
  });

  it('draws a chart of a function block without live state, and says a body of another language is not drawn here', async () => {
    // The view draws every POU of the document; the editor one at a time, by its tab.
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { container } = await opened(
      bottling({
        programs: [{ entry: entryOf('Doors', { language: 'sfc' }), source: DOORS }],
        check: () => ({ ok: true, diagnostics: [], program: 'Doors', version: VERSION, variables: [], chart: [] }),
        watch: watchOf({ 'Shut.X': bool(true) }),
        runtimes: runtimesOf(runtimeOf('Doors', { version: VERSION })),
      }),
      'Doors'
    );
    const pous = [...container.querySelectorAll('[data-testid="sfc-pou"]')].map((p) => p.getAttribute('aria-label'));
    expect(pous).toEqual(['Door', 'Doors']);
    const door = container.querySelector('[data-testid="sfc-pou"][aria-label="Door"]') as HTMLElement;
    expect(byTestId(door, 'sfc-pou-not-live')?.textContent).toContain('each instance has its own');
    expect([...door.querySelectorAll('[data-testid="sfc-element"]')].map((e) => e.getAttribute('data-kind'))).toEqual([
      'step',
      'transition',
      'step',
      'actionBlock',
      'transition',
      'jumpStep',
    ]);
    // Drawn without a state, whatever the program's watch says.
    expect(door.querySelectorAll('[data-testid="sfc-element"][data-state]:not([data-state="none"])')).toHaveLength(0);
    const doors = container.querySelector('[data-testid="sfc-pou"][aria-label="Doors"]') as HTMLElement;
    expect(byTestId(doors, 'sfc-pou-other')?.textContent).toContain('in FBD');
    expect(byTestId(doors, 'sfc-diagram')).toBeNull();
  });

  it('follows a chart saved elsewhere, as an LD or FBD diagram, and not a Structured Text being typed', () => {
    expect(['st', 'ld', 'fbd', 'sfc'].filter(followsSavedSource)).toEqual(['ld', 'fbd', 'sfc']);
  });

  it('names the language of an action or a condition in LD or FBD, lists a network drawn in the chart, and marks a diagnostic in a body at what holds it', async () => {
    const { container } = await opened(
      bottling({
        programs: [{ entry: entryOf('Mixer', { language: 'sfc' }), source: MIXER }],
        check: () => ({
          ok: false,
          diagnostics: [
            // In the FBD body of the action of block 10 (localId 11): its element 3, not step Fill (3).
            { line: 110, col: 1, end_line: 110, end_col: 9, message: 'in the inline body', element: { pou: 'Mixer', inline: 11, local_id: 3 } },
            // In the LD action Open of the POU: its coil 2, not transition 2.
            { line: 44, col: 1, end_line: 44, end_col: 9, message: 'in the action Open', element: { pou: 'Mixer', action: 'Open', local_id: 2 } },
            // The contact of the rung drawn in the chart for transition 2.
            { line: 132, col: 1, end_line: 132, end_col: 9, message: 'in the drawn rung', element: { pou: 'Mixer', local_id: 21, x: -100, y: 40 } },
          ],
        }),
      }),
      'Mixer'
    );
    expect(element(container, 2).querySelector('[data-testid="sfc-condition"]')?.textContent).toBe('(network in the chart, from 21)');
    expect(element(container, 4).querySelector('[data-testid="sfc-condition"]')?.textContent).toBe('(in FBD)');
    expect(element(container, 7).querySelector('[data-testid="sfc-condition"]')?.textContent).toBe('(network in the chart, from 34)');
    const what = (id: number) => container.querySelector(`[data-testid="sfc-action"][data-local-id="${id}"] [data-testid="sfc-action-text"]`)?.textContent;
    expect([what(11), what(13), what(17)]).toEqual(['(in FBD)', 'Open', '(in LD)']);
    expect(container.querySelector('[data-testid="sfc-pou-action"][data-name="Open"]')?.textContent).toContain('Its body is in LD and is not shown here.');
    expect(byTestId(container, 'sfc-not-drawn')?.textContent).toBe(
      'Not drawn: ' +
        ['leftPowerRail 20', 'contact 21', 'inVariable 30', 'inVariable 31', 'block 32', 'connector 33', 'continuation 34']
          .map((e) => `${e} (of a network of LD or FBD drawn in the chart)`)
          .join('; ') +
        '.'
    );
    const marks = [...container.querySelectorAll('[data-testid="sfc-diagnostic"]')].map((m) => `${m.getAttribute('data-local-id')}:${m.textContent}`);
    expect(marks).toEqual(['11:in the inline body', '13:in the action Open', '2:in the drawn rung']);
    expect(texts(container, 'diagnostic')).toEqual([
      'Mixer element 11, its body element 3in the inline body',
      'Mixer action Open element 2in the action Open',
      'Mixer element 21in the drawn rung',
    ]);
  });

  it('lists the second of two elements with one localId as not drawn, and draws the first', () => {
    const read = readSfc(BOTTLING.replace('<comment localId="90"', '<comment localId="3"'));
    if (!read.ok) throw new Error(read.error);
    const pou = read.pous[0];
    if (!pou) throw new Error('no POU');
    expect(pou.notDrawn).toEqual(['comment 3: another element has its localId']);
    const placed = place(pou, false);
    expect(placed.size).toBe(pou.elements.length);
    expect(placed.get(3)?.e.kind).toBe('step');
  });

  it("draws a transition's priority only among the transitions of a selection divergence, where it counts", async () => {
    const source = BOTTLING.replace('<transition localId="2">', '<transition localId="2" priority="7">')
      .replace('<transition localId="8">', '<transition localId="8" priority="2">')
      .replace('<transition localId="9">', '<transition localId="9" priority="1">');
    const { container } = await opened(bottling({ programs: [{ entry: entryOf('Bottling', { language: 'sfc' }), source }] }));
    const priorities = [...container.querySelectorAll('[data-testid="sfc-priority"]')].map(
      (p) => `${p.closest('[data-testid="sfc-element"]')?.getAttribute('data-local-id')}:${p.textContent}`
    );
    expect(priorities).toEqual(['8:2', '9:1']);
    // Transition 2 follows a step: its priority is not drawn, and its title says it is not used.
    expect(element(container, 2).querySelector('title')?.textContent).toContain('priority 7 not used: the transition follows no selection divergence');
  });

  it('lists an element it does not draw, and reads a negated condition and a priority', () => {
    const read = readSfc(
      BOTTLING.replace('<comment localId="90"', '<macroStep localId="91"><position x="0" y="0"/></macroStep><comment localId="90"')
        .replace('<transition localId="8">', '<transition localId="8" priority="2">')
        .replace('<condition><reference name="Filled"/>', '<condition negated="true"><reference name="Filled"/>')
    );
    if (!read.ok) throw new Error(read.error);
    const pou = read.pous[0];
    expect(pou?.notDrawn).toEqual(['macroStep 91']);
    expect(pou?.elements.find((e) => e.localId === 8)?.priority).toBe(2);
    expect(pou?.elements.find((e) => e.localId === 21)?.condition).toEqual({ kind: 'reference', name: 'Filled', negated: true });
  });

  it('draws each diagnostic at the element it names: an element, an action of a block, a transition of the POU', async () => {
    const { container } = await opened(
      bottling({
        check: () => ({
          ok: false,
          diagnostics: [
            {
              line: 51,
              col: 13,
              end_line: 51,
              end_col: 40,
              message: 'Idel is not declared',
              element: { pou: 'Bottling', local_id: 2, x: 200, y: 40, text: { field: 'condition', col: 1, end_col: 5 } },
            },
            {
              line: 101,
              col: 15,
              end_line: 101,
              end_col: 40,
              message: 'alarm2 is not declared',
              element: { pou: 'Bottling', local_id: 12, x: 0, y: 180, text: { field: 'reference', col: 1, end_col: 7 } },
            },
            {
              line: 46,
              col: 1,
              end_line: 46,
              end_col: 30,
              message: 'Fil is not declared',
              element: { pou: 'Bottling', transition: 'Filled', text: { field: 'body', col: 1, end_col: 4, line: 1 } },
            },
          ],
        }),
      })
    );
    const marks = [...container.querySelectorAll('[data-testid="sfc-diagnostic"]')];
    expect(marks.map((m) => m.getAttribute('data-local-id'))).toEqual(['2', '12', '21']);
    expect(marks.map((m) => m.textContent)).toEqual(['Idel is not declared', 'alarm2 is not declared', 'Fil is not declared']);
    // Around the transition: where it is drawn (its position, two and a half times), less 4.
    expect(marks[0]?.getAttribute('y')).toBe(String(40 * 2.5 - 4));
    // The transition of the POU says it under its text, with the line.
    expect(container.querySelector('[data-testid="sfc-pou-transition"][data-name="Filled"] [data-testid="sfc-named-diagnostic"]')?.textContent).toBe(
      'Line 1: Fil is not declared'
    );
    expect(texts(container, 'diagnostic')).toEqual([
      'Bottling element 2Idel is not declared',
      'Bottling element 12alarm2 is not declared',
      'Bottling transition Filled line 1Fil is not declared',
    ]);
    expect(liveness(container)).toBe('none');
    expect(byTestId(container, 'sfc-live')?.textContent).toContain('this text does not compile');
    // A diagnostic with no element of the drawing opens the document's text, read-only.
    expect(container.querySelector('textarea')).toBeNull();
    await doing(() => click([...container.querySelectorAll('[data-testid="diagnostic"]')][2] ?? null));
    expect((byTestId(container, 'sfc-document') as HTMLDetailsElement | null)?.open).toBe(true);
    await until('the text is drawn', () => container.querySelector('textarea'));
    expect((container.querySelector('textarea') as HTMLTextAreaElement).readOnly).toBe(true);
  });

  it('says why a document that is not a TC6 project is not drawn, and shows its text', async () => {
    const { container } = await opened(bottling({ programs: [{ entry: entryOf('Bottling', { language: 'sfc' }), source: 'PROGRAM Bottling END_PROGRAM' }] }));
    expect(byTestId(container, 'sfc-unreadable')?.textContent).toContain('not well-formed XML');
    expect(byTestId(container, 'sfc-diagram')).toBeNull();
    expect((byTestId(container, 'sfc-document') as HTMLDetailsElement | null)?.open).toBe(true);
    const area = container.querySelector('textarea');
    expect(area instanceof HTMLTextAreaElement && area.readOnly && area.value).toBe('PROGRAM Bottling END_PROGRAM');
  });
});

/** The bottling chart as an agent saves it again: its comment's text changed, the chart the same. */
const EDITED = BOTTLING.replace('Every element of the chart', 'Edited by an agent');

describe('a chart saved elsewhere', () => {
  it('is read again, checked quietly and drawn live again, the reason said while it is not checked', async () => {
    const backend = bottling();
    const { calls, container } = await opened(backend);
    await live(container);
    const checks = () => calls.filter((c) => c.path.endsWith('/check'));
    const before = checks().length;
    backend.programs[0]!.source = EDITED;
    await doing(() => usePrograms.getState().reread('Bottling'));
    // The new text is drawn, and was checked by itself: that very text, without a word to anybody.
    await until('the new text is drawn', () => (element(container, 90).textContent?.includes('Edited by an agent') ? true : null));
    await until('it is checked', () => (checks().length > before ? true : null));
    expect((checks().at(-1)?.body as { source?: string }).source).toBe(EDITED);
    await live(container);
    expect(step(container, 3).state).toBe('active');
    expect(byTestId(container, 'program-error')).toBeNull();
    expect(usePrograms.getState().busy).toBeNull();
  });

  it('says a followed text is not checked yet, never that it does not compile, when its check fails', async () => {
    const backend = bottling();
    const { container } = await opened(backend);
    await live(container);
    backend.intercept = (call) => (call.path.endsWith('/check') ? new Response('{"error":"down"}', { status: 502 }) : undefined);
    backend.programs[0]!.source = EDITED;
    await doing(() => usePrograms.getState().reread('Bottling'));
    await until('the new text is drawn', () => (element(container, 90).textContent?.includes('Edited by an agent') ? true : null));
    await settle();
    expect(liveness(container)).toBe('none');
    expect(byTestId(container, 'sfc-live')?.textContent).toBe('No live state is drawn: this text has not been checked yet.');
    // A quiet check that failed is not reported as an error of the editor.
    expect(byTestId(container, 'program-error')).toBeNull();
  });

  it('opens its text when the document followed can no longer be drawn', async () => {
    const backend = bottling();
    const { container } = await opened(backend);
    expect((byTestId(container, 'sfc-document') as HTMLDetailsElement).open).toBe(false);
    backend.programs[0]!.source = 'not xml';
    await doing(() => usePrograms.getState().reread('Bottling'));
    await until('it cannot be drawn', () => byTestId(container, 'sfc-unreadable'));
    await until('its text is open', () => ((byTestId(container, 'sfc-document') as HTMLDetailsElement).open ? true : null));
    expect((container.querySelector('textarea') as HTMLTextAreaElement | null)?.value).toBe('not xml');
  });
});

describe('the live state of a running Sequential Function Chart', () => {
  it('marks the active step with its time, and the others inactive with none', async () => {
    const { calls, container } = await opened(bottling());
    await live(container);
    expect(step(container, 3)).toEqual({ state: 'active', time: 'T 45 ms', status: 'T 45 ms', forced: null });
    // Idle is not active: no time is shown, though its T still holds its last one.
    expect(step(container, 1)).toEqual({ state: 'inactive', time: null, status: null, forced: null });
    for (const id of [10, 16, 22, 23, 27, 30]) expect(step(container, id).state).toBe('inactive');
    // The hidden variables were asked for by name: each transition's condition with its enable flag.
    const asked = calls.filter((call) => call.path.endsWith('/watch')).at(-1)?.body as { variables: string[] };
    expect(asked.variables).toEqual(expect.arrayContaining(['Convey.X', 'Convey.T', '__SFC_8', '__SFC_8_EN', '__SFC_20_Q', 'CountBottle.Q']));
  });

  it("writes a condition TRUE or FALSE only where it was evaluated in the last scan, never an earlier scan's", async () => {
    const { container } = await opened(bottling());
    await live(container);
    expect(condition(container, 8)).toEqual({ state: 'true', text: 'TRUE' });
    expect(element(container, 8).querySelector('[data-testid="sfc-transition-bar"]')?.getAttribute('stroke')).toBe('var(--c-good)');
    expect(condition(container, 9)).toEqual({ state: 'false', text: 'FALSE' });
    // Idle's transition is not enabled: its variable holds TRUE from an earlier scan, never shown.
    expect(condition(container, 2)).toEqual({ state: 'not-evaluated', text: 'not evaluated' });
    for (const id of [14, 21, 26, 29, 34]) expect(condition(container, id).state).toBe('not-evaluated');
  });

  it("writes each action's Q in its row", async () => {
    const { container } = await opened(bottling());
    await live(container);
    expect(action(container, 5)).toEqual({ state: 'on', text: 'TRUE', forced: null });
    expect(action(container, 6)).toEqual({ state: 'off', text: 'FALSE', forced: null });
    expect(action(container, 20)).toEqual({ state: 'off', text: 'FALSE', forced: null });
  });

  it('draws UNKNOWN, never a state, where a value cannot be read; every state UNKNOWN when none can', async () => {
    const { 'Reject.X': _x, 'Convey.T': _t, __SFC_9_EN: _en, __SFC_8: _held, capper: _q, ...without } = RUNNING;
    const backend = bottling({ watch: watchOf(without) });
    const { container } = await opened(backend);
    await live(container);
    // A step whose flag cannot be read is UNKNOWN, not inactive.
    expect(step(container, 10)).toEqual({ state: 'unknown', time: null, status: 'UNKNOWN', forced: null });
    // An active step whose time cannot be read: active, its time UNKNOWN.
    expect(step(container, 3)).toEqual({ state: 'active', time: 'T UNKNOWN', status: 'T UNKNOWN', forced: null });
    // A transition whose enable flag cannot be read is UNKNOWN, though its condition can be read.
    expect(condition(container, 9)).toEqual({ state: 'unknown', text: 'UNKNOWN' });
    // Enabled, but its condition cannot be read.
    expect(condition(container, 8)).toEqual({ state: 'unknown', text: 'UNKNOWN' });
    expect(action(container, 25)).toEqual({ state: 'unknown', text: 'UNKNOWN', forced: null });

    backend.watch = 'fails';
    await doing(() => usePrograms.getState().readWatch('Bottling', ['Idle.X']));
    expect(liveness(container)).toBe('unknown');
    for (const id of Object.keys(STEPS)) expect(step(container, Number(id)).state).toBe('unknown');
    for (const id of TRANSITIONS) expect(condition(container, id).state).toBe('unknown');
    for (const id of Object.keys(ACTIONS)) expect(action(container, Number(id)).state).toBe('unknown');
    expect(
      container.querySelectorAll(
        '[data-state="active"], [data-state="inactive"], [data-state="true"], [data-state="false"], [data-state="on"], [data-state="off"]'
      )
    ).toHaveLength(0);
  });

  it('draws UNKNOWN where the check does not say where a state is kept', async () => {
    const without = CHART.filter((p) => !(p.local_id === 3 && p.step === 'X') && p.local_id !== 8 && p.local_id !== 5);
    const { container } = await opened(bottling({ check: compiles(without) }));
    await live(container);
    expect(step(container, 3).state).toBe('unknown');
    expect(condition(container, 8).state).toBe('unknown');
    expect(action(container, 5).state).toBe('unknown');
    // A transition's probe without its enable flag: whether it was evaluated is not known.
    const noRan = CHART.map((p) => (p.local_id === 9 ? { ...p, ran: undefined } : p));
    endTest();
    resetStores();
    const again = await opened(bottling({ check: compiles(noRan) }));
    await live(again.container);
    expect(condition(again.container, 9).state).toBe('unknown');
  });

  it('marks FORCED where a step, a condition or an action is forced', async () => {
    const { container } = await opened(
      bottling({ watch: watchOf({ ...RUNNING, 'Fill.X': bool(false, true), __SFC_9: bool(true, true), valve: bool(true, true) }) })
    );
    await live(container);
    expect(step(container, 16)).toEqual({ state: 'inactive', time: null, status: ' FORCED', forced: 'true' });
    expect(element(container, 16).querySelector('[data-testid="variable-forced"]')?.textContent).toBe(' FORCED');
    expect(condition(container, 9)).toEqual({ state: 'true', text: 'TRUE FORCED' });
    expect(element(container, 9).getAttribute('data-forced')).toBe('true');
    expect(action(container, 18)).toEqual({ state: 'on', text: 'TRUE FORCED', forced: 'true' });
    expect(action(container, 32)).toEqual({ state: 'on', text: 'TRUE FORCED', forced: 'true' });
  });

  it('draws nothing live of a program in STOP, and says why', async () => {
    const { container } = await opened(
      bottling({ runtimes: runtimesOf(runtimeOf('Bottling', { version: VERSION, mode: 'stop' })), watch: watchOf(RUNNING, 'stop') })
    );
    await until('the state is read', () => (byTestId(container, 'sfc-live')?.textContent?.includes('STOP') ? true : null));
    expect(byTestId(container, 'sfc-live')?.textContent).toContain('so the chart is not scanned');
    expect(container.querySelectorAll('[data-testid="sfc-step-status"], [data-testid="sfc-condition-value"], [data-testid="sfc-action-value"]')).toHaveLength(0);
    expect(step(container, 3).state).toBe('none');
  });

  it('draws nothing live of another version, nor of a program no runtime runs', async () => {
    const { container } = await opened(bottling({ runtimes: runtimesOf(runtimeOf('Bottling', { version: 'bbbbbbbbbbbb' })) }));
    await until('the state is read', () => (byTestId(container, 'sfc-live')?.textContent?.includes('bbbbbbbbbbbb') ? true : null));
    expect(liveness(container)).toBe('none');
    expect(step(container, 3).state).toBe('none');
    endTest();
    resetStores();
    const none = await opened(bottling({ runtimes: runtimesOf() }));
    await until('the state is read', () => (byTestId(none.container, 'sfc-live')?.textContent?.includes('no runtime runs this program') ? true : null));
    expect(step(none.container, 3).state).toBe('none');
  });

  it('draws nothing live from a watch answered by another version', async () => {
    const backend = bottling();
    const inner = watchOf(RUNNING);
    backend.watch = (variables) => ({ ...inner(variables), state: { mode: 'run', program: 'Bottling', version: 'cccccccccccc', scans: 9 } });
    const { container } = await opened(backend);
    await until('the watch is read', () => (byTestId(container, 'sfc-live')?.textContent?.includes('not of this version') ? true : null));
    expect(step(container, 3).state).toBe('none');
    expect(container.querySelectorAll('[data-testid="sfc-condition-value"]')).toHaveLength(0);
  });
});

/** The harness signs in as an engineer; a viewer sees the same read-only chart. */
describe('a viewer', () => {
  it('sees the same chart, live', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { container } = await opened(bottling());
    await live(container);
    expect(step(container, 3).state).toBe('active');
    expect(byTestId(container, 'sfc-read-only')).not.toBeNull();
  });
});
