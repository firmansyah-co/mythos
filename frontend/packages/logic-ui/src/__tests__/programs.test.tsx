import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CheckResult, Variable } from '../bridge';
import { EDITOR } from '../entry';
import { PANES, ProgramEditor } from '../ProgramEditor';
import { useModuleUi } from '../../../ui-shell/src/modules/registry';
import { ProgramsTree } from '../ProgramsTree';
import { isDirty, runState, usePrograms } from '../programs';
import {
  answerNext,
  CURRENCY_CASES,
  button,
  byLabel,
  byTestId,
  click,
  connection,
  doing,
  endTest,
  entryOf,
  ioStatus,
  paneOf,
  project,
  render,
  resetStores,
  runtimeOf,
  runtimesOf,
  sample,
  server,
  settle,
  texts,
  typeInto,
  typeSource,
  unmountAll,
  until,
  watchOf,
  useController,
  useSession,
  useShell,
  useTelemetry,
  timeScale,
  type Backend,
} from './harness';

const LEVEL = 'external.Plant.LT1.level';
const SPEED = 'external.Plant.P1.speed';
const TANK = 'PROGRAM Tank\nVAR_INPUT level : INT; ok : BOOL; END_VAR\nVAR_OUTPUT speed : INT; run : BOOL; END_VAR\nEND_PROGRAM\n';

const variable = (path: string, type: string, section: string, bindable = true): Variable => ({ path, type, section, bindable });
const VARIABLES = [
  variable('level', 'INT', 'input'),
  variable('ok', 'BOOL', 'input'),
  variable('speed', 'INT', 'output'),
  variable('run', 'BOOL', 'output'),
  variable('t.ET', 'TIME', 'local', false),
  variable('note', 'STRING[8]', 'local', false),
];
const compiles = (): CheckResult => ({ ok: true, diagnostics: [], variables: VARIABLES, code_size: 120, memory_size: 16, binding_problems: [] });
/** A compiler that refuses any text with `oops` in it, at line 3, column 8. */
const check = (source: string): CheckResult =>
  source.includes('oops')
    ? { ok: false, diagnostics: [{ line: 3, col: 8, end_line: 3, end_col: 12, message: 'oops is not declared' }] }
    : compiles();

const tank = (over: Partial<Backend> = {}): Backend => ({
  programs: [{ entry: entryOf('Tank'), source: TANK }],
  check,
  runtimes: runtimesOf(),
  ...over,
});

const state = () => usePrograms.getState();
const doc = (name = 'Tank') => {
  const found = state().programs.find((d) => d.entry.name === name);
  if (!found) throw new Error(`no program ${name}`);
  return found;
};
/** What was asked to change something: a watch is a POST that only reads. */
// Not counted either: the check of the stored library the Library section makes when the project
// opens (`POST /library/check {}`, D4c R19), which writes nothing.
const writes = (calls: Array<{ method: string; path: string; body: unknown }>) =>
  calls
    .filter((call) => call.method !== 'GET' && !call.path.endsWith('/watch'))
    .filter((call) => !(call.path.endsWith('/logic/library/check') && JSON.stringify(call.body) === '{}'))
    .map((call) => `${call.method} ${call.path.replace(/^.*\/api\/v1/, '')}`);

/** The workbench has `plant` open, the controller runs it, and the logic UI has read it. */
async function opened(backend: Backend) {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant', io: ioStatus([connection('plc1', [LEVEL, SPEED])]) });
  await settle();
  const container = render(<ProgramEditor name="Tank" />);
  await settle();
  return { calls, container };
}

// Each test renders the whole program editor, its timers polling the runtimes, and waits for
// what it asserts. On this host a test takes 0.1-3.4 s at a load average of ~50; CI, sharing the
// machine with builds, went past vitest's 5 s default (PR #126). The bound is for a test that
// hangs, so it is set to what the work takes under load, not to what it takes on an idle host.
vi.setConfig({ testTimeout: 20_000 * timeScale });

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
});

describe('reading the programs', () => {
  it('says the programs are unreadable instead of showing a project with none', async () => {
    const { container } = await opened(tank({ intercept: (call) => (call.path.endsWith('/logic/programs') ? new Response('{"error":"disk"}', { status: 500, statusText: 'Internal Server Error' }) : undefined) }));

    expect(state().phase).toBe('unreadable');
    expect(byTestId(container, 'programs-unreadable')?.textContent).toContain('disk');
    expect(byTestId(container, 'program-missing')).toBeNull();
    const tree = render(<ProgramsTree openEditor={() => undefined} />);
    expect(byTestId(tree, 'tree-programs-unreadable')).not.toBeNull();
    // Nothing can be added to a list that was not read.
    expect(byTestId(tree, 'tree-add-program')).toBeNull();
  });

  it('shows an opened program as it is saved, checks it once, and lists its variables', async () => {
    const { calls, container } = await opened(tank());

    expect(container.querySelector('textarea')?.value).toBe(TANK);
    expect(isDirty(doc())).toBe(false);
    expect(byTestId(container, 'program-unsaved')).toBeNull();
    expect(byTestId(container, 'check-ok')?.textContent).toBe('COMPILES');
    // No runtime runs it: no variable has a value, and none is asked for.
    expect(texts(container, 'variable-row').map((row) => row.split('Bind')[0])).toEqual([
      'levelINTinput—',
      'okBOOLinput—',
      'speedINToutput—',
      'runBOOLoutput—',
      't.ETTIMElocal——',
      'noteSTRING[8]local——',
    ]);
    expect(calls.some((call) => call.path.endsWith('/watch'))).toBe(false);
    expect(writes(calls)).toEqual(['POST /projects/plant/logic/programs/Tank/check']);
  });

  it('puts the runtime in the task cards and the program\'s properties in the inspector, and takes them away after', async () => {
    const { container } = await opened(tank({ runtimes: runtimesOf(runtimeOf('Tank')) }));
    const editor = byTestId(container, 'program-editor') as HTMLElement;
    // The work area holds the program and its interface, and nothing else.
    expect(paneOf(container, 'task-cards', PANES.runtime)?.querySelector('[data-testid="program-runtime"]')).not.toBeNull();
    expect(editor.querySelector('[data-testid="program-runtime"]')).toBeNull();
    const properties = paneOf(container, 'inspector', PANES.program);
    expect(properties?.querySelector('[aria-label="Interval in milliseconds"]')).not.toBeNull();
    expect(editor.querySelector('[aria-label="Interval in milliseconds"]')).toBeNull();
    expect(editor.querySelector('[data-testid="program-interface"]')).not.toBeNull();
    // An ST program has no instructions card: that is the Ladder Diagram's.
    expect(useModuleUi.getState().taskCards[PANES.instructions]).toBeUndefined();
    // Opening the program selected its properties; a click in the editor selects them again.
    expect(useShell.getState().selection).toEqual({ kind: 'module', owner: PANES.program });
    useShell.getState().select(null);
    act(() => editor.dispatchEvent(new MouseEvent('mousedown', { bubbles: true })));
    expect(useShell.getState().selection).toEqual({ kind: 'module', owner: PANES.program });
    unmountAll();
    const ui = useModuleUi.getState();
    expect([ui.taskCards[PANES.runtime], ui.inspector[PANES.program], ui.diagnostics[PANES.check]]).toEqual([undefined, undefined, undefined]);
  });

  it('says a source that could not be read could not be read, and offers no empty editor', async () => {
    const { container } = await opened(tank({ intercept: (call) => (call.path.endsWith('/programs/Tank') ? new Response('{"error":"gone"}', { status: 500, statusText: 'Internal Server Error' }) : undefined) }));

    expect(byTestId(container, 'program-unreadable')?.textContent).toContain('gone');
    expect(container.querySelector('textarea')).toBeNull();
  });
});

describe('editing, checking and saving', () => {
  it('keeps an edit unsaved until it is saved, and saves only what changed', async () => {
    const backend = tank();
    const { calls, container } = await opened(backend);
    calls.length = 0;

    typeSource(container, `${TANK}(* edited *)`);
    expect(byTestId(container, 'program-unsaved')?.textContent).toBe('UNSAVED');
    expect(backend.programs[0]?.source).toBe(TANK);
    click(button(container, 'Save'));
    await settle();

    expect(writes(calls)).toEqual(['PUT /projects/plant/logic/programs/Tank/source']);
    expect(backend.programs[0]?.source).toBe(`${TANK}(* edited *)`);
    expect(byTestId(container, 'program-unsaved')).toBeNull();

    calls.length = 0;
    typeInto(byLabel(container, 'Interval in milliseconds'), '20');
    expect(byTestId(container, 'program-unsaved')).not.toBeNull();
    click(button(container, 'Save'));
    await settle();
    expect(writes(calls)).toEqual(['PUT /projects/plant/logic/programs/Tank/settings']);
    expect(backend.programs[0]?.entry.interval_ms).toBe(20);
    // The name and the language are not the client's to send.
    expect(Object.keys(calls.find((c) => c.method === 'PUT')?.body as object).sort()).toEqual([
      'inputs', 'interval_ms', 'outputs', 'published', 'run', 'runtime', 'target',
    ]);
  });

  it('discards an edit back to what is saved', async () => {
    const { container } = await opened(tank());
    typeSource(container, 'changed');
    typeInto(byLabel(container, 'Target profile'), 'rp2040');
    click(button(container, 'Discard'));

    expect(container.querySelector('textarea')?.value).toBe(TANK);
    expect((byLabel(container, 'Target profile') as HTMLInputElement).value).toBe('pc');
    expect(isDirty(doc())).toBe(false);
  });

  it('checks the text in the editor, saved or not, and shows each diagnostic where it is', async () => {
    const backend = tank();
    const { calls, container } = await opened(backend);
    const wrong = 'PROGRAM Tank\nVAR a : INT; END_VAR\n  a := oops;\nEND_PROGRAM';
    typeSource(container, wrong);
    calls.length = 0;
    click(button(container, 'Check'));
    await settle();

    // Only what was sent: the editor also reads the runtimes on a timer, which is not this.
    expect(calls.filter((c) => c.path.endsWith('/check')).map((c) => c.body)).toEqual([{ source: wrong }]);
    expect(backend.programs[0]?.source).toBe(TANK);
    expect(texts(container, 'diagnostic')).toEqual(['3:8oops is not declared']);
    expect(byTestId(container, 'source-error-line')?.textContent).toBe('3');
    expect(byTestId(container, 'source-error-mark')?.textContent).toBe('oops');
    expect(byTestId(container, 'check-ok')).toBeNull();
    // What the compiler said is in the inspector's Diagnostics tab, counted there, and the Check
    // the engineer asked for brought that tab forward; the toolbar says it in a word.
    expect(paneOf(container, 'diagnostics', PANES.check)?.querySelector('[data-testid="diagnostic"]')).not.toBeNull();
    expect(byTestId(container, 'program-editor')?.querySelector('[data-testid="diagnostic"]')).toBeNull();
    expect(useModuleUi.getState().diagnostics[PANES.check]?.item.count).toBe(1);
    expect(useShell.getState().inspectorTab).toBe('diagnostics');
    expect(byTestId(container, 'check-summary')?.textContent).toBe('1 ERROR');
    // The variables shown are those of the last text that compiled, and said to be.
    expect(byTestId(container, 'variables-stale')).not.toBeNull();
    expect(texts(container, 'variable-row')).toHaveLength(6);

    // Clicking a diagnostic puts the caret there.
    click(byTestId(container, 'diagnostic'));
    const area = container.querySelector('textarea') as HTMLTextAreaElement;
    expect(area.selectionStart).toBe(wrong.indexOf('oops'));

    // Once the text changes, the diagnostics are of another text: said, and no longer marked,
    // nor counted: what the text now holds is not known until it is checked.
    typeSource(container, `${wrong} `);
    expect(useModuleUi.getState().diagnostics[PANES.check]?.item.count).toBeNull();
    expect(byTestId(container, 'check-summary')?.textContent).toBe('CHANGED SINCE CHECK');
    expect(byTestId(container, 'check-stale')).not.toBeNull();
    expect(byTestId(container, 'source-error-mark')).toBeNull();
    expect(byTestId(container, 'source-error-line')).toBeNull();
    // A Check that compiles brings the Diagnostics tab forward too: its COMPILES is the answer.
    act(() => useShell.getState().setInspectorTab('properties'));
    typeSource(container, TANK);
    click(button(container, 'Check'));
    await settle();
    expect(useShell.getState().inspectorTab).toBe('diagnostics');
    expect(paneOf(container, 'diagnostics', PANES.check)?.querySelector('[data-testid="check-ok"]')?.textContent).toBe('COMPILES');
    expect(useModuleUi.getState().diagnostics[PANES.check]?.item.count).toBe(0);
  });

  it('underlines the whole of what each diagnostic is about, over words and lines', async () => {
    const wrong = 'PROGRAM Tank\nVAR a : INT; END_VAR\n  a := (oops +\n    NOT 1);\nEND_PROGRAM';
    const backend = tank({
      check: () => ({
        ok: false,
        diagnostics: [
          // About the program as a whole: listed, underlined nowhere.
          { line: 0, col: 0, end_line: 0, end_col: 0, message: 'the source has a mistake of its own' },
          // From a parenthesis on line 3 to its partner on line 4, a keyword between.
          { line: 3, col: 8, end_line: 4, end_col: 11, message: 'a INT and a BOOL cannot be combined' },
          // A point: the word there.
          { line: 2, col: 5, end_line: 2, end_col: 5, message: 'a is not used' },
        ],
      }),
    });
    const { container } = await opened(backend);
    typeSource(container, wrong);
    click(button(container, 'Check'));
    await settle();

    expect(texts(container, 'diagnostic')).toEqual([
      '0:0the source has a mistake of its own',
      '3:8a INT and a BOOL cannot be combined',
      '2:5a is not used',
    ]);
    // Each underlined piece is the source's own text: the range across a keyword and a line
    // break comes in more than one piece, and nothing else is underlined.
    const marks = [...container.querySelectorAll('[data-testid="source-error-mark"]')].map((m) => m.textContent);
    expect(marks.join('|')).toBe('a|(oops +\n    |NOT| |1|)');
    expect(marks.slice(1).join('')).toBe(wrong.slice(wrong.indexOf('('), wrong.indexOf(')') + 1));
    expect([...container.querySelectorAll('[data-testid="source-error-line"]')].map((l) => l.textContent)).toEqual(['2', '3']);
  });

  it('paints the text once where diagnostics share a word or a stretch of it', async () => {
    const wrong = 'PROGRAM Tank\nVAR a : INT; END_VAR\n  a := oops + oops2;\nEND_PROGRAM';
    const backend = tank({
      check: () => ({
        ok: false,
        diagnostics: [
          // Two in one stretch of names, apart; and one over the first and the name before it.
          { line: 3, col: 8, end_line: 3, end_col: 12, message: 'oops is not declared' },
          { line: 3, col: 15, end_line: 3, end_col: 20, message: 'oops2 is not declared' },
          { line: 3, col: 3, end_line: 3, end_col: 12, message: 'a INT cannot hold this' },
        ],
      }),
    });
    const { container } = await opened(backend);
    typeSource(container, wrong);
    click(button(container, 'Check'));
    await settle();

    const marks = [...container.querySelectorAll('[data-testid="source-error-mark"]')].map((m) => m.textContent);
    expect(marks).toEqual(['a := oops', 'oops2']);
    // What is painted under the text area is the text, each character once.
    expect(container.querySelector('.logic-source__paint')?.textContent).toBe(`${wrong}\n`);
  });

  it('keeps no result of a check that failed, rather than one of an earlier text', async () => {
    const backend = tank();
    const { container } = await opened(backend);
    expect(byTestId(container, 'check-ok')).not.toBeNull();
    backend.intercept = (call) => (call.path.endsWith('/check') ? new Response('{"error":"module died"}', { status: 500, statusText: 'Internal Server Error' }) : undefined);
    click(button(container, 'Check'));
    await settle();

    expect(byTestId(container, 'check-ok')).toBeNull();
    expect(byTestId(container, 'check-none')).not.toBeNull();
    expect(byTestId(container, 'program-error')?.textContent).toContain('module died');
  });
});

describe('bindings', () => {
  it('binds a variable to a tag, and an output has no safe value until one is given', async () => {
    const backend = tank();
    const { calls, container } = await opened(backend);

    typeInto(byLabel(container, 'Bind level'), 'input');
    typeInto(byLabel(container, 'Reads tag for level'), LEVEL);
    typeInto(byLabel(container, 'Bind speed'), 'output');
    typeInto(byLabel(container, 'Writes tag for speed'), SPEED);

    // Nothing was typed for the safe value: it is none, said so, and not zero.
    expect((byLabel(container, 'Safe value of speed') as HTMLInputElement).value).toBe('');
    expect(texts(container, 'binding-problem')).toEqual([
      'A safe value is required: what the tag is written whenever the program is not running.',
    ]);
    expect(doc().entry.outputs).toEqual([{ variable: 'speed', tag: SPEED, safe: null }]);

    typeInto(byLabel(container, 'Safe value of speed'), '0');
    expect(texts(container, 'binding-problem')).toEqual([]);
    calls.length = 0;
    click(button(container, 'Save'));
    await settle();
    expect(calls.find((c) => c.method === 'PUT')?.body).toMatchObject({
      inputs: [{ variable: 'level', tag: LEVEL }],
      outputs: [{ variable: 'speed', tag: SPEED, safe: 0 }],
    });
  });

  it('offers GOOD only for a BOOL, and TRUE or FALSE as the safe value of a BOOL output', async () => {
    const { container } = await opened(tank());
    const kinds = (label: string) => [...(byLabel(container, label) as HTMLSelectElement).options].map((o) => o.value);
    expect(kinds('Bind level')).toEqual(['', 'input', 'output', 'published']);
    expect(kinds('Bind ok')).toEqual(['', 'input', 'good', 'output', 'published']);
    // A TIME cannot be bound: there is nothing to choose.
    expect(byLabel(container, 'Bind t.ET')).toBeNull();
    // Nor a STRING, and the table says why: a tag holds no text.
    expect(byLabel(container, 'Bind note')).toBeNull();
    const titles = [...container.querySelectorAll('[data-testid="not-bindable"]')].map((e) => e.getAttribute('title'));
    expect(titles).toContain('A tag carries a number, never text: a STRING cannot be bound to one.');
    expect(titles).toContain('Only BOOL, integer and real variables can be bound to a tag.');

    typeInto(byLabel(container, 'Bind run'), 'output');
    const safe = byLabel(container, 'Safe value of run') as HTMLSelectElement;
    expect(safe.tagName).toBe('SELECT');
    expect(safe.value).toBe('');
    typeInto(safe, 'false');
    expect(doc().entry.outputs[0]?.safe).toBe(false);
  });

  it('shows a binding the program has no variable for, and removing one leaves the others', async () => {
    const entry = entryOf('Tank', {
      inputs: [{ variable: 'flow', tag: 'external.Plant.FT1.flow' }, { variable: 'level', tag: LEVEL }],
      published: [{ variable: 'level', tag: 'internal.Plant.Tank.level', writable: false }],
    });
    const { container } = await opened(tank({ programs: [{ entry, source: TANK }] }));

    expect(byTestId(container, 'bindings-without-variable')?.textContent).toContain('flow');
    click(byLabel(container, 'Remove: Reads tag for flow'));
    expect(byTestId(container, 'bindings-without-variable')).toBeNull();
    expect(doc().entry.inputs).toEqual([{ variable: 'level', tag: LEVEL }]);
    expect(doc().entry.published).toHaveLength(1);
  });

  it('shows a bound tag\'s value only while it is a current one', async () => {
    const entry = entryOf('Tank', { inputs: [{ variable: 'level', tag: LEVEL }] });
    const { container } = await opened(tank({ programs: [{ entry, source: TANK }] }));
    const value = () => {
      const cell = byTestId(container, 'tag-value');
      return `${cell?.getAttribute('data-state')}:${cell?.textContent}`;
    };

    expect(value()).toMatch(/^offline:/);
    await doing(() => useTelemetry.setState({ stream: 'OPEN', tags: new Map([[LEVEL, sample(LEVEL, { type: 'INT', value: 20 })]]) }));
    expect(value()).toBe('live:20');
    await doing(() =>
      useTelemetry.setState({ tags: new Map([[LEVEL, sample(LEVEL, { type: 'INT', value: 20, qualityName: 'BAD_COMM_FAILURE', qualityCode: 0 })]]) })
    );
    expect(value()).toMatch(/^stale:20/);
  });
});

describe('a forced value of a bound tag', () => {
  /** The FORCED mark of the binding to `tag`, or `null` when it has none. */
  const markOf = (container: HTMLElement, tag: string): Element | null => {
    const row = [...container.querySelectorAll('[data-testid="binding"]')].find(
      (el) => (el.querySelector('input.logic-binding__tag') as HTMLInputElement | null)?.value === tag
    );
    if (!row) throw new Error(`no binding to ${tag}`);
    return row.querySelector('[data-testid="tag-forced"]');
  };
  const bound = () =>
    entryOf('Tank', {
      inputs: [{ variable: 'level', tag: LEVEL }],
      outputs: [{ variable: 'speed', tag: SPEED, safe: 0 }],
    });

  it('is marked FORCED, and a bound tag that is not forced is not', async () => {
    const { container } = await opened(tank({ programs: [{ entry: bound(), source: TANK }] }));
    await doing(() => useController.setState({ forcedTags: [SPEED] }));
    expect(markOf(container, SPEED)?.textContent).toBe('FORCED');
    expect(markOf(container, LEVEL)).toBeNull();
  });

  it('is not marked while the forced tags are not known', async () => {
    const { container } = await opened(tank({ programs: [{ entry: bound(), source: TANK }] }));
    await doing(() => useController.setState({ forcedTags: null }));
    expect(markOf(container, SPEED)).toBeNull();
    expect(markOf(container, LEVEL)).toBeNull();
  });
});

describe('loading into a runtime', () => {
  it('saves, checks, builds, and puts the instance into the project\'s runtimes beside the others', async () => {
    const other = { id: 'Pump', module: 'vm', run: true, program: { name: 'Pump' } };
    const old = { id: 'Tank', module: 'vm', run: true, program: { name: 'Tank', source: 'old' } };
    const backend = tank({ config: { instances: [other, old] } });
    const { calls, container } = await opened(backend);
    typeSource(container, `${TANK}(* v2 *)`);
    calls.length = 0;
    click(byTestId(container, 'program-load'));
    await settle();

    expect(writes(calls)).toEqual([
      'PUT /projects/plant/logic/programs/Tank/source',
      'POST /projects/plant/logic/programs/Tank/check',
      'POST /projects/plant/logic/programs/Tank/build',
      'PUT /projects/plant/runtimes',
    ]);
    expect(backend.config?.instances).toEqual([
      other,
      { id: 'Tank', module: 'vm', run: false, program: { name: 'Tank', source: `${TANK}(* v2 *)` } },
    ]);
    expect(byTestId(container, 'program-built')?.textContent).toContain('Version build1 was built and loaded.');
    expect(byTestId(container, 'program-unsaved')).toBeNull();
  });

  it('builds and loads nothing of a program that does not compile, and says where it is wrong', async () => {
    const backend = tank({ config: { instances: [] } });
    const { calls, container } = await opened(backend);
    typeSource(container, 'PROGRAM Tank\n\n  a := oops;\nEND_PROGRAM');
    calls.length = 0;
    click(byTestId(container, 'program-load'));
    await settle();

    expect(writes(calls)).toEqual([
      'PUT /projects/plant/logic/programs/Tank/source',
      'POST /projects/plant/logic/programs/Tank/check',
    ]);
    expect(backend.config?.instances).toEqual([]);
    expect(byTestId(container, 'program-error')?.textContent).toBe('Program Tank was not loaded: it does not compile.');
    expect(texts(container, 'diagnostic')).toEqual(['3:8oops is not declared']);
    expect(byTestId(container, 'program-built')).toBeNull();
  });

  it('loads nothing while a binding names a variable the program does not have', async () => {
    const backend = tank({
      config: null,
      check: () => ({ ...compiles(), binding_problems: ['input x: the program has no variable flow'] }),
    });
    const { calls, container } = await opened(backend);
    calls.length = 0;
    click(byTestId(container, 'program-load'));
    await settle();

    expect(writes(calls)).toEqual(['POST /projects/plant/logic/programs/Tank/check']);
    expect(texts(container, 'binding-check-problem')).toEqual(['input x: the program has no variable flow']);
    expect(byTestId(container, 'program-error')?.textContent).toContain('its bindings do not match its variables');
  });

  it('says a build was saved and not started when the controller runs another project', async () => {
    const { container } = await opened(tank({ applied: false, warnings: [`output ${SPEED} is not a tag of this controller now`] }));
    await doing(() => useController.setState({ activeProjectId: 'other' }));
    click(byTestId(container, 'program-load'));
    await settle();

    const built = byTestId(container, 'program-built')?.textContent ?? '';
    expect(built).toContain('nothing was started');
    expect(built).not.toContain('and loaded.');
    expect(texts(container, 'build-warning')).toEqual([`output ${SPEED} is not a tag of this controller now`]);
    expect(byTestId(container, 'runtime-elsewhere')?.textContent).toContain('(other)');
  });

  it('says a program the controller saved but did not start did not start, and why', async () => {
    const backend = tank({
      intercept: (call) =>
        call.method === 'PUT' && call.path.endsWith('/runtimes')
          ? Response.json({ applied: true, changes: { started: [], failed: [['runtime:Tank', 'the module exited at start']] } })
          : undefined,
    });
    const { container } = await opened(backend);
    click(byTestId(container, 'program-load'));
    await settle();

    expect(byTestId(container, 'program-error')?.textContent).toBe(
      "Program Tank version build1 was saved into the project's runtimes but did not start: the module exited at start"
    );
    const built = byTestId(container, 'program-built')?.textContent ?? '';
    expect(built).toContain('but it did not start.');
    expect(built).not.toContain('and loaded.');
  });

  it('is not troubled by another runtime that did not start', async () => {
    const backend = tank({
      intercept: (call) =>
        call.method === 'PUT' && call.path.endsWith('/runtimes')
          ? Response.json({ applied: true, changes: { started: ['runtime:Tank'], failed: [['runtime:Pump', 'no such module']] } })
          : undefined,
    });
    const { container } = await opened(backend);
    click(byTestId(container, 'program-load'));
    await settle();

    expect(byTestId(container, 'program-error')).toBeNull();
    expect(byTestId(container, 'program-built')?.textContent).toContain('was built and loaded.');
  });

  it('says a build that was refused by the controller was not loaded', async () => {
    const backend = tank({
      intercept: (call) =>
        call.method === 'PUT' && call.path.endsWith('/runtimes')
          ? new Response('{"error":"the runtimes configuration has problems and was not saved"}', { status: 400, statusText: 'Bad Request' })
          : undefined,
    });
    const { container } = await opened(backend);
    click(byTestId(container, 'program-load'));
    await settle();

    expect(byTestId(container, 'program-error')?.textContent).toContain('was not saved');
    expect(byTestId(container, 'program-built')?.textContent).toContain('was built and is not loaded.');
  });

  it("a refused Load shows the core's problems", async () => {
    const backend = tank({
      config: { instances: [] },
      intercept: (call) =>
        call.method === 'PUT' && call.path.endsWith('/runtimes')
          ? Response.json(
              {
                error: 'the runtimes configuration has problems and was not saved',
                problems: [
                  { connection_id: 'runtime:Tank', message: `output ${SPEED} is written by runtime Line too` },
                  { connection_id: 'plc1', message: 'the connection has no tag external.Plant.P1.speed' },
                ],
              },
              { status: 400, statusText: 'Bad Request' }
            )
          : undefined,
    });
    const { container } = await opened(backend);
    click(byTestId(container, 'program-load'));
    await settle();

    expect(byTestId(container, 'program-error')?.querySelector('p')?.textContent).toBe(
      'Program Tank was not loaded: 400 Bad Request: the runtimes configuration has problems and was not saved'
    );
    expect(texts(container, 'program-load-problem')).toEqual([
      `runtime:Tank: output ${SPEED} is written by runtime Line too`,
      'plc1: the connection has no tag external.Plant.P1.speed',
    ]);
    expect(backend.config?.instances).toEqual([]);
  });
});

describe('how a loaded program started', () => {
  /** A controller whose save of the runtimes answers that it started Tank so. */
  const starting = (load: Record<string, unknown>) =>
    tank({
      intercept: (call) =>
        call.method === 'PUT' && /\/runtimes(\?|$)/.test(call.path)
          ? Response.json({ applied: true, changes: { swapped: ['runtime:Tank'], failed: [], runtimes: [load] } })
          : undefined,
    });
  const line = (container: HTMLElement) => byTestId(container, 'program-start')?.textContent ?? '';

  it('says a new version was changed online, and each variable that did not carry over with why', async () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => undefined);
    const { container } = await opened(
      starting({
        runtime: 'Tank',
        start: 'online',
        why: null,
        carried: 3,
        dropped: [{ variable: 'old_only', why: 'this build has no variable of that name' }],
        initialized: [
          { variable: 'added', why: 'the running program had no variable of that name' },
          { variable: 'kind', why: 'its type is INT now, not DINT' },
        ],
      })
    );
    click(byTestId(container, 'program-load'));
    await settle();

    expect(line(container)).toBe(
      "Changed online: it took over the running version's variables (3 carried over) and scans on without a restart. 1 of the running version's did not carry over, and 2 of its own started from their initial values."
    );
    expect(texts(container, 'program-dropped')).toEqual(['old_only (this build has no variable of that name)']);
    expect(texts(container, 'program-initialized')).toEqual([
      'added (the running program had no variable of that name)',
      '; kind (its type is INT now, not DINT)',
    ]);
    info.mockRestore();
  });

  it('says why a version was not changed online, and never that it was', async () => {
    const { container } = await opened(
      starting({
        runtime: 'Tank',
        start: 'warm',
        why: "the program running was 'Pump', not 'Tank'",
        carried: null,
        dropped: [],
        initialized: [],
      })
    );
    click(byTestId(container, 'program-load'));
    await settle();

    expect(line(container)).toBe(
      "Not changed online: the program running was 'Pump', not 'Tank'. Its RETAIN variables start from the values kept for them, every other variable from its initial value."
    );
    expect(line(container)).not.toContain('Changed online');
    expect(texts(container, 'program-dropped')).toEqual([]);
  });

  it('a warm start that withheld values lists them and does not say every RETAIN value was given', async () => {
    const { container } = await opened(
      starting({
        runtime: 'Tank',
        start: 'warm',
        why: 'its build was made again under its name',
        carried: null,
        dropped: [],
        initialized: [],
        not_given: [
          { variable: 'total', why: 'the values kept are of another program of that name' },
          { variable: 'hours', why: 'its type is DINT now, not INT' },
        ],
      })
    );
    click(byTestId(container, 'program-load'));
    await settle();

    expect(line(container)).toBe(
      'Not changed online: its build was made again under its name. Its RETAIN variables start from the values kept for them, except those listed as not given, every other variable from its initial value.'
    );
    expect(texts(container, 'program-not-given')).toEqual([
      'total (the values kept are of another program of that name)',
      '; hours (its type is DINT now, not INT)',
    ]);
  });

  it('claims nothing of how it started when the controller does not say', async () => {
    const { container } = await opened(tank());
    click(byTestId(container, 'program-load'));
    await settle();

    expect(byTestId(container, 'program-built')?.textContent).toContain('was built and loaded.');
    expect(line(container)).toBe('The controller did not say how it started.');
  });

  it('loads cold only when the engineer confirms, asks the controller for a cold start, and says it was', async () => {
    const backend = starting({ runtime: 'Tank', start: 'cold', why: 'a cold load was asked for', carried: null, dropped: [], initialized: [] });
    const { calls, container } = await opened(backend);
    vi.stubGlobal('confirm', () => false);
    calls.length = 0;
    click(byTestId(container, 'program-load-cold'));
    await settle();
    expect(writes(calls)).toEqual([]);

    let asked = '';
    vi.stubGlobal('confirm', (question: string) => {
      asked = question;
      return true;
    });
    click(byTestId(container, 'program-load-cold'));
    await settle();
    expect(asked).toContain('RETAIN variables included');
    expect(writes(calls)).toEqual([
      'POST /projects/plant/logic/programs/Tank/check',
      'POST /projects/plant/logic/programs/Tank/build',
      'PUT /projects/plant/runtimes?start=cold',
    ]);
    expect(line(container)).toBe('Loaded cold: every variable starts from its initial value, RETAIN ones included.');

    // Load without asking for cold sends no start: a running program is then changed online.
    vi.stubGlobal('confirm', () => true);
    calls.length = 0;
    click(byTestId(container, 'program-load'));
    await settle();
    expect(writes(calls).at(-1)).toBe('PUT /projects/plant/runtimes');
  });
});

describe('what the runtime is doing', () => {
  const mode = (container: HTMLElement) => byTestId(container, 'runtime-mode')?.textContent;

  it('says a program no runtime runs is not loaded, and offers no run or stop', async () => {
    const { container } = await opened(tank({ runtimes: runtimesOf(runtimeOf('Pump')) }));
    expect(mode(container)).toBe('NOT LOADED');
    expect((byTestId(container, 'runtime-run') as HTMLButtonElement).disabled).toBe(true);
    expect((byTestId(container, 'runtime-stop') as HTMLButtonElement).disabled).toBe(true);
  });

  it('shows a running program with its version and scan time, and each later reading', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')) });
    const { container } = await opened(backend);
    expect(mode(container)).toBe('RUN');
    expect(byTestId(container, 'runtime-version')?.textContent).toBe('Tank · version aaaaaaaaaaaa');
    expect(byTestId(container, 'runtime-scan-time')?.textContent).toBe('last 7 µs · longest 19 µs');
    // A program with no output writes none: "written by the program" would be a claim.
    expect(byTestId(container, 'runtime-outputs')?.textContent).toBe('none');

    const outputs = [{ tag: SPEED, variable: 'speed', safe: 0, written: 300, error: null }];
    backend.runtimes = runtimesOf(runtimeOf('Tank', { outputs }));
    await doing(() => state().readRuntimes());
    expect(byTestId(container, 'runtime-outputs')?.textContent).toBe('written by the program');

    backend.runtimes = runtimesOf(runtimeOf('Tank', { mode: 'fault', reason: 'division by zero at 0x1c', outputs_safe: true, outputs }));
    await doing(() => state().readRuntimes());
    expect(mode(container)).toBe('FAULT');
    expect(byTestId(container, 'runtime-mode')?.className).toContain('badge--bad');
    expect(byTestId(container, 'runtime-reason')?.textContent).toBe('division by zero at 0x1c');
    expect(byTestId(container, 'runtime-outputs')?.textContent).toBe('at their safe values');

    backend.runtimes = runtimesOf(runtimeOf('Tank', { mode: 'stop', outputs_safe: false, scans: 0, last_scan_us: null, max_scan_us: null, outputs }));
    await doing(() => state().readRuntimes());
    expect(mode(container)).toBe('STOP');
    expect(byTestId(container, 'runtime-mode')?.className).not.toContain('badge--good');
    // A stopped program whose outputs are not confirmed safe is said to be exactly that.
    expect(byTestId(container, 'runtime-outputs')?.textContent).toBe('NOT CONFIRMED at their safe values');
    // No scan has run: there is no scan time, and zero is not one.
    expect(byTestId(container, 'runtime-scan-time')?.textContent).toBe('NOT MEASURED');
  });

  it('shows no scan figure of a runtime whose module does not answer, never a zero', async () => {
    const backend = tank({
      runtimes: runtimesOf(
        runtimeOf('Tank', { mode: 'unreachable', reason: 'the module does not answer', scans: null, last_scan_us: null, max_scan_us: null, overruns: null })
      ),
    });
    const { container } = await opened(backend);
    expect(mode(container)).toBe('UNREACHABLE');
    expect(byTestId(container, 'runtime-scans')?.textContent).toBe('NOT MEASURED');
    expect(byTestId(container, 'runtime-scan-time')?.textContent).toBe('NOT MEASURED');
    expect(byTestId(container, 'runtime-overruns')?.textContent).toBe('NOT MEASURED');
  });

  it('says UNKNOWN while the runtimes have never been read', async () => {
    const { container } = await opened(tank({ runtimes: 'fails' }));
    expect(mode(container)).toBe('UNKNOWN');
    expect(byTestId(container, 'runtime-unknown-why')?.textContent).toContain('the runtimes cannot be read');
    expect((byTestId(container, 'runtime-unload') as HTMLButtonElement).disabled).toBe(true);
  });

  it('marks what it shows as last known when the runtimes can no longer be read, until they can', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')) });
    const { container } = await opened(backend);
    expect(byTestId(container, 'runtime-stale')).toBeNull();

    backend.runtimes = 'fails';
    await doing(() => state().readRuntimes());
    expect(mode(container)).toBe('RUN');
    expect(byTestId(container, 'runtime-stale')?.textContent).toBe('LAST KNOWN');
    expect(byTestId(container, 'runtime-stale-why')).not.toBeNull();
    // Run is not sent on the strength of a state that is no longer known. Stop is the safe
    // direction and is never taken away.
    expect((byTestId(container, 'runtime-run') as HTMLButtonElement).disabled).toBe(true);
    expect((byTestId(container, 'runtime-stop') as HTMLButtonElement).disabled).toBe(false);

    backend.runtimes = runtimesOf(runtimeOf('Tank', { mode: 'stop', outputs_safe: true }));
    await doing(() => state().readRuntimes());
    expect(mode(container)).toBe('STOP');
    expect(byTestId(container, 'runtime-stale')).toBeNull();
  });

  it('asks the runtime to stop and to run, and takes the program out of the runtimes', async () => {
    const other = { id: 'Pump', module: 'vm', run: true, program: {} };
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')), config: { instances: [{ id: 'Tank' }, other] } });
    const { calls, container } = await opened(backend);
    calls.length = 0;
    const asked: string[] = [];
    let agree = false;
    vi.stubGlobal('confirm', (question: string) => {
      asked.push(question);
      return agree;
    });

    // Stop is not asked about. Run is: it starts writing to the plant.
    click(byTestId(container, 'runtime-stop'));
    await settle();
    expect(asked).toEqual([]);
    click(byTestId(container, 'runtime-run'));
    await settle();
    expect(asked).toHaveLength(1);
    expect(asked[0]).toContain('warm restart');
    expect(writes(calls)).toEqual(['POST /runtimes/Tank/stop']);

    agree = true;
    click(byTestId(container, 'runtime-run'));
    await settle();
    click(byTestId(container, 'runtime-unload'));
    await settle();

    expect(writes(calls)).toEqual(['POST /runtimes/Tank/stop', 'POST /runtimes/Tank/run', 'PUT /projects/plant/runtimes']);
    expect(backend.config?.instances).toEqual([other]);
  });

  it('keeps Stop within reach while another request is still unanswered', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')) });
    const { container } = await opened(backend);
    // A check that never comes back.
    backend.intercept = (call) => (call.path.endsWith('/check') ? (new Promise(() => undefined) as unknown as Response) : undefined);
    click(button(container, 'Check'));
    await settle();

    expect((byTestId(container, 'program-load') as HTMLButtonElement).disabled).toBe(true);
    expect((byTestId(container, 'runtime-run') as HTMLButtonElement).disabled).toBe(true);
    expect((byTestId(container, 'runtime-stop') as HTMLButtonElement).disabled).toBe(false);
  });

  it('a program running in a runtime of several programs is not said to be not loaded', async () => {
    const line = runtimeOf('Line', { program: 'Pump, Tank', version: 'bbbbbbbbbbbb, cccccccccccc' });
    const backend = tank({
      runtimes: runtimesOf(line),
      config: { instances: [{ id: 'Line' }] },
      configured: [{ id: 'Line', programs: ['Pump', 'Tank'] }],
    });
    const { calls, container } = await opened(backend);
    const why = 'program Tank runs in runtime Line with other programs, which this editor does not show yet';
    expect(mode(container)).toBe('UNKNOWN');
    expect(byTestId(container, 'runtime-unknown-why')?.textContent).toBe(why);
    // Every command acts on the runtime named like the program: none is offered, and each says why.
    for (const id of ['program-load', 'program-load-cold', 'runtime-run', 'runtime-stop', 'runtime-unload', 'runtime-release-all']) {
      const command = byTestId(container, id) as HTMLButtonElement;
      expect(command.disabled, id).toBe(true);
      expect(command.title, id).toBe(why);
    }
    // Nor is any sent when asked for another way: nothing claims an effect it did not have.
    calls.length = 0;
    expect(await state().download('Tank')).toBe(false);
    expect(state().error).toBe(`Program Tank was not loaded: ${why}.`);
    expect(await state().unload('Tank')).toBe(false);
    expect(state().error).toBe(`Program Tank was not unloaded: ${why}.`);
    await state().command('Tank', false);
    expect(await state().force('Tank', 'speed', 1)).toBe(false);
    expect(await state().release('Tank', null)).toBe(false);
    expect(writes(calls)).toEqual([]);
    expect(backend.config?.instances).toEqual([{ id: 'Line' }]);

    // Which runtime runs it is the logic module's configuration, whatever PROGRAM the runtimes
    // report: taken out of Line, it is the runtime of its own name that runs it.
    backend.configured = [{ id: 'Line', programs: ['Pump', 'Tanker'] }];
    backend.runtimes = runtimesOf(runtimeOf('Line', { program: 'Pump, Tank' }), runtimeOf('Tank'));
    await doing(() => state().readRuntimes());
    expect(mode(container)).toBe('RUN');
    expect((byTestId(container, 'program-load') as HTMLButtonElement).disabled).toBe(false);
  });

  it('a program alone in a configured runtime is not said to be not loaded, whatever PROGRAM its source declares', async () => {
    // Its source declares PROGRAM Other: the controller reports runtime R running 'Other'.
    const { container } = await opened(
      tank({ runtimes: runtimesOf(runtimeOf('R', { program: 'Other' })), configured: [{ id: 'R', programs: ['Tank'] }] })
    );
    expect(mode(container)).toBe('UNKNOWN');
    expect(byTestId(container, 'runtime-unknown-why')?.textContent).toBe(
      'program Tank runs in runtime R, which this editor does not show yet'
    );
    expect((byTestId(container, 'program-load') as HTMLButtonElement).disabled).toBe(true);
    // The same, read as a view.
    expect(
      runState('Tank', {
        runtimes: [runtimeOf('R', { program: 'Other' })],
        modules: ['vm'],
        error: null,
        readAtMs: 0,
        configured: { tank: 'R' },
        configuredError: null,
      })
    ).toEqual({ kind: 'unknown', runtime: 'R', why: 'program Tank runs in runtime R, which this editor does not show yet' });
  });

  it('a program not loaded is said to be not loaded when another program running alone declares its PROGRAM name', async () => {
    // A project with no configured runtime: Pump2's source is a copy of Tank's, so it declares
    // PROGRAM Tank, and the controller reports runtime Pump2 running 'Tank'.
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Pump2', { program: 'Tank' })) });
    const { container } = await opened(backend);
    expect(mode(container)).toBe('NOT LOADED');
    expect((byTestId(container, 'program-load') as HTMLButtonElement).disabled).toBe(false);
    const view = { runtimes: [runtimeOf('Pump2', { program: 'Tank' })], modules: ['vm'], error: null, readAtMs: 0 };
    expect(runState('Tank', view)).toEqual({ kind: 'not-loaded' });
    expect(runState('Tank', { ...view, configured: {}, configuredError: null })).toEqual({ kind: 'not-loaded' });
  });

  it('a program put in a configured runtime while its build alone runs says so, and offers no Load the module refuses', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')), configured: [{ id: 'Line', programs: ['Tank'] }] });
    const { calls, container } = await opened(backend);
    const why =
      'program Tank is in runtime Line, which is not loaded and which this editor does not show yet; its build alone, loaded before, still runs as runtime Tank';
    expect(mode(container)).toBe('UNKNOWN');
    expect(byTestId(container, 'runtime-unknown-why')?.textContent).toBe(why);
    for (const id of ['program-load', 'program-load-cold']) {
      const command = byTestId(container, id) as HTMLButtonElement;
      expect(command.disabled, id).toBe(true);
      expect(command.title, id).toBe(why);
    }
    calls.length = 0;
    expect(await state().download('Tank')).toBe(false);
    expect(state().error).toBe(`Program Tank was not loaded: ${why}.`);
    expect(writes(calls)).toEqual([]);
  });

  it('a program is not said to be not loaded while the configured runtimes cannot be read', async () => {
    const backend = tank({ configured: 'fails' });
    const { container } = await opened(backend);
    expect(mode(container)).toBe('UNKNOWN');
    expect(byTestId(container, 'runtime-unknown-why')?.textContent).toBe(
      "whether program Tank runs in a runtime configured in the logic module is not known: 500 Internal Server Error: logic/tasks.json cannot be read as the project's runtimes: expected value at line 1 column 1"
    );
    // A runtime of its own name runs it, whatever else is not known.
    backend.runtimes = runtimesOf(runtimeOf('Tank'));
    await doing(() => state().readRuntimes());
    expect(mode(container)).toBe('RUN');
  });

  it('says so when the runtime runs another version than the one just built', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank', { version: 'older' })) });
    const { container } = await opened(backend);
    click(byTestId(container, 'program-load'));
    await settle();
    expect(byTestId(container, 'program-version-differs')?.textContent).toContain('(older)');

    backend.runtimes = runtimesOf(runtimeOf('Tank', { version: 'build1' }));
    await doing(() => state().readRuntimes());
    expect(byTestId(container, 'program-version-differs')).toBeNull();
  });
});

describe('a program a configured runtime runs', () => {
  it("an assigned program's Task panel names its runtime and task and edits nothing the runtime ignores", async () => {
    const line = {
      id: 'Line',
      module: 'vm',
      target: 'pc',
      run: true,
      tasks: [
        { name: 'slow', kind: { type: 'cyclic' as const, interval_ms: 250, phase_ms: 5 }, priority: 4, watchdog_ms: 3000, programs: ['Pump'] },
        { name: 'fast', kind: { type: 'cyclic' as const, interval_ms: 10, phase_ms: 0 }, priority: 1, programs: ['tank'] },
      ],
    };
    const { container } = await opened(tank({ configured: [line], runtimes: runtimesOf(runtimeOf('Line', { program: 'Pump, Tank' })) }));
    await until('the configured runtimes are read', () => byTestId(container, 'program-task') !== null);
    const inspector = paneOf(container, 'inspector', PANES.program);
    if (!inspector) throw new Error('no properties');

    expect(byTestId(inspector, 'program-task-runs')?.textContent).toBe('Runs in task fast of runtime Line.');
    expect(byTestId(inspector, 'program-task-kind')?.textContent).toBe('cyclic, every 10 ms');
    expect(byTestId(inspector, 'program-task-priority')?.textContent).toBe('1');
    expect(byTestId(inspector, 'program-task-watchdog')?.textContent).toBe("the runtime's default");
    expect(byTestId(inspector, 'program-task-unused')?.textContent).toContain('not used while the program is in a runtime');
    // The program's own interval, module, target and run are not offered: the runtime ignores them.
    for (const label of ['Interval in milliseconds', 'Runtime module', 'Target profile']) expect(byLabel(inspector, label), label).toBeNull();
    expect(inspector.querySelector('input[type="checkbox"]')).toBeNull();

    // The way to the runtime, from the panel and from the UNKNOWN reason beside the runtime.
    click(byTestId(inspector, 'program-task-open'));
    expect(useShell.getState().editors.map((e) => `${e.kind}:${e.target ?? ''}`)).toContain('logic-runtime:Line');
    expect(byTestId(container, 'runtime-open')?.textContent).toBe('Open runtime Line');
  });

  it('a program in no runtime shows its own task settings', async () => {
    const { container } = await opened(tank({ configured: [] }));
    await until('the configured runtimes are read', () => usePrograms.getState().runtimes.assigned !== null);
    const inspector = paneOf(container, 'inspector', PANES.program);
    if (!inspector) throw new Error('no properties');
    expect(byTestId(inspector, 'program-task')).toBeNull();
    expect((byLabel(inspector, 'Interval in milliseconds') as HTMLInputElement | null)?.value).toBe('100');
    expect(byTestId(inspector, 'program-task-unknown')).toBeNull();
    expect(byTestId(container, 'runtime-open')).toBeNull();
  });
});

describe('the values of the running program', () => {
  const values = (container: HTMLElement) =>
    [...container.querySelectorAll('[data-testid="variable-value"]')].map(
      (cell) => `${cell.getAttribute('data-state')}:${cell.textContent}`
    );
  const RUNNING = {
    level: { type: 'INT', value: 20 },
    ok: { type: 'BOOL', value: true },
    // A BOOL that is not one: the runtime sent something else.
    run: { type: 'BOOL', value: 5 },
    't.ET': { type: 'TIME', value: 1_500_000_000 },
    note: { type: 'STRING[8]', value: "it's 5" },
  };

  it('shows each variable as the running program has it, and says what is not a value', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')), watch: watchOf(RUNNING) });
    const { calls, container } = await opened(backend);

    // Every variable with one value is asked for by name, bound to a tag or not.
    const asked = calls.filter((call) => call.path.endsWith('/runtimes/Tank/watch'));
    expect(asked.length).toBeGreaterThan(0);
    expect(asked[0]?.body).toEqual({ variables: ['level', 'ok', 'speed', 'run', 't.ET', 'note'] });
    expect(values(container)).toEqual([
      'live:20',
      'live:TRUE',
      // The running version has no such variable: said, and not shown as a value or as zero.
      'absent:not in the running program',
      'none:—',
      'live:1500 ms',
      // A STRING as ST writes it.
      "live:'it$'s 5'",
    ]);
  });

  it('marks the values of a program that is not running as what it stopped with', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank', { mode: 'stop' })), watch: watchOf(RUNNING, 'stop') });
    const { container } = await opened(backend);
    expect(values(container).slice(0, 2)).toEqual(['stopped:20stop', 'stopped:TRUEstop']);
  });

  it('says UNKNOWN when the values cannot be read, and shows none that was read before', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')), watch: watchOf(RUNNING) });
    const { container } = await opened(backend);
    expect(values(container)[0]).toBe('live:20');

    backend.watch = 'fails';
    await doing(() => state().readWatch('Tank', ['level', 'ok', 'speed', 'run', 't.ET', 'note']));
    expect(values(container)).toEqual(Array(6).fill('unknown:UNKNOWN'));
    expect(container.querySelector('[data-testid="variable-value"]')?.getAttribute('title')).toContain('does not answer');
  });

  it('says UNKNOWN for values that are not current, and shows them again once they are', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')), watch: watchOf(RUNNING) });
    const { calls, container } = await opened(backend);
    expect(values(container)[0]).toBe('live:20');
    for (const { label, ages, current, why } of CURRENCY_CASES) {
      await answerNext(backend, calls, 'Tank', watchOf(RUNNING, 'run', ages));
      if (current) {
        expect(values(container).slice(0, 2), label).toEqual(['live:20', 'live:TRUE']);
        continue;
      }
      expect(values(container), label).toEqual(Array(6).fill('unknown:UNKNOWN'));
      expect(container.querySelector('[data-testid="variable-value"]')?.getAttribute('title'), label).toContain(why);
    }
  });

  it('shows what a stopped program stopped with only while it was read lately', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank', { mode: 'stop' })), watch: watchOf(RUNNING, 'stop') });
    const { calls, container } = await opened(backend);
    // Not scanning is what STOP is: no scan age is asked of it, only a recent read.
    await answerNext(backend, calls, 'Tank', watchOf(RUNNING, 'stop', { scan_age_us: undefined }));
    expect(values(container).slice(0, 2)).toEqual(['stopped:20stop', 'stopped:TRUEstop']);
    await answerNext(backend, calls, 'Tank', watchOf(RUNNING, 'stop', { age_ms: 1501 }));
    expect(values(container)[0]).toBe('unknown:UNKNOWN');
  });

  it('stops showing the last values when no newer answer comes', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')), watch: watchOf(RUNNING) });
    const { container } = await opened(backend);
    expect(values(container)[0]).toBe('live:20');
    // A controller that answers nothing any more: no request comes back to redraw the editor.
    backend.watch = 'hangs';
    backend.runtimes = 'hangs';
    // Its own reason, which only the watch's rule gives (the runtimes' comes later).
    const reason = () => container.querySelector('[data-testid="variable-value"]')?.getAttribute('title') ?? '';
    await until('the values are no longer current', () => (reason().includes('no newer answer has come') ? true : null));
    expect(values(container)[0]).toBe('unknown:UNKNOWN');
  });

  it('ages what it read on a monotonic clock: a wall clock set back an hour keeps nothing current', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')), watch: watchOf(RUNNING) });
    const { container } = await opened(backend);
    expect(values(container)[0]).toBe('live:20');
    const wall = Date.now.bind(Date);
    const back = vi.spyOn(Date, 'now').mockImplementation(() => wall() - 3_600_000);
    try {
      // The runtimes go on answering (and redrawing); only the watch is left hanging.
      backend.watch = 'hangs';
      await until('the values are no longer current', () => (values(container)[0] === 'unknown:UNKNOWN' ? true : null));
      expect(container.querySelector('[data-testid="variable-value"]')?.getAttribute('title')).toContain('no newer answer has come');
    } finally {
      back.mockRestore();
    }
  });

  it('marks the runtime last known when the runtimes go unanswered, and shows no value', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')), watch: watchOf(RUNNING) });
    const { container } = await opened(backend);
    expect(values(container)[0]).toBe('live:20');
    expect(byTestId(container, 'runtime-stale')).toBeNull();
    // Asked, never answered: no error comes back to say so.
    backend.runtimes = 'hangs';
    await until('the runtime is marked last known', () => byTestId(container, 'runtime-stale'));
    expect(byTestId(container, 'runtime-stale')?.textContent).toBe('LAST KNOWN');
    expect(byTestId(container, 'runtime-stale-why')?.textContent).toContain('what was last known, not what is');
    expect(values(container)[0]).toBe('unknown:UNKNOWN');
  });

  it('shows no value once the runtimes can no longer be read', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')), watch: watchOf(RUNNING) });
    const { container } = await opened(backend);
    expect(values(container)[0]).toBe('live:20');

    backend.runtimes = 'fails';
    await doing(() => state().readRuntimes());
    expect(values(container)[0]).toBe('unknown:UNKNOWN');
  });
});

describe('forcing a variable of the running program', () => {
  const RUNNING = { level: { type: 'INT', value: 20 }, speed: { type: 'INT', value: 300 }, ok: { type: 'BOOL', value: true } };
  const forceCalls = (calls: Array<{ method: string; path: string; body: unknown }>) =>
    calls
      .filter((call) => /\/runtimes\/Tank\/(force|release)$/.test(call.path))
      .map((call) => `${call.path.replace(/^.*\/runtimes\/Tank\//, '')} ${JSON.stringify(call.body)}`);
  const forceButtons = (container: HTMLElement) =>
    [...container.querySelectorAll('[data-testid="variable-force"]')].map((b) => b.getAttribute('aria-label'));

  it('offers Force for each BOOL and number of a running program, and sends what the engineer typed', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank', { forced: [], forced_tags: [] })), watch: watchOf(RUNNING) });
    const { calls, container } = await opened(backend);

    // Not a TIME, not a STRING: those cannot be forced.
    expect(forceButtons(container)).toEqual(['Force level', 'Force ok', 'Force speed', 'Force run']);
    expect(byTestId(container, 'runtime-forced')?.textContent).toBe('none');
    expect((byTestId(container, 'runtime-release-all') as HTMLButtonElement).disabled).toBe(true);

    const asked: string[] = [];
    vi.stubGlobal('prompt', (message: string) => (asked.push(message), '444'));
    click(byLabel(container, 'Force speed'));
    await settle();
    expect(asked[0]).toContain('Force speed (INT) of Tank');
    expect(asked[0]).toContain('an output bound to it is written this value');
    vi.stubGlobal('prompt', () => 'true');
    click(byLabel(container, 'Force ok'));
    await settle();
    expect(forceCalls(calls)).toEqual(['force {"variable":"speed","value":444}', 'force {"variable":"ok","value":true}']);
  });

  it('sends nothing when the engineer cancels or types what is not a value of the type, and says why', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank', { forced: [] })), watch: watchOf(RUNNING) });
    const { calls, container } = await opened(backend);

    vi.stubGlobal('prompt', () => null);
    click(byLabel(container, 'Force speed'));
    await settle();
    for (const typed of ['fast', '2.5', '']) {
      vi.stubGlobal('prompt', () => typed);
      click(byLabel(container, 'Force speed'));
      await settle();
    }
    vi.stubGlobal('prompt', () => 'yes');
    click(byLabel(container, 'Force ok'));
    await settle();
    expect(forceCalls(calls)).toEqual([]);
    expect(byTestId(container, 'program-error')?.textContent).toBe('yes is not a value of ok, a BOOL: nothing was forced.');
  });

  it('marks a forced value, lists what is forced, and releases one or every one after asking', async () => {
    const backend = tank({
      runtimes: runtimesOf(runtimeOf('Tank', { forced: [{ variable: 'speed', value: 444 }], forced_tags: [SPEED] })),
      watch: watchOf({ ...RUNNING, speed: { type: 'INT', value: 444, forced: true } }),
    });
    const { calls, container } = await opened(backend);

    const speed = [...container.querySelectorAll('[data-testid="variable-value"]')][2] as HTMLElement;
    expect(speed.getAttribute('data-forced')).toBe('true');
    expect(speed.textContent).toBe('444FORCED');
    expect(container.querySelectorAll('[data-testid="variable-forced"]')).toHaveLength(1);
    expect(byTestId(container, 'runtime-forced-badge')?.textContent).toBe('FORCED 1');
    expect(texts(container, 'runtime-forced-variable')).toEqual(['speed = 444']);
    // A forced variable is released, not forced again.
    expect(forceButtons(container)).toEqual(['Force level', 'Force ok', 'Force run']);

    vi.stubGlobal('confirm', () => false);
    click(byLabel(container, 'Release speed'));
    click(byTestId(container, 'runtime-release-all'));
    await settle();
    expect(forceCalls(calls)).toEqual([]);

    vi.stubGlobal('confirm', () => true);
    click(byLabel(container, 'Release speed'));
    await settle();
    click(byTestId(container, 'runtime-release-all'));
    await settle();
    expect(forceCalls(calls)).toEqual(['release {"variable":"speed"}', 'release {}']);
  });

  it('says UNKNOWN for what is forced when the controller does not say, and offers no Force', async () => {
    const backend = tank({ runtimes: runtimesOf(runtimeOf('Tank')), watch: watchOf(RUNNING) });
    const { container } = await opened(backend);
    expect(byTestId(container, 'runtime-forced')?.textContent).toBe('UNKNOWN');
    expect(byTestId(container, 'runtime-forced-badge')).toBeNull();
    expect(forceButtons(container)).toEqual([]);
  });

  it('offers no Force for a program that is not running, nor to a viewer', async () => {
    const stopped = tank({ runtimes: runtimesOf(runtimeOf('Tank', { mode: 'stop', forced: [] })), watch: watchOf(RUNNING, 'stop') });
    const { container } = await opened(stopped);
    expect(forceButtons(container)).toEqual([]);
    unmountAll();

    useSession.setState({ phase: 'signed-in', user: { username: 'otto', display_name: 'Otto', role: 'operator', via: 'session' } });
    const forced = tank({
      runtimes: runtimesOf(runtimeOf('Tank', { forced: [{ variable: 'speed', value: 444 }] })),
      watch: watchOf(RUNNING),
    });
    const { container: asOperator } = await opened(forced);
    expect(forceButtons(asOperator)).toEqual([]);
    expect(byTestId(asOperator, 'variable-release')).toBeNull();
    expect((byTestId(asOperator, 'runtime-release-all') as HTMLButtonElement).disabled).toBe(true);
    // What is forced is shown to everyone.
    expect(byTestId(asOperator, 'runtime-forced-badge')?.textContent).toBe('FORCED 1');
  });
});

describe('who may change a program', () => {
  it('lets a viewer read and check, and change nothing', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { container } = await opened(tank({ runtimes: runtimesOf(runtimeOf('Tank')) }));

    expect(container.querySelector('textarea')?.readOnly).toBe(true);
    for (const id of ['program-load', 'program-load-cold', 'runtime-run', 'runtime-stop', 'runtime-unload']) {
      expect((byTestId(container, id) as HTMLButtonElement).disabled, id).toBe(true);
    }
    expect((button(container, 'Save') as HTMLButtonElement).disabled).toBe(true);
    expect((button(container, 'Check') as HTMLButtonElement).disabled).toBe(false);
    expect((byLabel(container, 'Bind level') as HTMLSelectElement).disabled).toBe(true);
    expect(button(container, 'Remove program…')).toBeNull();
    const tree = render(<ProgramsTree openEditor={() => undefined} />);
    expect(byTestId(tree, 'tree-add-program')).toBeNull();
    expect(texts(tree, 'tree-program')).toEqual(['TankST']);
  });
});

describe('the Programs section of the tree', () => {
  it('adds a program by name, refusing what is not a name before asking the controller', async () => {
    const backend = tank();
    const { calls } = await opened(backend);
    const openedEditors: string[] = [];
    const tree = render(<ProgramsTree openEditor={(name) => openedEditors.push(name)} />);
    calls.length = 0;

    click(byTestId(tree, 'tree-add-program'));
    typeInto(byLabel(tree, 'Program name'), '2pump');
    expect(byTestId(tree, 'tree-program-name-invalid')).not.toBeNull();
    await doing(() => byLabel(tree, 'Program name')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
    expect(writes(calls)).toEqual([]);

    typeInto(byLabel(tree, 'Program name'), 'Pump');
    await doing(() => byLabel(tree, 'Program name')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
    expect(writes(calls)).toEqual(['POST /projects/plant/logic/programs']);
    expect(texts(tree, 'tree-program')).toEqual(['TankST', 'PumpST']);
    expect(openedEditors).toEqual(['Pump']);
  });

  it('marks a program with unsaved changes', async () => {
    const { container } = await opened(tank());
    const tree = render(<ProgramsTree openEditor={() => undefined} />);
    expect(texts(tree, 'tree-program')).toEqual(['TankST']);
    typeSource(container, 'changed');
    expect(texts(tree, 'tree-program')).toEqual(['Tank*ST']);
  });

  it('removes a program only when asked twice, and closes its editor', async () => {
    const backend = tank();
    const calls = server(backend);
    useShell.setState({ project: project('plant'), view: 'project' });
    await settle();
    const removed: string[] = [];
    const container = render(<ProgramEditor name="Tank" onRemoved={(name) => removed.push(name)} />);
    await settle();
    calls.length = 0;

    click(button(container, 'Remove program…'));
    expect(writes(calls)).toEqual([]);
    click(byTestId(container, 'program-remove-confirm'));
    await settle();

    expect(writes(calls)).toEqual(['DELETE /projects/plant/logic/programs/Tank']);
    expect(removed).toEqual(['Tank']);
    expect(state().programs).toEqual([]);
  });
});

describe('in the workbench', () => {
  it('opens a program in an editor of its own kind, titled with its name', async () => {
    server(tank());
    useShell.setState({ project: project('plant'), view: 'project' });
    await settle();
    const tree = render(<ProgramsTree openEditor={(name) => useShell.getState().openEditor(EDITOR, name)} />);
    await doing(() => byTestId(tree, 'tree-program')?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })));

    expect(useShell.getState().editors.map((editor) => editor.id)).toEqual([`${EDITOR}:Tank`]);
  });
});
