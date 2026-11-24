/// <reference types="node" />
// The project library's blocks in the Ladder Diagram and Function Block Diagram editors, against
// the workbench's real host: offered in the Instructions card in a group of their own, as the
// module's check of the stored library answers them (the harness's `compileLibrary`), or one
// line that says why none is; placed as boxes with their declared pins; checked against their
// declaration as it is now, without the diagram being rewritten. Every assertion reads a
// rendered element by its data-testid, or the text the editor wrote.

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CheckResult } from '../bridge';
import { open as openFbd } from '../fbdmodel';
import type { LibraryBlock, LibraryCheck } from '../library';
import { InstructionsCard } from '../LdInstructions';
import { open as openLd } from '../ldmodel';
import { ProgramEditor } from '../ProgramEditor';
import { usePrograms } from '../programs';
import {
  byTestId,
  click,
  emit,
  endTest,
  entryOf,
  fbdOf,
  ladderOf,
  project,
  render,
  resetStores,
  server,
  settle,
  texts,
  timeScale,
  typeInto,
  until,
  useController,
  useShell,
  type Backend,
  type Call,
} from './harness';
import { checksWith, MOTOR, SCALE } from './libraryanswer';

const FIXTURES = resolve(__dirname, '../../../../../crates/mythos-logic-compiler/tests');
/** What the editors wrote with Motor and Scale (ldmodel.test.ts, fbdmodel.test.ts), compiled and run by the compiler's tests. */
const LD_WRITTEN = readFileSync(resolve(FIXTURES, 'ladder_library/editor_library.xml'), 'utf8');
const FBD_WRITTEN = readFileSync(resolve(FIXTURES, 'fbd_library/editor_library.xml'), 'utf8');

const LIBRARY_CHANGED = 'LOGIC_LIBRARY_CHANGED';
const DIAGNOSTIC = { line: 2, col: 1, end_line: 2, end_col: 4, message: 'expected END_VAR', source: 'logic/library/Motors.st' };

interface Lang {
  name: 'Ladder Diagram' | 'Function Block Diagram';
  p: 'ld' | 'fbd';
  language: 'ld' | 'fbd';
  empty: string;
  written: string;
  compiles: () => CheckResult;
}

const LD: Lang = {
  name: 'Ladder Diagram',
  p: 'ld',
  language: 'ld',
  empty: ladderOf('Main', ''),
  written: LD_WRITTEN,
  compiles: () => ({ ok: true, diagnostics: [], program: 'Main', version: 'v1', variables: [], power: [] }),
};
const FBD: Lang = {
  name: 'Function Block Diagram',
  p: 'fbd',
  language: 'fbd',
  empty: fbdOf('Main', ''),
  written: FBD_WRITTEN,
  compiles: () => ({ ok: true, diagnostics: [], program: 'Main', version: 'v1', variables: [], values: [] }),
};

/** A program Main in `lang` holding `text`, and the stored library's check answering `library`. */
const backendOf = (lang: Lang, text: string, library: LibraryCheck | (() => LibraryCheck), over: Partial<Backend> = {}): Backend => ({
  programs: [{ entry: entryOf('Main', { language: lang.language }), source: text }],
  check: lang.compiles,
  compileLibrary: typeof library === 'function' ? library : () => library,
  ...over,
});

async function opened(lang: Lang, backend: Backend) {
  const calls = server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant' });
  await settle();
  const container = render(<ProgramEditor name="Main" />);
  await settle();
  await until('the diagram is drawn', () => byTestId(container, `${lang.p}-editor`));
  return { calls, container };
}

/** The text the program holds now, in the editor. */
const source = () => usePrograms.getState().programs.find((d) => d.entry.name === 'Main')?.source ?? '';

/** The palette's Library group: its items and its one line. */
function libraryGroup(container: HTMLElement, p: string) {
  const group = [...container.querySelectorAll(`[data-testid="${p}-palette-group"]`)].find((g) => g.getAttribute('data-group') === 'Library');
  if (!group) throw new Error('no Library group');
  return {
    items: [...group.querySelectorAll(`[data-testid="${p}-palette-item"]`)].map((x) => x.getAttribute('data-item')),
    names: [...group.querySelectorAll(`[data-testid="${p}-palette-item"]`)].map((x) => x.getAttribute('aria-label')),
    why: texts(group as HTMLElement, `${p}-palette-library-why`),
  };
}

const item = (container: HTMLElement, p: string, id: string) => container.querySelector(`[data-testid="${p}-palette-item"][data-item="${id}"]`);

function press(key: string, mods: { ctrl?: boolean } = {}, target?: Element | null) {
  const at = target ?? document.activeElement;
  if (!at) throw new Error('nothing has the focus');
  act(() => {
    at.dispatchEvent(new KeyboardEvent('keydown', { key, ctrlKey: mods.ctrl, bubbles: true, cancelable: true }));
  });
}

/** Select the box of type `type` on the diagram, as a click on it does. */
function selectBox(container: HTMLElement, p: string, text: string, type: string) {
  const id = boxIdOf(p, text, type);
  const el = container.querySelector(`[data-testid="${p}-element"][data-local-id="${id}"]`);
  if (!el) throw new Error(`box ${type} is not drawn`);
  act(() => el.dispatchEvent(new MouseEvent('click', { bubbles: true })));
}

/** The localId of the box of `type` in what the program holds. */
function boxIdOf(p: string, text: string, type: string): number {
  if (p === 'ld') {
    const read = openLd(text);
    if (!read.ok) throw new Error(read.error);
    for (const net of read.model.pous[0]?.networks ?? []) {
      for (const item of net.rung ?? []) if (item.t === 'el' && item.el.typeName === type) return item.el.id;
    }
  } else {
    const read = openFbd(text);
    if (!read.ok) throw new Error(read.error);
    for (const net of read.model.pous[0]?.networks ?? []) if (net.root?.t === 'box' && net.root.box.typeName === type) return net.root.box.id;
  }
  throw new Error(`no ${type} box`);
}

/** The pins of the box of `type` as the program holds it: `IN1,IN2 / OUT`, and its instance. */
function pinsInText(p: string, text: string, type: string): { pins: string; instance: string } {
  const id = boxIdOf(p, text, type);
  if (p === 'ld') {
    const read = openLd(text);
    if (!read.ok) throw new Error(read.error);
    const el = (read.model.pous[0]?.networks ?? []).flatMap((n) => n.rung ?? []).find((i) => i.t === 'el' && i.el.id === id);
    if (el?.t !== 'el') throw new Error('gone');
    return { pins: `${el.el.inputs.map((x) => x.formal).join(',')} / ${el.el.outputs.map((x) => x.formal).join(',')}`, instance: el.el.instance };
  }
  const read = openFbd(text);
  if (!read.ok) throw new Error(read.error);
  const root = (read.model.pous[0]?.networks ?? []).map((n) => n.root).find((r) => r?.t === 'box' && r.box.id === id);
  if (root?.t !== 'box') throw new Error('gone');
  return { pins: `${root.box.inputs.map((x) => x.formal).join(',')} / ${root.box.outputs.map((x) => x.formal).join(',')}`, instance: root.box.instance };
}

/** What the editor says at the selected element. */
const said = (container: HTMLElement, p: string) => texts(container, `${p}-element-problem`).filter((x) => !x.startsWith('The compiler'));

/** The library saved elsewhere, as the module sends it: the stored library is checked again. */
async function librarySaved() {
  emit(LIBRARY_CHANGED, { project: 'plant', name: 'Motors', what: 'saved' });
  await settle();
}

/** `lang.written` with a POU Motor of the document's own, its pins other than the library's Motor. */
const withOwnMotor = (lang: Lang) =>
  lang.written.replace(
    '<pous>',
    `<pous>
      <pou name="Motor" pouType="functionBlock"><interface><inputVars><variable name="x"><type><INT/></type></variable></inputVars><outputVars><variable name="done"><type><BOOL/></type></variable></outputVars></interface><body><ST><xhtml:p xmlns:xhtml="http://www.w3.org/1999/xhtml">done := x &gt; 0;</xhtml:p></ST></body></pou>`
  );

const withoutSpeed: LibraryBlock = { ...MOTOR, inputs: [{ name: 'run', type: 'BOOL' }] };
const withLimit: LibraryBlock = { ...MOTOR, inputs: [...MOTOR.inputs, { name: 'limit', type: 'INT' }] };

vi.setConfig({ testTimeout: 60_000 * timeScale });

beforeEach(() => {
  resetStores();
  window.localStorage.clear();
});

afterEach(() => {
  endTest();
});

for (const lang of [LD, FBD]) {
  const { p } = lang;
  /** The operand field of pin `formal` of the selected box. */
  const pinField = (container: HTMLElement, formal: string) =>
    container.querySelector(`[data-testid="${p}-pin"][data-formal="${formal}"] input[role="combobox"]`);
  describe(`the ${lang.name} palette`, () => {
    it(`library blocks are offered in the ${p.toUpperCase()} palette in their own group`, async () => {
      const { container } = await opened(lang, backendOf(lang, lang.empty, checksWith(MOTOR, SCALE)));
      const titles = [...container.querySelectorAll(`[data-testid="${p}-palette-group"]`)].map((g) => g.getAttribute('data-group'));
      // After the standard groups; the document has no block of its own, so no group of its own.
      expect(titles.slice(-3)).toEqual(['Strings', 'Library', 'Program control']);
      expect(libraryGroup(container, p)).toEqual({
        items: ['box:Motor', 'box:Scale'],
        names: ['Motor (function block)', 'Scale (function)'],
        why: [],
      });
      // A favourite like any box.
      click(item(container, p, 'box:Motor')?.parentElement?.querySelector(`[data-testid="${p}-favourite-add"]`) ?? null);
      expect([...container.querySelectorAll(`[data-testid="${p}-favourite"]`)].map((b) => b.getAttribute('data-item'))).toContain('box:Motor');
    });

    it('a library being checked is said, not shown empty', async () => {
      const { container } = await opened(
        lang,
        backendOf(lang, lang.empty, checksWith(MOTOR), {
          intercept: (call: Call) => (call.method === 'POST' && call.path.endsWith('/logic/library/check') ? new Promise<Response>(() => undefined) : undefined),
        })
      );
      expect(libraryGroup(container, p)).toEqual({
        items: [],
        names: [],
        why: ['The library is being checked; its blocks are offered once it answers.'],
      });
    });

    it('a library that does not check offers no block and says why', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.empty, { ok: false, diagnostics: [DIAGNOSTIC, DIAGNOSTIC] }));
      expect(libraryGroup(container, p)).toEqual({
        items: [],
        names: [],
        why: ['The library does not check (2 diagnostics); its blocks are offered once it does.'],
      });
    });

    it('a library that could not be checked offers no block and says the answer it got', async () => {
      const { container } = await opened(
        lang,
        backendOf(lang, lang.empty, checksWith(MOTOR), {
          intercept: (call: Call) =>
            call.method === 'POST' && call.path.endsWith('/logic/library/check')
              ? Response.json({ error: 'logic/library: Permission denied (os error 13)' }, { status: 500, statusText: 'Internal Server Error' })
              : undefined,
        })
      );
      expect(libraryGroup(container, p).items).toEqual([]);
      expect(libraryGroup(container, p).why).toEqual([
        'The library could not be checked: logic/library: Permission denied (os error 13); its blocks are not offered.',
      ]);
    });

    it('a library whose list could not be read asks no check and does not say one is under way', async () => {
      const { calls, container } = await opened(
        lang,
        backendOf(lang, lang.empty, checksWith(MOTOR), {
          intercept: (call: Call) =>
            call.method === 'GET' && /\/logic\/library$/.test(call.path)
              ? Response.json({ error: 'logic/library: Permission denied (os error 13)' }, { status: 500, statusText: 'Internal Server Error' })
              : undefined,
        })
      );
      expect(calls.filter((c) => c.method === 'POST' && c.path.endsWith('/logic/library/check'))).toEqual([]);
      expect(libraryGroup(container, p)).toEqual({
        items: [],
        names: [],
        why: ['The library could not be read: logic/library: Permission denied (os error 13); its blocks are not offered.'],
      });
    });

    it('a library block named like one of the document’s own is not offered, and the group says so', async () => {
      const { container } = await opened(lang, backendOf(lang, withOwnMotor(lang), checksWith(MOTOR, SCALE)));
      expect(libraryGroup(container, p).items).toEqual(['box:Scale']);
      const group = [...container.querySelectorAll(`[data-testid="${p}-palette-group"]`)].find((g) => g.getAttribute('data-group') === 'Library');
      expect(texts(group as HTMLElement, `${p}-palette-library-twins`)).toEqual(['Not offered: Motor, which this document declares too.']);
      const document = [...container.querySelectorAll(`[data-testid="${p}-palette-group"]`)].find((g) => g.getAttribute('data-group') === 'This document');
      expect([...(document?.querySelectorAll(`[data-testid="${p}-palette-item"]`) ?? [])].map((x) => x.getAttribute('data-item'))).toEqual(['box:Motor']);
    });

    it('an answer without blocks says so', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.empty, { ok: true, diagnostics: [] }));
      expect(libraryGroup(container, p)).toEqual({ items: [], names: [], why: ["This logic module does not answer the library's blocks."] });
    });

    it('a library that declares no block says so', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.empty, checksWith()));
      expect(libraryGroup(container, p)).toEqual({ items: [], names: [], why: ["The project's library declares no function or function block."] });
    });

    it('a malformed block entry is not drawn as a block', async () => {
      const malformed = { ok: true, diagnostics: [], blocks: [MOTOR, { ...SCALE, result: null }] } as unknown as LibraryCheck;
      const { container } = await opened(lang, backendOf(lang, lang.empty, malformed));
      expect(libraryGroup(container, p)).toEqual({
        items: [],
        names: [],
        why: ["The library's blocks were answered in a shape this workbench does not read."],
      });
    });

    it('a block saved in the library while the editor is open is offered once the library is checked again', async () => {
      let blocks: LibraryBlock[] = [MOTOR];
      const { container } = await opened(lang, backendOf(lang, lang.empty, () => checksWith(...blocks)));
      click(byTestId(container, `${p}-add-network`));
      click(item(container, p, 'box:Motor'));
      const placed = source();
      expect(libraryGroup(container, p).items).toEqual(['box:Motor']);
      blocks = [MOTOR, SCALE];
      await librarySaved();
      await until('Scale is offered', () => libraryGroup(container, p).items.includes('box:Scale'));
      // Taken by the editor as it is: what it wrote is kept, and its undo history with it.
      expect(source()).toBe(placed);
      click(byTestId(container, `${p}-undo`));
      expect(source()).not.toContain('typeName="Motor"');
    });
  });

  describe(`a library block placed in a ${lang.name}`, () => {
    it('a library function block is placed with its pins and an instance', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.empty, checksWith(MOTOR, SCALE)));
      click(byTestId(container, `${p}-add-network`));
      click(item(container, p, 'box:Motor'));
      const instance = byTestId(container, `${p}-box-instance`) as HTMLInputElement;
      expect(document.activeElement).toBe(instance);
      expect(instance.value).toBe('motor1');
      press('Enter');
      expect(source()).toContain('<variable name="motor1"><type><derived name="Motor"/></type></variable>');
      // LD: the power at RUN, a BOOL first input, and out of RUNNING, as for a standard block.
      expect(pinsInText(p, source(), 'Motor')).toEqual({ pins: 'RUN,SPEED / RUNNING,RPM', instance: 'motor1' });
    });

    it('a library function is placed with its pins and no instance', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.empty, checksWith(MOTOR, SCALE)));
      click(byTestId(container, `${p}-add-network`));
      click(item(container, p, 'box:Scale'));
      expect(byTestId(container, `${p}-box-instance`)).toBeNull();
      expect(source()).not.toContain('instanceName=');
      // LD adds EN and ENO to a function whose first input is not a BOOL; FBD adds none.
      expect(pinsInText(p, source(), 'Scale')).toEqual({ pins: p === 'ld' ? 'EN,RAW,FACTOR / ENO,OUT' : 'RAW,FACTOR / OUT', instance: '' });
    });

    it('typing a library block’s name into an empty box gives it its pins', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.empty, checksWith(MOTOR, SCALE)));
      click(byTestId(container, `${p}-add-network`));
      click(item(container, p, 'box'));
      const type = byTestId(container, `${p}-box-type`) as HTMLInputElement;
      typeInto(type, 'motor');
      press('Enter', {}, type);
      expect(pinsInText(p, source(), 'Motor')).toEqual({ pins: 'RUN,SPEED / RUNNING,RPM', instance: 'motor1' });
    });
  });

  describe(`a library block's pins in a ${lang.name} opened from its text`, () => {
    it('an operand of the wrong family at a library pin is said at once', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.written, checksWith(MOTOR, SCALE)));
      selectBox(container, p, lang.written, 'Motor');
      expect(said(container, p)).toEqual([]);
      const speed = pinField(container, 'SPEED');
      typeInto(speed, 'T#1s');
      press('Enter', {}, speed);
      expect(said(container, p)).toEqual(['SPEED: T#1s is a TIME; this takes a INT.']);
    });

    it('a variable declared at a library pin gets the pin’s type', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.written, checksWith(MOTOR, SCALE)));
      selectBox(container, p, lang.written, 'Motor');
      const speed = pinField(container, 'SPEED');
      typeInto(speed, 'setpoint');
      press('Enter', {}, speed);
      expect((container.querySelector('[aria-label="Type of setpoint"]') as HTMLSelectElement | null)?.value).toBe('INT');
      click(byTestId(container, 'ld-declare'));
      expect(source()).toContain('<variable name="setpoint"><type><INT/></type></variable>');
    });
  });

  describe(`a ${lang.name} box whose library block changed`, () => {
    it('a box of a library block that was deleted says the library does not declare it', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.written, checksWith(SCALE)));
      selectBox(container, p, lang.written, 'Motor');
      expect(said(container, p)).toEqual(["Motor is not a function or function block of this program or of the project's library, nor a standard one this editor knows."]);
    });

    it('while the library’s blocks are not known, a box the editor does not know says why', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.written, { ok: false, diagnostics: [DIAGNOSTIC, DIAGNOSTIC] }));
      selectBox(container, p, lang.written, 'Motor');
      expect(said(container, p)).toEqual([
        "Motor is not a function or function block this editor knows: it knows this program's own and the standard ones. If the project's library declares it, Check compiles the program with it.",
        "The project's library's blocks are not known here: the library does not check (2 diagnostics); its blocks are offered once it does.",
      ]);
    });

    it('a box whose library block lost a pin says so and keeps it', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.written, checksWith(withoutSpeed, SCALE)));
      selectBox(container, p, lang.written, 'Motor');
      expect(said(container, p)).toEqual(['SPEED is not a pin of Motor as it is declared now.']);
      expect(pinField(container, 'SPEED')).not.toBeNull();
      expect(source()).toBe(lang.written);
    });

    it('a box whose library block gained an input says so', async () => {
      const { container } = await opened(lang, backendOf(lang, lang.written, checksWith(withLimit, SCALE)));
      selectBox(container, p, lang.written, 'Motor');
      expect(said(container, p)).toEqual(['Motor declares the input LIMIT, which this box does not have.']);
      expect(source()).toBe(lang.written);
    });

    it('a box whose library block is a function now shows the instance it keeps, so it can be cleared', async () => {
      const fn: LibraryBlock = { name: 'Motor', kind: 'function', inputs: MOTOR.inputs, outputs: [], result: 'BOOL' };
      const { container } = await opened(lang, backendOf(lang, lang.written, checksWith(fn, SCALE)));
      selectBox(container, p, lang.written, 'Motor');
      expect(said(container, p)).toContain('Motor is a function as it is declared now, which has no instance; this box still names motor1.');
      const instance = byTestId(container, `${p}-box-instance`) as HTMLInputElement | null;
      expect(instance?.value).toBe('motor1');
      expect(source()).toBe(lang.written);
    });

    it('a library change rewrites no diagram', async () => {
      let motor = MOTOR;
      const { container } = await opened(lang, backendOf(lang, lang.written, () => checksWith(motor, SCALE)));
      selectBox(container, p, lang.written, 'Motor');
      expect(said(container, p)).toEqual([]);
      motor = withoutSpeed;
      await librarySaved();
      await until('the lost pin is said', () => said(container, p).length > 0);
      expect(said(container, p)).toEqual(['SPEED is not a pin of Motor as it is declared now.']);
      expect(source()).toBe(lang.written);
      expect(byTestId(container, 'program-unsaved')).toBeNull();
    });
  });
}

describe('the palette of another project', () => {
  it('a block of the project closed before is not offered', async () => {
    let answerPlant: (r: Response) => void = () => undefined;
    server({
      programs: [],
      compileLibrary: () => checksWith(SCALE),
      intercept: (call: Call) =>
        call.method === 'POST' && call.path.includes('/projects/plant/logic/library/check')
          ? new Promise<Response>((resolve) => {
              answerPlant = resolve;
            })
          : undefined,
    });
    useShell.setState({ project: project('plant'), view: 'project' });
    await settle();
    const container = render(<InstructionsCard />);
    await settle();
    expect(libraryGroup(container, 'ld').why).toEqual(['The library is being checked; its blocks are offered once it answers.']);
    // The project is switched while plant's check is not answered; then plant's answer comes.
    act(() => useShell.setState({ project: project('other'), view: 'project' }));
    await settle();
    await until('the other project is checked', () => libraryGroup(container, 'ld').items.length > 0);
    act(() => answerPlant(Response.json(checksWith(MOTOR))));
    await settle();
    expect(libraryGroup(container, 'ld').items).toEqual(['box:Scale']);
  });
});
