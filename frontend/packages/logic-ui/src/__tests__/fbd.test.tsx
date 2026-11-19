// A Function Block Diagram program in the workbench (ADR-013 L3c-1): its language in the tree,
// its diagram drawn read-only from the TC6 document, each diagnostic at its element, and its live
// state, drawn only from watched values and only while the runtime runs the version on screen:
// each line with the value the compiler kept for its output (a BOOL on or off, a number as its
// value), a function that did not run as not computed, a network the scan did not run as not
// executed, UNKNOWN where a value cannot be read, FORCED kept. Every assertion reads a rendered
// element by its data-testid.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CheckResult, ValueProbe, Variable } from '../bridge';
import { fbdNetworks, readFbd } from '../fbd';
import { ProgramEditor } from '../ProgramEditor';
import { ProgramsTree } from '../ProgramsTree';
import { usePrograms } from '../programs';
import {
  byLabel,
  byTestId,
  click,
  doing,
  endTest,
  entryOf,
  fbdOf,
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
  useController,
  useShell,
  watchOf,
  timeScale,
  type Backend,
} from './harness';

const VERSION = 'aaaaaaaaaaaa';

const pos = (x: number, y: number) => `<position x="${x}" y="${y}"/>`;
const conn = (from: string) => {
  const [id, formal] = from.split('.');
  return `<connectionPointIn><connection refLocalId="${id}"${formal ? ` formalParameter="${formal}"` : ''}/></connectionPointIn>`;
};
const inVar = (id: number, x: number, y: number, expression: string) =>
  `<inVariable localId="${id}">${pos(x, y)}<connectionPointOut/><expression>${expression}</expression></inVariable>`;
const outVar = (id: number, x: number, y: number, expression: string, from: string) =>
  `<outVariable localId="${id}">${pos(x, y)}${conn(from)}<expression>${expression}</expression></outVariable>`;
/** A block; an input is `[formal, '7' (a line from 7) or '=text' (an expression), negated]`. */
const block = (id: number, x: number, y: number, type: string, inputs: Array<[string, string, boolean?]>, outputs: Array<[string, boolean?]>) =>
  `<block localId="${id}" typeName="${type}">${pos(x, y)}<inputVariables>${inputs
    .map(
      ([formal, from, negated]) =>
        `<variable formalParameter="${formal}"${negated ? ' negated="true"' : ''}>${
          from.startsWith('=') ? `<connectionPointIn><expression>${from.slice(1)}</expression></connectionPointIn>` : conn(from)
        }</variable>`
    )
    .join('')}</inputVariables><inOutVariables/><outputVariables>${outputs
    .map(([formal, negated]) => `<variable formalParameter="${formal}"${negated ? ' negated="true"' : ''}><connectionPointOut/></variable>`)
    .join('')}</outputVariables></block>`;

/**
 * Four networks, the last written first in the document: `a AND NOT b` to `q`; `ADD(x, 5)` under
 * EN `en` to `s`, its ENO to `e`; `skip` to a jump to L; `MUL(x, 2)` through the connector LINE to
 * its continuation, to `m`; then the label L.
 */
const MAIN = fbdOf(
  'Main',
  [
    inVar(30, 0, 260, 'x'),
    block(31, 120, 260, 'MUL', [['IN1', '30'], ['IN2', '=2']], [['OUT']]),
    `<connector localId="32" name="LINE">${pos(240, 260)}${conn('31')}</connector>`,
    `<continuation localId="33" name="LINE">${pos(0, 300)}<connectionPointOut/></continuation>`,
    outVar(34, 120, 300, 'm', '33'),
    `<label localId="40" label="L">${pos(0, 340)}</label>`,
    inVar(1, 0, 0, 'a'),
    inVar(2, 0, 40, 'b'),
    block(3, 120, 0, 'AND', [['IN1', '1'], ['IN2', '2', true]], [['OUT']]),
    outVar(4, 240, 0, 'q', '3'),
    inVar(10, 0, 100, 'x'),
    inVar(12, 0, 140, 'en'),
    block(11, 120, 100, 'ADD', [['EN', '12'], ['IN1', '10'], ['IN2', '=5']], [['ENO'], ['OUT']]),
    outVar(13, 240, 100, 's', '11.OUT'),
    outVar(14, 240, 140, 'e', '11.ENO'),
    inVar(20, 0, 200, 'skip'),
    `<jump localId="21" label="L">${pos(120, 200)}${conn('20')}</jump>`,
    `<comment localId="90" height="20" width="160">${pos(300, 0)}<content><xhtml:p xmlns:xhtml="http://www.w3.org/1999/xhtml">Every element</xhtml:p></content></comment>`,
  ].join('\n')
);

const variable = (path: string, type: string, hidden = false): Variable => ({ path, type, section: 'local', bindable: !hidden, hidden });
/** What the logic module says keeps MAIN's values, as the compiler gives it for this document. */
const VALUES: ValueProbe[] = [
  { local_id: 1, output: '', variable: '__FBD_1', negated: false },
  { local_id: 1, network: [1, 3, 4, 2], variable: '__NET_1', negated: false },
  { local_id: 2, output: '', variable: '__FBD_2', negated: false },
  { local_id: 3, output: 'OUT', variable: '__FBD_3', negated: false },
  { local_id: 10, output: '', variable: '__FBD_10', negated: false },
  { local_id: 10, network: [10, 11, 13, 12, 14], variable: '__NET_10', negated: false },
  { local_id: 11, output: 'ENO', variable: '__FBD_11_ENO', negated: false },
  { local_id: 11, output: 'OUT', variable: '__FBD_11', negated: false, ran: '__FBD_11_ENO' },
  { local_id: 12, output: '', variable: '__FBD_12', negated: false },
  { local_id: 20, output: '', variable: '__FBD_20', negated: false },
  { local_id: 20, network: [20, 21], variable: '__NET_20', negated: false },
  { local_id: 30, output: '', variable: '__FBD_30', negated: false },
  { local_id: 30, network: [30, 31, 34], variable: '__NET_30', negated: false },
  { local_id: 31, output: 'OUT', variable: '__FBD_31', negated: false },
  { local_id: 33, output: '', variable: '__FBD_31', negated: false },
];
const TYPES: Record<string, string> = { __FBD_10: 'INT', __FBD_11: 'INT', __FBD_30: 'INT', __FBD_31: 'INT' };
const compiles = (values: ValueProbe[] = VALUES) => (): CheckResult => ({
  ok: true,
  diagnostics: [],
  program: 'Main',
  version: VERSION,
  variables: [
    ...['a', 'b', 'q', 'en', 'e', 'skip'].map((p) => variable(p, 'BOOL')),
    ...['x', 's', 'm'].map((p) => variable(p, 'INT')),
    ...[...new Set(values.flatMap((p) => (p.variable === undefined ? [] : [p.variable])))].map((p) => variable(p, TYPES[p] ?? 'BOOL', true)),
  ],
  code_size: 100,
  memory_size: 20,
  binding_problems: [],
  values,
});

const bool = (value: boolean) => ({ type: 'BOOL', value });
const int = (value: number) => ({ type: 'INT', value });
/** a, NOT b: q on; x 2, en: s is 7, m is 4; skip off: every network ran. */
const RUNNING = {
  a: bool(true),
  b: bool(false),
  q: bool(true),
  x: int(2),
  en: bool(true),
  s: int(7),
  e: bool(true),
  skip: bool(false),
  m: int(4),
  __FBD_1: bool(true),
  __FBD_2: bool(false),
  __FBD_3: bool(true),
  __FBD_10: int(2),
  __FBD_11_ENO: bool(true),
  __FBD_11: int(7),
  __FBD_12: bool(true),
  __FBD_20: bool(false),
  __FBD_30: int(2),
  __FBD_31: int(4),
  __NET_1: bool(true),
  __NET_10: bool(true),
  __NET_20: bool(true),
  __NET_30: bool(true),
};

const main = (over: Partial<Backend> = {}): Backend => ({
  programs: [{ entry: entryOf('Main', { language: 'fbd' }), source: MAIN }],
  check: compiles(),
  runtimes: runtimesOf(runtimeOf('Main', { version: VERSION })),
  watch: watchOf(RUNNING),
  ...over,
});

async function opened(backend: Backend, name = 'Main') {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant' });
  await settle();
  const container = render(<ProgramEditor name={name} />);
  await settle();
  await until('the Function Block Diagram is drawn', () => byTestId(container, 'fbd-view') ?? byTestId(container, 'fbd-unreadable'));
  return { calls, container };
}

const element = (container: HTMLElement, id: number) => {
  const found = container.querySelector(`[data-testid="fbd-element"][data-local-id="${id}"]`);
  if (!found) throw new Error(`no element ${id}`);
  return found;
};
const line = (container: HTMLElement, from: number, to: number) => {
  const found = container.querySelector(`[data-testid="fbd-line"][data-from="${from}"][data-to="${to}"]`);
  if (!found) throw new Error(`no line ${from} to ${to}`);
  return found.getAttribute('data-state');
};
const value = (container: HTMLElement, from: number, output: string) => {
  const found = container.querySelector(`[data-testid="fbd-value"][data-from="${from}"][data-output="${output}"]`);
  return found === null ? null : { state: found.getAttribute('data-state'), text: found.textContent, forced: found.getAttribute('data-forced') };
};
const live = async (container: HTMLElement) =>
  until('the diagram is live', () => (byTestId(container, 'fbd-live')?.getAttribute('data-live') === 'live' ? true : null));

vi.setConfig({ testTimeout: 20_000 * timeScale });

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
});

describe('a Function Block Diagram in the tree', () => {
  it("shows each program's language, and adds one in FBD", async () => {
    const backend = main();
    backend.programs.unshift({ entry: entryOf('Tank'), source: 'PROGRAM Tank\nEND_PROGRAM\n' });
    const { calls } = await opened(backend);
    const tree = render(<ProgramsTree openEditor={() => undefined} />);
    expect(texts(tree, 'tree-program-language')).toEqual(['ST', 'FBD']);

    click(byTestId(tree, 'tree-add-program'));
    typeInto(byLabel(tree, 'Program name'), 'Blender');
    const language = byLabel(tree, 'Program language');
    if (!(language instanceof HTMLSelectElement)) throw new Error('no language');
    typeInto(language, 'fbd');
    await doing(() => byLabel(tree, 'Program name')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));

    const made = calls.filter((call) => call.method === 'POST' && call.path.endsWith('/logic/programs'));
    expect(made.map((call) => call.body)).toEqual([{ name: 'Blender', language: 'fbd' }]);
    expect(texts(tree, 'tree-program-language')).toEqual(['ST', 'FBD', 'FBD']);
  });
});

describe('drawing a Function Block Diagram', () => {
  it('draws every kind of element where the document puts it, and numbers the networks in evaluation order', async () => {
    const { container } = await opened(main({ check: () => ({ ok: true, diagnostics: [], program: 'Main', version: 'other', variables: [] }) }));

    // Read-only, for an engineer too: no text area, no editor.
    expect(container.querySelector('textarea')).toBeNull();
    expect(byTestId(container, 'ld-editor')).toBeNull();
    const kinds = [...container.querySelectorAll('[data-testid="fbd-element"]')].map(
      (e) => `${e.getAttribute('data-kind')}:${e.getAttribute('data-local-id')}`
    );
    expect(kinds.sort()).toEqual(
      [
        'inVariable:1',
        'inVariable:2',
        'block:3',
        'outVariable:4',
        'inVariable:10',
        'block:11',
        'inVariable:12',
        'outVariable:13',
        'outVariable:14',
        'inVariable:20',
        'jump:21',
        'inVariable:30',
        'block:31',
        'connector:32',
        'continuation:33',
        'outVariable:34',
        'label:40',
        'comment:90',
      ].sort()
    );
    // Each block with its type and pins, an expression written at its pin, a negated pin marked.
    expect(element(container, 3).textContent).toContain('AND');
    const negated = [...element(container, 3).querySelectorAll('[data-testid="fbd-pin"]')].map(
      (p) => `${p.getAttribute('data-pin')}:${p.getAttribute('data-negated') ?? 'false'}`
    );
    expect(negated).toEqual(['IN1:false', 'IN2:true', 'OUT:false']);
    expect(element(container, 3).querySelectorAll('circle')).toHaveLength(1);
    expect(element(container, 11).textContent).toContain('IN2 := 5');
    expect(element(container, 21).textContent).toContain('>> L');
    expect(element(container, 40).textContent).toContain('L:');
    expect(element(container, 32).textContent).toContain('LINE');
    expect(element(container, 90).textContent).toContain('Every element');
    // Every line, the continuation's from the continuation.
    expect(line(container, 2, 3)).toBe('none');
    expect(line(container, 31, 32)).toBe('none');
    expect(line(container, 33, 34)).toBe('none');
    expect(line(container, 11, 14)).toBe('none');
    // Networks top to bottom, whatever the document's order; the connector joins its line.
    expect(
      [...container.querySelectorAll('[data-testid="fbd-network"]')].map((n) => `${n.textContent}:${n.getAttribute('data-ids')}`)
    ).toEqual(['1:1,3,4,2', '2:10,11,13,12,14', '3:20,21', '4:30,31,34']);
    // Not the running version: nothing is drawn live.
    expect(byTestId(container, 'fbd-live')?.getAttribute('data-live')).toBe('none');
    expect(container.querySelectorAll('[data-testid="fbd-value"]')).toHaveLength(0);
    expect(container.querySelectorAll('[data-testid="fbd-element"]:not([data-state="none"])')).toHaveLength(0);
  });

  it('lists an element FBD does not have as not drawn, and reads the networks as the compiler does', () => {
    const read = readFbd(fbdOf('Main', `${inVar(1, 0, 0, 'a')}<contact localId="9">${pos(0, 0)}</contact>`));
    if (!read.ok) throw new Error(read.error);
    expect(read.pous[0]?.notDrawn).toEqual(['contact 9']);
    const main = readFbd(MAIN);
    if (!main.ok) throw new Error(main.error);
    expect(fbdNetworks(main.pous[0] as never)).toEqual([[1, 3, 4, 2], [10, 11, 13, 12, 14], [20, 21], [30, 31, 34]]);
  });

  it('draws each diagnostic at the element it names, and lists where it is', async () => {
    const { container } = await opened(
      main({
        check: () => ({
          ok: false,
          diagnostics: [
            {
              line: 9,
              col: 1,
              end_line: 9,
              end_col: 40,
              message: 'xx is not declared',
              element: { pou: 'Main', local_id: 10, x: 0, y: 100, text: { field: 'expression', col: 1, end_col: 3 } },
            },
          ],
        }),
      })
    );
    const marks = [...container.querySelectorAll('[data-testid="fbd-diagnostic"]')];
    expect(marks.map((m) => m.getAttribute('data-local-id'))).toEqual(['10']);
    expect(marks[0]?.getAttribute('x')).toBe('-4');
    expect(marks[0]?.getAttribute('y')).toBe('96');
    expect(marks[0]?.textContent).toBe('xx is not declared');
    expect(texts(container, 'diagnostic')).toEqual(['Main element 10xx is not declared']);
    expect(byTestId(container, 'fbd-live')?.getAttribute('data-live')).toBe('none');
  });

  it('says why a document that is not a TC6 project is not drawn', async () => {
    const { container } = await opened(main({ programs: [{ entry: entryOf('Main', { language: 'fbd' }), source: 'PROGRAM Main END_PROGRAM' }] }));
    expect(byTestId(container, 'fbd-unreadable')?.textContent).toContain('not well-formed XML');
    expect(byTestId(container, 'fbd-diagram')).toBeNull();
  });
});

describe('the live state of a running Function Block Diagram', () => {
  it('draws each line with the value its output kept: a BOOL on or off, a number as its value', async () => {
    const { calls, container } = await opened(main());
    await live(container);
    expect(line(container, 1, 3)).toBe('on');
    expect(line(container, 2, 3)).toBe('off');
    expect(line(container, 3, 4)).toBe('on');
    expect(value(container, 3, 'OUT')).toEqual({ state: 'on', text: 'TRUE', forced: null });
    expect(value(container, 2, '')).toEqual({ state: 'off', text: 'FALSE', forced: null });
    // A number is written at its output, as the variables table writes it.
    expect(line(container, 10, 11)).toBe('value');
    expect(value(container, 10, '')).toEqual({ state: 'value', text: '2', forced: null });
    expect(value(container, 11, 'OUT')).toEqual({ state: 'value', text: '7', forced: null });
    expect(value(container, 11, 'ENO')).toEqual({ state: 'on', text: 'TRUE', forced: null });
    // Through the connector: the continuation carries what the MUL gave.
    expect(line(container, 31, 32)).toBe('value');
    expect(line(container, 33, 34)).toBe('value');
    expect(value(container, 33, '')?.text).toBe('4');
    // Under a variable its own value.
    expect(element(container, 34).querySelector('[data-testid="fbd-variable-value"]')?.textContent).toBe('4');
    // The hidden variables were asked for by name, with what the elements read.
    const asked = calls.filter((call) => call.path.endsWith('/watch')).at(-1)?.body as { variables: string[] };
    expect(asked.variables).toEqual(expect.arrayContaining(['__FBD_11', '__FBD_11_ENO', '__NET_30', 'x', 'm']));
    for (const id of [1, 3, 11, 31, 34]) expect(element(container, id).getAttribute('data-state')).toBe('on');
  });

  it('draws a function that did not run as not computed, never with an older value', async () => {
    const { container } = await opened(main({ watch: watchOf({ ...RUNNING, en: bool(false), __FBD_12: bool(false), __FBD_11_ENO: bool(false), e: bool(false) }) }));
    await live(container);
    expect(value(container, 11, 'OUT')).toEqual({ state: 'not-computed', text: 'not computed', forced: null });
    expect(line(container, 11, 13)).toBe('not-computed');
    expect(value(container, 11, 'ENO')).toEqual({ state: 'off', text: 'FALSE', forced: null });
    expect(line(container, 11, 14)).toBe('off');
    expect(container.querySelector('[data-testid="fbd-value"][data-from="11"][data-output="OUT"]')?.textContent).not.toContain('7');
  });

  it('draws a network the scan did not run as not executed, its FORCED marks kept', async () => {
    const { container } = await opened(
      main({ watch: watchOf({ ...RUNNING, skip: bool(true), __FBD_20: bool(true), __NET_30: bool(false), x: { type: 'INT', value: 2, forced: true } }) })
    );
    await live(container);
    const network = [...container.querySelectorAll('[data-testid="fbd-network"]')].map((n) => n.getAttribute('data-state'));
    expect(network).toEqual(['on', 'on', 'on', 'not-executed']);
    for (const id of [30, 31, 34]) expect(element(container, id).getAttribute('data-state')).toBe('not-executed');
    expect(line(container, 30, 31)).toBe('not-executed');
    expect(line(container, 33, 34)).toBe('not-executed');
    // No value from a scan that did not run it.
    expect(value(container, 31, 'OUT')).toEqual({ state: 'not-executed', text: '', forced: null });
    // x is forced: marked where it is drawn in a network that ran, and not valued where it did not.
    expect(element(container, 10).getAttribute('data-forced')).toBe('true');
    expect(element(container, 10).querySelector('[data-testid="variable-forced"]')?.textContent).toBe('FORCED');
    expect(element(container, 30).querySelector('[data-testid="fbd-variable-value"]')).toBeNull();
  });

  it('marks a forced output FORCED where its value is written', async () => {
    const { container } = await opened(main({ watch: watchOf({ ...RUNNING, __FBD_3: { type: 'BOOL', value: false, forced: true } }) }));
    await live(container);
    expect(value(container, 3, 'OUT')).toEqual({ state: 'off', text: 'FALSE FORCED', forced: 'true' });
  });

  it('draws UNKNOWN, never a value, when one value or all values cannot be read', async () => {
    const { __FBD_10: _kept, __FBD_11_ENO: _ran, ...without } = RUNNING;
    const backend = main({ watch: watchOf(without) });
    const { container } = await opened(backend);
    await live(container);
    expect(value(container, 10, '')).toEqual({ state: 'unknown', text: 'UNKNOWN', forced: null });
    expect(line(container, 10, 11)).toBe('unknown');
    // Whether the ADD ran cannot be read: its result is UNKNOWN, not 7.
    expect(value(container, 11, 'OUT')).toEqual({ state: 'unknown', text: 'UNKNOWN', forced: null });
    expect(line(container, 1, 3)).toBe('on');

    backend.watch = 'fails';
    await doing(() => usePrograms.getState().readWatch('Main', ['a']));
    expect(byTestId(container, 'fbd-live')?.getAttribute('data-live')).toBe('unknown');
    for (const [from, to] of [
      [1, 3],
      [3, 4],
      [10, 11],
      [33, 34],
    ] as const) {
      expect(line(container, from, to)).toBe('unknown');
    }
    expect([...container.querySelectorAll('[data-testid="fbd-network"]')].map((n) => n.getAttribute('data-state'))).toEqual([
      'unknown',
      'unknown',
      'unknown',
      'unknown',
    ]);
    expect(container.querySelectorAll('[data-state="on"], [data-state="off"], [data-state="value"]')).toHaveLength(0);
  });

  it('draws nothing live of a program in STOP', async () => {
    const { container } = await opened(
      main({ runtimes: runtimesOf(runtimeOf('Main', { version: VERSION, mode: 'stop' })), watch: watchOf(RUNNING, 'stop') })
    );
    await until('the state is read', () => (byTestId(container, 'fbd-live')?.textContent?.includes('STOP') ? true : null));
    expect(byTestId(container, 'fbd-live')?.textContent).toContain('so nothing is computed');
    expect(container.querySelectorAll('[data-testid="fbd-value"]')).toHaveLength(0);
  });
});
