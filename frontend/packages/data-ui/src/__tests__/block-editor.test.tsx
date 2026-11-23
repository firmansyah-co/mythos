import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { BlockEditor, deleteBlockQuestion, RUNTIMES_HEADING } from '../BlockEditor';
import { DATA_CHANGED_EVENT } from '../bridge';
import { DataTree } from '../DataTree';
import { BLOCK_EDITOR, TYPE_EDITOR, UNSAVED_BLOCKS_OWNER } from '../entry';
import { useData } from '../store';
import {
  allByTestId,
  byTestId,
  click,
  diagnostic,
  emit,
  endTest,
  layoutOf,
  project,
  render,
  resetStores,
  server,
  settle,
  texts,
  timeScale,
  useModuleUi,
  useSession,
  useShell,
  writes,
  type Backend,
  type Compiled,
} from './harness';

const RECIPE = 'TYPE Recipe :\nSTRUCT\n  speed : INT;\nEND_STRUCT;\nEND_TYPE\n';
const ALARM = 'TYPE Alarm :\nSTRUCT\n  on : BOOL;\nEND_STRUCT;\nEND_TYPE\n';
const KEYWORD = "data block 'IF': IF is a keyword of IEC 61131-3 and cannot be used as a name";
const NOT_CHECK = 'there is no type Nope';

vi.setConfig({ testTimeout: 20_000 * timeScale });

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
  vi.restoreAllMocks();
});

/** Each block laid out from its type's text; a block named IF refused as the compiler refuses it. */
function compiler(types: Record<string, string>, blocks: Backend['blocks']): Compiled {
  if (blocks.some((b) => b.name === 'IF')) return { ok: false, diagnostics: [diagnostic(KEYWORD, 0, 0)] };
  return {
    ok: true,
    layouts: Object.fromEntries(
      blocks.map((b) => [
        b.name,
        {
          layout: layoutOf(types[b.type] ?? ''),
          size: 4,
          members: [
            { name: 'SPEED', type: 'INT', offset: 0, size: 2 },
            { name: 'TEMP', type: 'INT', offset: 2, size: 2 },
          ],
        },
      ])
    ),
  };
}

/** A state that does not check: a type's file names a type that does not exist. */
const notChecking = (): Compiled => ({ ok: false, diagnostics: [diagnostic(NOT_CHECK, 3, 11, 'data/types/Recipe.st')] });

const plant = (over: Partial<Backend> = {}): Backend => ({
  types: { Alarm: ALARM, Recipe: RECIPE },
  blocks: [{ name: 'Line1', type: 'Recipe', retain: false }],
  compile: compiler,
  ...over,
});

/** The workbench has `plant` open, the tree drawn, and an editor on block `name` (made new with `fresh`). */
async function opened(backend: Backend, name = 'Line1', fresh = false) {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  await settle();
  const tree = render(
    <DataTree
      openType={(n) => useShell.getState().openEditor(TYPE_EDITOR, n)}
      openBlock={(n) => useShell.getState().openEditor(BLOCK_EDITOR, n)}
    />
  );
  if (fresh) useData.getState().openNewBlock(name);
  useShell.getState().openEditor(BLOCK_EDITOR, name);
  const removed: string[] = [];
  const container = render(<BlockEditor name={name} onRemoved={(gone) => removed.push(gone)} />);
  await settle();
  return { calls, tree, container, removed };
}

function choose(container: HTMLElement, type: string): void {
  const select = byTestId(container, 'block-type');
  if (!(select instanceof HTMLSelectElement)) throw new Error('no type field');
  act(() => {
    select.value = type;
    select.dispatchEvent(new Event('change', { bubbles: true }));
  });
}

const radio = (container: HTMLElement, which: 'yes' | 'no') => {
  const input = byTestId(container, `block-retain-${which}`);
  if (!(input instanceof HTMLInputElement)) throw new Error('no retain choice');
  return input;
};
const saveButton = (container: HTMLElement) => {
  const button = byTestId(container, 'block-save');
  if (!(button instanceof HTMLButtonElement)) throw new Error('no Save');
  return button;
};
const cells = (row: Element | null) => [...(row?.querySelectorAll('td') ?? [])].map((cell) => cell.textContent);

describe('what a block shows as saved', () => {
  it('a block of a state that does not check shows its layout as NOT KNOWN', async () => {
    const { container } = await opened(plant({ compile: notChecking }));
    expect(byTestId(container, 'block-layout')?.textContent).toBe('NOT KNOWN');
    expect(byTestId(container, 'block-size')?.textContent).toBe('NOT KNOWN');
    expect(byTestId(container, 'block-not-known')?.textContent).toBe(
      "Layout, size and members are NOT KNOWN: the project's types and data blocks do not check."
    );
    expect(byTestId(container, 'block-members-unknown')?.textContent).toBe(
      "Members: NOT KNOWN: the project's types and data blocks do not check."
    );
    expect(allByTestId(container, 'block-member')).toEqual([]);
    // Type and retain are answered whether or not the state checks.
    expect((byTestId(container, 'block-type') as HTMLSelectElement).value).toBe('Recipe');
    expect(radio(container, 'no').checked).toBe(true);
  });

  it('a block of a state that checks shows the layout, size and members the module answered', async () => {
    const { container } = await opened(plant());
    expect(byTestId(container, 'block-layout')?.textContent).toBe(layoutOf(RECIPE));
    expect(byTestId(container, 'block-size')?.textContent).toBe('4 bytes');
    expect(allByTestId(container, 'block-member').map(cells)).toEqual([
      ['SPEED', 'INT', '0', '2'],
      ['TEMP', 'INT', '2', '2'],
    ]);
    expect(byTestId(container, 'block-not-known')).toBeNull();
    expect(byTestId(container, 'block-members-unknown')).toBeNull();
    expect(byTestId(container, 'block-unsaved')).toBeNull();
  });
});

describe('saving a data block', () => {
  it("the block list after a save is the module's answer", async () => {
    const backend = plant();
    const { container, tree, calls } = await opened(backend);
    // Somebody else made a block, and no event of it has come yet.
    backend.blocks = [...backend.blocks, { name: 'Line2', type: 'Alarm', retain: true }];
    choose(container, 'Alarm');
    click(radio(container, 'yes'));
    expect(byTestId(container, 'block-unsaved')?.textContent).toBe('UNSAVED');
    expect(useModuleUi.getState().unsaved[UNSAVED_BLOCKS_OWNER]).toBe('org.mythos.data');
    click(saveButton(container));
    await settle();
    expect(writes(calls)).toEqual(['PUT /data/blocks/Line1']);
    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ type: 'Alarm', retain: true });
    // The list is the answer's: the other block too, and this one laid out by the module.
    expect(allByTestId(tree, 'tree-block').map((row) => row.querySelector('.tree__label')?.textContent)).toEqual(['Line1', 'Line2']);
    expect(texts(tree, 'tree-block-type')).toEqual(['Alarm', 'Alarm']);
    expect(byTestId(container, 'block-layout')?.textContent).toBe(layoutOf(ALARM));
    expect(byTestId(container, 'block-unsaved')).toBeNull();
    expect(useModuleUi.getState().unsaved[UNSAVED_BLOCKS_OWNER]).toBeUndefined();
    expect(byTestId(container, 'block-save-changed')?.textContent).toBe('Data blocks laid out anew or new: Line1.');
  });

  it("a refused block save lists the compiler's reasons", async () => {
    const { container, tree, calls } = await opened(plant(), 'IF', true);
    choose(container, 'Recipe');
    click(radio(container, 'no'));
    click(saveButton(container));
    await settle();
    expect(writes(calls)).toEqual(['PUT /data/blocks/IF']);
    const refused = byTestId(container, 'save-refused');
    expect(byTestId(refused!, 'block-refusal-error')?.textContent).toBe("data block 'IF' does not check and was not saved");
    expect(texts(refused as HTMLElement, 'block-refusal-diagnostic')).toEqual([`about the whole ${KEYWORD}`]);
    // Nothing is taken as saved: still new, still what was chosen, not listed.
    expect(byTestId(container, 'block-new')?.textContent).toBe('NOT SAVED YET');
    expect((byTestId(container, 'block-type') as HTMLSelectElement).value).toBe('Recipe');
    expect(radio(container, 'no').checked).toBe(true);
    expect(byTestId(container, 'block-layout')?.textContent).toBe('NOT KNOWN');
    expect(allByTestId(tree, 'tree-block').map((row) => row.querySelector('.tree__label')?.textContent)).toEqual(['Line1']);
  });

  it('a new block has no retain chosen, and cannot be saved until one is', async () => {
    const { container, calls } = await opened(plant(), 'Line9', true);
    expect(byTestId(container, 'block-new')?.textContent).toBe('NOT SAVED YET');
    expect((byTestId(container, 'block-type') as HTMLSelectElement).value).toBe('');
    choose(container, 'Recipe');
    expect(radio(container, 'yes').checked).toBe(false);
    expect(radio(container, 'no').checked).toBe(false);
    expect(byTestId(container, 'block-retain-none')?.textContent).toBe('Not chosen: the data module asks for it.');
    expect(saveButton(container).disabled).toBe(true);
    click(saveButton(container));
    await settle();
    expect(writes(calls)).toEqual([]);

    click(radio(container, 'yes'));
    expect(saveButton(container).disabled).toBe(false);
    click(saveButton(container));
    await settle();
    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ type: 'Recipe', retain: true });
    expect(byTestId(container, 'block-new')).toBeNull();
    expect(byTestId(container, 'block-save-answer')?.textContent).toBe(
      'Saved; the module made the block. Data blocks laid out anew or new: Line9.'
    );
  });
});

describe('the comparison with the runtimes configuration', () => {
  it('names each difference, says "laid out as saved" at most, and says configured', async () => {
    const layout = layoutOf(RECIPE);
    const { container } = await opened(
      plant({
        config: {
          instances: [
            // As the file has it: retain absent when false.
            { id: 'press', data: [{ name: 'Line1', type: 'Recipe', layout, size: 4, members: [] }] },
            { id: 'oven', data: [{ name: 'LINE1', type: 'Alarm', layout: 'ffffffffffffffff', size: 4, members: [], retain: true }] },
            { id: 'kiln', data: [{ name: 'Other', type: 'Recipe', layout, size: 4, members: [] }] },
          ],
        },
      })
    );
    expect(byTestId(container, 'runtimes-heading')?.textContent).toBe(RUNTIMES_HEADING);
    expect(RUNTIMES_HEADING).toBe("Configured in the project's runtimes");
    expect(byTestId(container, 'runtimes-note')?.textContent).toBe(
      'The configuration as the project saved it. Whether the controller runs this project and that configuration is not shown here.'
    );
    expect(cells(byTestId(container, 'runtime-row-press'))).toEqual(['press', 'Recipe', layout, 'not retained', 'laid out as saved']);
    expect(cells(byTestId(container, 'runtime-row-oven'))).toEqual([
      'oven',
      'Alarm',
      'ffffffffffffffff',
      'retained',
      `built with layout ffffffffffffffff, saved as ${layout}; of type Alarm there, Recipe here; retained there, not here`,
    ]);
    expect(byTestId(container, 'runtime-row-kiln')).toBeNull();
    expect(byTestId(container, 'runtimes-footnote')?.textContent).toBe(
      'An initial value changed in the type is not seen here: build the programs again to be sure.'
    );
    expect(byTestId(container, 'runtimes-none')).toBeNull();
  });

  it('when the saved state does not check, a row compares nothing and says the saved layout is NOT KNOWN', async () => {
    const { container } = await opened(
      plant({
        compile: notChecking,
        config: { instances: [{ id: 'press', data: [{ name: 'Line1', type: 'Alarm', layout: 'ffffffffffffffff', size: 4, members: [] }] }] },
      })
    );
    expect(byTestId(byTestId(container, 'runtime-row-press')!, 'runtime-verdict')?.textContent).toBe(
      "the saved layout is NOT KNOWN: the project's types and data blocks do not check, so nothing is compared"
    );
  });

  it('a configuration that cannot be read is said, not taken for "no runtime carries it"', async () => {
    const { container } = await opened(plant({ config: 'fails' }));
    expect(byTestId(container, 'runtimes-error')?.textContent).toBe(
      "The project's runtimes configuration could not be read: runtimes.json cannot be read: expected value at line 1 column 1. Which runtimes carry this block is NOT KNOWN."
    );
    expect(byTestId(container, 'runtimes-none')).toBeNull();
  });

  it('a configuration that is read and carries the block nowhere says so', async () => {
    const { container } = await opened(
      plant({ config: { instances: [{ id: 'press', data: [{ name: 'Other', type: 'Recipe', layout: 'x', size: 4, members: [] }] }, { id: 'oven' }] } })
    );
    expect(byTestId(container, 'runtimes-none')?.textContent).toBe('No runtime of this project carries this block.');
    expect(byTestId(container, 'runtimes-error')).toBeNull();
    expect(container.querySelector('[data-testid^="runtime-row-"]')).toBeNull();
  });
});

describe('who may change a data block', () => {
  it('a viewer sees it read-only', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { container, tree } = await opened(plant());
    expect(byTestId(container, 'block-save')).toBeNull();
    expect(byTestId(container, 'block-delete')).toBeNull();
    expect((byTestId(container, 'block-type') as HTMLSelectElement).disabled).toBe(true);
    expect((byTestId(container, 'block-retain') as HTMLFieldSetElement).disabled).toBe(true);
    expect(byTestId(container, 'block-read-only')?.textContent).toBe('Only an engineer changes data blocks.');
    expect(byTestId(tree, 'tree-add-block')).toBeNull();
  });
});

describe('deleting a data block', () => {
  it('asks first, naming what follows, and deletes nothing when declined', async () => {
    const asked: string[] = [];
    const confirm = vi.spyOn(window, 'confirm').mockImplementation((text) => {
      asked.push(String(text));
      return false;
    });
    const { container, calls, removed, tree } = await opened(plant());
    click(byTestId(container, 'block-delete'));
    await settle();
    expect(asked).toEqual([deleteBlockQuestion('Line1')]);
    expect(asked[0]).toBe('Delete data block Line1? A program that declares it no longer compiles; nothing is rebuilt.');
    expect(writes(calls)).toEqual([]);
    expect(removed).toEqual([]);

    confirm.mockReturnValue(true);
    click(byTestId(container, 'block-delete'));
    await settle();
    expect(writes(calls)).toEqual(['DELETE /data/blocks/Line1']);
    expect(removed).toEqual(['Line1']);
    expect(byTestId(tree, 'tree-blocks-empty')?.textContent).toBe('No data blocks yet.');
  });
});

describe('following changes made elsewhere', () => {
  const changed = (over: Record<string, unknown> = {}) =>
    emit(DATA_CHANGED_EVENT, { project: 'plant', what: 'block', name: 'Line1', change: 'saved', ...over });

  it('an editor without changes shows the block as read again, and closes when it was deleted', async () => {
    const backend = plant();
    const { container } = await opened(backend);
    backend.blocks = [{ name: 'Line1', type: 'Alarm', retain: true }];
    changed();
    await settle();
    expect((byTestId(container, 'block-type') as HTMLSelectElement).value).toBe('Alarm');
    expect(radio(container, 'yes').checked).toBe(true);
    expect(byTestId(container, 'block-layout')?.textContent).toBe(layoutOf(ALARM));
    expect(byTestId(container, 'block-elsewhere')).toBeNull();

    backend.blocks = [];
    changed({ change: 'deleted' });
    await settle();
    expect(useShell.getState().editors.map((e) => e.target)).toEqual([]);
  });

  it('an editor with changes keeps them and says the block changed elsewhere', async () => {
    const backend = plant();
    const { container } = await opened(backend);
    click(radio(container, 'yes'));
    backend.blocks = [{ name: 'Line1', type: 'Alarm', retain: false }];
    changed();
    await settle();
    expect(radio(container, 'yes').checked).toBe(true);
    expect(byTestId(container, 'block-elsewhere')?.textContent).toBe(
      'Line1 was changed elsewhere since it was opened; saving replaces that change.'
    );
    expect(byTestId(container, 'block-unsaved')?.textContent).toBe('UNSAVED');

    backend.blocks = [];
    changed({ change: 'deleted' });
    await settle();
    expect(byTestId(container, 'block-elsewhere')?.textContent).toBe(
      'Line1 was deleted elsewhere since it was opened; to make it again, choose its type and retain and save.'
    );
    expect(useShell.getState().editors.map((e) => e.target)).toEqual(['Line1']);
  });

  it('an event of another project is ignored', async () => {
    const backend = plant();
    const { container, calls } = await opened(backend);
    const before = calls.length;
    backend.blocks = [];
    changed({ project: 'other', change: 'deleted' });
    await settle();
    expect(calls.length).toBe(before);
    expect(useShell.getState().editors.map((e) => e.target)).toEqual(['Line1']);
    expect(byTestId(container, 'block-editor')).not.toBeNull();
  });
});
