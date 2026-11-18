// A Ladder Diagram program in the workbench (ADR-013 L3b-1, L3b-2a): its language in the tree,
// its diagram drawn read-only from the TC6 document, each diagnostic at its element, and its
// live state, drawn only from watched values and only while the runtime runs the version on
// screen: each element, link and junction with the power the compiler kept for it, a rung the
// scan did not run as not executed, UNKNOWN where a value cannot be read. Every assertion reads
// a rendered element by its data-testid.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CheckResult, PowerProbe, Variable } from '../bridge';
import { readLadder, networks } from '../ladder';
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
  ladderOf,
  project,
  render,
  resetStores,
  runtimeOf,
  runtimesOf,
  server,
  settle,
  texts,
  typeInto,
  unmountAll,
  until,
  useController,
  useSession,
  useShell,
  watchOf,
  type Backend,
} from './harness';

const VERSION = 'aaaaaaaaaaaa';

const contact = (id: number, x: number, y: number, variable: string, from: string, attrs = '') =>
  `<contact localId="${id}"${attrs}><position x="${x}" y="${y}"/><connectionPointIn><connection refLocalId="${from}"/></connectionPointIn><connectionPointOut/><variable>${variable}</variable></contact>`;
const coil = (id: number, x: number, y: number, variable: string, from: string, attrs = '') =>
  `<coil localId="${id}"${attrs}><position x="${x}" y="${y}"/><connectionPointIn><connection refLocalId="${from}"/></connectionPointIn><connectionPointOut/><variable>${variable}</variable></coil>`;

/**
 * Two rungs, the lower one written first in the document: a timer `t` on `go` to the coil
 * `done`; and above it `a` and NOT `b` in series, a branch point (contact 11) feeding the coil
 * `q` and the negated coil `r`.
 */
const MAIN = ladderOf(
  'Main',
  [
    contact(41, 40, 100, 'go', '1'),
    `<block localId="40" typeName="TON" instanceName="t"><position x="120" y="100"/>
       <inputVariables>
         <variable formalParameter="IN"><connectionPointIn><connection refLocalId="41"/></connectionPointIn></variable>
         <variable formalParameter="PT"><connectionPointIn><expression>T#1s</expression></connectionPointIn></variable>
       </inputVariables><inOutVariables/>
       <outputVariables><variable formalParameter="Q"><connectionPointOut/></variable><variable formalParameter="ET"><connectionPointOut/></variable></outputVariables>
     </block>`,
    `<coil localId="42"><position x="300" y="100"/><connectionPointIn><connection refLocalId="40" formalParameter="Q"/></connectionPointIn><connectionPointOut/><variable>done</variable></coil>`,
    contact(10, 40, 20, 'a', '1'),
    contact(11, 100, 20, 'b', '10', ' negated="true"'),
    coil(12, 300, 20, 'q', '11'),
    coil(13, 300, 50, 'r', '11', ' negated="true"'),
  ].join('\n')
);

const variable = (path: string, type: string, hidden = false): Variable => ({ path, type, section: 'local', bindable: !hidden, hidden });
/** The power the compiler keeps for MAIN: each element's output, the timer's Q, each rung's flag. */
const KEPT = ['__LD_10', '__LD_11', '__LD_12', '__LD_13', '__LD_40_Q', '__LD_41', '__LD_42', '__NET_10', '__NET_41'];
const VARIABLES = [
  variable('a', 'BOOL'),
  variable('b', 'BOOL'),
  variable('go', 'BOOL'),
  variable('q', 'BOOL'),
  variable('r', 'BOOL'),
  variable('done', 'BOOL'),
  variable('t.Q', 'BOOL'),
  ...KEPT.map((path) => variable(path, 'BOOL', true)),
];
/** What the logic module says keeps MAIN's power, as the compiler gives it for this document. */
const POWER: PowerProbe[] = [
  { local_id: 1, output: '', on: true },
  { local_id: 10, output: '', variable: '__LD_10', negated: false },
  { local_id: 10, network: [10, 11, 12, 13], variable: '__NET_10', negated: false },
  { local_id: 11, output: '', variable: '__LD_11', negated: false },
  { local_id: 12, output: '', variable: '__LD_12', negated: false },
  { local_id: 13, output: '', variable: '__LD_13', negated: false },
  { local_id: 40, output: 'Q', variable: '__LD_40_Q', negated: false },
  { local_id: 41, output: '', variable: '__LD_41', negated: false },
  { local_id: 41, network: [41, 40, 42], variable: '__NET_41', negated: false },
  { local_id: 42, output: '', variable: '__LD_42', negated: false },
];
const compiles = (power: PowerProbe[] = POWER) => (): CheckResult => ({
  ok: true,
  diagnostics: [],
  program: 'Main',
  version: VERSION,
  // The hidden variables a check lists are those its power names.
  variables: [
    ...VARIABLES.filter((v) => v.hidden !== true),
    ...power.flatMap((p) => (p.variable === undefined ? [] : [variable(p.variable, 'BOOL', true)])),
  ],
  code_size: 100,
  memory_size: 20,
  binding_problems: [],
  power,
});

const bool = (value: boolean) => ({ type: 'BOOL', value });
/** a, NOT b: the series conducts and powers both coils; go is off, so is the timer. */
const RUNNING = {
  a: bool(true),
  b: bool(false),
  go: bool(false),
  q: bool(true),
  r: bool(false),
  done: bool(false),
  't.Q': bool(false),
  __LD_10: bool(true),
  __LD_11: bool(true),
  __LD_12: bool(true),
  __LD_13: bool(true),
  __LD_40_Q: bool(false),
  __LD_41: bool(false),
  __LD_42: bool(false),
  __NET_10: bool(true),
  __NET_41: bool(true),
};

const main = (over: Partial<Backend> = {}): Backend => ({
  programs: [{ entry: entryOf('Main', { language: 'ld' }), source: MAIN }],
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
  await until('the Ladder Diagram is drawn', () => byTestId(container, 'ld-view') ?? byTestId(container, 'ld-editor') ?? byTestId(container, 'ld-unreadable'));
  return { calls, container };
}

const element = (container: HTMLElement, id: number) => {
  const found = container.querySelector(`[data-testid="ld-element"][data-local-id="${id}"]`);
  if (!found) throw new Error(`no element ${id}`);
  return found;
};
const stateOf = (container: HTMLElement, id: number) => element(container, id).getAttribute('data-state');
const valueOf = (container: HTMLElement, id: number) => element(container, id).querySelector('[data-testid="ld-value"]')?.textContent ?? null;
const link = (container: HTMLElement, from: number, to: number) => {
  const found = container.querySelector(`[data-testid="ld-link"][data-from="${from}"][data-to="${to}"]`);
  if (!found) throw new Error(`no link ${from} to ${to}`);
  return found.getAttribute('data-state');
};

vi.setConfig({ testTimeout: 20_000 });

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
});

describe('a Ladder Diagram in the tree', () => {
  it('shows each program\'s language, and adds one in the language chosen', async () => {
    const backend = main();
    backend.programs.unshift({ entry: entryOf('Tank'), source: 'PROGRAM Tank\nEND_PROGRAM\n' });
    const { calls } = await opened(backend);
    const tree = render(<ProgramsTree openEditor={() => undefined} />);
    expect(texts(tree, 'tree-program-language')).toEqual(['ST', 'LD']);

    click(byTestId(tree, 'tree-add-program'));
    typeInto(byLabel(tree, 'Program name'), 'Mixer');
    const language = byLabel(tree, 'Program language');
    if (!(language instanceof HTMLSelectElement)) throw new Error('no language');
    typeInto(language, 'ld');
    await doing(() => byLabel(tree, 'Program name')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));

    const made = calls.filter((call) => call.method === 'POST' && call.path.endsWith('/logic/programs'));
    expect(made.map((call) => call.body)).toEqual([{ name: 'Mixer', language: 'ld' }]);
    expect(texts(tree, 'tree-program-language')).toEqual(['ST', 'LD', 'LD']);
  });
});

/** Signed in as a viewer: the read-only view, drawn where the document puts each element. */
const asViewer = () =>
  useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });

describe('drawing a Ladder Diagram', () => {
  it('draws every kind of element where the document puts it, and numbers the rungs in evaluation order', async () => {
    asViewer();
    const shapes = ladderOf(
      'Shapes',
      [
        // Written bottom first: the rung at y 300 is the last evaluated.
        `<return localId="80"><position x="200" y="300"/><connectionPointIn><connection refLocalId="81"/></connectionPointIn></return>`,
        contact(81, 40, 300, 'stop', '1'),
        `<label localId="70" label="END"><position x="0" y="260"/></label>`,
        contact(61, 40, 220, 'skip', '1'),
        `<jump localId="60" label="END"><position x="200" y="220"/><connectionPointIn><connection refLocalId="61"/></connectionPointIn></jump>`,
        contact(20, 40, 20, 'a', '1', ' edge="rising"'),
        coil(21, 200, 20, 's', '20', ' storage="set"'),
        contact(30, 40, 60, 'b', '1', ' edge="falling"'),
        coil(31, 200, 60, 's', '30', ' storage="reset"'),
        coil(32, 200, 90, 'p', '30', ' edge="rising"'),
        coil(33, 200, 120, 'n', '30', ' edge="falling"'),
        `<inVariable localId="50"><position x="40" y="160"/><connectionPointOut/><expression>level</expression></inVariable>`,
        `<block localId="51" typeName="MUL"><position x="120" y="160"/>
           <inputVariables>
             <variable formalParameter="IN1"><connectionPointIn><connection refLocalId="50"/></connectionPointIn></variable>
             <variable formalParameter="IN2"><connectionPointIn><expression>10</expression></connectionPointIn></variable>
           </inputVariables><inOutVariables/>
           <outputVariables><variable formalParameter="OUT"><connectionPointOut/></variable></outputVariables>
         </block>`,
        `<outVariable localId="52"><position x="260" y="160"/><connectionPointIn><connection refLocalId="51"/></connectionPointIn><expression>speed</expression></outVariable>`,
        `<inOutVariable localId="53"><position x="260" y="190"/><connectionPointIn><connection refLocalId="51"/></connectionPointIn><connectionPointOut/><expression>total</expression></inOutVariable>`,
        `<comment localId="90" height="20" width="160"><position x="40" y="0"/><content><xhtml:p xmlns:xhtml="http://www.w3.org/1999/xhtml">Every element</xhtml:p></content></comment>`,
      ].join('\n')
    );
    const { container } = await opened(
      { programs: [{ entry: entryOf('Shapes', { language: 'ld' }), source: shapes }], check: () => ({ ok: true, diagnostics: [], program: 'Shapes', version: 'other', variables: [] }), runtimes: runtimesOf() },
      'Shapes'
    );

    // Read-only: the diagram, never a text area.
    expect(container.querySelector('textarea')).toBeNull();
    const kinds = [...container.querySelectorAll('[data-testid="ld-element"]')].map(
      (e) => `${e.getAttribute('data-kind')}:${e.getAttribute('data-local-id')}`
    );
    expect(kinds.sort()).toEqual(
      [
        'leftPowerRail:1',
        'rightPowerRail:2',
        'contact:20',
        'contact:30',
        'contact:61',
        'contact:81',
        'coil:21',
        'coil:31',
        'coil:32',
        'coil:33',
        'block:51',
        'inVariable:50',
        'outVariable:52',
        'inOutVariable:53',
        'label:70',
        'jump:60',
        'return:80',
        'comment:90',
      ].sort()
    );
    // Each contact and coil says what it is: P and N for edges, S and R for storage.
    expect(element(container, 20).textContent).toContain('P');
    expect(element(container, 30).textContent).toContain('N');
    expect(element(container, 21).textContent).toContain('S');
    expect(element(container, 31).textContent).toContain('R');
    expect(element(container, 51).textContent).toContain('IN2 := 10');
    expect(element(container, 60).textContent).toContain('>> END');
    expect(element(container, 70).textContent).toContain('END:');
    expect(element(container, 90).textContent).toContain('Every element');
    // Rungs top to bottom, whatever the document's order (§8.2.7): the label is not a rung.
    expect(
      [...container.querySelectorAll('[data-testid="ld-rung"]')].map((r) => `${r.textContent}:${r.getAttribute('data-ids')}`)
    ).toEqual(['1:20,21', '2:30,31,32,33', '3:50,51,52,53', '4:61,60', '5:81,80']);
    // Not the running version: nothing is drawn live.
    expect(byTestId(container, 'ld-live')?.getAttribute('data-live')).toBe('none');
    expect(container.querySelectorAll('[data-testid="ld-element"]:not([data-state="none"])')).toHaveLength(0);
    expect(container.querySelectorAll('[data-testid="ld-value"]')).toHaveLength(0);
  });

  it('draws each diagnostic at the element it names, and lists where it is', async () => {
    asViewer();
    const { container } = await opened(
      main({
        check: () => ({
          ok: false,
          diagnostics: [
            {
              line: 18,
              col: 9,
              end_line: 18,
              end_col: 31,
              message: 'bb is not declared',
              element: { pou: 'Main', local_id: 11, x: 100, y: 20, text: { field: 'variable', col: 1, end_col: 3 } },
            },
            { line: 5, col: 1, end_line: 5, end_col: 2, message: 'BYTE is not a type', element: { pou: 'Main', variable: 'z' } },
          ],
        }),
      })
    );

    const marks = [...container.querySelectorAll('[data-testid="ld-diagnostic"]')];
    expect(marks.map((m) => m.getAttribute('data-local-id'))).toEqual(['11']);
    // Around the element where the document puts it.
    expect(marks[0]?.getAttribute('x')).toBe('96');
    expect(marks[0]?.getAttribute('y')).toBe('16');
    expect(marks[0]?.textContent).toBe('bb is not declared');
    expect(texts(container, 'diagnostic')).toEqual(['Main element 11bb is not declared', 'Main variable zBYTE is not a type']);
    // What does not compile is not drawn live.
    expect(byTestId(container, 'ld-live')?.getAttribute('data-live')).toBe('none');
    expect(stateOf(container, 10)).toBe('none');
  });

  it('says why a document that is not a TC6 project is not drawn', async () => {
    const { container } = await opened(main({ programs: [{ entry: entryOf('Main', { language: 'ld' }), source: 'PROGRAM Main END_PROGRAM' }] }));
    expect(byTestId(container, 'ld-unreadable')?.textContent).toContain('not well-formed XML');
    expect(byTestId(container, 'ld-diagram')).toBeNull();
  });
});

describe('the live state of a running Ladder Diagram', () => {
  it('draws each element and link with the power its rung kept, and each contact and coil with its variable', async () => {
    // The read-only view numbers rungs through the program; an engineer's editor numbers them per
    // network (L3b-2b), so this reads the view as a viewer sees it.
    asViewer();
    const { calls, container } = await opened(main());

    expect(byTestId(container, 'ld-live')?.getAttribute('data-live')).toBe('live');
    // a is TRUE and b FALSE: the series conducts, and both coils have power.
    for (const id of [10, 11, 12, 13]) expect(stateOf(container, id)).toBe('on');
    expect(stateOf(container, 41)).toBe('off');
    expect(stateOf(container, 42)).toBe('off');
    // The negated coil r has power, and its variable is FALSE: both are drawn, each for what it is.
    expect(element(container, 13).getAttribute('data-variable-state')).toBe('off');
    expect(valueOf(container, 13)).toBe('OFF');
    expect(valueOf(container, 11)).toBe('ON');
    // Every link from the power its source kept: none is left without a state, none guessed.
    expect(link(container, 1, 10)).toBe('on');
    expect(link(container, 10, 11)).toBe('on');
    expect(link(container, 11, 12)).toBe('on');
    expect(link(container, 11, 13)).toBe('on');
    expect(link(container, 1, 41)).toBe('on');
    expect(link(container, 41, 40)).toBe('off');
    expect(link(container, 40, 42)).toBe('off');
    expect(texts(container, 'ld-rung')).toEqual(['1', '2']);
    expect([...container.querySelectorAll('[data-testid="ld-rung"]')].map((r) => r.getAttribute('data-state'))).toEqual(['on', 'on']);
    // Powered solid, not powered dashed.
    const path = (from: number, to: number) => container.querySelector(`[data-testid="ld-link"][data-from="${from}"][data-to="${to}"]`);
    expect(path(10, 11)?.getAttribute('stroke-dasharray')).toBeNull();
    expect(path(41, 40)?.getAttribute('stroke-dasharray')).toBe('6 4');
    // The front-end's variables are watched for the diagram and not listed in the table.
    const asked = calls.filter((call) => call.path.endsWith('/runtimes/Main/watch')).at(-1);
    expect((asked?.body as { variables: string[] }).variables).toEqual(expect.arrayContaining(KEPT));
    expect(texts(container, 'variable-row').some((row) => row.includes('__'))).toBe(false);
    expect(texts(container, 'variable-row')).toHaveLength(7);
  });

  it('draws a contact with the power it passed on, which its variable alone does not say', async () => {
    // a is FALSE: the negated contact on b (FALSE) would conduct, but no power reaches it.
    const { container } = await opened(
      main({ watch: watchOf({ ...RUNNING, a: bool(false), __LD_10: bool(false), __LD_11: bool(false), __LD_12: bool(false), __LD_13: bool(false), q: bool(false), r: bool(true) }) })
    );
    expect(stateOf(container, 10)).toBe('off');
    expect(stateOf(container, 11)).toBe('off');
    expect(element(container, 11).getAttribute('data-variable-state')).toBe('on');
    expect(valueOf(container, 11)).toBe('ON');
    expect(link(container, 10, 11)).toBe('off');
    expect(link(container, 11, 13)).toBe('off');
    expect(link(container, 1, 10)).toBe('on');
  });

  it('draws where links join with the junction’s power, and each branch with its own', async () => {
    // Start and the motor's own contact in parallel, then NOT stop, then the motor's coil.
    const sealIn = ladderOf(
      'Main',
      [
        contact(2, 40, 20, 'start', '1'),
        contact(4, 40, 60, 'motor', '1'),
        `<contact localId="3" negated="true"><position x="160" y="20"/><connectionPointIn><connection refLocalId="2"/><connection refLocalId="4"/></connectionPointIn><connectionPointOut/><variable>stop</variable></contact>`,
        coil(5, 300, 20, 'motor', '3'),
      ].join('\n')
    );
    const power: PowerProbe[] = [
      { local_id: 1, output: '', on: true },
      { local_id: 2, output: '', variable: '__LD_2', negated: false },
      { local_id: 2, network: [2, 3, 5, 4], variable: '__NET_2', negated: false },
      { local_id: 3, output: '', variable: '__LD_3', negated: false },
      { local_id: 3, input: '', variable: '__LD_3_IN', negated: false },
      { local_id: 4, output: '', variable: '__LD_4', negated: false },
      { local_id: 5, output: '', variable: '__LD_5', negated: false },
    ];
    // The scan start went on: the seal contact read the motor before its coil wrote it.
    const values = {
      start: bool(true),
      stop: bool(false),
      motor: bool(true),
      __LD_2: bool(true),
      __LD_4: bool(false),
      __LD_3_IN: bool(true),
      __LD_3: bool(true),
      __LD_5: bool(true),
      __NET_2: bool(true),
    };
    const { container } = await opened(
      main({ programs: [{ entry: entryOf('Main', { language: 'ld' }), source: sealIn }], check: compiles(power), watch: watchOf(values) })
    );
    expect(link(container, 2, 3)).toBe('on');
    // The motor's contact had no power in that scan, though its variable is TRUE now.
    expect(stateOf(container, 4)).toBe('off');
    expect(link(container, 4, 3)).toBe('off');
    const joined = container.querySelectorAll('[data-testid="ld-junction"]');
    expect(joined).toHaveLength(1);
    expect(joined[0]?.getAttribute('data-to')).toBe('3');
    expect(joined[0]?.getAttribute('data-state')).toBe('on');
  });

  it('draws a rung the last scan did not run as not executed, its elements with no power, FORCED kept', async () => {
    const jumps = ladderOf(
      'Main',
      [
        contact(61, 40, 20, 'skip', '1'),
        `<jump localId="60" label="PAST"><position x="200" y="20"/><connectionPointIn><connection refLocalId="61"/></connectionPointIn></jump>`,
        contact(20, 40, 60, 'a', '1'),
        coil(21, 200, 60, 'q', '20'),
        `<label localId="70" label="PAST"><position x="0" y="100"/></label>`,
        contact(30, 40, 120, 'a', '1'),
        coil(31, 200, 120, 'r', '30'),
      ].join('\n')
    );
    const power: PowerProbe[] = [
      { local_id: 1, output: '', on: true },
      { local_id: 20, output: '', variable: '__LD_20', negated: false },
      { local_id: 20, network: [20, 21], variable: '__NET_20', negated: false },
      { local_id: 21, output: '', variable: '__LD_21', negated: false },
      { local_id: 30, output: '', variable: '__LD_30', negated: false },
      { local_id: 30, network: [30, 31], variable: '__NET_30', negated: false },
      { local_id: 31, output: '', variable: '__LD_31', negated: false },
      { local_id: 61, output: '', variable: '__LD_61', negated: false },
      { local_id: 61, network: [61, 60], variable: '__NET_61', negated: false },
    ];
    // The jump was taken: rung 2 did not run, and what it keeps is from a scan that did.
    const values = {
      skip: bool(true),
      a: { type: 'BOOL', value: false, forced: true },
      q: bool(true),
      r: bool(false),
      __LD_61: bool(true),
      __NET_61: bool(true),
      __LD_20: bool(true),
      __LD_21: bool(true),
      __NET_20: bool(false),
      __LD_30: bool(false),
      __LD_31: bool(false),
      __NET_30: bool(true),
    };
    const { container } = await opened(
      main({ programs: [{ entry: entryOf('Main', { language: 'ld' }), source: jumps }], check: compiles(power), watch: watchOf(values) })
    );
    expect([...container.querySelectorAll('[data-testid="ld-rung"]')].map((r) => `${r.getAttribute('data-ids')}:${r.getAttribute('data-state')}`)).toEqual([
      '61,60:on',
      '20,21:not-executed',
      '30,31:on',
    ]);
    for (const id of [20, 21]) {
      expect(stateOf(container, id)).toBe('not-executed');
      expect(valueOf(container, id)).toBeNull();
    }
    expect(link(container, 1, 20)).toBe('not-executed');
    expect(link(container, 20, 21)).toBe('not-executed');
    expect(element(container, 20).getAttribute('data-forced')).toBe('true');
    expect(element(container, 20).querySelector('[data-testid="variable-forced"]')?.textContent).toBe('FORCED');
    // The rungs that ran: as they were.
    expect(stateOf(container, 61)).toBe('on');
    expect(link(container, 1, 61)).toBe('on');
    expect(stateOf(container, 30)).toBe('off');
    expect(link(container, 30, 31)).toBe('off');
  });

  it('draws UNKNOWN, never a state, when the values cannot be read', async () => {
    const backend = main();
    const { container } = await opened(backend);
    expect(stateOf(container, 10)).toBe('on');

    backend.watch = 'fails';
    await doing(() => usePrograms.getState().readWatch('Main', ['a']));
    expect(byTestId(container, 'ld-live')?.getAttribute('data-live')).toBe('unknown');
    for (const id of [10, 11, 12, 13, 41, 42]) {
      expect(stateOf(container, id)).toBe('unknown');
      expect(valueOf(container, id)).toBe('UNKNOWN');
    }
    // Not even the rail's link: whether its rung ran cannot be read.
    for (const [from, to] of [
      [1, 10],
      [10, 11],
      [11, 12],
      [40, 42],
    ] as const) {
      expect(link(container, from, to)).toBe('unknown');
    }
    expect([...container.querySelectorAll('[data-testid="ld-rung"]')].map((r) => r.getAttribute('data-state'))).toEqual(['unknown', 'unknown']);
    expect(container.querySelectorAll('[data-state="on"], [data-state="off"], [data-state="not-executed"]')).toHaveLength(0);
  });

  it('draws one value the runtime does not have as UNKNOWN, and the others as they are', async () => {
    const { a: _a, __LD_11: _kept, ...without } = RUNNING;
    const { container } = await opened(main({ watch: watchOf(without) }));
    // The contact's power is kept, its variable is not read: each is drawn as it is.
    expect(stateOf(container, 10)).toBe('on');
    expect(valueOf(container, 10)).toBe('UNKNOWN');
    expect(element(container, 10).getAttribute('data-variable-state')).toBe('unknown');
    expect(stateOf(container, 11)).toBe('unknown');
    expect(link(container, 11, 12)).toBe('unknown');
    expect(stateOf(container, 12)).toBe('on');
    // A rung whose flag is not read: all of it UNKNOWN.
    const { __NET_41: _flag, ...unflagged } = RUNNING;
    unmountAll();
    resetStores();
    const again = (await opened(main({ watch: watchOf(unflagged) }))).container;
    expect(stateOf(again, 41)).toBe('unknown');
    expect(link(again, 1, 41)).toBe('unknown');
    expect(stateOf(again, 10)).toBe('on');
  });

  it('marks a forced variable FORCED where it is drawn', async () => {
    const { container } = await opened(
      main({ watch: watchOf({ ...RUNNING, a: { type: 'BOOL', value: false, forced: true }, __LD_10: bool(false), __LD_11: bool(false) }) })
    );
    expect(stateOf(container, 10)).toBe('off');
    expect(element(container, 10).getAttribute('data-forced')).toBe('true');
    expect(element(container, 10).querySelector('[data-testid="variable-forced"]')?.textContent).toBe('FORCED');
    expect(element(container, 11).querySelector('[data-testid="variable-forced"]')).toBeNull();
  });

  it('draws nothing live of another version, or of a program not in RUN', async () => {
    let { container } = await opened(main({ runtimes: runtimesOf(runtimeOf('Main', { version: 'bbbbbbbbbbbb' })) }));
    expect(byTestId(container, 'ld-live')?.textContent).toContain('runs version bbbbbbbbbbbb');
    expect(stateOf(container, 10)).toBe('none');
    expect(link(container, 1, 10)).toBe('none');
    unmountAll();
    resetStores();

    ({ container } = await opened(main({ runtimes: runtimesOf(runtimeOf('Main', { version: VERSION, mode: 'stop' })), watch: watchOf(RUNNING, 'stop') })));
    expect(byTestId(container, 'ld-live')?.textContent).toContain('STOP');
    expect(stateOf(container, 10)).toBe('none');
    expect(container.querySelectorAll('[data-testid="ld-value"]')).toHaveLength(0);
    expect([...container.querySelectorAll('[data-testid="ld-rung"]')].map((r) => r.getAttribute('data-state'))).toEqual(['none', 'none']);
  });
});

describe('reading a TC6 document', () => {
  it('orders networks as the front-end does, and lists what it does not draw', () => {
    const read = readLadder(
      ladderOf('P', `${contact(5, 40, 50, 'x', '1')}${contact(3, 40, 10, 'y', '1')}<connector localId="9"><position x="0" y="0"/></connector>`)
    );
    if (!read.ok) throw new Error(read.error);
    const [pou] = read.pous;
    if (!pou) throw new Error('no POU');
    expect(networks(pou)).toEqual([[3], [5]]);
    expect(pou.notDrawn).toEqual(['connector 9']);
    expect(readLadder('<project/>')).toEqual({
      ok: false,
      error: 'the document is not a PLCopen TC6 XML v2.01 project: its root is project in the namespace of none',
    });
  });
});
