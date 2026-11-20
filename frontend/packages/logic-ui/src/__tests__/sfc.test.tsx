// A Sequential Function Chart program in the workbench (ADR-013 L3d-1): the chart is not drawn
// yet (its view is L3d-2), and the workbench says so: the program has its language in the tree,
// and its editor shows its TC6 document as text, read-only for everyone, each diagnostic
// underlined at the element it is about, nothing drawn live; it is checked and loaded as any
// program. Every assertion reads a rendered element by its data-testid.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CheckResult } from '../bridge';
import { ProgramEditor } from '../ProgramEditor';
import { ProgramsTree } from '../ProgramsTree';
import {
  byTestId,
  endTest,
  entryOf,
  project,
  render,
  resetStores,
  runtimeOf,
  runtimesOf,
  server,
  settle,
  texts,
  until,
  useController,
  useShell,
  watchOf,
  timeScale,
  type Backend,
} from './harness';

const CHART = `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="Main" pouType="program">
      <interface><localVars><variable name="go"><type><BOOL/></type></variable></localVars></interface>
      <body><SFC>
        <step localId="1" name="Start" initialStep="true"><position x="0" y="0"/></step>
        <transition localId="2"><position x="0" y="40"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn>
          <condition><inline name=""><ST><xhtml:p xmlns:xhtml="http://www.w3.org/1999/xhtml">goo</xhtml:p></ST></inline></condition></transition>
      </SFC></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
`;

/** The compiler's answer for CHART: its condition names a variable it does not have. */
const refused = (): CheckResult => ({
  ok: false,
  diagnostics: [
    {
      line: 8,
      col: 9,
      end_line: 8,
      end_col: 41,
      message: 'goo is not declared',
      element: { pou: 'Main', local_id: 2, x: 0, y: 40, text: { field: 'condition', col: 1, end_col: 4, line: 1 } },
    },
  ],
});

const compiles = (): CheckResult => ({
  ok: true,
  diagnostics: [],
  program: 'Main',
  version: 'bbbbbbbbbbbb',
  variables: [
    { path: 'go', type: 'BOOL', section: 'local', bindable: true, hidden: false },
    { path: 'Start.X', type: 'BOOL', section: 'local', bindable: true, hidden: false },
  ],
  code_size: 100,
  memory_size: 20,
  binding_problems: [],
  chart: [
    { local_id: 1, step: 'X', variable: 'Start.X', negated: false },
    { local_id: 1, step: 'T', variable: 'Start.T', negated: false },
  ],
});

const chart = (over: Partial<Backend> = {}): Backend => ({
  programs: [
    { entry: entryOf('Tank'), source: 'PROGRAM Tank\nEND_PROGRAM\n' },
    { entry: entryOf('Main', { language: 'sfc' }), source: CHART },
  ],
  check: refused,
  runtimes: runtimesOf(runtimeOf('Main', { version: 'bbbbbbbbbbbb' })),
  watch: watchOf({ go: { type: 'BOOL', value: false }, 'Start.X': { type: 'BOOL', value: true } }),
  ...over,
});

async function opened(backend: Backend) {
  server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant' });
  await settle();
  const container = render(<ProgramEditor name="Main" />);
  await settle();
  await until('the chart is shown', () => byTestId(container, 'sfc-panel'));
  return container;
}

vi.setConfig({ testTimeout: 20_000 * timeScale });

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
});

describe('a Sequential Function Chart in the workbench', () => {
  it("names the program's language in the tree", async () => {
    await opened(chart());
    const tree = render(<ProgramsTree openEditor={() => undefined} />);
    expect(texts(tree, 'tree-program-language')).toEqual(['ST', 'SFC']);
  });

  it('says the chart is not drawn yet, and shows its document read-only with each diagnostic underlined', async () => {
    const container = await opened(chart());
    expect(byTestId(container, 'sfc-not-drawn')?.textContent).toContain('not drawn yet');
    const area = container.querySelector('textarea');
    if (!(area instanceof HTMLTextAreaElement)) throw new Error('no text');
    expect(area.readOnly).toBe(true);
    expect(area.value).toBe(CHART);
    // No diagram of another language is drawn in its place.
    expect(byTestId(container, 'fbd-view')).toBeNull();
    expect(byTestId(container, 'ld-editor')).toBeNull();
    await until('the diagnostic is underlined', () => byTestId(container, 'source-error-mark'));
    expect(byTestId(container, 'source-error-line')?.getAttribute('title') ?? '').toContain('goo is not declared');
  });

  it('draws nothing live of a running chart, and the program still loads as any', async () => {
    const container = await opened(chart({ check: compiles }));
    expect(byTestId(container, 'sfc-not-drawn')?.textContent).toContain('its live state is not shown');
    expect(byTestId(container, 'program-load')).not.toBeNull();
    expect(byTestId(container, 'program-load-cold')).not.toBeNull();
  });
});
