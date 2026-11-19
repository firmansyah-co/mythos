// The Ladder Diagram editor in the workbench (ADR-013 L3b-2b), against the workbench's real
// host: an engineer builds rungs with the keyboard, the palette, the favourites and by
// dragging; titles, comments, moves and deletes networks; undoes and redoes; declares a
// variable where it is typed; sees a wrong operand at once and the compiler's word on it
// after; and saves the TC6 document through the source route. Everyone else gets the
// read-only view. Every assertion reads a rendered element by its data-testid, or the text
// the editor wrote.

import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CheckResult } from '../bridge';
import { open, type Item } from '../ldmodel';
import { PANES, ProgramEditor } from '../ProgramEditor';
import { useModuleUi } from '../../../ui-shell/src/modules/registry';
import { usePrograms } from '../programs';
import {
  paneOf,
  byTestId,
  click,
  endTest,
  entryOf,
  project,
  render,
  resetStores,
  server,
  settle,
  texts,
  typeInto,
  unmountAll,
  until,
  useController,
  useSession,
  useShell,
  timeScale,
  type Backend,
} from './harness';

/** The logic module's template for a new LD program. */
const TEMPLATE = `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <fileHeader companyName="" productName="MythOS logic" productVersion="0.3.0" creationDateTime="2026-11-18T00:00:00"/>
  <contentHeader name="Motor">
    <coordinateInfo>
      <ld><scaling x="1" y="1"/></ld>
    </coordinateInfo>
  </contentHeader>
  <types>
    <dataTypes/>
    <pous>
      <pou name="Motor" pouType="program">
        <interface/>
        <body>
          <LD>
            <leftPowerRail localId="1">
              <position x="0" y="0"/>
              <connectionPointOut formalParameter=""/>
            </leftPowerRail>
            <rightPowerRail localId="2">
              <position x="400" y="0"/>
            </rightPowerRail>
          </LD>
        </body>
      </pou>
    </pous>
  </types>
  <instances>
    <configurations/>
  </instances>
</project>
`;

const compiles = (): CheckResult => ({ ok: true, diagnostics: [], program: 'Motor', version: 'v1', variables: [], power: [] });

const motor = (over: Partial<Backend> = {}): Backend => ({
  programs: [{ entry: entryOf('Motor', { language: 'ld' }), source: TEMPLATE }],
  check: compiles,
  ...over,
});

async function opened(backend: Backend) {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant' });
  await settle();
  const container = render(<ProgramEditor name="Motor" />);
  await settle();
  await until('the Ladder Diagram is drawn', () => byTestId(container, 'ld-editor') ?? byTestId(container, 'ld-view') ?? byTestId(container, 'ld-unreadable'));
  return { calls, container };
}

/** The text the program holds now, in the editor. */
const source = () => usePrograms.getState().programs.find((d) => d.entry.name === 'Motor')?.source ?? '';

/** Network `n` of the text, read again: kinds, operands and parallels. */
function rung(text: string, n: number): string {
  const read = open(text);
  if (!read.ok) throw new Error(read.error);
  const net = read.model.pous[0]?.networks[n];
  if (!net) return 'none';
  if (net.rung === null) return `kept: ${net.why}`;
  const show = (items: Item[]): string =>
    items
      .map((i) =>
        i.t === 'par'
          ? `(${i.branches.map(show).join(' | ')})`
          : i.el.kind === 'block'
            ? `${i.el.typeName}[${i.el.instance}]`
            : `${i.el.kind}${i.el.negated ? '/' : ''}${i.el.operand ? ` ${i.el.operand}` : ''}`
      )
      .join(' ');
  return show(net.rung);
}

const editor = (container: HTMLElement) => {
  const found = byTestId(container, 'ld-editor');
  if (!(found instanceof HTMLElement)) throw new Error('no editor');
  return found;
};

/** A key pressed where the focus is (or on `target`), as a browser sends it. */
function press(key: string, mods: { shift?: boolean; ctrl?: boolean; alt?: boolean } = {}, target?: Element | null) {
  const at = target ?? document.activeElement;
  if (!at) throw new Error('nothing has the focus');
  act(() => {
    at.dispatchEvent(new KeyboardEvent('keydown', { key, shiftKey: mods.shift, ctrlKey: mods.ctrl, altKey: mods.alt, bubbles: true, cancelable: true }));
  });
}

/** Type an operand into the field that has the focus and take it with Enter (Ctrl+Enter: and declare it). */
function operand(text: string, declare = false) {
  const field = document.activeElement;
  expect(field?.getAttribute('role'), 'an operand field has the focus').toBe('combobox');
  typeInto(field, text);
  press('Enter', { ctrl: declare });
}

const elementOf = (container: HTMLElement, operandText: string, kind = 'contact') => {
  const found = [...container.querySelectorAll(`[data-testid="ld-element"][data-kind="${kind}"]`)].find((e) =>
    (e.querySelector('title')?.textContent ?? '').endsWith(`: ${operandText}`)
  );
  if (!found) throw new Error(`no ${kind} ${operandText}`);
  return found;
};

// A test here makes dozens of edits, each re-laid out and checked. On a CI runner sharing its
// cores with builds the first one went past 20 s (collecting alone took 235 s there); the bound
// is for a test that hangs, so it is set to what the work takes under that load.
vi.setConfig({ testTimeout: 60_000 * timeScale });

beforeEach(() => {
  resetStores();
  window.localStorage.clear();
});

afterEach(() => {
  endTest();
});

describe('building rungs', () => {
  it('builds a seal-in rung from the keyboard, declares its variables where they are typed, and saves it', async () => {
    const { calls, container } = await opened(motor());
    const root = editor(container);
    act(() => root.focus());
    press('r', { ctrl: true });
    expect(texts(container, 'ld-network-select')).toEqual(['Network 1']);
    // Shift+F2, F3, F7: a normally open contact, a normally closed one, a coil; each operand
    // typed where the focus goes and declared with Ctrl+Enter as proposed.
    press('F2', { shift: true });
    operand('start', true);
    press('F3', { shift: true });
    operand('stop', true);
    press('F7', { shift: true });
    operand('motor', true);
    expect(rung(source(), 0)).toBe('contact start contact/ stop coil motor');
    expect(source()).toContain(
      '<interface><inputVars><variable name="start"><type><BOOL/></type></variable><variable name="stop"><type><BOOL/></type></variable></inputVars><outputVars><variable name="motor"><type><BOOL/></type></variable></outputVars></interface>'
    );
    // The branch: opened at start (Shift+F8), closed there (Shift+F9), and a contact put in it.
    act(() => (elementOf(container, 'start') as unknown as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true })));
    act(() => root.focus());
    press('F8', { shift: true });
    expect(byTestId(container, 'ld-branch-open')).not.toBeNull();
    press('F9', { shift: true });
    expect(byTestId(container, 'ld-gap')).not.toBeNull();
    press('F2', { shift: true });
    operand('motor');
    expect(rung(source(), 0)).toBe('(contact start | contact motor) contact/ stop coil motor');
    expect(byTestId(container, 'program-unsaved')).not.toBeNull();
    // Ctrl+S saves the document through the source route, as Save does.
    act(() => root.focus());
    press('s', { ctrl: true });
    await settle();
    const saved = calls.filter((c) => c.method === 'PUT' && c.path.endsWith('/logic/programs/Motor/source'));
    expect(saved).toHaveLength(1);
    expect((saved[0]?.body as { source: string }).source).toBe(source());
    expect(byTestId(container, 'program-unsaved')).toBeNull();
  });

  it('puts instructions from the palette by click and by drag, and a box with its instance', async () => {
    const { container } = await opened(motor());
    click(byTestId(container, 'ld-add-network'));
    const item = (id: string) => container.querySelector(`[data-testid="ld-palette-item"][data-item="${id}"]`);
    click(item('contact-p'));
    operand('go', true);
    // A timer from the palette: its instance proposed, and Enter declares it.
    click(item('box:TON'));
    const instance = byTestId(container, 'ld-box-instance') as HTMLInputElement;
    expect(document.activeElement).toBe(instance);
    expect(instance.value).toBe('ton1');
    press('Enter');
    expect(source()).toContain('<variable name="ton1"><type><derived name="TON"/></type></variable>');
    const pt = container.querySelector('[data-testid="ld-pin"][data-formal="PT"] input');
    typeInto(pt, 'T#2s');
    press('Enter', {}, pt);
    expect(source()).toContain('<expression>T#2s</expression>');
    // A coil dragged from the palette onto the point after the timer.
    const gaps = [...container.querySelectorAll('[data-testid="ld-gap"]')];
    const last = gaps.find((g) => g.getAttribute('data-path') === '' && g.getAttribute('data-index') === '2');
    act(() => item('coil-set')?.dispatchEvent(new Event('dragstart', { bubbles: true })));
    act(() => last?.dispatchEvent(new Event('drop', { bubbles: true, cancelable: true })));
    operand('done', true);
    expect(rung(source(), 0)).toBe('contact go TON[ton1] coil done');
    expect(source()).toMatch(/<coil localId="\d+" width="30" height="20" storage="set">/);
  });

  it('keeps favourites a viewer chose, and puts from them', async () => {
    const { container } = await opened(motor());
    const favourites = () => [...container.querySelectorAll('[data-testid="ld-favourite"]')].map((b) => b.getAttribute('data-item'));
    // TIA's default set.
    expect(favourites()).toEqual(['contact-no', 'contact-nc', 'coil', 'box', 'branch-open', 'branch-close']);
    click(container.querySelector('[data-testid="ld-palette-item"][data-item="contact-p"] + [data-testid="ld-favourite-add"]'));
    expect(favourites()).toContain('contact-p');
    click(container.querySelector('[data-testid="ld-favourite"][data-item="box"] + [data-testid="ld-favourite-remove"]'));
    expect(favourites()).not.toContain('box');
    expect(JSON.parse(window.localStorage.getItem('mythos.logic.ld.favourites') ?? '[]')).toEqual(favourites());
    click(byTestId(container, 'ld-add-network'));
    click(container.querySelector('[data-testid="ld-favourite"][data-item="contact-p"]'));
    operand('edge');
    expect(source()).toMatch(/<contact localId="\d+" width="30" height="20" edge="rising">/);
    // The palette is the workbench's Instructions task card, beside the networks, not in them;
    // it goes when the editor goes.
    const card = paneOf(container, 'task-cards', PANES.instructions);
    expect(card?.querySelector('[data-testid="ld-palette"]')).not.toBeNull();
    expect(byTestId(container, 'ld-editor')?.querySelector('[data-testid="ld-palette"]')).toBeNull();
    unmountAll();
    expect(useModuleUi.getState().taskCards[PANES.instructions]).toBeUndefined();
  });
});

describe('networks', () => {
  it('titles, comments, moves and deletes networks; undo and redo go back and forth', async () => {
    const { container } = await opened(motor());
    const root = editor(container);
    for (const name of ['one', 'two']) {
      click(byTestId(container, 'ld-add-network'));
      click(container.querySelector('[data-testid="ld-favourite"][data-item="coil"]'));
      operand(name);
    }
    const title = container.querySelectorAll('[data-testid="ld-network-title"]')[1] as HTMLInputElement;
    act(() => title.focus());
    typeInto(title, 'The second');
    act(() => title.blur());
    const comment = container.querySelectorAll('[data-testid="ld-network-comment"]')[1] as HTMLTextAreaElement;
    act(() => comment.focus());
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set?.call(comment, 'Why it is there');
      comment.dispatchEvent(new Event('input', { bubbles: true }));
    });
    act(() => comment.blur());
    expect(source()).toContain('title="The second"');
    expect(source()).toContain('>Why it is there</xhtml:p>');
    click(container.querySelector('[aria-label="Move network 2 up"]'));
    expect([rung(source(), 0), rung(source(), 1)]).toEqual(['coil two', 'coil one']);
    expect((container.querySelector('[data-testid="ld-network-title"]') as HTMLInputElement).value).toBe('The second');
    // Alt+↓ moves the selected network back down.
    click(container.querySelector('[data-testid="ld-network-select"]'));
    act(() => root.focus());
    press('ArrowDown', { alt: true });
    expect([rung(source(), 0), rung(source(), 1)]).toEqual(['coil one', 'coil two']);
    click(container.querySelector('[aria-label="Delete network 1"]'));
    expect(rung(source(), 0)).toBe('coil two');
    // Undone, it is back; redone, it goes again.
    click(byTestId(container, 'ld-undo'));
    expect([rung(source(), 0), rung(source(), 1)]).toEqual(['coil one', 'coil two']);
    act(() => root.focus());
    press('y', { ctrl: true });
    expect(rung(source(), 0)).toBe('coil two');
    press('z', { ctrl: true });
    press('z', { ctrl: true });
    expect([rung(source(), 0), rung(source(), 1)]).toEqual(['coil two', 'coil one']);
  });

  it('keeps a network another tool wrote that it does not draw as a rung, and says why', async () => {
    const foreign = TEMPLATE.replace(
      '<rightPowerRail',
      `<inVariable localId="10"><position x="40" y="20"/><connectionPointOut/><expression>level &gt; 10</expression></inVariable>
       <outVariable localId="11"><position x="200" y="20"/><connectionPointIn><connection refLocalId="10"/></connectionPointIn><expression>high</expression></outVariable>
       <rightPowerRail`
    );
    const { container } = await opened(motor({ programs: [{ entry: entryOf('Motor', { language: 'ld' }), source: foreign }] }));
    expect(texts(container, 'ld-network-kept')[0]).toMatch(/^Kept as the document has it: the variable 1[01] is wired as part of the rung/);
    act(() => (container.querySelector('[data-testid="ld-element"][data-local-id="10"]') as Element).dispatchEvent(new MouseEvent('click', { bubbles: true })));
    click(container.querySelector('[data-testid="ld-favourite"][data-item="contact-no"]'));
    expect(texts(container, 'ld-edit-error')[0]).toMatch(/^Network 1 is kept as the document has it/);
    expect(source()).toBe(foreign);
  });
});

describe('operands', () => {
  it('shows a wrong operand at once, then what the compiler says at its element', async () => {
    let checks = 0;
    const backend = motor({
      check: (text) => {
        checks += 1;
        const read = open(text);
        const contact = read.ok ? read.model.pous[0]?.networks[0]?.rung?.[0] : undefined;
        if (contact?.t !== 'el' || contact.el.operand !== 'nosuch') return compiles();
        return {
          ok: false,
          diagnostics: [{ line: 20, col: 1, end_line: 20, end_col: 7, message: 'nosuch is not declared', element: { pou: 'Motor', local_id: contact.el.id, x: 0, y: 0 } }],
        };
      },
    });
    const { container } = await opened(backend);
    const before = checks;
    click(byTestId(container, 'ld-add-network'));
    click(container.querySelector('[data-testid="ld-favourite"][data-item="contact-no"]'));
    // Suggestions are the program's BOOL variables; none yet.
    operand('nosuch');
    // At once, from the editor's own check: written under the element and marked around it.
    expect(texts(container, 'ld-element-problem')).toEqual(['nosuch is not declared.']);
    expect(container.querySelectorAll('[data-testid="ld-problem"]')).toHaveLength(1);
    expect(byTestId(container, 'ld-declare-offer')?.textContent).toContain('nosuch is not declared. Declare it as');
    // Then from the compiler, by itself once the edits pause.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 900));
    });
    await settle();
    expect(checks).toBeGreaterThan(before);
    expect(texts(container, 'ld-element-problem')).toEqual(['nosuch is not declared.', 'The compiler: nosuch is not declared']);
    expect(container.querySelectorAll('[data-testid="ld-diagnostic"]')).toHaveLength(1);
    // Declared from the offer, as an input: the editor's own word on it goes.
    const section = container.querySelector('[aria-label="Section of nosuch"]');
    typeInto(section, 'inputVars');
    click(byTestId(container, 'ld-declare'));
    expect(source()).toContain('<inputVars><variable name="nosuch"><type><BOOL/></type></variable></inputVars>');
    expect(texts(container, 'ld-element-problem').filter((p) => !p.startsWith('The compiler'))).toEqual([]);
  });

  it('offers the program’s variables by name and type while an operand is typed', async () => {
    const declared = TEMPLATE.replace(
      '<interface/>',
      '<interface><inputVars><variable name="start"><type><BOOL/></type></variable><variable name="speed"><type><INT/></type></variable></inputVars><localVars><variable name="t"><type><derived name="TON"/></type></variable></localVars></interface>'
    );
    const { container } = await opened(motor({ programs: [{ entry: entryOf('Motor', { language: 'ld' }), source: declared }] }));
    click(byTestId(container, 'ld-add-network'));
    click(container.querySelector('[data-testid="ld-favourite"][data-item="contact-no"]'));
    const field = document.activeElement as HTMLInputElement;
    typeInto(field, 's');
    // A contact reads a BOOL: only the BOOL variables are offered, and t's output Q among them.
    expect(texts(container, 'ld-suggestions')[0]).toBe('start BOOL');
    typeInto(field, 't');
    expect(texts(container, 'ld-suggestions')[0]).toBe('t.Q BOOL');
    press('ArrowDown', {}, field);
    press('Enter', {}, field);
    expect(rung(source(), 0)).toBe('contact t.Q');
    // An INT where a BOOL is read is wrong at once.
    act(() => (elementOf(container, 't.Q') as unknown as HTMLElement).dispatchEvent(new MouseEvent('click', { bubbles: true })));
    const again = byTestId(container, 'ld-operand') as HTMLInputElement;
    typeInto(again, 'speed');
    press('Enter', {}, again);
    expect(texts(container, 'ld-element-problem')).toEqual(['speed is INT; this takes a BOOL.']);
  });
});

describe('a document the editor cannot edit', () => {
  it('says why, and draws it read-only', async () => {
    // Two elements of one localId: the front-end refuses the document, and the editor too.
    const twice = TEMPLATE.replace(
      '<rightPowerRail',
      `<coil localId="7"><position x="40" y="20"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn><connectionPointOut/><variable>a</variable></coil>
       <coil localId="7"><position x="40" y="80"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn><connectionPointOut/><variable>b</variable></coil>
       <rightPowerRail`
    );
    const { container } = await opened(motor({ programs: [{ entry: entryOf('Motor', { language: 'ld' }), source: twice }] }));
    expect(texts(container, 'ld-editor-unreadable')).toEqual(['The diagram cannot be edited: two elements of Motor have the localId 7.']);
    expect(byTestId(container, 'ld-view')).not.toBeNull();
    expect(container.querySelectorAll('[data-testid="ld-element"][data-kind="coil"]')).toHaveLength(2);
  });
});

describe('who edits', () => {
  it('gives a viewer and an operator the read-only view', async () => {
    for (const role of ['viewer', 'operator'] as const) {
      useSession.setState({ phase: 'signed-in', user: { username: role, display_name: role, role, via: 'session' } });
      const { container } = await opened(motor());
      expect(byTestId(container, 'ld-editor')).toBeNull();
      expect(byTestId(container, 'ld-view')).not.toBeNull();
      expect(texts(container, 'ld-panel').join(' ')).toContain('Read-only: only an engineer changes a program.');
      unmountAll();
      vi.unstubAllGlobals();
      resetStores();
    }
  });
});
