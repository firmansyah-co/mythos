// The editor of a library source (D4c, R4–R13, R16–R18): the stored text in the Structured Text
// editor, checked with the whole library before it is saved, a text that does not check held
// unless the engineer saves it anyway, the module's refusals in its own words, what is shown as
// saved read back from the module, and changes made elsewhere followed without losing typed
// text. Every assertion reads a rendered element by its test id, or the requests sent.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Diagnostic } from '../bridge';
import { DATA_CHANGED_EVENT, deleteQuestion, LIBRARY_CHANGED_EVENT, LIBRARY_EDITOR, saveAnywayQuestion, type LibraryCheck } from '../library';
import { LIBRARY_DIAGNOSTICS_OWNER, LibraryEditor } from '../LibraryEditor';
import { useLibrary } from '../librarystore';
import {
  byTestId,
  click,
  emit,
  endTest,
  paneOf,
  project,
  render,
  resetStores,
  server,
  settle,
  texts,
  timeScale,
  typeSource,
  until,
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

const MOTORS = 'FUNCTION_BLOCK Motor\nVAR_INPUT run : BOOL; END_VAR\nEND_FUNCTION_BLOCK\n';
const MOTORS2 = 'FUNCTION_BLOCK Motor\nVAR_INPUT run, stop : BOOL; END_VAR\nEND_FUNCTION_BLOCK\n';
const VALVES = 'FUNCTION_BLOCK Valve\nVAR open : BOOL; END_VAR\nEND_FUNCTION_BLOCK\n';
const NOPE = 'Nope is not a type';
/** MOTORS with a mistake the stand-in compiler finds at line 2, column 17. */
const BROKEN = 'FUNCTION_BLOCK Motor\nVAR_INPUT run : Nope; END_VAR\nEND_FUNCTION_BLOCK\n';
/** Where the caret goes for that mistake: its offset in BROKEN. */
const AT_NOPE = BROKEN.indexOf('Nope');

/** A compiler that refuses every source whose text names `Nope`, at its place, in that file. */
function nope(sources: Record<string, string>): LibraryCheck {
  const diagnostics: Diagnostic[] = [];
  for (const [file, text] of Object.entries(sources)) {
    const lines = text.split('\n');
    const line = lines.findIndex((l) => l.includes('Nope'));
    if (line < 0) continue;
    const col = (lines[line] ?? '').indexOf('Nope') + 1;
    diagnostics.push({ line: line + 1, col, end_line: line + 1, end_col: col + 4, message: NOPE, source: file });
  }
  return { ok: diagnostics.length === 0, diagnostics };
}

const motors = (over: Partial<Backend> = {}): Backend => ({
  programs: [],
  library: { Motors: MOTORS, Valves: VALVES },
  compileLibrary: nope,
  ...over,
});

/** The workbench has `plant` open and an editor on library source `name`. */
async function opened(backend: Backend, name = 'Motors') {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  await settle();
  useShell.getState().openEditor(LIBRARY_EDITOR, name);
  const closed: string[] = [];
  const container = render(<LibraryEditor name={name} onClose={(gone) => closed.push(gone)} />);
  await settle();
  return { calls, container, closed };
}

const area = (container: HTMLElement) => {
  const found = byTestId(container, 'library-editor')?.querySelector('textarea');
  if (!(found instanceof HTMLTextAreaElement)) throw new Error('no source editor');
  return found;
};
const doc = (name = 'Motors') => {
  const found = useLibrary.getState().docs[name];
  if (!found) throw new Error(`no editor on ${name}`);
  return found;
};
const panel = (container: HTMLElement) => {
  const pane = paneOf(container, 'diagnostics', LIBRARY_DIAGNOSTICS_OWNER);
  if (!pane) throw new Error('no diagnostics panel');
  return pane;
};
const listed = (container: HTMLElement) =>
  [...panel(container).querySelectorAll('[data-testid="library-diagnostic"]')].map((d) => ({
    kind: d.getAttribute('data-kind'),
    tag: d.tagName,
    text: d.textContent,
  }));
const of = (calls: Call[], method: string) => calls.filter((c) => c.method === method && c.path.includes('/logic/library'));
/** A check of one source's text (`{name, source}`); the check of the stored library (`{}`, stage 2's R19) is not one. */
const ofSource = (c: Call) => c.method === 'POST' && c.path.endsWith('/library/check') && (c.body as { name?: unknown } | undefined)?.name !== undefined;
const checks = (calls: Call[]) => of(calls, 'POST').filter(ofSource).map((c) => c.body);
const puts = (calls: Call[]) => of(calls, 'PUT').map((c) => `${c.path.replace(/^.*\/logic/, '')} ${JSON.stringify(c.body)}`);
const editors = () => useShell.getState().editors.map((e) => `${e.kind}:${e.target ?? ''}`);
const changed = (over: Record<string, unknown> = {}) =>
  emit(LIBRARY_CHANGED_EVENT, { project: 'plant', name: 'Motors', what: 'saved', ...over });

/** A response held until the test lets it go. */
function held(): { response: Promise<Response>; release: (response: Response) => void } {
  let release: (response: Response) => void = () => undefined;
  const response = new Promise<Response>((resolve) => {
    release = resolve;
  });
  return { response, release };
}

describe('opening a library source', () => {
  it('shows the stored text, and a 404 in the module\'s words with no editor', async () => {
    const { container, calls } = await opened(motors());
    expect(area(container).value).toBe(MOTORS);
    expect(byTestId(container, 'library-check')?.textContent).toBe('NOT CHECKED');
    expect(byTestId(container, 'library-unsaved')).toBeNull();
    expect(byTestId(container, 'library-new')).toBeNull();
    expect(byTestId(container, 'library-read-error')).toBeNull();
    expect(of(calls, 'GET').map((c) => c.path)).toContain('/api/v1/projects/plant/logic/library/Motors');
    // Nothing checked yet is nothing known: no count on the Diagnostics tab.
    expect(useModuleUi.getState().diagnostics[LIBRARY_DIAGNOSTICS_OWNER]?.item.count).toBeNull();
    expect(useModuleUi.getState().diagnostics[LIBRARY_DIAGNOSTICS_OWNER]?.item.title).toBe('Check: Motors');
    endTest();

    resetStores();
    const gone = await opened(motors(), 'Pumps');
    expect(byTestId(gone.container, 'library-read-error')?.textContent).toBe(
      "The library source Pumps could not be read" + "the project has no library source 'Pumps'"
    );
    expect(byTestId(gone.container, 'library-editor')).toBeNull();
    expect(gone.container.querySelector('textarea')).toBeNull();
  });

  it('a name is sent in the address encoded', async () => {
    const { calls, container } = await opened(motors(), 'a b/c');
    expect(of(calls, 'GET').map((c) => c.path)).toContain('/api/v1/projects/plant/logic/library/a%20b%2Fc');
    // The core decodes the address before it routes: a name holding `/` never reaches the module,
    // and what is shown is the core's answer, not the module's name rule.
    expect(byTestId(container, 'library-read-error')?.textContent).toBe(
      'The library source a b/c could not be read' + "module 'logic' serves no GET /library/a b/c"
    );
  });

  it('a source that is not text is said, and can be replaced or deleted', async () => {
    const backend = motors({ library: { Motors: MOTORS, Old: { unreadable: 'stream did not contain valid UTF-8' } } });
    const { container, calls, closed } = await opened(backend, 'Old');
    expect(byTestId(container, 'library-read-error')?.textContent).toBe(
      'The stored file of Old could not be read as text: /projects/plant/logic/library/Old.st: stream did not contain valid UTF-8. The text below is not the stored text; saving replaces the file.'
    );
    // An empty draft, which is not the stored text: shown with Save and Delete.
    expect(area(container).value).toBe('');
    expect(byTestId(container, 'library-save')).not.toBeNull();
    expect(byTestId(container, 'library-delete')).not.toBeNull();

    // Replaced: checked with the draft in place of the unreadable file, then saved.
    const old = 'FUNCTION Old : INT\n  Old := 1;\nEND_FUNCTION\n';
    typeSource(container, old);
    click(byTestId(container, 'library-save'));
    await settle();
    expect(checks(calls)).toEqual([{ name: 'Old', source: old }]);
    expect(puts(calls)).toEqual([`/library/Old ${JSON.stringify({ source: old })}`]);
    expect(byTestId(container, 'library-read-error')).toBeNull();
    expect(area(container).value).toBe(old);
    expect(byTestId(container, 'library-unsaved')).toBeNull();

    // Or deleted, as stored again unreadable.
    backend.library = { ...backend.library, Old: { unreadable: 'stream did not contain valid UTF-8' } };
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    click(byTestId(container, 'library-delete'));
    await settle();
    expect(of(calls, 'DELETE').map((c) => c.path)).toEqual(['/api/v1/projects/plant/logic/library/Old']);
    expect(closed).toEqual(['Old']);
    expect(useLibrary.getState().names).toEqual(['Motors']);
  });

  it('a CRLF file opens clean', async () => {
    const crlf = MOTORS.replace(/\n/g, '\r\n');
    const { container } = await opened(motors({ library: { Motors: crlf } }));
    expect(doc().saved).toBe(crlf);
    expect(byTestId(container, 'library-unsaved')).toBeNull();
    expect(useModuleUi.getState().unsaved['logic-library']).toBeUndefined();
    // A text area gives its text back with `\n`: the same text, so still not unsaved.
    typeSource(container, MOTORS);
    expect(byTestId(container, 'library-unsaved')).toBeNull();
    expect(useModuleUi.getState().unsaved['logic-library']).toBeUndefined();
  });
});

describe('checking a library source', () => {
  it("Check sends the draft and lists the module's diagnostics, its own drawn on the text", async () => {
    const TYPE: Diagnostic = { line: 2, col: 3, end_line: 2, end_col: 8, message: 'Speed is declared twice', source: 'data/types/Recipe.st' };
    const WHOLE: Diagnostic = { line: 0, col: 0, end_line: 0, end_col: 0, message: 'the library cannot be compiled' };
    const PROBLEM: Diagnostic = { line: 0, col: 0, end_line: 0, end_col: 0, message: "data/blocks.json cannot be read as the project's data blocks" };
    const backend = motors({
      library: { Motors: MOTORS, Valves: BROKEN.replace('Motor', 'Valve') },
      compileLibrary: (sources) => {
        const found = nope(sources);
        return { ok: false, diagnostics: [...found.diagnostics, TYPE, WHOLE], project_problems: [PROBLEM] };
      },
    });
    const { container, calls } = await opened(backend);
    typeSource(container, BROKEN);
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(checks(calls)).toEqual([{ name: 'Motors', source: BROKEN }]);
    expect(byTestId(container, 'library-check')?.textContent).toBe('DOES NOT CHECK');

    // Its own drawn on its text, at line 2; no other.
    const editor = byTestId(container, 'library-editor') as HTMLElement;
    expect(texts(editor, 'source-error-mark')).toEqual(['Nope']);
    expect(texts(editor, 'source-error-line')).toEqual(['2']);

    // Listed: its own and another listed source's as places to go; a type's and the whole's plain.
    expect(listed(container)).toEqual([
      { kind: 'own', tag: 'BUTTON', text: `2:17${NOPE}` },
      { kind: 'library', tag: 'BUTTON', text: `logic/library/Valves.st 2:17${NOPE}` },
      { kind: 'plain', tag: 'LI', text: 'data/types/Recipe.st 2:3Speed is declared twice' },
      { kind: 'plain', tag: 'LI', text: 'about the wholethe library cannot be compiled' },
    ]);
    expect(byTestId(panel(container), 'library-project-problems-note')?.textContent).toBe(
      "The project's types and data blocks do not check, so the library was checked without the data blocks, and without the types unless they check by themselves."
    );
    expect(texts(panel(container), 'library-project-problem')).toEqual([`about the whole${PROBLEM.message}`]);
    expect(useModuleUi.getState().diagnostics[LIBRARY_DIAGNOSTICS_OWNER]?.item.count).toBe(4);

    // Its own diagnostic puts the caret at its place.
    click(panel(container).querySelector('[data-kind="own"]'));
    await settle();
    expect(area(container).selectionStart).toBe(AT_NOPE);

    // The text changes: the check is of an earlier text, and nothing of it is drawn.
    typeSource(container, `${BROKEN}(* more *)\n`);
    expect(byTestId(container, 'library-check')?.textContent).toBe('DOES NOT CHECK (EARLIER TEXT)');
    expect(texts(editor, 'source-error-mark')).toEqual([]);
    expect(byTestId(panel(container), 'library-check-stale')?.textContent).toBe(
      'This is the check of an earlier text. The text has changed since.'
    );
  });

  it('an answer for an older text is not applied', async () => {
    const first = held();
    let asked = 0;
    const backend = motors({
      intercept: (call) => {
        if (!ofSource(call)) return undefined;
        asked += 1;
        return asked === 1 ? first.response : undefined;
      },
    });
    const { container, calls } = await opened(backend);
    typeSource(container, BROKEN);
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKING');
    typeSource(container, MOTORS2);
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS');
    // The first check's answer comes last: it is of the older text, and is dropped.
    first.release(Response.json(nope({ 'logic/library/Motors.st': BROKEN })));
    await settle();
    expect(checks(calls)).toEqual([
      { name: 'Motors', source: BROKEN },
      { name: 'Motors', source: MOTORS2 },
    ]);
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS');
    expect(doc().check?.text).toBe(MOTORS2);
    expect(listed(container)).toEqual([]);
  });

  it('a diagnostic of another library source opens that source', async () => {
    const backend = motors({
      library: { Motors: MOTORS, Valves: BROKEN.replace('Motor', 'Valve') },
      compileLibrary: (sources) => {
        const found = nope(sources);
        // One of a source the module does not list (gone since the list was read): plain.
        const gone: Diagnostic = { line: 1, col: 1, end_line: 1, end_col: 2, message: 'from a source not listed', source: 'logic/library/Gone.st' };
        return { ok: false, diagnostics: [...found.diagnostics, gone] };
      },
    });
    const { container } = await opened(backend);
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(listed(container)).toEqual([
      { kind: 'library', tag: 'BUTTON', text: `logic/library/Valves.st 2:17${NOPE}` },
      { kind: 'plain', tag: 'LI', text: 'logic/library/Gone.st 1:1from a source not listed' },
    ]);
    click(panel(container).querySelector('[data-kind="library"]'));
    expect(editors()).toEqual(['logic-library:Motors', 'logic-library:Valves']);
    expect(useShell.getState().activeEditorId).toBe(useShell.getState().editors[1]?.id);

    // The workbench mounts that editor: once its text is read, the caret is at the place.
    const valves = render(<LibraryEditor name="Valves" onClose={() => undefined} />);
    await until('the Valves editor shows its text', () => byTestId(valves, 'library-editor'));
    await settle();
    const text = BROKEN.replace('Motor', 'Valve');
    expect(area(valves).value).toBe(text);
    expect(area(valves).selectionStart).toBe(text.indexOf('Nope'));
    expect(useLibrary.getState().pending).toBeNull();
  });

  it('a check that is ok claims nothing about programs', async () => {
    const { container } = await opened(motors());
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS');
    expect(byTestId(panel(container), 'library-check-ok')?.textContent).toBe(
      "The library checks with this text, with the project's types and data blocks."
    );
    expect(byTestId(panel(container), 'library-check-footnote')?.textContent).toBe(
      'Programs are not checked here: a program that declares a name the library declares, or uses what this text no longer has, is refused at its own check.'
    );
    const said = byTestId(panel(container), 'library-check-result')?.textContent ?? '';
    expect(said).not.toMatch(/compiles|COMPILES/);
    expect(useModuleUi.getState().diagnostics[LIBRARY_DIAGNOSTICS_OWNER]?.item.count).toBe(0);
  });
});

describe('what an ok check claims', () => {
  it("an ok check with the project's problems does not claim it was made with the types and data blocks", async () => {
    const PROBLEM: Diagnostic = { line: 0, col: 0, end_line: 0, end_col: 0, message: "data/blocks.json cannot be read as the project's data blocks" };
    const { container } = await opened(motors({ compileLibrary: () => ({ ok: true, diagnostics: [], project_problems: [PROBLEM] }) }));
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS');
    expect(byTestId(panel(container), 'library-check-ok')?.textContent).toBe(
      "The library checks with this text, but not with all of the project's types and data blocks: they do not check, and were left out (below)."
    );
    expect(byTestId(panel(container), 'library-project-problems-note')?.textContent).toBe(
      "The project's types and data blocks do not check, so the library was checked without the data blocks, and without the types unless they check by themselves."
    );
    expect(texts(panel(container), 'library-project-problem')).toEqual([`about the whole${PROBLEM.message}`]);
  });
});

describe('a check of an earlier library', () => {
  it('a check is of an earlier library once another source is saved or deleted elsewhere', async () => {
    const backend = motors();
    const { container } = await opened(backend);
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS');
    expect(byTestId(container, 'library-check')?.className).toBe('badge badge--good');

    backend.library = { ...backend.library, Valves: BROKEN.replace('Motor', 'Valve') };
    changed({ name: 'Valves' });
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS (EARLIER LIBRARY)');
    expect(byTestId(container, 'library-check')?.className).toBe('badge badge--neutral');
    expect(byTestId(panel(container), 'library-check-earlier-library')?.textContent).toBe(
      'This check was made with an earlier library: another library source was saved or deleted since. Check again.'
    );
    expect(useModuleUi.getState().diagnostics[LIBRARY_DIAGNOSTICS_OWNER]?.item.count).toBeNull();

    // Checked again: of the library as it is now.
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('DOES NOT CHECK');
    expect(byTestId(panel(container), 'library-check-earlier-library')).toBeNull();
  });

  it("a change of this very source elsewhere leaves its check as it is (the check put this text in its place)", async () => {
    const backend = motors();
    const { container } = await opened(backend);
    typeSource(container, MOTORS2);
    click(byTestId(container, 'library-check-button'));
    await settle();
    changed();
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS');
  });
});

describe('saving a library source', () => {
  it.each([
    ['a-b', "'a-b' is not a library source name: a letter, then letters, digits or _, at most 64"],
    ['con', "'con' cannot be a library source name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)"],
    ['motors', "the project already has a library source 'Motors'; names that differ only in case are one name"],
  ])("a name the module refuses is shown in the module's words, and nothing is saved (%s)", async (name, words) => {
    const backend = motors();
    const calls = server(backend);
    useShell.setState({ project: project('plant'), view: 'project' });
    await settle();
    // Typed into the tree's Add row (a list read before `Motors` was made, for `motors`).
    useLibrary.getState().openNew(name);
    useShell.getState().openEditor(LIBRARY_EDITOR, name);
    const container = render(<LibraryEditor name={name} onClose={() => undefined} />);
    await settle();
    expect(byTestId(container, 'library-new')?.textContent).toBe('NOT SAVED YET');
    expect(area(container).value).toBe('');
    typeSource(container, MOTORS);

    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(byTestId(container, 'check-refused')?.textContent).toBe(words);
    expect(byTestId(container, 'library-check')?.textContent).toBe('NOT CHECKED');

    click(byTestId(container, 'library-save'));
    await settle();
    expect(byTestId(container, 'save-refused')?.textContent).toBe(words);
    expect(byTestId(container, 'save-held')).toBeNull();
    expect(puts(calls)).toEqual([]);
    expect(checks(calls)).toEqual([
      { name, source: MOTORS },
      { name, source: MOTORS },
    ]);
    expect(byTestId(container, 'library-new')?.textContent).toBe('NOT SAVED YET');
    expect(area(container).value).toBe(MOTORS);
    expect(backend.library).toEqual({ Motors: MOTORS, Valves: VALVES });
  });

  it('a text that does not check is not saved unless the engineer saves it anyway', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const backend = motors();
    const { container, calls } = await opened(backend);
    typeSource(container, BROKEN);
    click(byTestId(container, 'library-save'));
    await settle();
    // Checked first, and held: nothing written.
    expect(checks(calls)).toEqual([{ name: 'Motors', source: BROKEN }]);
    expect(puts(calls)).toEqual([]);
    expect(byTestId(container, 'save-held')?.querySelector('p')?.textContent).toBe(
      'Not saved: with this text the library does not check, and every program of the project would be refused.'
    );
    expect(byTestId(container, 'library-check')?.textContent).toBe('DOES NOT CHECK');
    expect(listed(container)).toEqual([{ kind: 'own', tag: 'BUTTON', text: `2:17${NOPE}` }]);
    expect(byTestId(container, 'library-unsaved')?.textContent).toBe('UNSAVED');
    expect(backend.library?.Motors).toBe(MOTORS);

    // Save anyway asks; cancelled, nothing is written.
    click(byTestId(container, 'library-save-anyway'));
    await settle();
    expect(confirm).toHaveBeenCalledWith(saveAnywayQuestion('Motors'));
    expect(confirm.mock.calls.at(-1)?.[0]).toBe(
      'Save Motors although the library does not check with it? Every program is compiled with every library source, and a mistake in one refuses every program.'
    );
    expect(puts(calls)).toEqual([]);

    // Confirmed: one PUT of the text, which the module writes as it writes any.
    confirm.mockReturnValue(true);
    click(byTestId(container, 'library-save-anyway'));
    await settle();
    expect(puts(calls)).toEqual([`/library/Motors ${JSON.stringify({ source: BROKEN })}`]);
    expect(backend.library?.Motors).toBe(BROKEN);
    expect(byTestId(container, 'save-held')).toBeNull();
    expect(byTestId(container, 'library-unsaved')).toBeNull();
  });

  it('Save anyway is withdrawn once the held text is changed, and never sends a text that was not checked', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const ALSO = 'FUNCTION_BLOCK Motor\nVAR_INPUT run : Nope; stop : Nope; END_VAR\nEND_FUNCTION_BLOCK\n';
    const { container, calls } = await opened(motors());
    typeSource(container, BROKEN);
    click(byTestId(container, 'library-save'));
    await settle();
    expect(byTestId(container, 'library-save-anyway')).not.toBeNull();
    typeSource(container, ALSO);
    expect(byTestId(container, 'save-held')).toBeNull();
    expect(byTestId(container, 'library-save-anyway')).toBeNull();
    // Even asked for directly, the store sends nothing it did not check and hold.
    expect(await useLibrary.getState().save('Motors', true)).toBe(false);
    // Back to the held text: still not offered, since the hold was dropped with the edit.
    typeSource(container, BROKEN);
    expect(byTestId(container, 'library-save-anyway')).toBeNull();
    expect(puts(calls)).toEqual([]);
  });

  it('a held save is dropped when a check of the new text says it checks, or the library or types change', async () => {
    const backend = motors();
    const { container } = await opened(backend);
    typeSource(container, BROKEN);
    click(byTestId(container, 'library-save'));
    await settle();
    expect(byTestId(container, 'save-held')).not.toBeNull();
    // A check of the same text that does not check keeps the hold.
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(byTestId(container, 'save-held')).not.toBeNull();
    // Another source changed: the hold is of an earlier library.
    changed({ name: 'Valves' });
    await settle();
    expect(byTestId(container, 'save-held')).toBeNull();

    click(byTestId(container, 'library-save'));
    await settle();
    expect(byTestId(container, 'save-held')).not.toBeNull();
    emit(DATA_CHANGED_EVENT, { project: 'plant', what: 'type', name: 'Recipe', change: 'saved' });
    await settle();
    expect(byTestId(container, 'save-held')).toBeNull();

    click(byTestId(container, 'library-save'));
    await settle();
    expect(byTestId(container, 'save-held')).not.toBeNull();
    typeSource(container, MOTORS2);
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS');
    expect(byTestId(container, 'save-held')).toBeNull();
    expect(doc().held).toBeNull();
  });

  it('after a save the stored text is shown, not the typed one', async () => {
    const backend = motors({ libraryStores: (source) => `${source}\n` });
    const { container, calls } = await opened(backend);
    typeSource(container, MOTORS2);
    click(byTestId(container, 'library-save'));
    await settle();
    const order = calls.filter((c) => c.path.includes('/logic/library/Motors') || ofSource(c)).map((c) => c.method);
    // Read, check, write, read back.
    expect(order).toEqual(['GET', 'POST', 'PUT', 'GET']);
    expect(area(container).value).toBe(`${MOTORS2}\n`);
    expect(doc().saved).toBe(`${MOTORS2}\n`);
    expect(byTestId(container, 'library-unsaved')).toBeNull();
    expect(byTestId(container, 'library-saved')?.textContent).toBe('Saved.');
    expect(useModuleUi.getState().unsaved['logic-library']).toBeUndefined();
  });

  it('a new source is made by its first save, and the tree lists it from the module', async () => {
    const backend = motors();
    const calls = server(backend);
    useShell.setState({ project: project('plant'), view: 'project' });
    await settle();
    useLibrary.getState().openNew('Pumps');
    const container = render(<LibraryEditor name="Pumps" onClose={() => undefined} />);
    await settle();
    const pumps = 'FUNCTION_BLOCK Pump\nEND_FUNCTION_BLOCK\n';
    typeSource(container, pumps);
    click(byTestId(container, 'library-save'));
    await settle();
    expect(puts(calls)).toEqual([`/library/Pumps ${JSON.stringify({ source: pumps })}`]);
    expect(byTestId(container, 'library-saved')?.textContent).toBe('Saved; the module made the source.');
    expect(byTestId(container, 'library-new')).toBeNull();
    // The list is read again, never edited here.
    expect(of(calls, 'GET').filter((c) => c.path.endsWith('/logic/library'))).toHaveLength(2);
    expect(useLibrary.getState().names).toEqual(['Motors', 'Pumps', 'Valves']);
  });

  it('a save whose read-back fails stays unsaved and says so', async () => {
    let put = false;
    const backend = motors({
      intercept: (call) => {
        if (call.method === 'PUT') put = true;
        if (put && call.method === 'GET' && call.path.endsWith('/library/Motors')) {
          return Response.json({ error: '/projects/plant/logic/library/Motors.st: Input/output error (os error 5)' }, { status: 500, statusText: 'Internal Server Error' });
        }
        return undefined;
      },
    });
    const { container } = await opened(backend);
    typeSource(container, MOTORS2);
    click(byTestId(container, 'library-save'));
    await settle();
    expect(backend.library?.Motors).toBe(MOTORS2);
    expect(byTestId(container, 'library-read-back')?.textContent).toBe(
      'Saved; the stored text could not be read back: /projects/plant/logic/library/Motors.st: Input/output error (os error 5). The text below is what was sent, not what the module stored.'
    );
    expect(area(container).value).toBe(MOTORS2);
    expect(doc().saved).toBeNull();
    expect(byTestId(container, 'library-unsaved')?.textContent).toBe('UNSAVED');
    expect(useModuleUi.getState().unsaved['logic-library']).toBe('org.mythos.logic');
  });

  it.each([
    [409, "the project already has a library source 'Motors'; names that differ only in case are one name"],
    [413, 'the request body is larger than 1048576 bytes'],
    [500, '/projects/plant/logic/library/Motors.st: No space left on device (os error 28)'],
  ])("a refused save keeps the draft and shows the module's words (%i)", async (status, words) => {
    const backend = motors({
      intercept: (call) => (call.method === 'PUT' ? Response.json({ error: words }, { status, statusText: 'Refused' }) : undefined),
    });
    const { container, calls } = await opened(backend);
    typeSource(container, MOTORS2);
    click(byTestId(container, 'library-save'));
    await settle();
    expect(puts(calls)).toHaveLength(1);
    expect(byTestId(container, 'save-refused')?.textContent).toBe(words);
    expect(area(container).value).toBe(MOTORS2);
    expect(doc().saved).toBe(MOTORS);
    expect(byTestId(container, 'library-unsaved')?.textContent).toBe('UNSAVED');
    expect(byTestId(container, 'library-saved')).toBeNull();
  });
});

describe('deleting a library source', () => {
  it('Delete asks first; cancelled, nothing is deleted', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const backend = motors();
    const { container, calls, closed } = await opened(backend);
    click(byTestId(container, 'library-delete'));
    await settle();
    expect(confirm).toHaveBeenCalledWith(deleteQuestion('Motors'));
    expect(confirm.mock.calls.at(-1)?.[0]).toBe(
      'Delete library source Motors? Every program is then compiled without it: one that uses what it held no longer compiles. Nothing is built or loaded.'
    );
    expect(of(calls, 'DELETE')).toEqual([]);
    expect(closed).toEqual([]);
    expect(backend.library?.Motors).toBe(MOTORS);

    confirm.mockReturnValue(true);
    click(byTestId(container, 'library-delete'));
    await settle();
    expect(of(calls, 'DELETE').map((c) => c.path)).toEqual(['/api/v1/projects/plant/logic/library/Motors']);
    expect(closed).toEqual(['Motors']);
    expect(useLibrary.getState().names).toEqual(['Valves']);
  });

  it('a delete closes the editor without reading the source again, and the name can be made again', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const calls = server(motors());
    useShell.setState({ project: project('plant'), view: 'project' });
    await settle();
    useShell.getState().openEditor(LIBRARY_EDITOR, 'Motors');
    // Mounted while the workbench has it open, as its work area mounts the active editor.
    function Mounted() {
      const open = useShell((state) => state.editors.some((e) => e.kind === LIBRARY_EDITOR && e.target === 'Motors'));
      if (!open) return null;
      return (
        <LibraryEditor
          name="Motors"
          onClose={(gone) => {
            const id = useShell.getState().editors.find((e) => e.kind === LIBRARY_EDITOR && e.target === gone)?.id;
            if (id) useShell.getState().closeEditor(id);
          }}
        />
      );
    }
    const container = render(<Mounted />);
    await settle();
    click(byTestId(container, 'library-delete'));
    await settle();
    const after = calls.slice(calls.findIndex((c) => c.method === 'DELETE') + 1).map((c) => `${c.method} ${c.path.replace(/^.*\/logic/, '')}`);
    expect(after.filter((c) => c.startsWith('GET'))).toEqual(['GET /library']);
    expect(editors()).toEqual([]);
    expect(useLibrary.getState().docs).toEqual({});

    // Made again under its name: a new source, not a page that it could not be read.
    useLibrary.getState().openNew('Motors');
    useShell.getState().openEditor(LIBRARY_EDITOR, 'Motors');
    await settle();
    expect(byTestId(container, 'library-new')?.textContent).toBe('NOT SAVED YET');
    expect(byTestId(container, 'library-read-error')).toBeNull();
  });

  it("a refused delete shows the module's words and keeps the editor", async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const words = '/projects/plant/logic/library/Motors.st: Permission denied (os error 13)';
    const { container, closed } = await opened(
      motors({ intercept: (call) => (call.method === 'DELETE' ? Response.json({ error: words }, { status: 500, statusText: 'Internal Server Error' }) : undefined) })
    );
    click(byTestId(container, 'library-delete'));
    await settle();
    expect(byTestId(container, 'delete-refused')?.textContent).toBe(words);
    expect(closed).toEqual([]);
    expect(area(container).value).toBe(MOTORS);
  });

  it('a new source offers no Delete', async () => {
    server(motors());
    useShell.setState({ project: project('plant'), view: 'project' });
    await settle();
    useLibrary.getState().openNew('Pumps');
    const container = render(<LibraryEditor name="Pumps" onClose={() => undefined} />);
    await settle();
    expect(byTestId(container, 'library-delete')).toBeNull();
    expect(byTestId(container, 'library-save')).not.toBeNull();
  });
});

describe('who may change the library', () => {
  it('a viewer can check but not save or delete', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { container, calls } = await opened(motors());
    expect(byTestId(container, 'library-read-only')?.textContent).toBe('Only an engineer changes the library.');
    expect(byTestId(container, 'library-save')).toBeNull();
    expect(byTestId(container, 'library-delete')).toBeNull();
    expect(byTestId(container, 'library-discard')).toBeNull();
    expect(area(container).readOnly).toBe(true);
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(checks(calls)).toEqual([{ name: 'Motors', source: MOTORS }]);
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS');
    // A save held for another reason is still not offered to be forced.
    useLibrary.setState((state) => ({ docs: { ...state.docs, Motors: { ...doc(), held: doc().draft } } }));
    await settle();
    expect(byTestId(container, 'save-held')).not.toBeNull();
    expect(byTestId(container, 'library-save-anyway')).toBeNull();
    expect(of(calls, 'PUT')).toEqual([]);
    expect(of(calls, 'DELETE')).toEqual([]);
  });

  it('an engineer may', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'enzo', display_name: 'Enzo', role: 'engineer', via: 'session' } });
    const { container } = await opened(motors());
    expect(byTestId(container, 'library-read-only')).toBeNull();
    expect(byTestId(container, 'library-save')).not.toBeNull();
    expect(byTestId(container, 'library-delete')).not.toBeNull();
    expect(area(container).readOnly).toBe(false);
  });
});

describe('unsaved changes', () => {
  it('unsaved changes are reported to the workbench', async () => {
    const { container } = await opened(motors());
    expect(useModuleUi.getState().unsaved['logic-library']).toBeUndefined();
    typeSource(container, MOTORS2);
    expect(byTestId(container, 'library-unsaved')?.textContent).toBe('UNSAVED');
    expect(useModuleUi.getState().unsaved['logic-library']).toBe('org.mythos.logic');
    click(byTestId(container, 'library-discard'));
    expect(area(container).value).toBe(MOTORS);
    expect(byTestId(container, 'library-unsaved')).toBeNull();
    expect(useModuleUi.getState().unsaved['logic-library']).toBeUndefined();
  });

  it('closing the project drops what was open, and an answer for it is not applied', async () => {
    const late = held();
    const { container } = await opened(
      motors({ intercept: (call) => (ofSource(call) ? late.response : undefined) })
    );
    typeSource(container, MOTORS2);
    click(byTestId(container, 'library-check-button'));
    await settle();
    useShell.setState({ project: null, view: 'portal' });
    await settle();
    expect(useLibrary.getState().docs).toEqual({});
    expect(useLibrary.getState().names).toBeNull();
    expect(useModuleUi.getState().unsaved['logic-library']).toBeUndefined();
    late.release(Response.json({ ok: true, diagnostics: [] }));
    await settle();
    expect(useLibrary.getState().docs).toEqual({});
  });
});

describe('following changes made elsewhere', () => {
  it('an edit is kept when the source is changed elsewhere, and it says so', async () => {
    const backend = motors();
    const { container } = await opened(backend);
    const typed = `${MOTORS}(* mine *)\n`;
    typeSource(container, typed);
    backend.library = { ...backend.library, Motors: MOTORS2 };
    changed();
    await settle();
    expect(area(container).value).toBe(typed);
    expect(byTestId(container, 'library-elsewhere')?.textContent).toBe(
      'Motors was changed elsewhere since it was opened; saving replaces that change.'
    );
    expect(byTestId(container, 'library-unsaved')?.textContent).toBe('UNSAVED');

    const { Motors: _gone, ...rest } = backend.library;
    backend.library = rest;
    changed({ what: 'deleted' });
    await settle();
    expect(area(container).value).toBe(typed);
    expect(byTestId(container, 'library-elsewhere')?.textContent).toBe(
      'Motors was deleted elsewhere since it was opened; saving makes it again.'
    );
    expect(editors()).toEqual(['logic-library:Motors']);
  });

  it('Discard after a change elsewhere shows the stored text, read again', async () => {
    const backend = motors();
    const { container } = await opened(backend);
    typeSource(container, `${MOTORS}(* mine *)\n`);
    backend.library = { ...backend.library, Motors: MOTORS2 };
    changed();
    await settle();
    click(byTestId(container, 'library-discard'));
    await settle();
    expect(area(container).value).toBe(MOTORS2);
    expect(doc().saved).toBe(MOTORS2);
    expect(byTestId(container, 'library-unsaved')).toBeNull();
    expect(byTestId(container, 'library-elsewhere')).toBeNull();
  });

  it('Discard after a delete elsewhere closes the editor: nothing is stored to go back to', async () => {
    const backend = motors();
    const { container, closed } = await opened(backend);
    typeSource(container, `${MOTORS}(* mine *)\n`);
    const { Motors: _gone, ...rest } = backend.library ?? {};
    backend.library = rest;
    changed({ what: 'deleted' });
    await settle();
    click(byTestId(container, 'library-discard'));
    await settle();
    expect(closed).toEqual(['Motors']);
    expect(useLibrary.getState().docs).toEqual({});
  });

  it('text typed while a change elsewhere is read is kept when that read fails', async () => {
    for (const [status, said] of [
      [404, 'Motors was deleted elsewhere since it was opened; saving makes it again.'],
      [500, 'Motors was changed elsewhere since it was opened; saving replaces that change.'],
    ] as const) {
      resetStores();
      const read = held();
      let armed = false;
      const backend = motors({
        intercept: (call) => (armed && call.method === 'GET' && call.path.endsWith('/library/Motors') ? read.response : undefined),
      });
      const { container } = await opened(backend);
      armed = true;
      changed();
      await settle();
      const typed = `${MOTORS}(* typed meanwhile *)\n`;
      typeSource(container, typed);
      read.release(Response.json({ error: 'refused' }, { status, statusText: 'Refused' }));
      await settle();
      expect(area(container).value).toBe(typed);
      expect(byTestId(container, 'library-read-error')).toBeNull();
      expect(byTestId(container, 'library-elsewhere')?.textContent).toBe(said);
      expect(byTestId(container, 'library-unsaved')?.textContent).toBe('UNSAVED');
      endTest();
    }
  });

  it('a clean editor follows a change made elsewhere, and closes when the source is deleted elsewhere', async () => {
    const backend = motors();
    const { container } = await opened(backend);
    backend.library = { ...backend.library, Motors: MOTORS2 };
    changed();
    await settle();
    expect(area(container).value).toBe(MOTORS2);
    expect(doc().saved).toBe(MOTORS2);
    expect(byTestId(container, 'library-elsewhere')).toBeNull();
    expect(byTestId(container, 'library-unsaved')).toBeNull();

    const { Motors: _gone, ...rest } = backend.library;
    backend.library = rest;
    changed({ what: 'deleted' });
    await settle();
    expect(editors()).toEqual([]);
    expect(useLibrary.getState().names).toEqual(['Valves']);
  });

  it("the echo of this editor's own save does not say changed elsewhere", async () => {
    const backend = motors();
    const { container } = await opened(backend);
    typeSource(container, MOTORS2);
    click(byTestId(container, 'library-save'));
    await settle();
    expect(byTestId(container, 'library-unsaved')).toBeNull();
    // Typed on while the event of that save comes: no warning of a change elsewhere.
    typeSource(container, `${MOTORS2}(* more *)\n`);
    changed();
    await settle();
    expect(byTestId(container, 'library-elsewhere')).toBeNull();
    expect(area(container).value).toBe(`${MOTORS2}(* more *)\n`);

    // A later save by somebody else is one: it is said.
    backend.library = { ...backend.library, Motors: MOTORS };
    changed();
    await settle();
    expect(byTestId(container, 'library-elsewhere')?.textContent).toBe(
      'Motors was changed elsewhere since it was opened; saving replaces that change.'
    );
  });

  it("the echo heard while this editor's save is unanswered is taken as its own", async () => {
    const write = held();
    const backend = motors({ intercept: (call) => (call.method === 'PUT' ? write.response : undefined) });
    const { container } = await opened(backend);
    typeSource(container, MOTORS2);
    click(byTestId(container, 'library-save'));
    await settle();
    changed();
    backend.library = { ...backend.library, Motors: MOTORS2 };
    write.release(Response.json({ name: 'Motors', created: false }));
    await settle();
    typeSource(container, `${MOTORS2}(* more *)\n`);
    // The event came before the answer: it was this save's, and none is left to match.
    changed();
    await settle();
    expect(byTestId(container, 'library-elsewhere')?.textContent).toBe(
      'Motors was changed elsewhere since it was opened; saving replaces that change.'
    );
  });

  it('an event of another project changes nothing', async () => {
    const backend = motors();
    const { container, calls } = await opened(backend);
    const before = calls.length;
    backend.library = { ...backend.library, Motors: MOTORS2 };
    changed({ project: 'other' });
    changed({ project: 'other', what: 'deleted' });
    await settle();
    expect(calls.length).toBe(before);
    expect(area(container).value).toBe(MOTORS);
    expect(editors()).toEqual(['logic-library:Motors']);
  });
});

describe("after the project's types or data blocks changed", () => {
  const typesChanged = (project = 'plant') => emit(DATA_CHANGED_EVENT, { project, what: 'type', name: 'Recipe', change: 'saved' });

  it('the check is said to be with earlier types, and nothing of it is drawn', async () => {
    const { container } = await opened(motors());
    typeSource(container, BROKEN);
    click(byTestId(container, 'library-check-button'));
    await settle();
    const editor = byTestId(container, 'library-editor') as HTMLElement;
    expect(byTestId(container, 'library-check')?.textContent).toBe('DOES NOT CHECK');
    expect(texts(editor, 'source-error-mark')).toEqual(['Nope']);

    typesChanged();
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('DOES NOT CHECK (EARLIER TYPES)');
    expect(byTestId(container, 'library-check')?.className).toBe('badge badge--neutral');
    expect(texts(editor, 'source-error-mark')).toEqual([]);
    expect(byTestId(panel(container), 'library-check-earlier-types')?.textContent).toBe(
      'This check was made with earlier types and data blocks of the project. They have changed since: check again.'
    );
    expect(useModuleUi.getState().diagnostics[LIBRARY_DIAGNOSTICS_OWNER]?.item.count).toBeNull();

    // Checked again: of the types as they are now.
    click(byTestId(container, 'library-check-button'));
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('DOES NOT CHECK');
    expect(byTestId(panel(container), 'library-check-earlier-types')).toBeNull();
    expect(texts(editor, 'source-error-mark')).toEqual(['Nope']);
  });

  it('a check asked before the change and answered after it is with earlier types', async () => {
    const late = held();
    const { container } = await opened(motors({ intercept: (call) => (ofSource(call) ? late.response : undefined) }));
    click(byTestId(container, 'library-check-button'));
    await settle();
    typesChanged();
    await settle();
    late.release(Response.json({ ok: true, diagnostics: [] }));
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS (EARLIER TYPES)');
  });

  it('a change of another project changes nothing', async () => {
    const { container } = await opened(motors());
    click(byTestId(container, 'library-check-button'));
    await settle();
    typesChanged('other');
    await settle();
    expect(byTestId(container, 'library-check')?.textContent).toBe('CHECKS');
  });
});
