// The editor of a configured runtime (C8d-1, R4–R11, R21–R23): the stored runtime in a form with
// no decision made for the engineer, checked and saved through the logic module, whose words are
// shown as they are; what is shown as saved is what it read back; changes made elsewhere followed
// without losing what is typed. Every assertion reads a rendered element by its test id, or the
// requests sent.

import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { RuntimesAnswer } from '../bridge';
import { deleteRuntimeQuestion, RUNTIME_EDITOR, RUNTIMES_CHANGED_EVENT, type RuntimeEntry } from '../runtimes';
import { numberOf, RUNTIME_DIAGNOSTICS_OWNER, RUNTIME_STATUS_OWNER, RuntimeEditor, WATCHDOG_DEFAULT } from '../RuntimeEditor';
import { refusalOf, WITHHELD } from '../library';
import { useRuntimes } from '../runtimestore';
import {
  byTestId,
  click,
  emit,
  endTest,
  entryOf,
  paneOf,
  project,
  render,
  resetStores,
  server,
  settle,
  texts,
  timeScale,
  typeInto,
  useModuleUi,
  useSession,
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

const LINE: RuntimeEntry = {
  id: 'Line',
  module: 'vm',
  target: 'pc',
  run: true,
  tasks: [{ name: 'fast', kind: { type: 'cyclic', interval_ms: 10, phase_ms: 0 }, priority: 1, programs: ['Tank'] }],
};

/** The installed modules as the core lists them, each with what its profile says of tasks. */
function installed(...modules: Array<{ provides: string; kinds?: string[] }>): RuntimesAnswer {
  return {
    runtimes: [],
    modules: modules.map(({ provides, kinds }) => ({
      provides,
      profile: (kinds === undefined ? { name: 'pc' } : { name: 'pc', tasks: { kinds } }) as { name?: string },
    })),
    problem: null,
  };
}

const plant = (over: Partial<Backend> = {}): Backend => ({
  programs: [program('Tank'), program('Pump'), program('Mixer')],
  configured: [LINE],
  runtimes: installed({ provides: 'vm', kinds: ['cycle', 'cyclic', 'startup', 'event'] }),
  ...over,
});

/** The workbench has `plant` open and an editor on runtime `id` (`fresh`: one added with "Add new runtime"). */
async function opened(backend: Backend, id = 'Line', fresh = false) {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  await settle();
  if (fresh) useRuntimes.getState().openNew(id);
  useShell.getState().openEditor(RUNTIME_EDITOR, id);
  const closed: string[] = [];
  const container = render(<RuntimeEditor id={id} onClose={(gone) => closed.push(gone)} />);
  await settle();
  return { calls, container, closed };
}

const of = (calls: Call[], method: string) => calls.filter((c) => c.method === method && c.path.includes('/logic/runtimes'));
const at = (calls: Call[], method: string) => of(calls, method).map((c) => `${c.path.replace(/^.*\/logic/, '')} ${JSON.stringify(c.body ?? null)}`);
const editors = () => useShell.getState().editors.map((e) => `${e.kind}:${e.target ?? ''}`);
const doc = (id = 'Line') => {
  const found = useRuntimes.getState().docs[id];
  if (!found) throw new Error(`no editor on ${id}`);
  return found;
};
const field = (container: HTMLElement, id: string, index = 0) => container.querySelectorAll(`[data-testid="${id}"]`)[index] ?? null;
const value = (container: HTMLElement, id: string, index = 0) => (field(container, id, index) as HTMLInputElement | HTMLSelectElement | null)?.value;
const checked = (container: HTMLElement, id: string) => (field(container, id) as HTMLInputElement | null)?.checked;
const disabled = (container: HTMLElement, id: string, index = 0) => (field(container, id, index) as HTMLButtonElement | HTMLInputElement | null)?.disabled;
const panel = (container: HTMLElement) => {
  const pane = paneOf(container, 'diagnostics', RUNTIME_DIAGNOSTICS_OWNER);
  if (!pane) throw new Error('no diagnostics panel');
  return pane;
};
/** The tasks the fake module has stored for runtime Line. */
const configuredTasks = (backend: Backend) => (backend.configured === 'fails' ? null : backend.configured?.find((r) => r.id === 'Line') as RuntimeEntry | undefined)?.tasks;
const changed = (over: Record<string, unknown> = {}) => emit(RUNTIMES_CHANGED_EVENT, { project: 'plant', runtime: 'Line', what: 'saved', ...over });

/** A response held until the test lets it go. */
function held(): { response: Promise<Response>; release: (response: Response) => void } {
  let release: (response: Response) => void = () => undefined;
  const response = new Promise<Response>((resolve) => {
    release = resolve;
  });
  return { response, release };
}

/** A new runtime's form filled in: module vm, target pc, run, one cyclic task `fast` at priority 3 running `programs`. */
async function fill(container: HTMLElement, programs: string[]) {
  typeInto(field(container, 'runtime-module'), 'vm');
  typeInto(field(container, 'runtime-target'), 'pc');
  click(field(container, 'runtime-run'));
  click(field(container, 'runtime-add-task'));
  typeInto(field(container, 'task-name'), 'fast');
  typeInto(field(container, 'task-kind'), 'cyclic');
  typeInto(field(container, 'task-interval'), '10');
  typeInto(field(container, 'task-priority'), '3');
  for (const name of programs) typeInto(field(container, 'task-add-program'), name);
  await settle();
}

describe('opening a runtime', () => {
  it("shows the stored runtime, and a 404 in the module's words", async () => {
    const { container, calls } = await opened(plant());
    expect(of(calls, 'GET').map((c) => c.path)).toContain('/api/v1/projects/plant/logic/runtimes/Line');
    expect(value(container, 'runtime-module')).toBe('vm');
    expect(value(container, 'runtime-target')).toBe('pc');
    expect(checked(container, 'runtime-run')).toBe(true);
    expect(checked(container, 'runtime-stop')).toBe(false);
    expect(texts(container, 'runtime-task')).toHaveLength(1);
    expect(value(container, 'task-name')).toBe('fast');
    expect(value(container, 'task-kind')).toBe('cyclic');
    expect(value(container, 'task-interval')).toBe('10');
    expect(value(container, 'task-phase')).toBe('0');
    expect(value(container, 'task-priority')).toBe('1');
    // No watchdog stored: empty, with the runtime's default said, not a number made up.
    expect(value(container, 'task-watchdog')).toBe('');
    expect(field(container, 'task-watchdog')?.parentElement?.querySelector('.field__hint')?.textContent).toBe(WATCHDOG_DEFAULT);
    expect(texts(container, 'task-program-name')).toEqual(['Tank']);
    // Configured; whether it runs is the Runtime card's, which is shown beside the form.
    expect(byTestId(container, 'runtime-configured')?.textContent).toBe(
      "Runtime Line as configured in the project's logic module. Whether the controller runs it is on the Runtime card, as the controller lists it."
    );
    expect(byTestId(paneOf(container, 'task-cards', RUNTIME_STATUS_OWNER) ?? container, 'runtime-status')).not.toBeNull();
    expect(byTestId(container, 'runtime-check-state')?.textContent).toBe('NOT CHECKED');
    expect(byTestId(container, 'runtime-unsaved')).toBeNull();
    expect(byTestId(container, 'runtime-new')).toBeNull();
    expect(byTestId(container, 'runtime-missing')).toBeNull();
    expect(useModuleUi.getState().diagnostics[RUNTIME_DIAGNOSTICS_OWNER]?.item.count).toBeNull();
    expect(useModuleUi.getState().diagnostics[RUNTIME_DIAGNOSTICS_OWNER]?.item.title).toBe('Check: Line');
    endTest();

    resetStores();
    const gone = await opened(plant(), 'Nope');
    expect(byTestId(gone.container, 'runtime-read-error')?.textContent).toBe(
      'The runtime Nope could not be read' + "the project has no runtime 'Nope'"
    );
    expect(byTestId(gone.container, 'runtime-editor')).toBeNull();
    expect(byTestId(gone.container, 'runtime-module')).toBeNull();
  });

  it('an implied id is said to be a program running alone', async () => {
    const { container } = await opened(plant(), 'Pump');
    expect(byTestId(container, 'runtime-implied')?.textContent).toBe('Pump is a program running alone, not a configured runtime.');
    expect(byTestId(container, 'runtime-editor')).toBeNull();
    expect(byTestId(container, 'runtime-module')).toBeNull();
    // Nothing that is not edited is unsaved.
    expect(useModuleUi.getState().unsaved['logic-runtimes']).toBeUndefined();
  });

  it('an id is sent in the address encoded', async () => {
    const { calls } = await opened(plant(), 'a b/c');
    expect(of(calls, 'GET').map((c) => c.path)).toContain('/api/v1/projects/plant/logic/runtimes/a%20b%2Fc');
  });
});

describe('a new runtime', () => {
  it('a new runtime has no run choice, no kind and no priority, and is not checked until they are given', async () => {
    const { container, calls } = await opened(plant({ configured: [] }), 'Line', true);
    expect(byTestId(container, 'runtime-new')?.textContent).toBe('NOT SAVED YET');
    // No decision made for the engineer: neither run choice, no module chosen.
    expect(checked(container, 'runtime-run')).toBe(false);
    expect(checked(container, 'runtime-stop')).toBe(false);
    expect(value(container, 'runtime-module')).toBe('');
    expect(value(container, 'runtime-target')).toBe('');
    expect(texts(container, 'runtime-missing-field')).toEqual([
      'whether it runs when loaded: choose "Run when loaded" or "Stay in STOP when loaded"',
    ]);
    expect(disabled(container, 'runtime-check')).toBe(true);
    expect(disabled(container, 'runtime-save')).toBe(true);

    click(byTestId(container, 'runtime-add-task'));
    expect(value(container, 'task-kind')).toBe('');
    expect(value(container, 'task-priority')).toBe('');
    expect(doc().draft.tasks).toEqual([{ name: '', kind: null, priority: null, programs: [] }]);
    expect(texts(container, 'runtime-missing-field')).toEqual([
      'whether it runs when loaded: choose "Run when loaded" or "Stay in STOP when loaded"',
      'task 1: its kind',
      'task 1: its priority',
    ]);
    click(byTestId(container, 'runtime-stop'));
    typeInto(field(container, 'task-name'), 'fast');
    typeInto(field(container, 'task-kind'), 'cyclic');
    expect(texts(container, 'runtime-missing-field')).toEqual(['task fast: its interval', 'task fast: its priority']);
    // Not sent while incomplete, from the store either; and nothing says it checks.
    await useRuntimes.getState().check('Line');
    await useRuntimes.getState().save('Line');
    await settle();
    expect(of(calls, 'POST')).toEqual([]);
    expect(of(calls, 'PUT')).toEqual([]);
    expect(byTestId(container, 'runtime-check-state')?.textContent).toBe('NOT CHECKED');

    typeInto(field(container, 'task-interval'), '10');
    typeInto(field(container, 'task-priority'), '0');
    typeInto(field(container, 'runtime-module'), 'vm');
    typeInto(field(container, 'runtime-target'), 'pc');
    typeInto(field(container, 'task-add-program'), 'Tank');
    expect(byTestId(container, 'runtime-missing')).toBeNull();
    expect(disabled(container, 'runtime-check')).toBe(false);
    click(byTestId(container, 'runtime-check'));
    await settle();
    expect(at(calls, 'POST')).toEqual([
      `/runtimes/Line/check ${JSON.stringify({ module: 'vm', target: 'pc', run: false, tasks: [{ name: 'fast', kind: { type: 'cyclic', interval_ms: 10 }, priority: 0, programs: ['Tank'] }] })}`,
    ]);
    expect(byTestId(container, 'runtime-check-state')?.textContent).toBe('CHECKS');
    // A check writes nothing.
    expect(of(calls, 'PUT')).toEqual([]);
  });

  it('an id the module refuses is shown in its words', async () => {
    // A device name of Windows passes the path's rule; the module's check of the draft says T2.
    const con = await opened(plant({ configured: [] }), 'CON', true);
    await fill(con.container, ['Tank']);
    click(byTestId(con.container, 'runtime-check'));
    await settle();
    expect(texts(panel(con.container), 'runtime-problem')).toEqual([
      "'CON' cannot be a runtime id: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)",
    ]);
    expect(byTestId(con.container, 'runtime-check-state')?.textContent).toBe('DOES NOT CHECK');
    endTest();

    // An id that is not a runtime id is refused by the path's rule (T1), as the module says it.
    resetStores();
    const spaced = await opened(plant({ configured: [] }), 'a b', true);
    await fill(spaced.container, ['Tank']);
    click(byTestId(spaced.container, 'runtime-check'));
    await settle();
    expect(byTestId(spaced.container, 'check-refused')?.textContent).toBe("'a b' is not a runtime id: use 1 to 64 letters, digits, '-' or '_'");
    expect(byTestId(spaced.container, 'runtime-check-state')?.textContent).toBe('NOT CHECKED');
    // Saved: the same words, and nothing stored.
    click(byTestId(spaced.container, 'runtime-save'));
    await settle();
    expect(byTestId(spaced.container, 'save-refused')?.textContent).toBe("'a b' is not a runtime id: use 1 to 64 letters, digits, '-' or '_'");
    expect(byTestId(spaced.container, 'runtime-new')?.textContent).toBe('NOT SAVED YET');
  });
});

describe('the form', () => {
  it('a program another runtime holds is offered disabled with its runtime', async () => {
    const { container } = await opened(plant({ configured: [LINE, { id: 'Other', programs: ['Pump'] }] }));
    const options = [...container.querySelectorAll('[data-testid="task-add-program-option"]')] as HTMLOptionElement[];
    // Tank is in this runtime already and is not offered twice.
    expect(options.map((o) => [o.textContent, o.value, o.disabled])).toEqual([
      ['Pump (in runtime Other)', 'Pump', true],
      ['Mixer', 'Mixer', false],
    ]);
    typeInto(field(container, 'task-add-program'), 'Mixer');
    expect(texts(container, 'task-program-name')).toEqual(['Tank', 'Mixer']);
    // In the draft: not offered again.
    expect(texts(container, 'task-add-program-option')).toEqual(['Pump (in runtime Other)']);
  });

  it('kinds the module does not run are marked, and a module that says nothing is said', async () => {
    const backend = plant({ runtimes: installed({ provides: 'vm', kinds: ['cyclic', 'startup'] }, { provides: 'plc' }) });
    const { container } = await opened(backend);
    expect(texts(container, 'task-kind-option')).toEqual(['cycle (not run by vm)', 'cyclic', 'startup', 'event (not run by vm)']);
    expect(byTestId(container, 'runtime-kinds-note')).toBeNull();

    typeInto(field(container, 'runtime-module'), 'plc');
    expect(byTestId(container, 'runtime-kinds-note')?.textContent).toBe('plc does not say it runs tasks: the core refuses a runtime with tasks for it.');
    // Nothing is marked as not run: the module does not say which it runs.
    expect(texts(container, 'task-kind-option')).toEqual(['cycle', 'cyclic', 'startup', 'event']);

    // A stored module that is not installed is shown as such, not replaced.
    typeInto(field(container, 'runtime-module'), 'vm');
    backend.configured = [{ ...LINE, module: 'gone' }];
    endTest();
    resetStores();
    const stored = await opened({ ...backend, configured: [{ ...LINE, module: 'gone' }] });
    expect(value(stored.container, 'runtime-module')).toBe('gone');
    expect([...stored.container.querySelectorAll('[data-testid="runtime-module"] option')].map((o) => o.textContent)).toEqual([
      'gone (not installed)',
      'vm',
      'plc',
    ]);
  });
});

describe('checking', () => {
  it("Check sends the draft and lists the module's problems and each program's diagnostics", async () => {
    const backend = plant({
      configured: [{ ...LINE, tasks: [{ ...(LINE.tasks[0] as RuntimeEntry['tasks'][number]), programs: ['Tank', 'Pump'] }] }],
      checkRuntimeProgram: (name) =>
        name === 'Tank'
          ? { ok: false, diagnostics: [{ line: 3, col: 5, end_line: 3, end_col: 9, message: 'Nope is not a type', source: 'Tank.st' }], binding_problems: [] }
          : { ok: false, diagnostics: [], binding_problems: ["output 'q' is bound to tag 'Valve', which is not writable"], version: 'v-Pump' },
    });
    const { container, calls } = await opened(backend);
    // The draft, changed and not saved, is what is checked: a task name the module refuses.
    typeInto(field(container, 'task-name'), 'a b');
    click(byTestId(container, 'runtime-check'));
    await settle();
    expect(at(calls, 'POST')).toEqual([
      `/runtimes/Line/check ${JSON.stringify({
        module: 'vm',
        target: 'pc',
        run: true,
        tasks: [{ name: 'a b', kind: { type: 'cyclic', interval_ms: 10, phase_ms: 0 }, priority: 1, programs: ['Tank', 'Pump'] }],
      })}`,
    ]);
    const shown = panel(container);
    expect(texts(shown, 'runtime-problem')).toEqual(["runtime 'Line': 'a b' is not a task name: use 1 to 64 letters, digits, '-' or '_'"]);
    expect([...shown.querySelectorAll('[data-testid="runtime-check-program"]')].map((p) => p.getAttribute('data-program'))).toEqual(['Tank', 'Pump']);
    expect(texts(shown, 'runtime-diagnostic')).toEqual(['Tank.st 3:5Nope is not a type']);
    expect(texts(shown, 'runtime-binding-problem')).toEqual(["output 'q' is bound to tag 'Valve', which is not writable"]);
    expect(byTestId(shown, 'runtime-check-ok')).toBeNull();
    expect(byTestId(container, 'runtime-check-state')?.textContent).toBe('DOES NOT CHECK');
    // The count is every problem the module answered: one of the runtime, one diagnostic, one binding problem.
    expect(useModuleUi.getState().diagnostics[RUNTIME_DIAGNOSTICS_OWNER]?.item.count).toBe(3);
    // A diagnostic opens its program, where it is drawn.
    click(byTestId(shown, 'runtime-diagnostic'));
    expect(editors()).toContain('logic-program:Tank');
    // Nothing was saved.
    expect(of(calls, 'PUT')).toEqual([]);
    expect(byTestId(container, 'runtime-unsaved')?.textContent).toBe('UNSAVED');

    // The draft changes: the check is said to be of an earlier one.
    typeInto(field(container, 'task-name'), 'fast');
    expect(byTestId(container, 'runtime-check-state')?.textContent).toBe('DOES NOT CHECK (EARLIER DRAFT)');
    expect(byTestId(panel(container), 'runtime-check-stale')?.textContent).toBe('This is the check of an earlier draft. The draft has changed since.');
  });

  it("a target that is not a profile is said by each program, and each program is checked once in the project's spelling", async () => {
    const backend = plant({
      configured: [
        {
          ...LINE,
          tasks: [
            { name: 'fast', kind: { type: 'cyclic', interval_ms: 10, phase_ms: 0 }, priority: 1, programs: ['tank'] },
            { name: 'slow', kind: { type: 'cyclic', interval_ms: 100, phase_ms: 0 }, priority: 2, programs: ['TANK', 'Pump'] },
          ],
        },
      ],
    });
    const { container } = await opened(backend);
    typeInto(field(container, 'runtime-target'), 'esp33');
    click(byTestId(container, 'runtime-check'));
    await settle();
    const shown = panel(container);
    // One program however it is spelled, as the module's `members` finds it by its stored entry.
    expect([...shown.querySelectorAll('[data-testid="runtime-check-program"]')].map((p) => p.getAttribute('data-program'))).toEqual(['Tank', 'Pump']);
    expect(texts(shown, 'runtime-diagnostic')).toEqual(["about the whole'esp33' is not a target profile", "about the whole'esp33' is not a target profile"]);
    expect(texts(shown, 'runtime-problem')).toContain("'esp33' is not a target profile");
    expect(byTestId(container, 'runtime-check-state')?.textContent).toBe('DOES NOT CHECK');
  });

  it('an answer for an earlier draft is not applied', async () => {
    const backend = plant();
    const holds: Array<ReturnType<typeof held>> = [];
    backend.intercept = (call) => {
      if (call.method !== 'POST' || !call.path.endsWith('/check') || !call.path.includes('/logic/runtimes')) return undefined;
      const hold = held();
      holds.push(hold);
      return hold.response;
    };
    const { container } = await opened(backend);
    typeInto(field(container, 'task-priority'), '5');
    click(byTestId(container, 'runtime-check'));
    typeInto(field(container, 'task-priority'), '6');
    click(byTestId(container, 'runtime-check'));
    await settle();
    expect(holds).toHaveLength(2);
    expect(byTestId(container, 'runtime-check-state')?.textContent).toBe('CHECKING');
    const answer = (problems: string[]) => Response.json({ ok: problems.length === 0, runtime: 'Line', programs: [], problems });
    // The newer answers first, then the older: the older is dropped.
    holds[1]?.release(answer([]));
    await settle();
    holds[0]?.release(answer(['an answer for priority 5']));
    await settle();
    expect(byTestId(container, 'runtime-check-state')?.textContent).toBe('CHECKS');
    expect(texts(panel(container), 'runtime-problem')).toEqual([]);
    expect(doc().check?.answer.problems).toEqual([]);
  });
});

describe('saving and deleting', () => {
  it('Save sends the four keys and shows what the module read back', async () => {
    // A hand edit spelled the program in lower case; the module respells it on a save.
    const backend = plant({ configured: [{ ...LINE, tasks: [{ ...(LINE.tasks[0] as RuntimeEntry['tasks'][number]), programs: ['tank'] }] }] });
    const { container, calls } = await opened(backend);
    expect(texts(container, 'task-program-name')).toEqual(['tank']);
    typeInto(field(container, 'task-priority'), '4');
    expect(byTestId(container, 'runtime-unsaved')?.textContent).toBe('UNSAVED');
    click(byTestId(container, 'runtime-save'));
    await settle();
    // Save does not check first; exactly the four keys, the id in the path.
    expect(of(calls, 'POST')).toEqual([]);
    expect(at(calls, 'PUT')).toEqual([
      `/runtimes/Line ${JSON.stringify({
        module: 'vm',
        target: 'pc',
        run: true,
        tasks: [{ name: 'fast', kind: { type: 'cyclic', interval_ms: 10, phase_ms: 0 }, priority: 4, programs: ['tank'] }],
      })}`,
    ]);
    // Read back after the save, and that is what is shown: the module's spelling.
    expect(of(calls, 'GET').map((c) => c.path).filter((p) => p.endsWith('/runtimes/Line'))).toHaveLength(2);
    expect(texts(container, 'task-program-name')).toEqual(['Tank']);
    expect(doc().saved?.tasks[0]?.programs).toEqual(['Tank']);
    expect(byTestId(container, 'runtime-unsaved')).toBeNull();
    expect(byTestId(container, 'runtime-saved')?.textContent).toBe('Saved. Nothing was built or loaded.');
  });

  it("a refused save keeps the draft and shows the module's words", async () => {
    const backend = plant();
    const { container, calls } = await opened(backend);
    typeInto(field(container, 'task-add-program'), 'Pump');
    // Meanwhile another engineer put Pump into another runtime.
    backend.configured = [LINE, { id: 'Other', programs: ['Pump'] }];
    click(byTestId(container, 'runtime-save'));
    await settle();
    expect(of(calls, 'PUT')).toHaveLength(1);
    expect(byTestId(container, 'save-refused')?.textContent).toBe("program 'Pump' is in runtime 'Other' already; a program is in one runtime");
    // The draft stays, unsaved, and the stored runtime is what it was.
    expect(texts(container, 'task-program-name')).toEqual(['Tank', 'Pump']);
    expect(byTestId(container, 'runtime-unsaved')?.textContent).toBe('UNSAVED');
    expect(doc().saved?.tasks[0]?.programs).toEqual(['Tank']);
    expect(byTestId(container, 'runtime-saved')).toBeNull();

    // A task rule of the protocol, in its words (400).
    click(container.querySelector('[aria-label="Take Pump out of task fast"]'));
    typeInto(field(container, 'task-interval'), '0');
    click(byTestId(container, 'runtime-save'));
    await settle();
    expect(byTestId(container, 'save-refused')?.textContent).toBe("runtime 'Line': task 'fast' has interval_ms 0; it must be at least 1");
    expect(value(container, 'task-interval')).toBe('0');
    expect(doc().saved).toEqual(LINE);
  });

  it('a number the module cannot read is refused in its words, and nothing is taken as saved', async () => {
    const backend = plant();
    const { container } = await opened(backend);
    typeInto(field(container, 'task-priority'), '-1');
    click(byTestId(container, 'runtime-save'));
    await settle();
    expect(byTestId(container, 'save-refused')?.textContent).toBe('the body is not what this route takes: invalid value: integer `-1`, expected u8');
    expect(byTestId(container, 'runtime-unsaved')).not.toBeNull();
    expect(configuredTasks(backend)).toEqual(LINE.tasks);

    typeInto(field(container, 'task-priority'), '1');
    typeInto(field(container, 'task-interval'), '1.5');
    click(byTestId(container, 'runtime-save'));
    await settle();
    expect(byTestId(container, 'save-refused')?.textContent).toBe('the body is not what this route takes: invalid type: floating point `1.5`, expected u64');
    expect(configuredTasks(backend)).toEqual(LINE.tasks);
  });

  it('a cycle task saved without its shortest cycle is shown as the module stored it', async () => {
    const backend = plant();
    const { container, calls } = await opened(backend);
    typeInto(field(container, 'task-kind'), 'cycle');
    expect(value(container, 'task-min-cycle')).toBe('');
    click(byTestId(container, 'runtime-save'));
    await settle();
    // Sent without it; the module fills its default, 0, and that is what is shown.
    expect((of(calls, 'PUT').at(-1)?.body as { tasks: Array<{ kind: unknown }> }).tasks[0]?.kind).toEqual({ type: 'cycle' });
    expect(value(container, 'task-min-cycle')).toBe('0');
    expect(byTestId(container, 'runtime-unsaved')).toBeNull();
  });

  it('Delete asks first; cancelled, nothing is deleted', async () => {
    const backend = plant();
    const { container, calls, closed } = await opened(backend);
    const ask = vi.spyOn(window, 'confirm').mockReturnValue(false);
    click(byTestId(container, 'runtime-delete'));
    await settle();
    expect(ask).toHaveBeenCalledWith(deleteRuntimeQuestion('Line'));
    expect(of(calls, 'DELETE')).toEqual([]);
    expect(closed).toEqual([]);
    expect(byTestId(container, 'runtime-editor')).not.toBeNull();

    ask.mockReturnValue(true);
    click(byTestId(container, 'runtime-delete'));
    await settle();
    expect(of(calls, 'DELETE').map((c) => c.path)).toEqual(['/api/v1/projects/plant/logic/runtimes/Line']);
    expect(closed).toEqual(['Line']);
    expect(useRuntimes.getState().docs.Line).toBeUndefined();
    // The list is read again: none left.
    expect(useRuntimes.getState().list?.runtimes).toEqual([]);
  });

  it('a refused delete shows the module\'s words', async () => {
    const backend = plant();
    const { container, closed } = await opened(backend);
    backend.configured = [];
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    click(byTestId(container, 'runtime-delete'));
    await settle();
    expect(byTestId(container, 'delete-refused')?.textContent).toBe("the project has no runtime 'Line'");
    expect(closed).toEqual([]);
  });
});

describe('who may change a runtime', () => {
  it('a viewer can check but not save, delete or load', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { container, calls } = await opened(plant());
    expect(byTestId(container, 'runtime-read-only')?.textContent).toBe('Only an engineer changes runtimes.');
    for (const id of ['runtime-save', 'runtime-discard', 'runtime-delete', 'runtime-add-task', 'task-add-program', 'task-remove', 'runtime-load']) {
      expect(byTestId(container, id)).toBeNull();
    }
    // The Runtime card is shown to a viewer, without a command: Run, Stop and Unload are an engineer's.
    const card = byTestId(paneOf(container, 'task-cards', RUNTIME_STATUS_OWNER) ?? container, 'runtime-status');
    expect(card).not.toBeNull();
    for (const id of ['runtime-card-run', 'runtime-card-stop', 'runtime-card-unload']) {
      expect(card?.querySelector(`[data-testid="${id}"]`) ?? null).toBeNull();
      expect(byTestId(container, id)).toBeNull();
    }
    expect(disabled(container, 'runtime-target')).toBe(true);
    expect(disabled(container, 'task-priority')).toBe(true);
    expect((byTestId(container, 'runtime-run-choice') as HTMLFieldSetElement | null)?.disabled).toBe(true);
    // Check is a read: a viewer may.
    click(byTestId(container, 'runtime-check'));
    await settle();
    expect(of(calls, 'POST').map((c) => c.path)).toEqual(['/api/v1/projects/plant/logic/runtimes/Line/check']);
    expect(byTestId(container, 'runtime-check-state')?.textContent).toBe('CHECKS');
  });
});

describe('changes made elsewhere', () => {
  it('a change elsewhere is followed when clean and said when dirty', async () => {
    const backend = plant();
    const { container } = await opened(backend);
    // Clean: read again. The event names the id as the request spelled it, here in another case.
    backend.configured = [{ ...LINE, target: 'esp32' }];
    changed({ runtime: 'LINE' });
    await settle();
    expect(value(container, 'runtime-target')).toBe('esp32');
    expect(byTestId(container, 'runtime-elsewhere')).toBeNull();

    // Changed here: kept, and said.
    typeInto(field(container, 'task-priority'), '7');
    backend.configured = [{ ...LINE, target: 'rp2040' }];
    changed();
    await settle();
    expect(value(container, 'task-priority')).toBe('7');
    expect(value(container, 'runtime-target')).toBe('esp32');
    expect(byTestId(container, 'runtime-elsewhere')?.textContent).toBe('Line was changed elsewhere since it was opened; saving replaces that change.');

    // Deleted elsewhere while changed here: kept, and saving makes it again.
    backend.configured = [];
    changed({ what: 'deleted' });
    await settle();
    expect(byTestId(container, 'runtime-elsewhere')?.textContent).toBe('Line was deleted elsewhere since it was opened; saving makes it again.');
    expect(value(container, 'task-priority')).toBe('7');
    expect(editors()).toEqual(['logic-runtime:Line']);
  });

  it('a runtime deleted elsewhere closes its clean editor', async () => {
    const backend = plant();
    await opened(backend);
    backend.configured = [];
    changed({ what: 'deleted' });
    await settle();
    // Closed through the workbench (entry.tsx closes the editor the store names).
    expect(editors()).toEqual([]);
  });

  it("the echo of this editor's own save is not a change elsewhere", async () => {
    const backend = plant();
    const { container, calls } = await opened(backend);
    typeInto(field(container, 'task-priority'), '4');
    click(byTestId(container, 'runtime-save'));
    await settle();
    // Changed again here, then the event of the save arrives.
    typeInto(field(container, 'task-priority'), '9');
    const reads = of(calls, 'GET').length;
    changed();
    await settle();
    expect(byTestId(container, 'runtime-elsewhere')).toBeNull();
    expect(value(container, 'task-priority')).toBe('9');
    // The list is read again, the runtime is not.
    expect(of(calls, 'GET').slice(reads).map((c) => c.path)).toEqual(['/api/v1/projects/plant/logic/runtimes']);

    // The next event is somebody else's.
    changed();
    await settle();
    expect(byTestId(container, 'runtime-elsewhere')?.textContent).toBe('Line was changed elsewhere since it was opened; saving replaces that change.');
  });

  it("the echo of an own save that comes while a later save is out is not a change elsewhere, and the next change elsewhere is followed", async () => {
    const backend = plant();
    const { container } = await opened(backend);
    typeInto(field(container, 'task-priority'), '4');
    click(byTestId(container, 'runtime-save'));
    await settle();
    // Saved; its event has not come yet. Changed again, and saved again: that save is held.
    typeInto(field(container, 'task-priority'), '40');
    let hold: ReturnType<typeof held> | null = null;
    backend.intercept = (call) => {
      if (call.method !== 'PUT' || !call.path.includes('/logic/runtimes/')) return undefined;
      hold = held();
      return hold.response;
    };
    click(byTestId(container, 'runtime-save'));
    await settle();
    if (hold === null) throw new Error('the second save was not sent');
    const second: ReturnType<typeof held> = hold;
    // The event of the first save comes now, while the second is out; the second is refused.
    changed();
    await settle();
    const words = "runtime 'Line': task 'fast' has priority 40; priorities are 0 (highest) to 31";
    second.release(Response.json({ error: words }, { status: 400, statusText: 'Bad Request' }));
    await settle();
    backend.intercept = undefined;
    expect(byTestId(container, 'save-refused')?.textContent).toBe(words);
    // Nobody else changed it.
    expect(byTestId(container, 'runtime-elsewhere')).toBeNull();

    // Discarded: the stored runtime again, clean.
    click(byTestId(container, 'runtime-discard'));
    await settle();
    expect(byTestId(container, 'runtime-unsaved')).toBeNull();
    expect(value(container, 'task-priority')).toBe('4');
    // Another engineer changes the target: the clean editor follows.
    backend.configured = [{ ...LINE, target: 'esp32', tasks: [{ ...(LINE.tasks[0] as RuntimeEntry['tasks'][number]), priority: 4 }] }];
    changed();
    await settle();
    expect(value(container, 'runtime-target')).toBe('esp32');
  });

  it("with the module's events, a save's own event that comes before its answer is not a change elsewhere", async () => {
    // The fake sends LOGIC_RUNTIMES_CHANGED as the module does on every 200 write, before the
    // answer: the event is heard while the save is out, and is decided once it is answered.
    const backend = plant({ runtimeEvents: true });
    const { container } = await opened(backend);
    typeInto(field(container, 'task-priority'), '4');
    click(byTestId(container, 'runtime-save'));
    await settle();
    expect(byTestId(container, 'runtime-saved')).not.toBeNull();
    expect(byTestId(container, 'runtime-elsewhere')).toBeNull();
    expect(value(container, 'task-priority')).toBe('4');
    // No echo is left over: a change elsewhere after it, with changes here, is said.
    typeInto(field(container, 'task-priority'), '5');
    changed();
    await settle();
    expect(byTestId(container, 'runtime-elsewhere')?.textContent).toBe('Line was changed elsewhere since it was opened; saving replaces that change.');
  });

  it('an event of another project changes nothing', async () => {
    const backend = plant();
    const { container, calls } = await opened(backend);
    const before = calls.length;
    backend.configured = [{ ...LINE, target: 'esp32' }];
    changed({ project: 'other' });
    await settle();
    expect(calls.length).toBe(before);
    expect(value(container, 'runtime-target')).toBe('pc');
  });
});

describe('unsaved changes', () => {
  it('a watchdog, phase or shortest cycle typed and cleared again is not an unsaved change', async () => {
    const backend = plant({ configured: [LINE, { ...LINE, id: 'Mix', tasks: [{ name: 'free', kind: { type: 'cycle', min_cycle_ms: 0 }, priority: 2, programs: ['Mixer'] }] }] });
    const { container } = await opened(backend);
    typeInto(field(container, 'task-watchdog'), '50');
    expect(byTestId(container, 'runtime-unsaved')).not.toBeNull();
    typeInto(field(container, 'task-watchdog'), '');
    expect(value(container, 'task-watchdog')).toBe('');
    expect(byTestId(container, 'runtime-unsaved')).toBeNull();
    // The phase is stored as 0, the module's default an empty field stands for.
    typeInto(field(container, 'task-phase'), '3');
    expect(byTestId(container, 'runtime-unsaved')).not.toBeNull();
    typeInto(field(container, 'task-phase'), '');
    expect(value(container, 'task-phase')).toBe('');
    expect(byTestId(container, 'runtime-unsaved')).toBeNull();
    expect(useModuleUi.getState().unsaved['logic-runtimes']).toBeUndefined();
    endTest();

    resetStores();
    const mix = await opened(backend, 'Mix');
    expect(value(mix.container, 'task-min-cycle')).toBe('0');
    typeInto(field(mix.container, 'task-min-cycle'), '');
    expect(value(mix.container, 'task-min-cycle')).toBe('');
    expect(byTestId(mix.container, 'runtime-unsaved')).toBeNull();
  });

  it('text that is not a finite number is said, and nothing is sent', async () => {
    // What a browser keeps but cannot read comes with an empty value and `badInput`; what reads as
    // Infinity would go out as JSON null. Neither is taken as "nothing typed".
    expect(numberOf('5x', true)).toBeNaN();
    expect(numberOf('', true)).toBeNaN();
    expect(numberOf('1e400')).toBeNaN();
    expect(numberOf('')).toBeNull();
    expect(numberOf(' 50 ')).toBe(50);
    const backend = plant();
    const { container, calls } = await opened(backend);
    typeInto(field(container, 'task-watchdog'), '1e400');
    expect(texts(container, 'runtime-missing-field')).toEqual(['task fast: its watchdog as a number (what is typed is not one)']);
    expect(disabled(container, 'runtime-save')).toBe(true);
    expect(disabled(container, 'runtime-check')).toBe(true);
    expect(byTestId(container, 'runtime-unsaved')).not.toBeNull();
    const sent = of(calls, 'PUT').length + of(calls, 'POST').length;
    click(byTestId(container, 'runtime-save'));
    click(byTestId(container, 'runtime-check'));
    await settle();
    expect(of(calls, 'PUT').length + of(calls, 'POST').length).toBe(sent);
  });

  it('a refused check or read whose answer the workbench withheld says so', async () => {
    const { container } = await opened(plant());
    // `host.api` threw without the answer beside it: the module's reasons are not known here.
    const bare = refusalOf(new Error('400 Bad Request: something'));
    expect(bare.withheld).toBe(true);
    act(() => useRuntimes.setState((state) => ({ docs: { ...state.docs, Line: { ...doc(), checkRefusal: bare } } })));
    expect(byTestId(container, 'check-refused')?.querySelector('p')?.textContent).toBe('400 Bad Request: something');
    expect(byTestId(container, 'check-refused-withheld')?.textContent).toBe(WITHHELD);
    act(() => useRuntimes.setState((state) => ({ docs: { ...state.docs, Line: { ...doc(), phase: 'unreadable', readError: bare } } })));
    expect(byTestId(container, 'read-error-withheld')?.textContent).toBe(WITHHELD);
    // With the answer beside it, nothing is said withheld.
    const answered = refusalOf(Object.assign(new Error('404 Not Found'), { status: 404, body: { error: "the project has no runtime 'Line'" } }));
    act(() => useRuntimes.setState((state) => ({ docs: { ...state.docs, Line: { ...doc(), readError: answered } } })));
    expect(byTestId(container, 'runtime-read-error')?.querySelector('p')?.textContent).toBe("the project has no runtime 'Line'");
    expect(byTestId(container, 'read-error-withheld')).toBeNull();
  });

  it('unsaved changes are reported to the workbench', async () => {
    const { container } = await opened(plant());
    expect(byTestId(container, 'runtime-unsaved')).toBeNull();
    expect(useModuleUi.getState().unsaved['logic-runtimes']).toBeUndefined();

    typeInto(field(container, 'task-priority'), '2');
    expect(byTestId(container, 'runtime-unsaved')?.textContent).toBe('UNSAVED');
    expect(useModuleUi.getState().unsaved['logic-runtimes']).toBe('org.mythos.logic');
    // Back to what is stored: not unsaved, though the draft is another object.
    typeInto(field(container, 'task-priority'), '1');
    expect(byTestId(container, 'runtime-unsaved')).toBeNull();
    expect(useModuleUi.getState().unsaved['logic-runtimes']).toBeUndefined();

    // Discard returns to the saved runtime.
    typeInto(field(container, 'runtime-target'), 'esp32');
    expect(useModuleUi.getState().unsaved['logic-runtimes']).toBe('org.mythos.logic');
    click(byTestId(container, 'runtime-discard'));
    await settle();
    expect(value(container, 'runtime-target')).toBe('pc');
    expect(byTestId(container, 'runtime-unsaved')).toBeNull();
    expect(useModuleUi.getState().unsaved['logic-runtimes']).toBeUndefined();
  });
});
