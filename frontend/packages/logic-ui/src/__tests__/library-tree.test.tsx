// The "Library" section of the project tree (D4c, R1–R3, R11): the sources as the logic module
// listed them, a list that could not be read said as such, and a row to add one that refuses no
// name and writes nothing. Every assertion reads a rendered element by its test id.

import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { activate as activateData } from '../../../data-ui/src/entry';
import { createHost } from '../../../ui-shell/src/modules/host';
import { orderedTreeSections } from '../../../ui-shell/src/modules/registry';
import { LIBRARY_EDITOR } from '../library';
import { useLibrary } from '../librarystore';
import { LibraryTree } from '../LibraryTree';
import {
  byLabel,
  byTestId,
  click,
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

/** Every request that is not a read of the list or a source. */
const notReads = (calls: Call[]) => calls.filter((c) => c.method !== 'GET').map((c) => `${c.method} ${c.path}`);

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
