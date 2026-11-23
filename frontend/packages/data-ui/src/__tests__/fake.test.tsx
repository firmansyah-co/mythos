// The fake controller of these tests answers as the data module does where the module's own
// tests pin it (plugins/data/src/tests.rs): a test built on it proves nothing in a world the
// module never makes.

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { endTest, project, resetStores, server, settle, useShell, type Backend } from './harness';

const RECIPE = 'TYPE Recipe :\nSTRUCT\n  speed : INT;\nEND_STRUCT;\nEND_TYPE\n';
const LINE = 'TYPE LineState :\nSTRUCT\n  on : BOOL;\nEND_STRUCT;\nEND_TYPE\n';

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
});

async function opened(backend: Backend) {
  server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  await settle();
}

const put = (path: string, body: unknown) =>
  fetch(`/api/v1/projects/plant/data/${path}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });

describe('the fake data module', () => {
  it('a type file that is not text makes the state not check, said of that file, and refuses a save beside it', async () => {
    await opened({ types: { Recipe: { unreadable: 'not UTF-8' } }, blocks: [{ name: 'Line1', type: 'Recipe', retain: false }] });
    const blocks = (await (await fetch('/api/v1/projects/plant/data/blocks')).json()) as {
      ok: boolean;
      diagnostics: Array<{ source?: string; message: string }>;
      blocks: Array<{ layout?: string }>;
    };
    expect(blocks.ok).toBe(false);
    expect(blocks.diagnostics.map((d) => d.source)).toEqual(['data/types/Recipe.st']);
    expect(blocks.diagnostics[0]?.message.startsWith("data/types/Recipe.st cannot be read as a type's source: it is not UTF-8 text")).toBe(true);
    expect(blocks.blocks[0]?.layout).toBeUndefined();
    expect((await put('types/LineState', { source: LINE })).status).toBe(400);
    // Mended by its own route: the state before did not check, so what changed is not known.
    const mended = await put('types/Recipe', { source: RECIPE });
    expect(mended.status).toBe(200);
    expect(((await mended.json()) as { changed: unknown }).changed).toBeNull();
  });

  it('refuses a data block of a type the project does not have', async () => {
    await opened({ types: {}, blocks: [] });
    const response = await put('blocks/B', { type: 'Nope', retain: false });
    expect(response.status).toBe(400);
    expect(await response.json()).toEqual({
      error: "data block 'B' does not check and was not saved",
      diagnostics: [{ line: 0, col: 0, end_line: 0, end_col: 0, message: "data block 'B': there is no type Nope" }],
    });
  });
});
