// The Sequential Function Chart editor in the workbench (ADR-013 L3d-3), against the workbench's
// real host: an engineer builds a sequence with the keyboard, the Instructions task card, the
// favourites and by dragging, as S7-GRAPH's sequence editor works (a step and a transition as a
// pair, alternative and simultaneous branches, a jump); names steps, writes conditions and the
// action table in place, declaring a variable where it is named; is refused what would not be a
// well-formed chart and told why; undoes and redoes; sees what is wrong at once and the compiler's
// word at its element after; is told when a change of the chart's steps restarts it at the next
// online change; and saves the TC6 document through the source route. Everyone else gets the
// read-only view. Every assertion reads a rendered element by its data-testid, or the text the
// editor wrote.

import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CheckResult } from '../bridge';
import { PANES, ProgramEditor } from '../ProgramEditor';
import { usePrograms } from '../programs';
import { open, type Item } from '../sfcmodel';
import { useModuleUi } from '../../../ui-shell/src/modules/registry';
import {
  byTestId,
  click,
  endTest,
  entryOf,
  paneOf,
  project,
  render,
  resetStores,
  runtimeOf,
  runtimesOf,
  server,
  settle,
  texts,
  timeScale,
  typeInto,
  unmountAll,
  until,
  useController,
  useSession,
  useShell,
  watchOf,
  type Backend,
} from './harness';

/** The logic module's template for a new SFC program. */
const TEMPLATE = `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <fileHeader companyName="" productName="MythOS logic" productVersion="0.4.0" creationDateTime="2026-11-20T00:00:00Z"/>
  <contentHeader name="Tank">
    <coordinateInfo>
      <sfc><scaling x="1" y="1"/></sfc>
    </coordinateInfo>
  </contentHeader>
  <types>
    <dataTypes/>
    <pous>
      <pou name="Tank" pouType="program">
        <interface/>
        <body>
          <SFC>
            <step localId="1" name="Start" initialStep="true">
              <position x="0" y="0"/>
              <connectionPointOut formalParameter=""/>
            </step>
          </SFC>
        </body>
      </pou>
    </pous>
  </types>
  <instances>
    <configurations/>
  </instances>
</project>
`;

const compiles = (): CheckResult => ({ ok: true, diagnostics: [], program: 'Tank', version: 'v1', variables: [], chart: [] });

const tank = (over: Partial<Backend> = {}): Backend => ({
  programs: [{ entry: entryOf('Tank', { language: 'sfc' }), source: TEMPLATE }],
  check: compiles,
  ...over,
});

async function opened(backend: Backend) {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant' });
  await settle();
  const container = render(<ProgramEditor name="Tank" />);
  await settle();
  await until('the chart is drawn', () => byTestId(container, 'sfc-editor') ?? byTestId(container, 'sfc-view') ?? byTestId(container, 'sfc-unreadable'));
  return { calls, container };
}

const source = () => usePrograms.getState().programs.find((d) => d.entry.name === 'Tank')?.source ?? '';

/** The chart of the text, read again, as `Start* -start- Fill[N valve] ->Start`. */
function chart(text: string): string {
  const read = open(text);
  if (!read.ok) throw new Error(read.error);
  const pou = read.model.pous[0];
  if (!pou) return 'none';
  if (pou.why !== null) return `kept: ${pou.why}`;
  const show = (items: readonly Item[]): string =>
    items
      .map((i) =>
        i.t === 'step'
          ? `${i.step.name}${i.step.initial ? '*' : ''}${i.step.actions.length ? `[${i.step.actions.map((a) => `${a.qualifier}${a.duration ? ` ${a.duration}` : ''} ${a.text}`).join('; ')}]` : ''}`
          : i.t === 'trans'
            ? `-${i.trans.text}-`
            : i.t === 'jump'
              ? `->${i.target}`
              : `${i.t}{ ${i.branches.map(show).join(' | ')} }`
      )
      .join(' ');
  return pou.charts.map((c) => show(c.items)).join(' || ');
}

const editor = (container: HTMLElement) => {
  const found = byTestId(container, 'sfc-editor');
  if (!(found instanceof HTMLElement)) throw new Error('no editor');
  return found;
};

function press(key: string, mods: { shift?: boolean; ctrl?: boolean; alt?: boolean } = {}, target?: Element | null) {
  const at = target ?? document.activeElement;
  if (!at) throw new Error('nothing has the focus');
  act(() => {
    at.dispatchEvent(new KeyboardEvent('keydown', { key, shiftKey: mods.shift, ctrlKey: mods.ctrl, altKey: mods.alt, bubbles: true, cancelable: true }));
  });
}

/** Type into a field (a text area too) the way a keystroke does. */
function type(at: Element | null, text: string) {
  if (!(at instanceof HTMLTextAreaElement)) return typeInto(at, text);
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set?.call(at, text);
    at.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

/** Type into the field that has the focus and take it with Enter (Ctrl+Enter: and declare it). */
function field(text: string, declare = false) {
  const at = document.activeElement;
  expect(at === null || at === document.body, 'a field has the focus').toBe(false);
  type(at, text);
  press('Enter', { ctrl: declare });
}

const drawn = (container: HTMLElement, id: number) => container.querySelector(`[data-testid="sfc-element"][data-local-id="${id}"]`);
const named = (container: HTMLElement, name: string) => container.querySelector(`[data-testid="sfc-element"][data-kind="step"][data-name="${name}"]`);
const clickSvg = (el: Element | null) => act(() => el?.dispatchEvent(new MouseEvent('click', { bubbles: true })));
const item = (container: HTMLElement, id: string) => container.querySelector(`[data-testid="sfc-palette-item"][data-item="${id}"]`);
/** The transitions drawn, by localId, top to bottom of the document. */
const transitions = (container: HTMLElement) => [...container.querySelectorAll('[data-testid="sfc-element"][data-kind="transition"]')].map((e) => Number(e.getAttribute('data-local-id')));

vi.setConfig({ testTimeout: 60_000 * timeScale });

beforeEach(() => {
  resetStores();
  window.localStorage.clear();
});

afterEach(() => {
  endTest();
});

describe('building a sequence', () => {
  it('builds steps, a condition and an action from the keyboard, declares where it names, and saves it', async () => {
    const { calls, container } = await opened(tank());
    const root = editor(container);
    clickSvg(drawn(container, 1));
    expect(byTestId(container, 'sfc-fields')?.getAttribute('data-kind')).toBe('step');
    // Shift+F2: a transition and a step after Start, the new step's name taking the focus.
    act(() => root.focus());
    press('F2', { shift: true });
    field('Fill');
    expect(chart(source())).toBe('Start* -FALSE- Fill');
    // Shift+F7: an N action of Fill; valve declared as proposed (a BOOL output) with Ctrl+Enter.
    act(() => root.focus());
    press('F7', { shift: true });
    field('valve', true);
    expect(chart(source())).toBe('Start* -FALSE- Fill[N valve]');
    expect(source()).toContain('<interface><outputVars><variable name="valve"><type><BOOL/></type></variable></outputVars></interface>');
    // ↑ goes to the transition; Enter to its condition.
    act(() => root.focus());
    press('ArrowUp');
    expect(byTestId(container, 'sfc-fields')?.getAttribute('data-kind')).toBe('transition');
    press('Enter');
    field('level > 10');
    expect(chart(source())).toBe('Start* -level > 10- Fill[N valve]');
    expect(source()).toContain('level &gt; 10');
    expect(byTestId(container, 'program-unsaved')).not.toBeNull();
    act(() => root.focus());
    press('s', { ctrl: true });
    await settle();
    const saved = calls.filter((c) => c.method === 'PUT' && c.path.endsWith('/logic/programs/Tank/source'));
    expect(saved).toHaveLength(1);
    expect((saved[0]?.body as { source: string }).source).toBe(source());
    expect(byTestId(container, 'program-unsaved')).toBeNull();
  });

  it('opens branches and a jump from the task card, by click and by drag, and the favourites', async () => {
    const { container } = await opened(tank());
    // The palette is the workbench's Instructions task card, not in the editor.
    const card = paneOf(container, 'task-cards', PANES.instructions);
    expect(card?.querySelector('[data-testid="sfc-palette"]')).not.toBeNull();
    expect(editor(container).querySelector('[data-testid="sfc-palette"]')).toBeNull();
    const favourites = () => [...container.querySelectorAll('[data-testid="sfc-favourite"]')].map((b) => b.getAttribute('data-item'));
    expect(favourites()).toEqual(['pair', 'alt', 'sim', 'jump', 'action:N']);
    // With nothing selected: at the end of the sequence.
    click(item(container, 'pair'));
    field('Fill');
    click(item(container, 'pair'));
    field('Drain');
    clickSvg(named(container, 'Fill'));
    click(container.querySelector('[data-testid="sfc-favourite"][data-item="alt"]'));
    // The new transition's condition takes the focus.
    field('stop');
    expect(chart(source())).toBe('Start* -FALSE- Fill sel{ -FALSE- | -stop- } Drain');
    // A simultaneous branch at Drain, which follows a convergence, not a transition: refused, and why.
    clickSvg(named(container, 'Drain'));
    click(item(container, 'sim'));
    expect(texts(container, 'sfc-edit-error')).toEqual(['A simultaneous branch opens right after a transition: no transition is right before this step.']);
    click(item(container, 'jump'));
    expect(chart(source())).toBe('Start* -FALSE- Fill sel{ -FALSE- | -stop- } Drain -FALSE- ->Start');
    expect(byTestId(container, 'sfc-edit-error')).toBeNull();
    // An S action dragged from the card onto Drain.
    act(() => item(container, 'action:S')?.dispatchEvent(new Event('dragstart', { bubbles: true })));
    act(() => named(container, 'Drain')?.dispatchEvent(new Event('drop', { bubbles: true, cancelable: true })));
    field('alarm');
    expect(chart(source())).toBe('Start* -FALSE- Fill sel{ -FALSE- | -stop- } Drain[S alarm] -FALSE- ->Start');
    unmountAll();
    expect(useModuleUi.getState().taskCards[PANES.instructions]).toBeUndefined();
  });

  it('edits the action table in place: qualifier, duration, statements; offers only what compiles', async () => {
    const { container } = await opened(tank());
    clickSvg(drawn(container, 1));
    click(byTestId(container, 'sfc-add-action'));
    field('horn');
    const qualifier = byTestId(container, 'sfc-action-qualifier') as HTMLSelectElement;
    expect([...qualifier.options].map((o) => o.value)).toEqual(['N', 'R', 'S', 'L', 'D', 'P', 'P1', 'DS']);
    act(() => {
      qualifier.value = 'L';
      qualifier.dispatchEvent(new Event('change', { bubbles: true }));
    });
    expect(chart(source())).toBe('Start*[L T#1s horn]');
    const duration = byTestId(container, 'sfc-action-duration');
    typeInto(duration, 'T#250ms');
    press('Enter', {}, duration);
    expect(chart(source())).toBe('Start*[L T#250ms horn]');
    // Statements in place of a name.
    const kind = byTestId(container, 'sfc-action-kind') as HTMLSelectElement;
    act(() => {
      kind.value = 'st';
      kind.dispatchEvent(new Event('change', { bubbles: true }));
    });
    field('count := count + 1;');
    expect(chart(source())).toBe('Start*[L T#250ms count := count + 1;]');
    expect(source()).toContain('<inline><ST><xhtml:p xmlns:xhtml="http://www.w3.org/1999/xhtml">count := count + 1;</xhtml:p></ST></inline>');
  });

  it('undoes and redoes every edit, and deletes a step with the transition before it', async () => {
    const { container } = await opened(tank());
    const root = editor(container);
    click(item(container, 'pair'));
    field('Fill');
    click(item(container, 'pair'));
    field('Drain');
    expect(chart(source())).toBe('Start* -FALSE- Fill -FALSE- Drain');
    clickSvg(named(container, 'Fill'));
    act(() => root.focus());
    press('Delete', {}, root);
    expect(chart(source())).toBe('Start* -FALSE- Drain');
    press('z', { ctrl: true }, root);
    expect(chart(source())).toBe('Start* -FALSE- Fill -FALSE- Drain');
    // Back past the naming of Drain: the step as it was put (Step1's name was free again).
    press('z', { ctrl: true }, root);
    expect(chart(source())).toBe('Start* -FALSE- Fill -FALSE- Step1');
    // Redone twice: the naming, then the deletion, the last edit.
    press('y', { ctrl: true }, root);
    press('y', { ctrl: true }, root);
    expect(chart(source())).toBe('Start* -FALSE- Drain');
    expect((byTestId(container, 'sfc-redo') as HTMLButtonElement).disabled).toBe(true);
    click(byTestId(container, 'sfc-undo'));
    expect(chart(source())).toBe('Start* -FALSE- Fill -FALSE- Drain');
    click(byTestId(container, 'sfc-redo'));
    expect(chart(source())).toBe('Start* -FALSE- Drain');
  });
});

describe('what is wrong, and what an online change does', () => {
  it('shows what is wrong at once, then what the compiler says at its element', async () => {
    let checks = 0;
    const backend = tank({
      check: (text) => {
        checks += 1;
        const read = open(text);
        const items = read.ok ? (read.model.pous[0]?.charts[0]?.items ?? []) : [];
        const t = items.find((i) => i.t === 'trans');
        if (t?.t !== 'trans' || t.trans.text !== 'NOPE') return compiles();
        return {
          ok: false,
          diagnostics: [{ line: 20, col: 1, end_line: 20, end_col: 5, message: 'NOPE is not declared', element: { pou: 'Tank', local_id: t.trans.id, x: 0, y: 30 } }],
        };
      },
    });
    const { container } = await opened(backend);
    click(item(container, 'pair'));
    field('Fill');
    click(item(container, 'jump'));
    // A name another step has: refused, and why.
    clickSvg(named(container, 'Fill'));
    const name = byTestId(container, 'sfc-step-name');
    typeInto(name, 'Start');
    press('Enter', {}, name);
    expect(texts(container, 'sfc-edit-error')).toEqual(['Start is already the name of a step, an action, a transition or a variable of Tank.']);
    const first = transitions(container)[0] as number;
    clickSvg(drawn(container, first));
    act(() => editor(container).focus());
    press('Enter');
    field('NOPE');
    const before = checks;
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 900));
    });
    await settle();
    expect(checks).toBeGreaterThan(before);
    expect(texts(container, 'sfc-element-problem')).toEqual(['The compiler: NOPE is not declared']);
    expect(container.querySelectorAll(`[data-testid="sfc-diagnostic"][data-local-id="${first}"]`)).toHaveLength(1);
    // An empty condition: the editor's own word, before any compiler.
    act(() => editor(container).focus());
    press('Enter');
    field(' ');
    expect(texts(container, 'sfc-element-problem')).toContain('The transition has no condition.');
    expect(container.querySelectorAll(`[data-testid="sfc-problem"][data-local-id="${first}"]`)).toHaveLength(1);
  });

  it('says a change of the steps restarts the chart at the next Load, and a change of a condition does not', async () => {
    const { container } = await opened(tank());
    expect(byTestId(container, 'sfc-shape-changed')).toBeNull();
    click(item(container, 'pair'));
    field('Fill');
    expect(byTestId(container, 'sfc-shape-changed')?.textContent).toContain('at the next Load the chart starts again from its initial step');
    act(() => editor(container).focus());
    press('z', { ctrl: true });
    press('z', { ctrl: true });
    expect(byTestId(container, 'sfc-shape-changed')).toBeNull();
  });
});

describe('a chart the editor cannot draw, and who edits', () => {
  it('keeps a chart another tool wrote that it cannot draw, says why, and draws it', async () => {
    const odd = TEMPLATE.replace(
      /<SFC>[\s\S]*<\/SFC>/,
      `<SFC>
            <step localId="1" name="A" initialStep="true"><position x="0" y="0"/></step>
            <macroStep localId="2"><position x="0" y="40"/></macroStep>
          </SFC>`
    );
    const { container } = await opened(tank({ programs: [{ entry: entryOf('Tank', { language: 'sfc' }), source: odd }] }));
    expect(texts(container, 'sfc-kept')).toEqual(['Kept as the document has it: it holds a macroStep (2), which this editor does not draw. The editor writes this chart back as it is and puts nothing into it.']);
    expect(drawn(container, 1)).not.toBeNull();
    expect(byTestId(container, 'sfc-palette-idle')).not.toBeNull();
  });

  it('draws the live state in the editor as the view does', async () => {
    const backend = tank({
      check: () => ({
        ...compiles(),
        version: 'aaaaaaaaaaaa',
        chart: [
          { local_id: 1, step: 'X', variable: 'Start.X', negated: false },
          { local_id: 1, step: 'T', variable: 'Start.T', negated: false },
        ],
      }),
      runtimes: runtimesOf(runtimeOf('Tank', { version: 'aaaaaaaaaaaa' })),
      watch: watchOf({ 'Start.X': { type: 'BOOL', value: true }, 'Start.T': { type: 'TIME', value: 45e6 } }),
    });
    const { container } = await opened(backend);
    await until('the chart is live', () => (byTestId(container, 'sfc-live')?.getAttribute('data-live') === 'live' ? true : null));
    await until('Start is drawn active', () => (drawn(container, 1)?.getAttribute('data-state') === 'active' ? true : null));
    expect(drawn(container, 1)?.querySelector('[data-testid="sfc-step-time"]')?.textContent).toBe('T 45 ms');
    expect(byTestId(container, 'sfc-editor')).not.toBeNull();
  });

  it('gives a viewer and an operator the read-only view', async () => {
    for (const role of ['viewer', 'operator'] as const) {
      useSession.setState({ phase: 'signed-in', user: { username: role, display_name: role, role, via: 'session' } });
      const { container } = await opened(tank());
      expect(byTestId(container, 'sfc-editor')).toBeNull();
      expect(byTestId(container, 'sfc-view')).not.toBeNull();
      expect(texts(container, 'sfc-read-only')).toEqual(['Read-only: only an engineer changes a program.']);
      unmountAll();
      vi.unstubAllGlobals();
      resetStores();
    }
  });
});
