import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DATA_CHANGED_EVENT } from '../bridge';
import { TYPE_EDITOR, UNSAVED_OWNER } from '../entry';
import { useData } from '../store';
import { DIAGNOSTICS_OWNER, deleteQuestion, TypeEditor } from '../TypeEditor';
import {
  allByTestId,
  byTestId,
  click,
  diagnostic,
  emit,
  endTest,
  held,
  layoutOf,
  paneOf,
  project,
  render,
  resetStores,
  server,
  settle,
  timeScale,
  typeSource,
  useModuleUi,
  useSession,
  useShell,
  writes,
  type Backend,
  type Compiled,
} from './harness';

const RECIPE = 'TYPE Recipe :\nSTRUCT\n  speed : INT;\nEND_STRUCT;\nEND_TYPE\n';
const RECIPE2 = 'TYPE Recipe :\nSTRUCT\n  speed : INT;\n  temp : REAL;\nEND_STRUCT;\nEND_TYPE\n';
const NOPE = 'there is no type Nope';

vi.setConfig({ testTimeout: 20_000 * timeScale });

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
  vi.restoreAllMocks();
});

/**
 * A compiler that refuses any type whose text names `Nope`, at the line it is on, column 11, in
 * that type's file; every other state checks.
 */
function nopeCompiler(types: Record<string, string>, blocks: Backend['blocks']): Compiled {
  for (const [stem, text] of Object.entries(types)) {
    const line = text.split('\n').findIndex((l) => l.includes('Nope'));
    if (line >= 0) return { ok: false, diagnostics: [diagnostic(NOPE, line + 1, 11, `data/types/${stem}.st`)] };
  }
  return {
    ok: true,
    layouts: Object.fromEntries(
      // A comment is not laid out: only the declarations make the layout.
      blocks.map((b) => [b.name, { layout: layoutOf((types[b.type] ?? '').replace(/\(\*.*?\*\)/g, '').trim()), size: 8, members: [] }])
    ),
  };
}

const recipe = (over: Partial<Backend> = {}): Backend => ({
  types: { Recipe: RECIPE },
  blocks: [{ name: 'Line1', type: 'Recipe', retain: true }],
  compile: nopeCompiler,
  ...over,
});

/** The workbench has `plant` open and an editor on type `name`. */
async function opened(backend: Backend, name = 'Recipe') {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  await settle();
  useShell.getState().openEditor(TYPE_EDITOR, name);
  const removed: string[] = [];
  const container = render(<TypeEditor name={name} onRemoved={(gone) => removed.push(gone)} />);
  await settle();
  return { calls, container, removed };
}

const source = (container: HTMLElement) => {
  const area = byTestId(container, 'type-source');
  if (!(area instanceof HTMLTextAreaElement)) throw new Error('no source');
  return area;
};
const doc = (name = 'Recipe') => {
  const found = useData.getState().docs[name];
  if (!found) throw new Error(`no editor on ${name}`);
  return found;
};
const diagnostics = (container: HTMLElement, id = 'diagnostic') => {
  const pane = paneOf(container, 'diagnostics', DIAGNOSTICS_OWNER);
  if (!pane) throw new Error('no diagnostics panel');
  return allByTestId(pane, id).map((item) => ({ own: item.getAttribute('data-own'), text: item.textContent }));
};

describe('opening a user type', () => {
  it('shows the stored text, unchanged and saved, with the blocks of its type and their layout', async () => {
    const { container, calls } = await opened(recipe());
    expect(source(container).value).toBe(RECIPE);
    expect(byTestId(container, 'type-unsaved')).toBeNull();
    expect(byTestId(container, 'type-check')?.textContent).toBe('NOT CHECKED');
    const row = byTestId(container, 'type-block');
    expect([...(row?.querySelectorAll('td') ?? [])].map((cell) => cell.textContent)).toEqual([
      'Line1',
      layoutOf(RECIPE.trim()),
      '8 bytes',
      'retained',
    ]);
    // Before anything is checked, the Diagnostics tab claims nothing.
    expect(useModuleUi.getState().diagnostics[DIAGNOSTICS_OWNER]?.item.count).toBeNull();
    expect(writes(calls)).toEqual([]);
  });

  it('a CRLF file opens clean', async () => {
    const crlf = RECIPE.replace(/\n/g, '\r\n');
    const { container } = await opened(recipe({ types: { Recipe: crlf } }));
    expect(doc().saved).toBe(crlf);
    expect(byTestId(container, 'type-unsaved')).toBeNull();
    expect(useModuleUi.getState().unsaved[UNSAVED_OWNER]).toBeUndefined();
    // A change is a change, whatever the line ends.
    typeSource(container, `${RECIPE}(* note *)\n`);
    expect(byTestId(container, 'type-unsaved')?.textContent).toBe('UNSAVED');
    expect(useModuleUi.getState().unsaved[UNSAVED_OWNER]).toBe('org.mythos.data');
  });

  it('a type file that is not text is said, and can be replaced', async () => {
    const { container, calls } = await opened(
      recipe({ types: { Recipe: { unreadable: 'stream did not contain valid UTF-8' } }, blocks: [] })
    );
    expect(byTestId(container, 'type-read-error')?.textContent).toBe(
      'The stored file of Recipe could not be read as text: /projects/p/data/types/Recipe.st: stream did not contain valid UTF-8. The text below is not the stored one; saving replaces the file with it.'
    );
    // Not the stored text: an empty one, offered for Save and Delete as the module's routes allow.
    expect(source(container).value).toBe('');
    expect(byTestId(container, 'type-delete')).not.toBeNull();
    expect((byTestId(container, 'type-save') as HTMLButtonElement).disabled).toBe(true);

    typeSource(container, RECIPE);
    click(byTestId(container, 'type-save'));
    await settle();
    expect(writes(calls)).toEqual(['PUT /data/types/Recipe']);
    expect(byTestId(container, 'type-read-error')).toBeNull();
    expect(source(container).value).toBe(RECIPE);
    expect(byTestId(container, 'type-unsaved')).toBeNull();
  });

  it('a type that cannot be read for another reason says so in the module words, with nothing to edit', async () => {
    const { container } = await opened(recipe(), 'Missing');
    expect(byTestId(container, 'type-read-error')?.textContent).toBe(
      "The user type Missing could not be readthe project has no type 'Missing'"
    );
    expect(byTestId(container, 'type-source')).toBeNull();
  });
});

describe('saving a user type', () => {
  it('after a save the stored text is shown, not the typed one', async () => {
    // The module stores the text with a line end added: what is shown as saved is what it stored.
    const backend = recipe({ stores: (text) => `${text}\n` });
    const { container, calls } = await opened(backend);
    typeSource(container, RECIPE2);
    expect(byTestId(container, 'type-unsaved')?.textContent).toBe('UNSAVED');
    click(byTestId(container, 'type-save'));
    await settle();

    expect(calls.find((c) => c.method === 'PUT')?.body).toEqual({ source: RECIPE2 });
    expect(source(container).value).toBe(`${RECIPE2}\n`);
    expect(doc().saved).toBe(`${RECIPE2}\n`);
    expect(byTestId(container, 'type-unsaved')).toBeNull();
    // The answer has no source, so the stored text was read again after the PUT.
    expect(calls.map((c) => `${c.method} ${c.path}`).slice(-3)).toContain('GET /api/v1/projects/plant/data/types/Recipe');
    const put = calls.findIndex((c) => c.method === 'PUT');
    expect(calls.slice(put + 1).some((c) => c.method === 'GET' && c.path.endsWith('/data/types/Recipe'))).toBe(true);
  });

  it('a save whose stored text cannot be read back is not shown as clean', async () => {
    let put = false;
    const { container } = await opened(
      recipe({
        intercept: (call) => {
          if (call.method === 'PUT') put = true;
          return put && call.method === 'GET' && call.path.endsWith('/data/types/Recipe')
            ? Response.json({ error: 'the disk went away' }, { status: 500, statusText: 'Internal Server Error' })
            : undefined;
        },
      })
    );
    typeSource(container, RECIPE2);
    click(byTestId(container, 'type-save'));
    await settle();
    expect(byTestId(container, 'type-read-back')?.textContent).toBe(
      'Saved; the stored text could not be read back: the disk went away. The text below is what was sent, not what the module stored.'
    );
    expect(byTestId(container, 'type-unsaved')?.textContent).toBe('UNSAVED');
    expect(doc().saved).toBeNull();
  });

  it('changed null is not shown as no change', async () => {
    // The stored state does not check: what the blocks were laid out as before is not known.
    const { container } = await opened(recipe({ types: { Recipe: RECIPE.replace('INT', 'Nope') } }));
    typeSource(container, RECIPE);
    click(byTestId(container, 'type-save'));
    await settle();
    expect(byTestId(container, 'save-changed')?.textContent).toBe(
      'Which blocks changed is not known (the project did not check before this save).'
    );

    // From a state that checks, the module's list is shown as it is.
    typeSource(container, RECIPE2);
    click(byTestId(container, 'type-save'));
    await settle();
    expect(byTestId(container, 'save-changed')?.textContent).toBe('Data blocks laid out anew or new: Line1.');
    typeSource(container, `${RECIPE2}(* the same layout *)\n`);
    click(byTestId(container, 'type-save'));
    await settle();
    expect(byTestId(container, 'save-changed')?.textContent).toBe("No data block's layout changed.");
  });

  it("a refused save shows the module's diagnostics and keeps the draft", async () => {
    const backend = recipe();
    const { container, calls } = await opened(backend);
    const typed = RECIPE.replace('INT', 'Nope');
    typeSource(container, typed);
    click(byTestId(container, 'type-save'));
    await settle();

    expect(writes(calls)).toEqual(['PUT /data/types/Recipe']);
    expect(byTestId(container, 'save-refused')?.querySelector('p')?.textContent).toBe(
      "with this source the project's types and data blocks do not check, and nothing was saved"
    );
    // The typed text stays, unsaved; the saved text is still the stored one.
    expect(source(container).value).toBe(typed);
    expect(byTestId(container, 'type-unsaved')?.textContent).toBe('UNSAVED');
    expect(doc().saved).toBe(RECIPE);
    expect(backend.types.Recipe).toBe(RECIPE);
    expect(diagnostics(container, 'refusal-diagnostic')).toEqual([{ own: 'true', text: `3:11 ${NOPE}` }]);
    expect(useModuleUi.getState().diagnostics[DIAGNOSTICS_OWNER]?.item.count).toBe(1);
  });

  const REFUSALS: Array<{ label: string; over: Partial<Backend>; name?: string; error: string }> = [
    {
      label: 'the one-declaration rule (400, no diagnostics)',
      over: {
        declarationProblem: (name) =>
          `the source of type '${name}' must declare the type ${name} and nothing else; it declares type ${name}, program P`,
      },
      error: "the source of type 'Recipe' must declare the type Recipe and nothing else; it declares type Recipe, program P",
    },
    {
      label: 'another spelling of a stored name (409)',
      over: { types: { RECIPE: RECIPE }, blocks: [] },
      name: 'Recipe',
      error: "the project already has a type 'RECIPE'; names that differ only in case are one name",
    },
    {
      label: 'a file the module cannot write (500)',
      over: {
        intercept: (call) =>
          call.method === 'PUT' ? Response.json({ error: '/projects/p/data/types/Recipe.st: Permission denied (os error 13)' }, { status: 500 }) : undefined,
      },
      error: '/projects/p/data/types/Recipe.st: Permission denied (os error 13)',
    },
    {
      label: 'a source over the core’s limit (413, not JSON)',
      over: {
        intercept: (call) =>
          call.method === 'PUT' ? new Response('length limit exceeded', { status: 413, statusText: 'Payload Too Large' }) : undefined,
      },
      error: '413 Payload Too Large',
    },
  ];

  for (const { label, over, name, error } of REFUSALS) {
    it(`a refusal without diagnostics is shown in the module's words: ${label}`, async () => {
      const editing = name ?? 'Recipe';
      const backend = recipe(over);
      server(backend);
      useShell.setState({ project: project('plant'), view: 'project' });
      await settle();
      // Another spelling of a stored name is opened as a new type, as the tree does.
      if (!Object.keys(backend.types).includes(editing)) useData.getState().openNew(editing);
      const container = render(<TypeEditor name={editing} onRemoved={() => undefined} />);
      await settle();
      typeSource(container, RECIPE2);
      click(byTestId(container, 'type-save'));
      await settle();
      const notice = byTestId(container, 'save-refused');
      expect(notice?.className).toBe('notice notice--bad');
      expect(notice?.textContent).toBe(error);
      expect(source(container).value).toBe(RECIPE2);
    });
  }
});

describe('checking a user type', () => {
  it("a mistake in another type's file is listed with its file", async () => {
    const { container } = await opened(
      recipe({
        compile: () => ({
          ok: false,
          diagnostics: [
            diagnostic('speed is declared twice', 4, 3, 'data/types/Recipe.st'),
            diagnostic('Nope is not a type', 2, 5, 'data/types/Alarm.st'),
            diagnostic('data/blocks.json cannot be read', 0, 0),
          ],
        }),
      })
    );
    click(byTestId(container, 'type-check-button'));
    await settle();
    expect(byTestId(container, 'type-check')?.textContent).toBe('DOES NOT CHECK');
    expect(diagnostics(container)).toEqual([
      { own: 'true', text: '4:3 speed is declared twice' },
      { own: 'false', text: 'data/types/Alarm.st 2:5 Nope is not a type' },
      { own: 'false', text: 'about the whole data/blocks.json cannot be read' },
    ]);
    // Only this type's own is a place in its text, and a button that goes there.
    const pane = paneOf(container, 'diagnostics', DIAGNOSTICS_OWNER);
    expect(allByTestId(pane!, 'diagnostic').map((item) => item.querySelector('button') !== null)).toEqual([true, false, false]);
    expect(useModuleUi.getState().diagnostics[DIAGNOSTICS_OWNER]?.item.count).toBe(3);
  });

  it('is a draft check: it writes nothing and checks the text as typed', async () => {
    const backend = recipe();
    const { container, calls } = await opened(backend);
    typeSource(container, RECIPE.replace('INT', 'Nope'));
    click(byTestId(container, 'type-check-button'));
    await settle();
    expect(calls.filter((c) => c.path.endsWith('/data/check')).map((c) => c.body)).toEqual([
      { type: 'Recipe', source: RECIPE.replace('INT', 'Nope') },
    ]);
    expect(writes(calls)).toEqual([]);
    expect(backend.types.Recipe).toBe(RECIPE);
    expect(diagnostics(container)).toEqual([{ own: 'true', text: `3:11 ${NOPE}` }]);
  });

  it('an answer for an older text is not applied', async () => {
    const first = held();
    let checks = 0;
    const { container } = await opened(
      recipe({
        intercept: (call) => {
          if (!call.path.endsWith('/data/check')) return undefined;
          checks += 1;
          return checks === 1 ? first.response : undefined;
        },
      })
    );
    typeSource(container, RECIPE.replace('INT', 'Nope'));
    click(byTestId(container, 'type-check-button'));
    await settle();
    typeSource(container, RECIPE2);
    click(byTestId(container, 'type-check-button'));
    await settle();
    expect(byTestId(container, 'type-check')?.textContent).toBe('CHECKS');

    // The answer to the first text comes last: it is not this text's, and is dropped.
    first.release(
      Response.json({ ok: false, diagnostics: [diagnostic(NOPE, 3, 11, 'data/types/Recipe.st')], blocks: [] })
    );
    await settle();
    expect(byTestId(container, 'type-check')?.textContent).toBe('CHECKS');
    expect(diagnostics(container)).toEqual([]);
    expect(doc().check?.text).toBe(RECIPE2);
  });
});

describe('who may change a user type', () => {
  it('a viewer sees it read-only and may check it', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const { container, calls } = await opened(recipe());
    expect(byTestId(container, 'type-save')).toBeNull();
    expect(byTestId(container, 'type-delete')).toBeNull();
    expect(source(container).readOnly).toBe(true);
    expect(byTestId(container, 'type-read-only')?.textContent).toBe('Only an engineer changes user types.');
    click(byTestId(container, 'type-check-button'));
    await settle();
    expect(byTestId(container, 'type-check')?.textContent).toBe('CHECKS');
    expect(writes(calls)).toEqual([]);
  });

  it('an operator sees it read-only too', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'otto', display_name: 'Otto', role: 'operator', via: 'session' } });
    const { container } = await opened(recipe());
    expect(byTestId(container, 'type-save')).toBeNull();
    expect(byTestId(container, 'type-delete')).toBeNull();
  });

  it('an engineer may save and delete', async () => {
    useSession.setState({ phase: 'signed-in', user: { username: 'erin', display_name: 'Erin', role: 'engineer', via: 'session' } });
    const { container } = await opened(recipe());
    expect(byTestId(container, 'type-save')).not.toBeNull();
    expect(byTestId(container, 'type-delete')).not.toBeNull();
    expect(source(container).readOnly).toBe(false);
  });
});

describe('deleting a user type', () => {
  it('asks first, naming what follows, and deletes nothing when declined', async () => {
    const asked: string[] = [];
    const confirm = vi.spyOn(window, 'confirm').mockImplementation((text) => {
      asked.push(String(text));
      return false;
    });
    const { container, calls, removed } = await opened(recipe({ blocks: [] }));
    click(byTestId(container, 'type-delete'));
    await settle();
    expect(asked).toEqual([deleteQuestion('Recipe')]);
    expect(asked[0]).toBe('Delete user type Recipe? A program that declares it no longer compiles; nothing is rebuilt.');
    expect(writes(calls)).toEqual([]);
    expect(removed).toEqual([]);

    confirm.mockReturnValue(true);
    click(byTestId(container, 'type-delete'));
    await settle();
    expect(writes(calls)).toEqual(['DELETE /data/types/Recipe']);
    expect(removed).toEqual(['Recipe']);
  });

  it('a refused delete is shown in the module words and keeps the type', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const { container, removed } = await opened(recipe());
    click(byTestId(container, 'type-delete'));
    await settle();
    expect(byTestId(container, 'delete-refused')?.textContent).toBe(
      "type 'Recipe' is the type of data block 'Line1': delete those blocks or give them another type first"
    );
    expect(removed).toEqual([]);
    expect(source(container).value).toBe(RECIPE);
  });
});

describe('following changes made elsewhere', () => {
  const changed = (over: Record<string, unknown> = {}) =>
    emit(DATA_CHANGED_EVENT, { project: 'plant', what: 'type', name: 'Recipe', change: 'saved', ...over });

  it('an editor without changes reads the type again, and closes when it was deleted', async () => {
    const backend = recipe({ blocks: [] });
    const { container } = await opened(backend);
    backend.types.Recipe = RECIPE2;
    changed();
    await settle();
    expect(source(container).value).toBe(RECIPE2);
    expect(byTestId(container, 'type-elsewhere')).toBeNull();

    delete backend.types.Recipe;
    changed({ change: 'deleted' });
    await settle();
    expect(useShell.getState().editors.map((e) => e.target)).toEqual([]);
    expect(useData.getState().types).toEqual([]);
  });

  it('an editor with changes keeps them and says the type changed elsewhere', async () => {
    const backend = recipe({ blocks: [] });
    const { container } = await opened(backend);
    const typed = `${RECIPE}(* mine *)\n`;
    typeSource(container, typed);
    backend.types.Recipe = RECIPE2;
    changed();
    await settle();
    expect(source(container).value).toBe(typed);
    expect(byTestId(container, 'type-elsewhere')?.textContent).toBe(
      'Recipe was changed elsewhere since it was opened; saving replaces that change.'
    );
    expect(byTestId(container, 'type-unsaved')?.textContent).toBe('UNSAVED');

    delete backend.types.Recipe;
    changed({ change: 'deleted' });
    await settle();
    expect(source(container).value).toBe(typed);
    expect(byTestId(container, 'type-elsewhere')?.textContent).toBe(
      'Recipe was deleted elsewhere since it was opened; saving makes it again.'
    );
    expect(useShell.getState().editors.map((e) => e.target)).toEqual(['Recipe']);
  });

  it('an event of another project is ignored, and this editor’s own save is not taken for another’s', async () => {
    const { container, calls } = await opened(recipe({ blocks: [] }));
    const before = calls.length;
    changed({ project: 'other' });
    await settle();
    expect(calls.length).toBe(before);

    // Saved here, typed on while the event of that save comes: no warning of a change elsewhere.
    typeSource(container, RECIPE2);
    click(byTestId(container, 'type-save'));
    await settle();
    typeSource(container, `${RECIPE2}(* more *)\n`);
    changed();
    await settle();
    expect(byTestId(container, 'type-elsewhere')).toBeNull();
    expect(source(container).value).toBe(`${RECIPE2}(* more *)\n`);
  });
});
