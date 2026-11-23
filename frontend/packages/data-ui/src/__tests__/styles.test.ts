// Every class the data UI draws with is the workbench's own (ui-shell's stylesheets) or is
// defined by data.css: none depends on another module's stylesheet, which is linked only while
// that module's UI is loaded.

import { readdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { expect, it } from 'vitest';

const PACKAGES = resolve(__dirname, '../../..');
const read = (path: string) => readFileSync(path, 'utf8');
const css = (dir: string) =>
  readdirSync(dir, { recursive: true, encoding: 'utf8' })
    .filter((file) => file.endsWith('.css'))
    .map((file) => read(join(dir, file)))
    .join('\n');

it("every class of the data UI is the workbench's own or data.css's", () => {
  const src = join(PACKAGES, 'data-ui/src');
  const used = new Set<string>();
  for (const file of readdirSync(src).filter((f) => f.endsWith('.tsx'))) {
    for (const match of read(join(src, file)).matchAll(/className=(?:"([^"]+)"|\{`([^`]+)`\})/g)) {
      for (const name of (match[1] ?? match[2] ?? '').split(/\s+/)) if (name && !name.includes('$')) used.add(name);
    }
  }
  // The check means nothing if it found nothing to check.
  expect([...used]).toEqual(expect.arrayContaining(['tree__node', 'btn', 'data-tree__input']));
  const defined = css(join(PACKAGES, 'ui-shell/src')) + '\n' + read(join(src, 'data.css'));
  const isDefined = (name: string) => new RegExp(`\\.${name.replace(/[-_]/g, (c) => `\\${c}`)}(?![\\w-])`).test(defined);
  expect([...used].filter((name) => !isDefined(name)).sort()).toEqual([]);
});
