// The Function Block Diagram editor in the workbench (ADR-013 L3c-2), against the workbench's real
// host: an engineer builds networks with the keyboard, the Instructions task card, the favourites
// and by dragging; joins a second signal into an input with an OR, as an input takes one line;
// titles, comments, moves and deletes networks; undoes and redoes; declares a variable where it is
// typed; sees a wrong operand at once and the compiler's word on it after; reads each output's type
// from the compiler; and saves the TC6 document through the source route. Everyone else gets the
// read-only view. Every assertion reads a rendered element by its data-testid, or the text the
// editor wrote.

import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CheckResult } from '../bridge';
import { boxesOf } from '../fbdedit';
import { open, type Box, type Root, type Src } from '../fbdmodel';
import { PANES, ProgramEditor } from '../ProgramEditor';
import { useModuleUi } from '../../../ui-shell/src/modules/registry';
import { usePrograms } from '../programs';
import {
  byTestId,
  click,
  endTest,
  entryOf,
  paneOf,
  project,
  render,
  resetStores,
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
  type Backend,
} from './harness';

/** The logic module's template for a new FBD program. */
const TEMPLATE = `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <fileHeader companyName="" productName="MythOS logic" productVersion="0.4.0" creationDateTime="2026-11-20T00:00:00Z"/>
  <contentHeader name="Mixer">
    <coordinateInfo>
      <fbd><scaling x="1" y="1"/></fbd>
    </coordinateInfo>
  </contentHeader>
  <types>
    <dataTypes/>
    <pous>
      <pou name="Mixer" pouType="program">
        <interface/>
        <body>
          <FBD/>
        </body>
      </pou>
    </pous>
  </types>
  <instances>
    <configurations/>
  </instances>
</project>
`;

const compiles = (): CheckResult => ({ ok: true, diagnostics: [], program: 'Mixer', version: 'v1', variables: [], values: [] });

const mixer = (over: Partial<Backend> = {}): Backend => ({
  programs: [{ entry: entryOf('Mixer', { language: 'fbd' }), source: TEMPLATE }],
  check: compiles,
  ...over,
});

async function opened(backend: Backend) {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant' });
  await settle();
  const container = render(<ProgramEditor name="Mixer" />);
  await settle();
  await until('the Function Block Diagram is drawn', () => byTestId(container, 'fbd-editor') ?? byTestId(container, 'fbd-view') ?? byTestId(container, 'fbd-unreadable'));
  return { calls, container };
}

const source = () => usePrograms.getState().programs.find((d) => d.entry.name === 'Mixer')?.source ?? '';

/** Network `n` of the text, read again, as `TYPE(IN1:=a, …) OUT => x`. */
function tree(text: string, n: number): string {
  const read = open(text);
  if (!read.ok) throw new Error(read.error);
  const net = read.model.pous[0]?.networks[n];
  if (!net) return 'none';
  if (net.why !== null) return `kept: ${net.why}`;
  const root: Root | null = net.root;
  if (!root) return 'empty';
  const src = (s: Src | null): string => (s === null ? '_' : s.t === 'operand' ? s.text : box(s.box));
  const box = (b: Box): string => `${b.typeName || '??'}${b.instance ? `[${b.instance}]` : ''}(${b.inputs.map((p) => `${p.negated ? '/' : ''}${p.formal}:=${src(p.src)}`).join(', ')})`;
  if (root.t === 'sink') return `${src(root.src)} => ${root.sink.text}`;
  const sinks = boxesOf(root)
    .flatMap((b) => b.outputs.flatMap((o) => o.to.map((t) => `${o.formal} ${t.kind === 'assign' ? '=>' : '>>'} ${t.text}`)))
    .join('; ');
  return `${box(root.box)}${sinks ? ` ${sinks}` : ''}`;
}

const editor = (container: HTMLElement) => {
  const found = byTestId(container, 'fbd-editor');
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

/** Type into the field that has the focus and take it with Enter (Ctrl+Enter: and declare it). */
function operand(text: string, declare = false) {
  const field = document.activeElement;
  expect(field?.getAttribute('role'), 'an operand field has the focus').toBe('combobox');
  typeInto(field, text);
  press('Enter', { ctrl: declare });
}

const pinTarget = (container: HTMLElement, formal: string, nth = 0) =>
  [...container.querySelectorAll(`[data-testid="fbd-pin-target"][data-formal="${formal}"]`)][nth] ?? null;

const clickSvg = (el: Element | null) => act(() => el?.dispatchEvent(new MouseEvent('click', { bubbles: true })));

vi.setConfig({ testTimeout: 60_000 * timeScale });

beforeEach(() => {
  resetStores();
  window.localStorage.clear();
});

afterEach(() => {
  endTest();
});

describe('building networks', () => {
  it('builds a seal-in from the keyboard, joins its second line with an OR, declares where it types, and saves it', async () => {
    const { calls, container } = await opened(mixer());
    const root = editor(container);
    act(() => root.focus());
    press('r', { ctrl: true });
    expect(texts(container, 'fbd-network-select')).toEqual(['Network 1']);
    // Shift+F2: an AND, its first input's field taking the focus; start declared as proposed.
    press('F2', { shift: true });
    operand('start', true);
    // IN2: its point selected, Enter goes to its field; stop, negated.
    clickSvg(pinTarget(container, 'IN2'));
    act(() => root.focus());
    press('Enter');
    operand('stop', true);
    click(container.querySelector('[data-testid="fbd-pin"][data-formal="IN2"] [data-testid="fbd-pin-negated"]'));
    // Shift+F7: an assignment from the AND's output; run declared, a BOOL output as proposed.
    act(() => root.focus());
    press('F7', { shift: true });
    operand('run', true);
    expect(tree(source(), 0)).toBe('AND(IN1:=start, /IN2:=stop) OUT => run');
    expect(source()).toContain(
      '<interface><inputVars><variable name="start"><type><BOOL/></type></variable><variable name="stop"><type><BOOL/></type></variable></inputVars><outputVars><variable name="run"><type><BOOL/></type></variable></outputVars></interface>'
    );
    // The seal: a second signal into IN1. An input takes one line, so an operand typed where an
    // OR's output now goes is refused, and "Join with OR" puts the OR in.
    clickSvg(pinTarget(container, 'IN1'));
    act(() => root.focus());
    press('F3', { shift: true });
    expect(tree(source(), 0)).toBe('AND(IN1:=OR(IN1:=start, IN2:=_), /IN2:=stop) OUT => run');
    operand('run');
    expect(tree(source(), 0)).toBe('AND(IN1:=OR(IN1:=start, IN2:=run), /IN2:=stop) OUT => run');
    expect(byTestId(container, 'program-unsaved')).not.toBeNull();
    act(() => root.focus());
    press('s', { ctrl: true });
    await settle();
    const saved = calls.filter((c) => c.method === 'PUT' && c.path.endsWith('/logic/programs/Mixer/source'));
    expect(saved).toHaveLength(1);
    expect((saved[0]?.body as { source: string }).source).toBe(source());
    expect(byTestId(container, 'program-unsaved')).toBeNull();
  });

  it('joins a second signal with an OR where an input already takes a box, never a second line', async () => {
    const { container } = await opened(mixer());
    click(byTestId(container, 'fbd-add-network'));
    const item = (id: string) => container.querySelector(`[data-testid="fbd-palette-item"][data-item="${id}"]`);
    click(item('box:AND'));
    operand('a');
    // A GT into IN1, from the palette with the input selected.
    clickSvg(pinTarget(container, 'IN1'));
    click(item('box:GT'));
    expect(tree(source(), 0)).toBe('AND(IN1:=GT(IN1:=a, IN2:=_), IN2:=_)');
    // The AND's IN1 (the root box's pins come first) shows where it comes from, with the OR
    // offered; there is no operand field there to type a second line into.
    clickSvg(pinTarget(container, 'IN1', 0));
    expect(container.querySelector('[data-testid="fbd-pin"][data-formal="IN1"] input[role="combobox"]')).toBeNull();
    expect(texts(container, 'fbd-pin-from')).toEqual([expect.stringMatching(/^from block \d+ \(GT\) OUT$/)]);
    click(byTestId(container, 'fbd-or-here'));
    operand('b');
    expect(tree(source(), 0)).toBe('AND(IN1:=OR(IN1:=GT(IN1:=a, IN2:=_), IN2:=b), IN2:=_)');
  });

  it('puts instructions from the palette by click and by drag, a timer with its instance, and both its outputs assigned', async () => {
    const { container } = await opened(mixer());
    click(byTestId(container, 'fbd-add-network'));
    const item = (id: string) => container.querySelector(`[data-testid="fbd-palette-item"][data-item="${id}"]`);
    click(item('box:TON'));
    const instance = byTestId(container, 'fbd-box-instance') as HTMLInputElement;
    expect(document.activeElement).toBe(instance);
    expect(instance.value).toBe('ton1');
    press('Enter');
    expect(source()).toContain('<variable name="ton1"><type><derived name="TON"/></type></variable>');
    const field = (formal: string) => container.querySelector(`[data-testid="fbd-pin"][data-formal="${formal}"] input[role="combobox"]`);
    typeInto(field('IN'), 'go');
    press('Enter', {}, field('IN'));
    typeInto(field('PT'), 'T#2s');
    press('Enter', {}, field('PT'));
    // An assignment dragged from the card onto the box: written from Q, its main output.
    const box = container.querySelector('[data-testid="fbd-element"][data-kind="block"]');
    act(() => item('assign')?.dispatchEvent(new Event('dragstart', { bubbles: true })));
    act(() => box?.dispatchEvent(new Event('drop', { bubbles: true, cancelable: true })));
    operand('done');
    // ET assigned from its own output's "=".
    click(container.querySelector('[data-testid="fbd-output"][data-formal="ET"] [data-testid="fbd-add-assignment"]'));
    operand('waited');
    expect(tree(source(), 0)).toBe('TON[ton1](IN:=go, PT:=T#2s) Q => done; ET => waited');
  });

  it('keeps favourites a viewer chose apart from the Ladder Diagram’s, and puts from them', async () => {
    const { container } = await opened(mixer());
    const favourites = () => [...container.querySelectorAll('[data-testid="fbd-favourite"]')].map((b) => b.getAttribute('data-item'));
    expect(favourites()).toEqual(['box:AND', 'box:OR', 'assign', 'box', 'branch', 'insert-input']);
    click(container.querySelector('[data-testid="fbd-palette-item"][data-item="box:XOR"] + [data-testid="fbd-favourite-add"]'));
    expect(favourites()).toContain('box:XOR');
    expect(JSON.parse(window.localStorage.getItem('mythos.logic.fbd.favourites') ?? '[]')).toEqual(favourites());
    expect(window.localStorage.getItem('mythos.logic.ld.favourites')).toBeNull();
    click(byTestId(container, 'fbd-add-network'));
    click(container.querySelector('[data-testid="fbd-favourite"][data-item="box:XOR"]'));
    expect(tree(source(), 0)).toBe('XOR(IN1:=_, IN2:=_)');
    // Insert input (Shift+F9 in the bar) gives the XOR a third.
    click(container.querySelector('[data-testid="fbd-favourite"][data-item="insert-input"]'));
    expect(tree(source(), 0)).toBe('XOR(IN1:=_, IN2:=_, IN3:=_)');
    // The palette is the workbench's Instructions task card, not in the editor; it goes with it.
    const card = paneOf(container, 'task-cards', PANES.instructions);
    expect(card?.querySelector('[data-testid="fbd-palette"]')).not.toBeNull();
    expect(byTestId(container, 'fbd-editor')?.querySelector('[data-testid="fbd-palette"]')).toBeNull();
    unmountAll();
    expect(useModuleUi.getState().taskCards[PANES.instructions]).toBeUndefined();
  });
});

describe('networks', () => {
  it('titles, comments, moves and deletes networks; undo and redo go back and forth', async () => {
    const { container } = await opened(mixer());
    const root = editor(container);
    for (const name of ['one', 'two']) {
      click(byTestId(container, 'fbd-add-network'));
      click(container.querySelector('[data-testid="fbd-palette-item"][data-item="assign"]'));
      operand(name);
    }
    const title = container.querySelectorAll('[data-testid="fbd-network-title"]')[1] as HTMLInputElement;
    act(() => title.focus());
    typeInto(title, 'The second');
    act(() => title.blur());
    expect(source()).toContain('title="The second"');
    click(container.querySelector('[aria-label="Move network 2 up"]'));
    expect([tree(source(), 0), tree(source(), 1)]).toEqual(['_ => two', '_ => one']);
    click(container.querySelector('[data-testid="fbd-network-select"]'));
    act(() => root.focus());
    press('ArrowDown', { alt: true });
    expect([tree(source(), 0), tree(source(), 1)]).toEqual(['_ => one', '_ => two']);
    click(container.querySelector('[aria-label="Delete network 1"]'));
    expect(tree(source(), 0)).toBe('_ => two');
    click(byTestId(container, 'fbd-undo'));
    expect([tree(source(), 0), tree(source(), 1)]).toEqual(['_ => one', '_ => two']);
    act(() => root.focus());
    press('y', { ctrl: true });
    expect(tree(source(), 0)).toBe('_ => two');
  });

  it('keeps a network another tool wrote that it does not draw, and says why', async () => {
    const foreign = TEMPLATE.replace(
      '<FBD/>',
      `<FBD>
            <inVariable localId="10"><position x="0" y="0"/><connectionPointOut/><expression>a</expression></inVariable>
            <connector localId="11" name="LINE"><position x="100" y="0"/><connectionPointIn><connection refLocalId="10"/></connectionPointIn></connector>
            <continuation localId="12" name="LINE"><position x="0" y="60"/><connectionPointOut/></continuation>
            <outVariable localId="13"><position x="100" y="60"/><connectionPointIn><connection refLocalId="12"/></connectionPointIn><expression>q</expression></outVariable>
          </FBD>`
    );
    const { container } = await opened(mixer({ programs: [{ entry: entryOf('Mixer', { language: 'fbd' }), source: foreign }] }));
    expect(texts(container, 'fbd-network-kept')[0]).toMatch(/^Kept as the document has it: it carries a line through the connector LINE \(11\)/);
    click(container.querySelector('[data-testid="fbd-network-select"]'));
    click(container.querySelector('[data-testid="fbd-favourite"][data-item="box:AND"]'));
    expect(texts(container, 'fbd-edit-error')[0]).toMatch(/^Network 1 is kept as the document has it/);
    expect(source()).toBe(foreign);
  });
});

describe('operands and types', () => {
  it('shows a wrong operand at once, then what the compiler says at its element, and declares it from where it was typed', async () => {
    let checks = 0;
    const backend = mixer({
      check: (text) => {
        checks += 1;
        const read = open(text);
        const root = read.ok ? read.model.pous[0]?.networks[0]?.root : null;
        const pin = root?.t === 'box' ? root.box.inputs[0] : undefined;
        if (root?.t !== 'box' || pin?.src?.t !== 'operand' || pin.src.text !== 'nosuch') return compiles();
        return {
          ok: false,
          diagnostics: [{ line: 20, col: 1, end_line: 20, end_col: 7, message: 'nosuch is not declared', element: { pou: 'Mixer', local_id: root.box.id, x: 0, y: 0 } }],
        };
      },
    });
    const { container } = await opened(backend);
    const before = checks;
    click(byTestId(container, 'fbd-add-network'));
    click(container.querySelector('[data-testid="fbd-favourite"][data-item="box:AND"]'));
    operand('nosuch');
    expect(texts(container, 'fbd-element-problem')).toEqual(['IN1: nosuch is not declared.', 'IN2 has no operand.']);
    expect(container.querySelectorAll('[data-testid="fbd-problem"]')).toHaveLength(1);
    expect(byTestId(container, 'ld-declare-offer')?.textContent).toContain('nosuch is not declared. Declare it as');
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 900));
    });
    await settle();
    expect(checks).toBeGreaterThan(before);
    expect(texts(container, 'fbd-element-problem')).toEqual(['IN1: nosuch is not declared.', 'IN2 has no operand.', 'The compiler: nosuch is not declared']);
    expect(container.querySelectorAll('[data-testid="fbd-diagnostic"]')).toHaveLength(1);
    click(byTestId(container, 'ld-declare'));
    expect(source()).toContain('<inputVars><variable name="nosuch"><type><BOOL/></type></variable></inputVars>');
    expect(texts(container, 'fbd-element-problem').filter((p) => !p.startsWith('The compiler'))).toEqual(['IN2 has no operand.']);
  });

  it('shows the type of each output as the compiler gives it, and UNKNOWN until it has', async () => {
    let compiled = false;
    const backend = mixer({
      check: (text) => {
        const read = open(text);
        const root = read.ok ? read.model.pous[0]?.networks[0]?.root : null;
        if (root?.t !== 'box' || !compiled) return compiles();
        return {
          ...compiles(),
          values: [{ local_id: root.box.id, output: 'OUT', variable: `__FBD_${root.box.id}` }],
          variables: [{ path: `__FBD_${root.box.id}`, type: 'REAL', section: 'local', bindable: false, hidden: true }],
        };
      },
    });
    const { container } = await opened(backend);
    click(byTestId(container, 'fbd-add-network'));
    click(container.querySelector('[data-testid="fbd-palette-item"][data-item="box:ADD"]'));
    expect(texts(container, 'fbd-output-type')).toEqual(['type UNKNOWN until the program compiles']);
    compiled = true;
    operand('1.5');
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 900));
    });
    await settle();
    // ADD is ANY_NUM by its declaration: what it gives here is the compiler's word, REAL.
    expect(texts(container, 'fbd-output-type')).toEqual(['REAL']);
  });
});

describe('a document the editor cannot edit', () => {
  it('says why, and draws it read-only', async () => {
    const twice = TEMPLATE.replace(
      '<FBD/>',
      `<FBD>
            <outVariable localId="7"><position x="40" y="20"/><connectionPointIn><expression>TRUE</expression></connectionPointIn><expression>a</expression></outVariable>
            <outVariable localId="7"><position x="40" y="80"/><connectionPointIn><expression>TRUE</expression></connectionPointIn><expression>b</expression></outVariable>
          </FBD>`
    );
    const { container } = await opened(mixer({ programs: [{ entry: entryOf('Mixer', { language: 'fbd' }), source: twice }] }));
    expect(texts(container, 'fbd-editor-unreadable')).toEqual(['The diagram cannot be edited: two elements of Mixer have the localId 7.']);
    expect(byTestId(container, 'fbd-view')).not.toBeNull();
  });
});

describe('who edits', () => {
  it('gives a viewer and an operator the read-only view', async () => {
    for (const role of ['viewer', 'operator'] as const) {
      useSession.setState({ phase: 'signed-in', user: { username: role, display_name: role, role, via: 'session' } });
      const { container } = await opened(mixer());
      expect(byTestId(container, 'fbd-editor')).toBeNull();
      expect(byTestId(container, 'fbd-view')).not.toBeNull();
      expect(texts(container, 'fbd-panel').join(' ')).toContain('Read-only: only an engineer changes a program.');
      unmountAll();
      vi.unstubAllGlobals();
      resetStores();
    }
  });
});
