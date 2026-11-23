import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DataTree } from '../DataTree';
import { TYPE_EDITOR } from '../entry';
import { TypeEditor } from '../TypeEditor';
import {
  allByTestId,
  byTestId,
  click,
  endTest,
  press,
  project,
  render,
  resetStores,
  server,
  settle,
  texts,
  timeScale,
  typeInput,
  useSession,
  useShell,
  writes,
  type Backend,
} from './harness';

const RECIPE = 'TYPE Recipe :\nSTRUCT\n  speed : INT;\nEND_STRUCT;\nEND_TYPE\n';

vi.setConfig({ testTimeout: 20_000 * timeScale });

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
});

/** The workbench has `plant` open and the data UI has read it; the tree drawn. */
async function opened(backend: Backend) {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  await settle();
  const tree = render(<DataTree openType={(name) => useShell.getState().openEditor(TYPE_EDITOR, name)} />);
  await settle();
  return { calls, tree };
}

const failing = (route: RegExp, status: number, message: string): Backend['intercept'] => (call) =>
  call.method === 'GET' && route.test(call.path) ? Response.json({ error: message }, { status, statusText: 'Internal Server Error' }) : undefined;

describe('the Data section of the project tree', () => {
  it('lists the user types and data blocks the module answered, under Data', async () => {
    const { tree } = await opened({
      types: { Recipe: RECIPE, Alarm: 'TYPE Alarm : STRUCT on : BOOL; END_STRUCT; END_TYPE' },
      blocks: [{ name: 'Line1', type: 'Recipe', retain: true }],
    });
    expect(byTestId(tree, 'tree-data')?.textContent).toBe('Data');
    expect(byTestId(tree, 'tree-types')?.textContent).toBe('User types');
    expect(byTestId(tree, 'tree-blocks')?.textContent).toBe('Data blocks');
    // In the module's order (byte order of the file names), as answered.
    expect(texts(tree, 'tree-type')).toEqual(['Alarm', 'Recipe']);
    expect(allByTestId(tree, 'tree-block').map((row) => row.querySelector('.tree__label')?.textContent)).toEqual(['Line1']);
    expect(texts(tree, 'tree-block-type')).toEqual(['Recipe']);
    expect(byTestId(tree, 'tree-types-empty')).toBeNull();
    expect(byTestId(tree, 'tree-data-error')).toBeNull();
  });

  it('says "no user types yet" only after the module answered an empty list', async () => {
    const { tree } = await opened({ types: {}, blocks: [] });
    expect(byTestId(tree, 'tree-types-empty')?.textContent).toBe('No user types yet.');
    expect(byTestId(tree, 'tree-blocks-empty')?.textContent).toBe('No data blocks yet.');
    expect(allByTestId(tree, 'tree-type')).toEqual([]);
  });

  it('a list that cannot be read says so and lists nothing', async () => {
    const why = '/projects/plant/data/blocks.json: expected value at line 1 column 1';
    const { tree } = await opened({
      types: { Recipe: RECIPE },
      blocks: [],
      intercept: failing(/\/data\/types$/, 500, why),
    });
    const said = tree.querySelector('[data-testid="tree-data-error"][data-list="types"]');
    expect(said?.textContent).toBe(`The user types could not be read: ${why}`);
    // Not an empty list, and nothing can be added to a list that was not read.
    expect(byTestId(tree, 'tree-types-empty')).toBeNull();
    expect(allByTestId(tree, 'tree-type')).toEqual([]);
    expect(byTestId(tree, 'tree-add-type')).toBeNull();
    // The blocks were read: they are listed as answered.
    expect(byTestId(tree, 'tree-blocks-empty')?.textContent).toBe('No data blocks yet.');
  });

  it('a block list that cannot be read is said under Data blocks, not shown as none', async () => {
    const { tree } = await opened({
      types: { Recipe: RECIPE },
      blocks: [{ name: 'Line1', type: 'Recipe', retain: false }],
      intercept: failing(/\/data\/blocks$/, 500, 'disk unreadable'),
    });
    expect(tree.querySelector('[data-testid="tree-data-error"][data-list="blocks"]')?.textContent).toBe(
      'The data blocks could not be read: disk unreadable'
    );
    expect(byTestId(tree, 'tree-blocks-empty')).toBeNull();
    expect(allByTestId(tree, 'tree-block')).toEqual([]);
    expect(texts(tree, 'tree-type')).toEqual(['Recipe']);
  });

  it('a new type is not listed until a save answered 200, and a name the module refuses is refused in its words', async () => {
    const backend: Backend = { types: {}, blocks: [] };
    const { calls, tree } = await opened(backend);

    click(byTestId(tree, 'tree-add-type'));
    typeInput(tree.querySelector('input[aria-label="Type name"]'), 'Recipe');
    press(tree.querySelector('input[aria-label="Type name"]'), 'Enter');
    await settle();
    // An editor opened on the name, and nothing asked of the module: the type does not exist yet.
    expect(useShell.getState().editors.map((e) => `${e.kind}:${e.target}`)).toEqual([`${TYPE_EDITOR}:Recipe`]);
    expect(writes(calls)).toEqual([]);
    expect(allByTestId(tree, 'tree-type')).toEqual([]);
    expect(byTestId(tree, 'tree-types-empty')?.textContent).toBe('No user types yet.');

    const editor = render(<TypeEditor name="Recipe" onRemoved={() => undefined} />);
    await settle();
    expect(byTestId(editor, 'type-new')?.textContent).toBe('NOT SAVED YET');
    // Asked of the module only as a draft would be, never read as stored.
    expect(calls.filter((c) => c.path.endsWith('/data/types/Recipe'))).toEqual([]);
    expect(byTestId(tree, 'tree-data')?.querySelector('.tree__mark--warn')?.textContent?.trim()).toBe('*');

    click(byTestId(editor, 'type-save'));
    await settle();
    expect(writes(calls)).toEqual(['PUT /data/types/Recipe']);
    expect(texts(tree, 'tree-type')).toEqual(['Recipe']);
    expect(byTestId(editor, 'type-new')).toBeNull();

    // A name the module refuses: its own words, and nothing listed.
    click(byTestId(tree, 'tree-add-type'));
    typeInput(tree.querySelector('input[aria-label="Type name"]'), '1x');
    press(tree.querySelector('input[aria-label="Type name"]'), 'Enter');
    await settle();
    const refused = render(<TypeEditor name="1x" onRemoved={() => undefined} />);
    await settle();
    click(byTestId(refused, 'type-save'));
    await settle();
    expect(byTestId(refused, 'save-refused')?.textContent).toBe(
      "'1x' is not a type name: a letter or '_' first, then letters, digits and single underscores, not ending in '_', at most 64"
    );
    expect(byTestId(refused, 'type-new')?.textContent).toBe('NOT SAVED YET');
    expect(texts(tree, 'tree-type')).toEqual(['Recipe']);
  });

  it('offers a viewer no row to add a type', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { tree } = await opened({ types: { Recipe: RECIPE }, blocks: [] });
    expect(texts(tree, 'tree-type')).toEqual(['Recipe']);
    expect(byTestId(tree, 'tree-add-type')).toBeNull();
  });

  it('offers an engineer the row to add a type', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'erin', display_name: 'Erin', role: 'engineer', via: 'session' } });
    const { tree } = await opened({ types: { Recipe: RECIPE }, blocks: [] });
    expect(byTestId(tree, 'tree-add-type')?.textContent).toBe('Add new type');
  });
});
