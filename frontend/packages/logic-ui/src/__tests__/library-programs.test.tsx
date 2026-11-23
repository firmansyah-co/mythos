// Open programs follow the project's library and its types (D4c stage 2, R14, R15, ADR-014): every
// program is compiled with every library source and with the project's types and data blocks, so
// a check made before either changed is of earlier ones. Right after the event, before a new check
// answers, no program says COMPILES and nothing of that check is drawn; the cause named is the
// true one; programs whose source was read are checked again, quietly, and the new answer clears
// the mark. A program's diagnostic in a library source the module listed opens that source at its
// place. Every assertion reads a rendered element by its test id, or the requests sent.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CheckResult, Diagnostic } from '../bridge';
import { DATA_CHANGED_EVENT, LIBRARY_CHANGED_EVENT } from '../library';
import { LibraryEditor } from '../LibraryEditor';
import { useLibrary } from '../librarystore';
import { PANES, ProgramEditor } from '../ProgramEditor';
import { usePrograms } from '../programs';
import {
  byTestId,
  click,
  emit,
  endTest,
  entryOf,
  ladderOf,
  paneOf,
  project,
  render,
  resetStores,
  runtimesOf,
  server,
  settle,
  texts,
  timeScale,
  until,
  useController,
  useModuleUi,
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

const TANK = 'PROGRAM Tank\nVAR m : Motor; END_VAR\n  m(run := TRUE);\nEND_PROGRAM\n';
const PUMP = 'PROGRAM Pump\nEND_PROGRAM\n';
const MOTORS = 'FUNCTION_BLOCK Motor\nVAR_INPUT run : BOOL; END_VAR\nEND_FUNCTION_BLOCK\n';
/** MOTORS with a mistake at line 2, column 17. */
const BROKEN = 'FUNCTION_BLOCK Motor\nVAR_INPUT run : Nope; END_VAR\nEND_FUNCTION_BLOCK\n';
const NOPE: Diagnostic = { line: 2, col: 17, end_line: 2, end_col: 21, message: 'Nope is not a type', source: 'logic/library/Motors.st' };
/** One of the program's own: drawn on its text at line 3. */
const OWN: Diagnostic = { line: 3, col: 3, end_line: 3, end_col: 4, message: 'Motor has no input run' };

const compiles = (): CheckResult => ({ ok: true, diagnostics: [], variables: [], code_size: 64, memory_size: 8, binding_problems: [] });
const refused = (...diagnostics: Diagnostic[]): CheckResult => ({ ok: false, diagnostics });

/** A response held until the test lets it go. */
function held(): { response: Promise<Response>; release: (response: Response) => void } {
  let release: (response: Response) => void = () => undefined;
  const response = new Promise<Response>((resolve) => {
    release = resolve;
  });
  return { response, release };
}

/** The program checks asked for, by program name, in order. */
const programChecks = (calls: Call[]) =>
  calls.filter((c) => c.method === 'POST' && /\/logic\/programs\/[^/]+\/check$/.test(c.path)).map((c) => c.path.split('/').at(-2));

/** The project `plant` with programs Tank (and the others given), its check answering what `answer` holds. */
function plant(answer: { now: () => CheckResult }, over: Partial<Backend> = {}): Backend {
  return {
    programs: [{ entry: entryOf('Tank'), source: TANK }],
    check: () => answer.now(),
    runtimes: runtimesOf(),
    library: { Motors: MOTORS },
    ...over,
  };
}

/** The workbench has `plant` open and the editor of `name` has checked its saved text once. */
async function opened(backend: Backend, name = 'Tank') {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant' });
  await settle();
  const container = render(<ProgramEditor name={name} />);
  await settle();
  await until('the check is shown', () => byTestId(container, 'check-result'));
  return { calls, container };
}

const summary = (container: HTMLElement) => byTestId(container, 'check-summary')?.textContent;
const panel = (container: HTMLElement) => {
  const pane = paneOf(container, 'diagnostics', PANES.check);
  if (!pane) throw new Error('no diagnostics panel');
  return pane;
};
const libraryChanged = (project = 'plant') => emit(LIBRARY_CHANGED_EVENT, { project, name: 'Motors', what: 'saved' });
const dataChanged = (project = 'plant') => emit(DATA_CHANGED_EVENT, { project, what: 'type', name: 'Recipe', change: 'saved' });

/** Hold the program checks asked after this, each until released, in the order asked. */
function holdChecks(backend: Backend): Array<ReturnType<typeof held>> {
  const holds: Array<ReturnType<typeof held>> = [];
  backend.intercept = (call) => {
    if (call.method !== 'POST' || !/\/logic\/programs\/[^/]+\/check$/.test(call.path)) return undefined;
    const hold = held();
    holds.push(hold);
    return hold.response;
  };
  return holds;
}

describe('a program checked before the library changed', () => {
  it('a program checked before the library changed no longer says COMPILES', async () => {
    const answer = { now: compiles };
    const backend = plant(answer);
    const { container } = await opened(backend);
    expect(summary(container)).toBe('COMPILES');
    expect(byTestId(panel(container), 'check-ok')?.textContent).toBe('COMPILES');

    // The new check hangs: what is shown meanwhile is said to be of an earlier library.
    const holds = holdChecks(backend);
    libraryChanged();
    await settle();
    expect(holds).toHaveLength(1);
    expect(summary(container)).toBe('LIBRARY CHANGED SINCE CHECK');
    expect(byTestId(container, 'check-summary')?.getAttribute('data-state')).toBe('neutral');
    expect(byTestId(panel(container), 'check-library-stale')?.textContent).toBe(
      'This is the check of an earlier library. The library has changed since.'
    );
    expect(byTestId(panel(container), 'check-ok')).toBeNull();
    // Nothing known of now: no count on the Diagnostics tab.
    expect(useModuleUi.getState().diagnostics[PANES.check]?.item.count).toBeNull();

    // Its answer clears the mark.
    holds[0]?.release(Response.json(refused(NOPE)));
    await settle();
    expect(summary(container)).toBe('1 ERROR');
    expect(byTestId(panel(container), 'check-library-stale')).toBeNull();
    expect(texts(panel(container), 'diagnostic-library')).toEqual([`logic/library/Motors.st 2:17${NOPE.message}`]);
  });

  it('nothing of the earlier check is drawn on the text or listed', async () => {
    const answer = { now: () => refused(OWN) };
    const backend = plant(answer);
    const { container } = await opened(backend);
    const editor = byTestId(container, 'source-editor') as HTMLElement;
    expect(texts(editor, 'source-error-line')).toEqual(['3']);
    expect(texts(panel(container), 'diagnostic')).toEqual([`3:3${OWN.message}`]);

    holdChecks(backend);
    libraryChanged();
    await settle();
    expect(summary(container)).toBe('LIBRARY CHANGED SINCE CHECK');
    expect(texts(editor, 'source-error-line')).toEqual([]);
    expect(texts(editor, 'source-error-mark')).toEqual([]);
    expect(texts(panel(container), 'diagnostic')).toEqual([]);
  });

  it('a diagram draws nothing live from a check of an earlier library', async () => {
    const answer = { now: (): CheckResult => ({ ...compiles(), program: 'Motor', version: 'v1', power: [] }) };
    const backend = plant(answer, { programs: [{ entry: entryOf('Motor', { language: 'ld' }), source: ladderOf('Motor', '') }] });
    const { container } = await opened(backend, 'Motor');
    expect(byTestId(container, 'ld-live')?.textContent).not.toBe(
      'No live state is drawn: the library has changed since this text was checked.'
    );
    holdChecks(backend);
    libraryChanged();
    await settle();
    expect(byTestId(container, 'ld-live')?.getAttribute('data-live')).toBe('none');
    expect(byTestId(container, 'ld-live')?.textContent).toBe('No live state is drawn: the library has changed since this text was checked.');
  });

  it('open programs are checked again when the library changes, and show what the new check said', async () => {
    const answer = { now: compiles };
    const backend = plant(answer, {
      programs: [
        { entry: entryOf('Tank'), source: TANK },
        { entry: entryOf('Pump'), source: PUMP },
      ],
    });
    const { container, calls } = await opened(backend);
    // Pump is read too (its editor was opened before, and is not the one shown).
    await usePrograms.getState().open('Pump');
    await settle();
    const before = programChecks(calls).length;

    answer.now = () => refused(NOPE);
    libraryChanged();
    await settle();
    expect(programChecks(calls).slice(before).sort()).toEqual(['Pump', 'Tank']);
    expect(summary(container)).toBe('1 ERROR');
    expect(texts(panel(container), 'diagnostic-library')).toEqual([`logic/library/Motors.st 2:17${NOPE.message}`]);
    // Quietly: nobody here asked, so the editor was never busy and no pane was brought forward.
    expect(usePrograms.getState().busy).toBeNull();
    expect(usePrograms.getState().programs.find((d) => d.entry.name === 'Pump')?.libraryStale).toBe(false);
  });

  it('a program never read is not checked', async () => {
    const answer = { now: compiles };
    const backend = plant(answer, {
      programs: [
        { entry: entryOf('Tank'), source: TANK },
        { entry: entryOf('Never'), source: PUMP.replace('Pump', 'Never') },
      ],
    });
    const { calls } = await opened(backend);
    const before = programChecks(calls).length;
    libraryChanged();
    await settle();
    expect(programChecks(calls).slice(before)).toEqual(['Tank']);
    expect(usePrograms.getState().programs.find((d) => d.entry.name === 'Never')?.source).toBeNull();
  });

  it("the library's change made here re-checks the open program too", async () => {
    const answer = { now: compiles };
    const backend = plant(answer);
    const { container, calls } = await opened(backend);
    // An engineer saves Motors in this workbench; the controller sends the event of that save.
    await useLibrary.getState().open('Motors');
    useLibrary.getState().edit('Motors', BROKEN);
    // Saved after its check: the stand-in compiler here takes every library.
    await useLibrary.getState().save('Motors');
    await settle();
    expect(backend.library?.Motors).toBe(BROKEN);
    const before = programChecks(calls).length;
    const holds = holdChecks(backend);
    libraryChanged();
    await settle();
    expect(programChecks(calls).slice(before)).toEqual(['Tank']);
    expect(summary(container)).toBe('LIBRARY CHANGED SINCE CHECK');
    holds[0]?.release(Response.json(refused(NOPE)));
    await settle();
    expect(summary(container)).toBe('1 ERROR');
  });

  it('an answer asked before the library changed does not clear the mark, and an older answer never replaces a newer one', async () => {
    const answer = { now: compiles };
    const backend = plant(answer);
    const { container } = await opened(backend);
    const holds = holdChecks(backend);
    // A check asked before the change (the Check button), then the change and its quiet check.
    void usePrograms.getState().check('Tank');
    await settle();
    libraryChanged();
    await settle();
    expect(holds).toHaveLength(2);

    // The newer answer first: shown.
    holds[1]?.release(Response.json(refused(NOPE)));
    await settle();
    expect(summary(container)).toBe('1 ERROR');
    // The older one after: of the earlier library, and older than what is shown; not applied.
    holds[0]?.release(Response.json(compiles()));
    await settle();
    expect(summary(container)).toBe('1 ERROR');

    // Asked before a change and answered after it, with nothing newer: kept, but still stale.
    const late = holdChecks(backend);
    void usePrograms.getState().check('Tank');
    await settle();
    libraryChanged();
    await settle();
    late[0]?.release(Response.json(compiles()));
    await settle();
    expect(summary(container)).toBe('LIBRARY CHANGED SINCE CHECK');
    late[1]?.release(Response.json(compiles()));
    await settle();
    expect(summary(container)).toBe('COMPILES');
  });
});

describe("a program checked before the project's types or data blocks changed", () => {
  it('says the data changed, never the library', async () => {
    const answer = { now: compiles };
    const backend = plant(answer);
    const { container, calls } = await opened(backend);
    const before = programChecks(calls).length;
    const holds = holdChecks(backend);
    dataChanged();
    await settle();
    expect(programChecks(calls).slice(before)).toEqual(['Tank']);
    expect(summary(container)).toBe('DATA CHANGED SINCE CHECK');
    expect(byTestId(panel(container), 'check-library-stale')?.textContent).toBe(
      'This is a check with earlier types and data blocks of the project. They have changed since.'
    );
    expect(byTestId(panel(container), 'check-library-stale')?.getAttribute('data-cause')).toBe('data');
    expect(byTestId(panel(container), 'check-ok')).toBeNull();

    // Then the library too, before either answer: both are said.
    libraryChanged();
    await settle();
    expect(summary(container)).toBe('LIBRARY AND DATA CHANGED SINCE CHECK');
    expect(byTestId(panel(container), 'check-library-stale')?.textContent).toBe(
      'This is the check of an earlier library, with earlier types and data blocks of the project. Both have changed since.'
    );

    // The newest answer clears both.
    holds[1]?.release(Response.json(compiles()));
    await settle();
    expect(summary(container)).toBe('COMPILES');
    holds[0]?.release(Response.json(refused(NOPE)));
    await settle();
    expect(summary(container)).toBe('COMPILES');
  });

  it('an event of another project changes nothing', async () => {
    const answer = { now: compiles };
    const backend = plant(answer);
    const { container, calls } = await opened(backend);
    const before = calls.length;
    libraryChanged('other');
    dataChanged('other');
    await settle();
    expect(calls.length).toBe(before);
    expect(summary(container)).toBe('COMPILES');
    expect(byTestId(panel(container), 'check-library-stale')).toBeNull();
  });
});

describe("a program's variables after the library or the project's types changed", () => {
  it('the Interface says its variables are of a check made with earlier types, until a check gives them again', async () => {
    const answer = {
      now: (): CheckResult => ({ ...compiles(), variables: [{ path: 'r.speed', type: 'REAL', section: 'local', bindable: true }] }),
    };
    const { container } = await opened(plant(answer));
    expect(texts(container, 'variable-row').map((r) => r.split('Bind')[0])).toEqual(['r.speedREALlocal—']);
    expect(byTestId(container, 'variables-stale')).toBeNull();

    // The type loses `speed`; the quiet check refuses the program, and gives no variables.
    answer.now = () => refused({ line: 2, col: 7, end_line: 2, end_col: 13, message: 'Recipe has no member speed', source: 'data/types/Recipe.st' });
    dataChanged();
    await settle();
    expect(summary(container)).toBe('1 ERROR');
    expect(byTestId(container, 'variables-stale')?.textContent).toBe(
      'These are the variables of the last text that compiled, with earlier types and data blocks of the project: they have changed since.'
    );

    libraryChanged();
    await settle();
    expect(byTestId(container, 'variables-stale')?.textContent).toBe(
      'These are the variables of the last text that compiled, with an earlier library and earlier types and data blocks of the project: they have changed since.'
    );

    // A check that compiles with them as they are now gives the variables again.
    answer.now = () => ({ ...compiles(), variables: [{ path: 'r.rate', type: 'REAL', section: 'local', bindable: true }] });
    libraryChanged();
    await settle();
    expect(summary(container)).toBe('COMPILES');
    expect(texts(container, 'variable-row').map((r) => r.split('Bind')[0])).toEqual(['r.rateREALlocal—']);
    expect(byTestId(container, 'variables-stale')).toBeNull();
  });

  it('variables from a check asked before the library changed are of the earlier library', async () => {
    const answer = { now: (): CheckResult => ({ ...compiles(), variables: [{ path: 'm.run', type: 'BOOL', section: 'local', bindable: true }] }) };
    const backend = plant(answer);
    const { container } = await opened(backend);
    const holds = holdChecks(backend);
    libraryChanged();
    await settle();
    libraryChanged();
    await settle();
    // The first re-check (asked before the second change) answers last-but-one, with variables.
    holds[0]?.release(Response.json(answer.now()));
    await settle();
    expect(byTestId(container, 'variables-stale')?.textContent).toBe(
      'These are the variables of the last text that compiled, with an earlier library: it has changed since.'
    );
    holds[1]?.release(Response.json(answer.now()));
    await settle();
    expect(byTestId(container, 'variables-stale')).toBeNull();
  });
});

describe("a program's diagnostic in a library source", () => {
  it("a program's diagnostic in a library source opens that source at its line", async () => {
    const answer = { now: () => refused(NOPE) };
    const backend = plant(answer, { library: { Motors: BROKEN } });
    const { container } = await opened(backend);
    const link = byTestId(panel(container), 'diagnostic-library');
    expect(link?.tagName).toBe('BUTTON');
    expect(link?.textContent).toBe(`logic/library/Motors.st 2:17${NOPE.message}`);
    expect(byTestId(panel(container), 'diagnostic-elsewhere')).toBeNull();

    click(link);
    const editors = useShell.getState().editors.map((e) => `${e.kind}:${e.target ?? ''}`);
    expect(editors).toContain('logic-library:Motors');
    expect(useShell.getState().editors.find((e) => e.id === useShell.getState().activeEditorId)?.target).toBe('Motors');

    // The workbench mounts that editor: once its text is read, the caret is at the place.
    const motors = render(<LibraryEditor name="Motors" onClose={() => undefined} />);
    await until('the Motors editor shows its text', () => byTestId(motors, 'library-editor'));
    await settle();
    const area = byTestId(motors, 'library-editor')?.querySelector('textarea');
    expect(area?.value).toBe(BROKEN);
    expect(area?.selectionStart).toBe(BROKEN.indexOf('Nope'));
    expect(useLibrary.getState().pending).toBeNull();
  });

  it('a CRLF library source is shown with LF line ends, so the caret lands on the place', async () => {
    const crlf = BROKEN.replace(/\n/g, '\r\n');
    const { container } = await opened(plant({ now: () => refused(NOPE) }, { library: { Motors: crlf } }));
    click(byTestId(panel(container), 'diagnostic-library'));
    const motors = render(<LibraryEditor name="Motors" onClose={() => undefined} />);
    await until('the Motors editor shows its text', () => byTestId(motors, 'library-editor'));
    await settle();
    const area = byTestId(motors, 'library-editor')?.querySelector('textarea');
    // A browser's text area holds `\n` line ends whatever it is given; the caret is placed in that text.
    expect(area?.value).toBe(BROKEN);
    expect(area?.selectionStart).toBe(BROKEN.indexOf('Nope'));
    // Shown as is: still the stored text, not an unsaved change.
    expect(byTestId(motors, 'library-unsaved')).toBeNull();
  });

  it('a place of the stored text is not gone to in an editor with unsaved changes, and it says so', async () => {
    const { container } = await opened(plant({ now: () => refused(NOPE) }, { library: { Motors: BROKEN } }));
    await useLibrary.getState().open('Motors');
    const draft = `(* typed here *)\n${BROKEN}`;
    useLibrary.getState().edit('Motors', draft);
    click(byTestId(panel(container), 'diagnostic-library'));
    const motors = render(<LibraryEditor name="Motors" onClose={() => undefined} />);
    await until('the Motors editor shows its text', () => byTestId(motors, 'library-editor'));
    await settle();
    const area = byTestId(motors, 'library-editor')?.querySelector('textarea');
    expect(area?.value).toBe(draft);
    // Where the text area put the caret when it was given the text (its end), not 2:17 of the draft.
    expect(area?.selectionStart).toBe(draft.length);
    expect(byTestId(motors, 'library-unplaced')?.textContent).toBe(
      'The caret was not moved to line 2, column 17: that place is in the stored text, and the text here is not that text.'
    );
    expect(useLibrary.getState().pending).toBeNull();
  });

  it('one about the whole source opens it, with no place to go to', async () => {
    const WHOLE: Diagnostic = { line: 0, col: 0, end_line: 0, end_col: 0, message: 'logic/library/Motors.st cannot be read as a library source: it is not UTF-8 text', source: 'logic/library/Motors.st' };
    const answer = { now: () => refused(WHOLE) };
    const { container } = await opened(plant(answer));
    const link = byTestId(panel(container), 'diagnostic-library');
    expect(link?.textContent).toBe(`logic/library/Motors.st${WHOLE.message}`);
    click(link);
    expect(useShell.getState().editors.map((e) => `${e.kind}:${e.target ?? ''}`)).toContain('logic-library:Motors');
    expect(useLibrary.getState().pending).toBeNull();
  });

  it('a diagnostic of a source the library does not list stays plain', async () => {
    const GONE: Diagnostic = { ...NOPE, source: 'logic/library/Gone.st' };
    const answer = { now: () => refused(GONE) };
    const { container } = await opened(plant(answer));
    expect(byTestId(panel(container), 'diagnostic-library')).toBeNull();
    const plain = byTestId(panel(container), 'diagnostic-elsewhere');
    expect(plain?.tagName).toBe('LI');
    expect(plain?.textContent).toBe(`logic/library/Gone.st 2:17${NOPE.message}`);
    endTest();

    // Nor one of a listed name when the list could not be read: nothing is known to be there.
    resetStores();
    const failing = plant(
      { now: () => refused(NOPE) },
      {
        intercept: (call) =>
          call.method === 'GET' && call.path.endsWith('/logic/library')
            ? Response.json({ error: '/projects/plant/logic/library: Permission denied (os error 13)' }, { status: 500, statusText: 'Internal Server Error' })
            : undefined,
      }
    );
    const unread = await opened(failing);
    expect(useLibrary.getState().names).toBeNull();
    expect(byTestId(panel(unread.container), 'diagnostic-library')).toBeNull();
    expect(byTestId(panel(unread.container), 'diagnostic-elsewhere')?.textContent).toBe(`logic/library/Motors.st 2:17${NOPE.message}`);
  });
});
