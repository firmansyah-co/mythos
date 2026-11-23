// The "Runtimes" section of the project tree (C8d-1, R1–R3, R11): the configured runtimes as the
// logic module listed them, a list that could not be read said as such, a hand edit's problems
// counted, and a row to add one that refuses no id and writes nothing. Every assertion reads a
// rendered element by its test id, or the requests sent.

import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { orderedTreeSections } from '../../../ui-shell/src/modules/registry';
import { RUNTIME_EDITOR } from '../runtimes';
import { useRuntimes } from '../runtimestore';
import { RuntimesTree } from '../RuntimesTree';
import {
  byLabel,
  byTestId,
  click,
  endTest,
  entryOf,
  project,
  render,
  resetStores,
  server,
  settle,
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

const programs = () => [
  { entry: entryOf('Tank'), source: 'PROGRAM Tank\nEND_PROGRAM\n' },
  { entry: entryOf('Pump'), source: 'PROGRAM Pump\nEND_PROGRAM\n' },
];

/** The workbench has `plant` open and the logic UI has read it; the Runtimes section drawn. */
async function opened(backend: Backend) {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  await settle();
  const tree = render(<RuntimesTree openEditor={(id) => useShell.getState().openEditor(RUNTIME_EDITOR, id)} />);
  await settle();
  return { calls, tree };
}

/** Every request to the runtimes that is not a read (the library's check of the stored library is the library's). */
const notReads = (calls: Call[]) =>
  calls.filter((c) => c.method !== 'GET' && c.path.includes('/logic/runtimes')).map((c) => `${c.method} ${c.path}`);

function press(element: Element | null, key: string): void {
  if (!(element instanceof HTMLElement)) throw new Error('nothing to press on');
  act(() => {
    element.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
  });
}

/** Type `id` into the Add row's field and press Enter. */
function add(tree: HTMLElement, id: string): void {
  click(byTestId(tree, 'tree-add-runtime'));
  const field = byLabel(tree, 'Runtime id');
  typeInto(field, id);
  press(field, 'Enter');
}

const editors = () => useShell.getState().editors.map((e) => `${e.kind}:${e.target ?? ''}`);
const labels = (tree: HTMLElement) =>
  [...tree.querySelectorAll('[data-testid="tree-runtime"] .tree__label')].map((l) => l.textContent);

describe('the Runtimes section of the project tree', () => {
  it("lists the configured runtimes in the module's order", async () => {
    const { tree, calls } = await opened({
      programs: programs(),
      configured: [
        { id: 'Zeta', programs: ['Tank'] },
        { id: 'Alpha', programs: ['Pump'] },
      ],
    });
    expect(byTestId(tree, 'tree-runtimes')?.textContent).toBe('Runtimes');
    // The file's order, not sorted.
    expect(labels(tree)).toEqual(['Zeta', 'Alpha']);
    expect(byTestId(tree, 'tree-runtimes-empty')).toBeNull();
    expect(byTestId(tree, 'tree-runtimes-error')).toBeNull();
    expect(byTestId(tree, 'tree-runtimes-problems')).toBeNull();
    expect(calls.map((c) => `${c.method} ${c.path}`)).toContain('GET /api/v1/projects/plant/logic/runtimes');

    // Opened on the editor of its kind, by its id.
    act(() => {
      tree.querySelectorAll('[data-testid="tree-runtime"]')[1]?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    });
    expect(editors()).toEqual(['logic-runtime:Alpha']);
    expect(useModuleUi.getState().editors.find((e) => e.item.kind === RUNTIME_EDITOR)?.item.title('Alpha')).toEqual(['Runtimes', 'Alpha']);
  });

  it('a program running alone is not listed as a configured runtime', async () => {
    const { tree } = await opened({ programs: programs(), configured: [{ id: 'Line', programs: ['Tank'] }] });
    // Pump runs alone (the module lists it as implied): it is under Programs, not here.
    expect(useRuntimes.getState().list?.implied.map((r) => r.id)).toEqual(['Pump']);
    expect(labels(tree)).toEqual(['Line']);
  });

  it('a list that cannot be read says so and lists nothing', async () => {
    const { tree } = await opened({ programs: programs(), configured: 'fails' });
    expect(byTestId(tree, 'tree-runtimes-error')?.textContent).toBe(
      "The runtimes could not be read: logic/tasks.json cannot be read as the project's runtimes: expected value at line 1 column 1"
    );
    expect(byTestId(tree, 'tree-runtime')).toBeNull();
    // Not read is not "none".
    expect(byTestId(tree, 'tree-runtimes-empty')).toBeNull();
    // Nothing to add to a list that is not known.
    expect(byTestId(tree, 'tree-add-runtime')).toBeNull();
    expect(useRuntimes.getState().list).toBeNull();
  });

  it('no runtime is said only after the module answered none', async () => {
    let answer: (response: Response) => void = () => undefined;
    const { tree } = await opened({
      programs: programs(),
      intercept: (call) =>
        call.method === 'GET' && call.path.endsWith('/logic/runtimes')
          ? new Promise<Response>((resolve) => {
              answer = resolve;
            })
          : undefined,
    });
    // Asked, not answered: reading, and not "none".
    expect(byTestId(tree, 'tree-runtimes-reading')?.textContent).toBe('Reading the runtimes…');
    expect(byTestId(tree, 'tree-runtimes-empty')).toBeNull();
    expect(byTestId(tree, 'tree-add-runtime')).toBeNull();
    answer(Response.json({ runtimes: [], implied: [], problems: [] }));
    await settle();
    expect(byTestId(tree, 'tree-runtimes-empty')?.textContent).toBe('No runtime is configured: every program runs alone.');
    expect(byTestId(tree, 'tree-runtimes-reading')).toBeNull();
    expect(byTestId(tree, 'tree-runtime')).toBeNull();
  });

  it("a hand-edited file's problems are counted on the node", async () => {
    const problems = [
      "the project already has a runtime 'Line'; names that differ only in case are one name",
      "program 'Tank' is in runtime 'Line' already; a program is in one runtime",
    ];
    const { tree } = await opened({
      programs: programs(),
      configured: [
        { id: 'Line', programs: ['Tank'] },
        { id: 'line', programs: ['Tank'] },
      ],
      runtimeProblems: problems,
    });
    const badge = byTestId(tree, 'tree-runtimes-problems');
    expect(badge?.textContent).toBe('2 PROBLEMS');
    expect(badge?.getAttribute('title')).toBe(problems.join('\n'));
    // On the Runtimes node itself, and the runtimes are still listed as the file holds them.
    expect(byTestId(tree, 'tree-runtimes')?.querySelector('[data-testid="tree-runtimes-problems"]')).toBe(badge);
    expect(labels(tree)).toEqual(['Line', 'line']);
  });

  it('a new id opens an editor marked not saved and writes nothing', async () => {
    const { tree, calls } = await opened({ programs: programs(), configured: [{ id: 'Line', programs: ['Tank'] }] });
    // No id is refused here, not even one the module will refuse: it says so itself.
    add(tree, 'a b');
    await settle();
    expect(editors()).toEqual(['logic-runtime:a b']);
    expect(byTestId(tree, 'tree-runtime-new')?.querySelector('.tree__label')?.textContent).toBe('a b (not saved yet)');
    const doc = useRuntimes.getState().docs['a b'];
    expect(doc?.isNew).toBe(true);
    // Nothing chosen that was not chosen.
    expect(doc?.draft).toEqual({ id: 'a b', module: '', target: '', run: null, tasks: [] });
    expect(notReads(calls)).toEqual([]);
    // Unsaved: the node is marked, and the workbench is told.
    expect(byTestId(tree, 'tree-runtimes')?.querySelector('.tree__mark--warn')).not.toBeNull();
    expect(useModuleUi.getState().unsaved['logic-runtimes']).toBe('org.mythos.logic');
  });

  it('an id held in another case opens the stored runtime', async () => {
    const { tree, calls } = await opened({ programs: programs(), configured: [{ id: 'Line', programs: ['Tank'] }] });
    add(tree, 'LINE');
    await settle();
    expect(editors()).toEqual(['logic-runtime:Line']);
    expect(useRuntimes.getState().docs.LINE).toBeUndefined();
    expect(byTestId(tree, 'tree-runtime-new')).toBeNull();
    expect(notReads(calls)).toEqual([]);
  });

  it('a viewer gets no Add row', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { tree } = await opened({ programs: programs(), configured: [{ id: 'Line', programs: ['Tank'] }] });
    expect(labels(tree)).toEqual(['Line']);
    expect(byTestId(tree, 'tree-add-runtime')).toBeNull();

    endTest();
    resetStores();
    useSession.setState({ phase: 'signed-in', user: { username: 'enzo', display_name: 'Enzo', role: 'engineer', via: 'session' } });
    const engineer = await opened({ programs: programs(), configured: [] });
    expect(byTestId(engineer.tree, 'tree-add-runtime')?.textContent).toBe('Add new runtime');
  });

  it('the section sits between Programs and Library', () => {
    const sections = orderedTreeSections(useModuleUi.getState().treeSections).map((s) => s.item.id);
    expect(sections).toEqual(['logic-programs', 'logic-runtimes', 'logic-library']);
    expect(useModuleUi.getState().treeSections.find((s) => s.item.id === 'logic-runtimes')?.item.order).toBe(22);
  });
});
