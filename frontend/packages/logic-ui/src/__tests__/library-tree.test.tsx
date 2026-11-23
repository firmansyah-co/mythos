// The "Library" section of the project tree (D4c, R1–R3, R11): the sources as the logic module
// listed them, a list that could not be read said as such, and a row to add one that refuses no
// name and writes nothing. Every assertion reads a rendered element by its test id.

import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { activate as activateData } from '../../../data-ui/src/entry';
import { createHost } from '../../../ui-shell/src/modules/host';
import { orderedTreeSections } from '../../../ui-shell/src/modules/registry';
import type { Diagnostic } from '../bridge';
import { DATA_CHANGED_EVENT, LIBRARY_CHANGED_EVENT, LIBRARY_EDITOR, type LibraryCheck } from '../library';
import { useLibrary } from '../librarystore';
import { LibraryTree } from '../LibraryTree';
import {
  byLabel,
  byTestId,
  click,
  emit,
  endTest,
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
});

const MOTORS = 'FUNCTION_BLOCK Motor\nVAR_INPUT run : BOOL; END_VAR\nEND_FUNCTION_BLOCK\n';

/** The workbench has `plant` open and the logic UI has read it; the Library section drawn. */
async function opened(backend: Backend) {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  await settle();
  const tree = render(<LibraryTree openEditor={(name) => useShell.getState().openEditor(LIBRARY_EDITOR, name)} />);
  await settle();
  return { calls, tree };
}

/**
 * Every request that is not a read: of the list, of a source, or the check of the stored library
 * (`POST /library/check {}`, which writes nothing).
 */
const notReads = (calls: Call[]) =>
  calls
    .filter((c) => c.method !== 'GET' && !(c.path.endsWith('/library/check') && JSON.stringify(c.body) === '{}'))
    .map((c) => `${c.method} ${c.path}`);

function press(element: Element | null, key: string): void {
  if (!(element instanceof HTMLElement)) throw new Error('nothing to press on');
  act(() => {
    element.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
  });
}

/** Type `name` into the Add row's field and press Enter. */
function add(tree: HTMLElement, name: string): void {
  click(byTestId(tree, 'tree-add-library'));
  const field = byLabel(tree, 'Library source name');
  typeInto(field, name);
  press(field, 'Enter');
}

const editors = () => useShell.getState().editors.map((e) => `${e.kind}:${e.target ?? ''}`);

describe('the Library section of the project tree', () => {
  it('lists the sources the module answered, in its order', async () => {
    const { tree } = await opened({ programs: [], library: { Valves: 'x', Motors: MOTORS, A_first: 'y' } });
    expect(byTestId(tree, 'tree-library')?.textContent).toBe('Library');
    expect(byTestId(tree, 'tree-library')?.getAttribute('aria-expanded')).toBe('true');
    // Byte order of the file names, as the module answers it: upper case before `_`, before lower.
    expect(texts(tree, 'tree-library-source')).toEqual(['A_firstST', 'MotorsST', 'ValvesST']);
    expect([...tree.querySelectorAll('[data-testid="tree-library-source"] .tree__label')].map((l) => l.textContent)).toEqual([
      'A_first',
      'Motors',
      'Valves',
    ]);
    expect(texts(tree, 'tree-library-language')).toEqual(['ST', 'ST', 'ST']);
    expect(byTestId(tree, 'tree-library-empty')).toBeNull();
    expect(byTestId(tree, 'tree-library-error')).toBeNull();
    expect(byTestId(tree, 'tree-library-reading')).toBeNull();

    // Opened with a double-click and with Enter, on the editor of its kind, by the stored name.
    const motors = tree.querySelectorAll('[data-testid="tree-library-source"]')[1] ?? null;
    act(() => {
      motors?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    });
    expect(editors()).toEqual(['logic-library:Motors']);
    press(tree.querySelectorAll('[data-testid="tree-library-source"]')[2] ?? null, 'Enter');
    expect(editors()).toEqual(['logic-library:Motors', 'logic-library:Valves']);
  });

  it('a list that cannot be read says so and lists nothing', async () => {
    const words = '/srv/mythos/projects/plant/logic/library: Permission denied (os error 13)';
    const { tree } = await opened({
      programs: [],
      library: { Motors: MOTORS },
      intercept: (call) =>
        call.method === 'GET' && call.path.endsWith('/logic/library')
          ? Response.json({ error: words }, { status: 500, statusText: 'Internal Server Error' })
          : undefined,
    });
    expect(byTestId(tree, 'tree-library-error')?.textContent).toBe(`The library could not be read: ${words}`);
    expect(byTestId(tree, 'tree-library-source')).toBeNull();
    expect(byTestId(tree, 'tree-library-empty')).toBeNull();
    // Nothing to add to a list that is not known.
    expect(byTestId(tree, 'tree-add-library')).toBeNull();
    expect(useLibrary.getState().names).toBeNull();
  });

  it('an empty library says so only after the module answered none', async () => {
    let answer: (response: Response) => void = () => undefined;
    const { tree } = await opened({
      programs: [],
      intercept: (call) =>
        call.method === 'GET' && call.path.endsWith('/logic/library')
          ? new Promise<Response>((resolve) => {
              answer = resolve;
            })
          : undefined,
    });
    // Asked, not answered: reading, and not "none".
    expect(byTestId(tree, 'tree-library-reading')?.textContent).toBe('Reading the library…');
    expect(byTestId(tree, 'tree-library-empty')).toBeNull();
    expect(byTestId(tree, 'tree-add-library')).toBeNull();
    answer(Response.json({ sources: [] }));
    await settle();
    expect(byTestId(tree, 'tree-library-empty')?.textContent).toBe('No library sources yet.');
    expect(byTestId(tree, 'tree-library-reading')).toBeNull();
    expect(byTestId(tree, 'tree-library-source')).toBeNull();
  });

  it('a new name opens an editor marked not saved and writes nothing', async () => {
    const { tree, calls } = await opened({ programs: [], library: { Motors: MOTORS } });
    // No name is refused here, not even one the module will refuse: it says so itself.
    add(tree, 'a-b');
    await settle();
    expect(editors()).toEqual(['logic-library:a-b']);
    expect(byTestId(tree, 'tree-library-new')?.querySelector('.tree__label')?.textContent).toBe('a-b (not saved yet)');
    expect(byTestId(tree, 'tree-library-new')?.querySelector('.tree__mark--warn')).not.toBeNull();
    const doc = useLibrary.getState().docs['a-b'];
    expect(doc?.isNew).toBe(true);
    // An empty text: nothing the user did not type.
    expect(doc?.draft).toBe('');
    expect(notReads(calls)).toEqual([]);
    // Still unsaved: the Library node is marked, and the workbench is told.
    expect(byTestId(tree, 'tree-library')?.querySelector('.tree__mark--warn')).not.toBeNull();
    expect(useModuleUi.getState().unsaved['logic-library']).toBe('org.mythos.logic');
  });

  it('a name the library has in another case opens the stored source', async () => {
    const { tree, calls } = await opened({ programs: [], library: { Motors: MOTORS } });
    add(tree, 'motors');
    await settle();
    expect(editors()).toEqual(['logic-library:Motors']);
    expect(useLibrary.getState().docs.motors).toBeUndefined();
    expect(byTestId(tree, 'tree-library-new')).toBeNull();
    expect(notReads(calls)).toEqual([]);
  });

  it('a viewer gets no Add row', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { tree } = await opened({ programs: [], library: { Motors: MOTORS } });
    expect(texts(tree, 'tree-library-source')).toEqual(['MotorsST']);
    expect(byTestId(tree, 'tree-add-library')).toBeNull();
  });

  it('an engineer gets the Add row', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'enzo', display_name: 'Enzo', role: 'engineer', via: 'session' } });
    const { tree } = await opened({ programs: [], library: {} });
    expect(byTestId(tree, 'tree-add-library')?.textContent).toBe('Add new source');
  });

  it('the section sits between Programs and Data', () => {
    // The data module's UI, as the workbench activates it beside this one (no project open: it reads nothing).
    const deactivateData = activateData(createHost('org.mythos.data'));
    try {
      const ids = orderedTreeSections(useModuleUi.getState().treeSections).map((s) => s.item.id);
      expect(ids).toEqual(['logic-programs', 'logic-library', 'data']);
      expect(useModuleUi.getState().treeSections.find((s) => s.item.id === 'logic-library')?.item.order).toBe(25);
    } finally {
      deactivateData();
    }
  });
});

describe('the state of the whole library (R19)', () => {
  const NOPE: Diagnostic = { line: 2, col: 17, end_line: 2, end_col: 21, message: 'Nope is not a type', source: 'logic/library/Motors.st' };
  const TWICE: Diagnostic = { line: 1, col: 16, end_line: 1, end_col: 21, message: 'Motor is declared twice', source: 'logic/library/Valves.st' };
  const fails = (): LibraryCheck => ({ ok: false, diagnostics: [NOPE, TWICE] });
  const checks = (): LibraryCheck => ({ ok: true, diagnostics: [] });
  /** The checks of the stored library sent: `POST /library/check {}`. */
  const storedChecks = (calls: Call[]) =>
    calls.filter((c) => c.method === 'POST' && c.path === '/api/v1/projects/plant/logic/library/check' && JSON.stringify(c.body) === '{}');
  const state = (tree: HTMLElement) => byTestId(tree, 'tree-library-state');

  /** A response held until the test lets it go. */
  function held(): { response: Promise<Response>; release: (response: Response) => void } {
    let release: (response: Response) => void = () => undefined;
    const response = new Promise<Response>((resolve) => {
      release = resolve;
    });
    return { response, release };
  }
  /** Hold every check of the stored library asked after this, in order. */
  function holdStored(backend: Backend): Array<ReturnType<typeof held>> {
    const holds: Array<ReturnType<typeof held>> = [];
    backend.intercept = (call) => {
      if (call.method !== 'POST' || JSON.stringify(call.body) !== '{}') return undefined;
      const hold = held();
      holds.push(hold);
      return hold.response;
    };
    return holds;
  }

  it('a library that does not check says so on the Library node, with the count the module answered', async () => {
    const { tree, calls } = await opened({ programs: [], library: { Motors: MOTORS }, compileLibrary: fails });
    expect(storedChecks(calls)).toHaveLength(1);
    expect(state(tree)?.textContent).toBe('DOES NOT CHECK · 2 PROBLEMS');
    expect(state(tree)?.getAttribute('title')).toBe('Every program of the project is refused while the library does not check.');
    expect(state(tree)?.className).toBe('badge badge--bad');
    // On the Library node itself.
    expect(byTestId(tree, 'tree-library')?.querySelector('[data-testid="tree-library-state"]')).toBe(state(tree));
  });

  it('a library that checks carries no mark: nothing is claimed about programs', async () => {
    const { tree, calls } = await opened({ programs: [], library: { Motors: MOTORS }, compileLibrary: checks });
    expect(storedChecks(calls)).toHaveLength(1);
    expect(state(tree)).toBeNull();
    expect(byTestId(tree, 'tree-library')?.textContent).toBe('Library');
  });

  it('a check that could not be made is NOT CHECKED with the module\'s words, never shown as checking', async () => {
    const words = '/projects/plant/data/blocks.json: Permission denied (os error 13)';
    const { tree } = await opened({
      programs: [],
      library: { Motors: MOTORS },
      compileLibrary: fails,
      intercept: (call) =>
        call.method === 'POST' && JSON.stringify(call.body) === '{}'
          ? Response.json({ error: words }, { status: 500, statusText: 'Internal Server Error' })
          : undefined,
    });
    expect(state(tree)?.textContent).toBe('NOT CHECKED');
    expect(state(tree)?.getAttribute('title')).toBe(`The library could not be checked: ${words}`);
    expect(state(tree)?.className).toBe('badge badge--neutral');
  });

  it('nothing is shown while no answer is in, and an answer from before a change is not shown after it', async () => {
    const backend: Backend = { programs: [], library: { Motors: MOTORS }, compileLibrary: fails };
    const holds = holdStored(backend);
    const { tree } = await opened(backend);
    expect(holds).toHaveLength(1);
    expect(state(tree)).toBeNull();
    holds[0]?.release(Response.json(fails()));
    await settle();
    expect(state(tree)?.textContent).toBe('DOES NOT CHECK · 2 PROBLEMS');

    // Changed: the library is checked again, and until it answers nothing is said of it.
    emit(LIBRARY_CHANGED_EVENT, { project: 'plant', name: 'Motors', what: 'saved' });
    await settle();
    expect(holds).toHaveLength(2);
    expect(state(tree)).toBeNull();
    holds[1]?.release(Response.json(checks()));
    await settle();
    expect(state(tree)).toBeNull();

    // The project's types changed: the same.
    emit(DATA_CHANGED_EVENT, { project: 'plant', what: 'type', name: 'Recipe', change: 'saved' });
    await settle();
    expect(holds).toHaveLength(3);
    holds[2]?.release(Response.json({ ok: false, diagnostics: [NOPE] }));
    await settle();
    expect(state(tree)?.textContent).toBe('DOES NOT CHECK · 1 PROBLEM');
  });

  it('an older answer never replaces a newer one', async () => {
    const backend: Backend = { programs: [], library: { Motors: MOTORS }, compileLibrary: fails };
    const holds = holdStored(backend);
    const { tree } = await opened(backend);
    emit(LIBRARY_CHANGED_EVENT, { project: 'plant', name: 'Motors', what: 'saved' });
    await settle();
    expect(holds).toHaveLength(2);
    holds[1]?.release(Response.json(checks()));
    await settle();
    holds[0]?.release(Response.json(fails()));
    await settle();
    expect(state(tree)).toBeNull();
    expect(useLibrary.getState().stored?.answer?.ok).toBe(true);
  });

  it('an event of another project changes nothing', async () => {
    const { tree, calls } = await opened({ programs: [], library: { Motors: MOTORS }, compileLibrary: fails });
    const before = calls.length;
    emit(LIBRARY_CHANGED_EVENT, { project: 'other', name: 'Motors', what: 'saved' });
    emit(DATA_CHANGED_EVENT, { project: 'other', what: 'type', name: 'Recipe', change: 'saved' });
    await settle();
    expect(calls.length).toBe(before);
    expect(state(tree)?.textContent).toBe('DOES NOT CHECK · 2 PROBLEMS');
  });
});
