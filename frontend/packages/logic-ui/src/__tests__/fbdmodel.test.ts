/// <reference types="node" />
// The Function Block Diagram editor's model (ADR-013 L3c-2): every edit, asserted on the TC6 text
// it writes. Each step is checked for what online change and another tool rely on: every element
// the step did not delete keeps its localId and its kind; a new element takes an id no element of
// the document has had in this run; the bytes outside the edited body (the file and content
// headers) are the document's own; every input of what is written takes at most one line
// (IEC 61131-3 ed. 3 §8.3.2); and what is written opens again and the view reads it.
//
// The documents the editor writes here are fixtures the compiler's tests run
// (crates/mythos-logic-compiler/tests/fbd/editor_*.xml, `fbd_from_the_editor.rs`), so what is
// written is what is compiled and run. Set MYTHOS_UPDATE_FIXTURES=1 to write them anew.

import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { readFbd } from '../fbd';
import {
  addInput,
  addNetwork,
  addSink,
  boxesOf,
  check,
  declareVariable,
  insert,
  moveNetwork,
  orAt,
  remove,
  setHeader,
  setInstance,
  setLabel,
  setNegated,
  setPinOperand,
  setSinkText,
  type Edited,
  type Sel,
} from '../fbdedit';
import { open, write, type Box, type DocModel, type Root, type Source, type Src } from '../fbdmodel';
import type { Ids } from '../ldmodel';
import { parseXml, type XNode } from '../ldxml';
import { motorLibrary } from './libraryanswer';

const FIXTURES = resolve(__dirname, '../../../../../crates/mythos-logic-compiler/tests/fbd');

/** The logic module's template for a new FBD program (plugins/logic `template`), its version and date fixed. */
const TEMPLATE = (name: string) => `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <fileHeader companyName="" productName="MythOS logic" productVersion="0.4.0" creationDateTime="2026-11-20T00:00:00Z"/>
  <contentHeader name="${name}">
    <coordinateInfo>
      <fbd><scaling x="1" y="1"/></fbd>
      <ld><scaling x="1" y="1"/></ld>
      <sfc><scaling x="1" y="1"/></sfc>
    </coordinateInfo>
  </contentHeader>
  <types>
    <dataTypes/>
    <pous>
      <pou name="${name}" pouType="program">
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

function parse(text: string): XNode {
  const parsed = parseXml(text);
  if (!parsed.ok) throw new Error(parsed.error);
  return parsed.root;
}

/** Every element of the document, by its POU and localId (`Main:12`), with its kind. */
function elementIds(text: string): Map<string, string> {
  const out = new Map<string, string>();
  const visit = (n: XNode, pou: string) => {
    const inside = n.name === 'pou' ? (n.attrs.find((a) => a.name === 'name')?.value ?? '') : pou;
    const id = n.attrs.find((a) => a.name === 'localId')?.value;
    if (id !== undefined) {
      if (out.has(`${inside}:${id}`)) throw new Error(`two elements of ${inside} have the localId ${id}`);
      out.set(`${inside}:${id}`, n.name);
    }
    n.children.forEach((c) => visit(c, inside));
  };
  visit(parse(text), '');
  return out;
}

/** The most lines any one input of the text takes. */
function mostLinesIntoOneInput(text: string): number {
  let most = 0;
  const visit = (n: XNode) => {
    if (n.name === 'connectionPointIn') most = Math.max(most, n.children.filter((c) => c.name === 'connection').length);
    n.children.forEach(visit);
  };
  visit(parse(text));
  return most;
}

const idOfKey = (key: string) => Number(key.slice(key.lastIndexOf(':') + 1));

function header(text: string): string {
  const start = text.search(/<(\w+:)?types>/);
  const end = text.search(/<\/(\w+:)?types>/);
  return text.slice(0, start) + text.slice(end);
}

/** A run of edits on one document, as the editor makes them: each step's text checked against the one before it. */
class Run {
  readonly source: Source;
  readonly ids: Ids;
  model: DocModel;
  text: string;
  sel: Sel | null = null;
  readonly ever = new Set<string>();
  readonly original: string;

  constructor(text: string, ids?: Ids) {
    const opened = open(text);
    if (!opened.ok) throw new Error(opened.error);
    this.source = opened.source;
    this.ids = ids ?? { next: opened.source.firstFree, used: new Set() };
    this.model = opened.model;
    this.text = text;
    this.original = text;
    for (const id of elementIds(text).keys()) this.ever.add(id);
  }

  step(edit: (m: DocModel) => Edited, deleted: number[] = []): this {
    const before = elementIds(this.text);
    const edited = edit(this.model);
    if (!edited.ok) throw new Error(edited.error);
    this.model = edited.model;
    if (edited.sel) this.sel = edited.sel;
    this.text = write(this.source, this.model, this.ids).text;
    const after = elementIds(this.text);
    for (const [id, kind] of before) {
      if (deleted.includes(idOfKey(id))) {
        expect(after.has(id), `element ${id} was deleted`).toBe(false);
        continue;
      }
      expect(after.get(id), `element ${id} (${kind}) keeps its localId`).toBe(kind);
    }
    for (const id of after.keys()) {
      if (!before.has(id)) expect(this.ever.has(id), `the new localId ${id} was never used before in this run`).toBe(false);
      this.ever.add(id);
    }
    expect(header(this.text)).toBe(header(this.original));
    expect(mostLinesIntoOneInput(this.text), 'an input takes one line').toBeLessThanOrEqual(1);
    expect(open(this.text).ok).toBe(true);
    expect(readFbd(this.text).ok).toBe(true);
    return this;
  }

  get id(): number {
    if (this.sel?.kind !== 'element') throw new Error('no element is selected');
    return this.sel.id;
  }

  /** Network `n` as the document says when read again. */
  tree(n: number, pou = 0): string {
    const opened = open(this.text);
    if (!opened.ok) throw new Error(opened.error);
    const net = opened.model.pous[pou]?.networks[n];
    if (!net) return 'none';
    if (net.why !== null) return `kept: ${net.why}`;
    return show(net.root);
  }
}

/** A tree as text: `TYPE[instance](IN1:=a, IN2:=OR(…)) => x, y`. */
function show(root: Root | null): string {
  if (!root) return 'empty';
  const src = (s: Src | null): string => (s === null ? '_' : s.t === 'operand' ? `${s.negated ? 'NOT ' : ''}${s.text}` : box(s.box) + (s.output === 'OUT' || s.output === 'Q' ? '' : `.${s.output}`));
  const box = (b: Box): string =>
    `${b.typeName || '??'}${b.instance ? `[${b.instance}]` : ''}(${b.inputs.map((p) => `${p.negated ? '/' : ''}${p.formal}:=${src(p.src)}`).join(', ')})`;
  const sinks = (b: Box) =>
    b.outputs
      .flatMap((o) => o.to.map((t) => `${o.formal}${t.kind === 'assign' ? (t.negated ? ' =/ ' : ' => ') : t.kind === 'jump' ? ' >> ' : ' '}${t.kind === 'return' ? 'RETURN' : t.text}`))
      .join('; ');
  if (root.t === 'sink') return `${src(root.src)} ${root.sink.kind === 'assign' ? '=>' : root.sink.kind === 'jump' ? '>>' : ''} ${root.sink.kind === 'return' ? 'RETURN' : root.sink.text}`.trim();
  const nested = boxesOf(root)
    .slice(1)
    .filter((b) => b.outputs.some((o) => o.to.length > 0))
    .map((b) => ` | ${b.typeName}: ${sinks(b)}`)
    .join('');
  return `${box(root.box)} ${sinks(root.box)}${nested}`.trim();
}

const network = (n: number): Sel => ({ kind: 'network', pou: 0, net: n });
const element = (n: number, id: number): Sel => ({ kind: 'element', pou: 0, net: n, id });
const pin = (n: number, box: number, formal: string): Extract<Sel, { kind: 'pin' }> => ({ kind: 'pin', pou: 0, net: n, box, formal });

function fixture(name: string, text: string): void {
  const path = resolve(FIXTURES, name);
  // The fixture is what the Rust tests compile; one that is missing is written only on request,
  // so deleting it cannot turn into a test that compares a document with itself.
  if (process.env.MYTHOS_UPDATE_FIXTURES === '1') writeFileSync(path, text);
  expect(existsSync(path), `${path} is missing; MYTHOS_UPDATE_FIXTURES=1 writes it`).toBe(true);
  expect(text).toBe(readFileSync(path, 'utf8'));
}

const declareAll = (r: Run, list: ReadonlyArray<readonly [string, string, 'inputVars' | 'outputVars' | 'localVars']>) => {
  for (const [name, type, section] of list) r.step((m) => declareVariable(m, 0, { name, type, section }, r.source.pous[0]?.vars ?? []));
};

/**
 * The mixer, built as an engineer builds it: a seal-in from AND, an OR joined where a second line
 * would have gone, and a negated input; a timer with both outputs assigned; boxes nested in a
 * box; a jump over a network to a label.
 */
function mixer(): Run {
  const r = new Run(TEMPLATE('Mixer'));
  // 1. run := (start OR run) AND NOT stop. AND first, its inputs typed; then the seal joined in
  // with an OR where a second line into IN1 would have gone.
  r.step((m) => addNetwork(m, 0, 0));
  r.step((m) => setHeader(m, r.ids, 0, 0, 'title', 'Run seal-in'));
  r.step((m) => insert(m, r.ids, network(0), { kind: 'box', type: 'AND' }, []));
  const and = r.id;
  r.step((m) => setPinOperand(m, r.ids, pin(0, and, 'IN1'), 'start'));
  r.step((m) => setPinOperand(m, r.ids, pin(0, and, 'IN2'), 'stop'));
  r.step((m) => setNegated(m, r.ids, 0, 0, { box: and, input: 'IN2' }, true));
  r.step((m) => insert(m, r.ids, element(0, and), { kind: 'assign' }, []));
  const run = r.id;
  r.step((m) => setSinkText(m, r.ids, 0, 0, run, 'run'));
  r.step((m) => orAt(m, r.ids, pin(0, and, 'IN1'), []));
  const sel = r.sel as Extract<Sel, { kind: 'pin' }>;
  expect(sel).toMatchObject({ kind: 'pin', formal: 'IN2' });
  r.step((m) => setPinOperand(m, r.ids, sel, 'run'));
  // 2. A timer: IN run, PT 50 ms; Q to lamp, ET to waited.
  r.step((m) => addNetwork(m, 0, 1));
  r.step((m) => setHeader(m, r.ids, 0, 1, 'title', 'Lamp 50 ms after run'));
  r.step((m) => insert(m, r.ids, network(1), { kind: 'box', type: 'TON' }, []));
  const ton = r.id;
  r.step((m) => setInstance(m, r.ids, 0, 1, ton, 'lampDelay'));
  r.step((m) => setPinOperand(m, r.ids, pin(1, ton, 'IN'), 'run'));
  r.step((m) => setPinOperand(m, r.ids, pin(1, ton, 'PT'), 'T#50ms'));
  r.step((m) => insert(m, r.ids, element(1, ton), { kind: 'assign' }, []));
  const lamp = r.id;
  r.step((m) => setSinkText(m, r.ids, 0, 1, lamp, 'lamp'));
  r.step((m) => addSink(m, r.ids, 0, 1, ton, 'ET'));
  const waited = r.id;
  r.step((m) => setSinkText(m, r.ids, 0, 1, waited, 'waited'));
  // 3. total := ADD(MUL(level, 2), 1): a box put into an input.
  r.step((m) => addNetwork(m, 0, 2));
  r.step((m) => insert(m, r.ids, network(2), { kind: 'box', type: 'ADD' }, []));
  const add = r.id;
  r.step((m) => setPinOperand(m, r.ids, pin(2, add, 'IN1'), 'level'));
  r.step((m) => insert(m, r.ids, pin(2, add, 'IN1'), { kind: 'box', type: 'MUL' }, []));
  const mul = r.id;
  r.step((m) => setPinOperand(m, r.ids, pin(2, mul, 'IN2'), '2'));
  r.step((m) => setPinOperand(m, r.ids, pin(2, add, 'IN2'), '1'));
  r.step((m) => insert(m, r.ids, element(2, add), { kind: 'assign' }, []));
  const total = r.id;
  r.step((m) => setSinkText(m, r.ids, 0, 2, total, 'total'));
  // 4. Over 100 the jump skips network 5; 5. low := TRUE; 6. labelled LATE: seen := level.
  r.step((m) => addNetwork(m, 0, 3));
  r.step((m) => insert(m, r.ids, network(3), { kind: 'box', type: 'GT' }, []));
  const gt = r.id;
  r.step((m) => setPinOperand(m, r.ids, pin(3, gt, 'IN1'), 'level'));
  r.step((m) => setPinOperand(m, r.ids, pin(3, gt, 'IN2'), '100'));
  r.step((m) => insert(m, r.ids, element(3, gt), { kind: 'jump' }, []));
  const jump = r.id;
  r.step((m) => setSinkText(m, r.ids, 0, 3, jump, 'LATE'));
  r.step((m) => addNetwork(m, 0, 4));
  r.step((m) => insert(m, r.ids, network(4), { kind: 'assign' }, []));
  const low = r.id;
  r.step((m) => setSinkText(m, r.ids, 0, 4, low, 'low'));
  r.step((m) => setPinOperand(m, r.ids, pin(4, low, ''), 'TRUE'));
  r.step((m) => addNetwork(m, 0, 5));
  r.step((m) => setLabel(m, r.ids, 0, 5, 'LATE'));
  r.step((m) => insert(m, r.ids, network(5), { kind: 'assign' }, []));
  const seen = r.id;
  r.step((m) => setSinkText(m, r.ids, 0, 5, seen, 'seen'));
  r.step((m) => setPinOperand(m, r.ids, pin(5, seen, ''), 'level'));
  declareAll(r, [
    ['start', 'BOOL', 'inputVars'],
    ['stop', 'BOOL', 'inputVars'],
    ['level', 'INT', 'inputVars'],
    ['run', 'BOOL', 'outputVars'],
    ['lamp', 'BOOL', 'outputVars'],
    ['total', 'INT', 'outputVars'],
    ['low', 'BOOL', 'outputVars'],
    ['seen', 'INT', 'outputVars'],
    ['lampDelay', 'TON', 'localVars'],
    ['waited', 'TIME', 'localVars'],
  ]);
  return r;
}

/**
 * A library function block and a library function put as the editor puts them, with the boxes
 * the module's answer gives (libraryanswer.ts): Motor with its instance and both outputs
 * assigned; Scale, its OUT assigned. An FBD box has no EN or ENO of its own.
 */
function withLibrary(): Run {
  const own = motorLibrary();
  const r = new Run(TEMPLATE('Main'));
  r.step((m) => addNetwork(m, 0, 0));
  r.step((m) => insert(m, r.ids, network(0), { kind: 'box', type: 'Motor' }, own));
  const motor = r.id;
  r.step((m) => setInstance(m, r.ids, 0, 0, motor, 'motor1'));
  r.step((m) => setPinOperand(m, r.ids, pin(0, motor, 'RUN'), 'start'));
  r.step((m) => setPinOperand(m, r.ids, pin(0, motor, 'SPEED'), 'speed'));
  r.step((m) => insert(m, r.ids, element(0, motor), { kind: 'assign' }, own));
  const running = r.id;
  r.step((m) => setSinkText(m, r.ids, 0, 0, running, 'running'));
  r.step((m) => addSink(m, r.ids, 0, 0, motor, 'RPM'));
  const rpm = r.id;
  r.step((m) => setSinkText(m, r.ids, 0, 0, rpm, 'rpm'));
  r.step((m) => addNetwork(m, 0, 1));
  r.step((m) => insert(m, r.ids, network(1), { kind: 'box', type: 'Scale' }, own));
  const scale = r.id;
  r.step((m) => setPinOperand(m, r.ids, pin(1, scale, 'RAW'), 'rpm'));
  r.step((m) => setPinOperand(m, r.ids, pin(1, scale, 'FACTOR'), '3'));
  r.step((m) => insert(m, r.ids, element(1, scale), { kind: 'assign' }, own));
  const scaled = r.id;
  r.step((m) => setSinkText(m, r.ids, 0, 1, scaled, 'scaled'));
  declareAll(r, [
    ['start', 'BOOL', 'inputVars'],
    ['speed', 'INT', 'inputVars'],
    ['running', 'BOOL', 'outputVars'],
    ['rpm', 'INT', 'outputVars'],
    ['scaled', 'INT', 'outputVars'],
    ['motor1', 'Motor', 'localVars'],
  ]);
  return r;
}

describe('the compiler runs what the editor writes with the library', () => {
  it('writes a library function block and a library function the compiler tests run (editor_library.xml)', () => {
    const r = withLibrary();
    expect([r.tree(0), r.tree(1)]).toEqual([
      'Motor[motor1](RUN:=start, SPEED:=speed) RUNNING => running; RPM => rpm',
      'Scale(RAW:=rpm, FACTOR:=3) OUT => scaled',
    ]);
    expect(r.text).toContain('<variable name="motor1"><type><derived name="Motor"/></type></variable>');
    const opened = open(r.text);
    if (!opened.ok) throw new Error(opened.error);
    const own = motorLibrary();
    const problems = check(opened.model.pous[0] as DocModel['pous'][number], opened.source.pous[0]?.vars ?? [], own, { boxes: own, why: null });
    expect([...problems.elements.values(), ...problems.networks.values()].flat()).toEqual([]);
    fixture('../fbd_library/editor_library.xml', r.text);
  });
});

describe('reading and writing back', () => {
  it('writes every FBD document of the corpus back byte for byte when nothing changed', () => {
    const files = readdirSync(FIXTURES).filter((f) => f.endsWith('.xml'));
    expect(files).toContain('blinker.xml');
    for (const f of files) {
      const text = readFileSync(resolve(FIXTURES, f), 'utf8');
      const opened = open(text);
      if (!opened.ok) throw new Error(`${f}: ${opened.error}`);
      expect(write(opened.source, opened.model, { next: opened.source.firstFree, used: new Set() }).text, f).toBe(text);
    }
  });

  it('writes a new program back byte for byte when nothing changed, and opens it with no network', () => {
    const r = new Run(TEMPLATE('Main'));
    expect(write(r.source, r.model, r.ids).text).toBe(TEMPLATE('Main'));
    expect(r.model.pous[0]?.networks).toEqual([]);
  });

  it('reads the blinker: the trees it draws, and each network it keeps as found with why', () => {
    const r = new Run(readFileSync(resolve(FIXTURES, 'blinker.xml'), 'utf8'));
    const nets = r.model.pous[0]?.networks ?? [];
    expect(nets.map((n) => (n.why === null ? show(n.root) : `kept: ${n.why}`))).toEqual([
      'TON[t1](/IN:=t2.Q, PT:=T#40ms) Q => blink',
      'TON[t2](IN:=blink, PT:=T#40ms)',
      'CTU[c](CU:=blink, R:=done, PV:=5) Q => done; CV => count',
      expect.stringMatching(/^kept: its lines run in a loop through block 41 \(a line drawn back\)/),
      expect.stringMatching(/^kept: it carries a line through the connector LVL/),
      'toggle >> SKIP',
      'ADD(EN:=blink, IN1:=steps, IN2:=1) ENO =/ held; OUT => steps',
      // The label SKIP after network 7: a network with no element.
      'empty',
    ]);
    expect(nets[7]?.label).toBe('SKIP');
    expect(nets[0]?.comment).toBe('Two timers, each starting the other: blink is TRUE for 40 ms, FALSE for 40 ms.');
  });
});

describe('networks', () => {
  it('adds, titles, comments, labels, moves and deletes networks, each kept in TC6', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'assign' }, []));
    const one = r.id;
    r.step((m) => setSinkText(m, r.ids, 0, 0, one, 'one'));
    r.step((m) => setPinOperand(m, r.ids, pin(0, one, ''), 'TRUE'));
    r.step((m) => addNetwork(m, 0, 1));
    r.step((m) => insert(m, r.ids, network(1), { kind: 'assign' }, []));
    const two = r.id;
    r.step((m) => setSinkText(m, r.ids, 0, 1, two, 'two'));
    r.step((m) => setHeader(m, r.ids, 0, 1, 'title', 'The second'));
    r.step((m) => setHeader(m, r.ids, 0, 1, 'comment', 'Why it is there'));
    r.step((m) => setLabel(m, r.ids, 0, 1, 'SECOND'));
    expect(r.text).toContain('<data name="urn:mythos:logic:ld-network" handleUnknown="preserve"><network xmlns="urn:mythos:logic:ld-network" title="The second"/></data>');
    expect(r.text).toMatch(/<label localId="\d+" height="20" width="\d+" label="SECOND">/);
    r.step((m) => moveNetwork(m, 0, 1, -1));
    expect([r.tree(0), r.tree(1)]).toEqual(['_ => two', 'TRUE => one']);
    const again = open(r.text);
    expect(again.ok && again.model.pous[0]?.networks[0]?.title).toBe('The second');
    expect(again.ok && again.model.pous[0]?.networks[0]?.label).toBe('SECOND');
    const header0 = r.model.pous[0]?.networks[0];
    r.step((m) => remove(m, network(0)), [two, header0?.commentId ?? -1, header0?.labelId ?? -1]);
    expect(r.tree(0)).toBe('TRUE => one');
  });
});

describe('boxes, operands and assignments', () => {
  it('builds the mixer the compiler tests run (editor_mixer.xml)', () => {
    const r = mixer();
    expect([0, 1, 2, 3, 4, 5].map((n) => r.tree(n))).toEqual([
      'AND(IN1:=OR(IN1:=start, IN2:=run), /IN2:=stop) OUT => run',
      'TON[lampDelay](IN:=run, PT:=T#50ms) Q => lamp; ET => waited',
      'ADD(IN1:=MUL(IN1:=level, IN2:=2), IN2:=1) OUT => total',
      'GT(IN1:=level, IN2:=100) OUT >> LATE',
      'TRUE => low',
      'level => seen',
    ]);
    // Laid out on the grid, the box after the boxes it reads, its operands left of it.
    const doc = readFbd(r.text);
    if (!doc.ok) throw new Error(doc.error);
    const el = (id: number) => doc.pous[0]?.elements.find((e) => e.localId === id);
    for (const e of doc.pous[0]?.elements ?? []) expect([e.x % 10, e.y % 10], `${e.kind} ${e.localId} on the grid`).toEqual([0, 0]);
    const opened = open(r.text);
    const add = opened.ok ? opened.model.pous[0]?.networks[2]?.root : null;
    if (add?.t !== 'box' || add.box.inputs[0]?.src?.t !== 'box') throw new Error('no nested MUL');
    const mul = add.box.inputs[0].src.box;
    expect((el(mul.id)?.x ?? 0) + (el(mul.id)?.width ?? 0)).toBeLessThan(el(add.box.id)?.x ?? 0);
    // Nothing the editor's own check says is wrong.
    const model = opened.ok ? opened.model : null;
    const vars = opened.ok ? (opened.source.pous[0]?.vars ?? []) : [];
    const problems = check(model?.pous[0] as DocModel['pous'][number], vars, []);
    expect([...problems.elements.values(), ...problems.networks.values()].flat()).toEqual([]);
    fixture('editor_mixer.xml', r.text);
  });

  it('never writes a second line into one input: an operand typed where a box feeds is refused, and an OR joins both', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'box', type: 'AND' }, []));
    const and = r.id;
    r.step((m) => insert(m, r.ids, pin(0, and, 'IN1'), { kind: 'box', type: 'GT' }, []));
    const gt = r.id;
    const refused = setPinOperand(r.model, r.ids, pin(0, and, 'IN1'), 'a');
    expect(refused).toEqual({
      ok: false,
      error: `IN1 takes the output of block ${gt} (GT); an input takes one line (IEC 61131-3 ed. 3 §8.3.2). Join both with an OR box, or delete the block first.`,
    });
    r.step((m) => orAt(m, r.ids, pin(0, and, 'IN1'), []));
    r.step((m) => setPinOperand(m, r.ids, r.sel as Extract<Sel, { kind: 'pin' }>, 'a'));
    expect(r.tree(0)).toBe('AND(IN1:=OR(IN1:=GT(IN1:=_, IN2:=_), IN2:=a), IN2:=_)');
    // An OR joins BOOLs only.
    r.step((m) => addNetwork(m, 0, 1));
    r.step((m) => insert(m, r.ids, network(1), { kind: 'box', type: 'ADD' }, []));
    const add = r.id;
    r.step((m) => insert(m, r.ids, pin(1, add, 'IN1'), { kind: 'box', type: 'MUL' }, []));
    expect(orAt(r.model, r.ids, pin(1, add, 'IN1'), [])).toEqual({ ok: false, error: 'An OR joins BOOL signals; IN1 takes a ANY_NUM.' });
  });

  it('puts a box after a box, between a box and its assignment, and into an assignment that reads an operand', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'assign' }, []));
    const q = r.id;
    r.step((m) => setSinkText(m, r.ids, 0, 0, q, 'q'));
    r.step((m) => setPinOperand(m, r.ids, pin(0, q, ''), 'a'));
    expect(r.tree(0)).toBe('a => q');
    // A box at the assignment: the operand goes into it, the assignment is written from it.
    r.step((m) => insert(m, r.ids, element(0, q), { kind: 'box', type: 'AND' }, []));
    const and = r.id;
    expect(r.tree(0)).toBe('AND(IN1:=a, IN2:=_) OUT => q');
    // After the box (as with the network selected): it goes on into the new one.
    r.step((m) => insert(m, r.ids, network(0), { kind: 'box', type: 'NOT' }, []));
    const not = r.id;
    expect(r.tree(0)).toBe('NOT(IN:=AND(IN1:=a, IN2:=_)) OUT => q');
    // Deleted, the NOT gives its place back to what it read.
    r.step((m) => remove(m, element(0, not)), [not]);
    expect(r.tree(0)).toBe('AND(IN1:=a, IN2:=_) OUT => q');
    // Three inputs, then the empty third one removed again.
    r.step((m) => addInput(m, element(0, and)));
    expect(r.tree(0)).toBe('AND(IN1:=a, IN2:=_, IN3:=_) OUT => q');
    r.step((m) => remove(m, pin(0, and, 'IN3')));
    expect(r.tree(0)).toBe('AND(IN1:=a, IN2:=_) OUT => q');
    // Deleted, the AND leaves its assignment reading what its first input read.
    const a = (() => {
      const o = open(r.text);
      const root = o.ok ? o.model.pous[0]?.networks[0]?.root : null;
      return root?.t === 'box' && root.box.inputs[0]?.src?.t === 'operand' ? root.box.inputs[0].src.varId : null;
    })();
    r.step((m) => remove(m, element(0, and)), [and]);
    expect(r.tree(0)).toBe('a => q');
    // The operand's variable kept its id through all of it.
    expect(r.text).toContain(`<inVariable localId="${a}"`);
  });

  it('never gives out an id twice, after a deletion, an undo, or a save and a new session', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'box', type: 'OR' }, []));
    const first = r.id;
    const before = r.model;
    r.step((m) => remove(m, element(0, first)), [first]);
    // Undone: the model before is written again, and the next id is past the one given out.
    r.model = before;
    r.text = write(r.source, before, r.ids).text;
    r.step((m) => insert(m, r.ids, network(0), { kind: 'box', type: 'AND' }, []));
    expect(r.id).toBeGreaterThan(first);
    // A new session reads the record the editor left in the POU's addData.
    const saved = r.text;
    expect(saved).toMatch(/<data name="urn:mythos:logic:ld-editor" handleUnknown="preserve"><localIds xmlns="urn:mythos:logic:ld-editor" next="\d+"\/><\/data>/);
    const next = new Run(saved);
    next.step((m) => addNetwork(m, 0, 1));
    next.step((m) => insert(m, next.ids, network(1), { kind: 'return' }, []));
    expect(next.id).toBeGreaterThan(r.id);
  });
});

describe('a document from another tool', () => {
  const blinker = () => readFileSync(resolve(FIXTURES, 'blinker.xml'), 'utf8');

  it('edits one network and writes each network it keeps as found byte for byte', () => {
    const r = new Run(blinker());
    const original = parse(r.original);
    const textOf = (id: number) => {
      let found: XNode | null = null;
      const visit = (n: XNode) => {
        if (n.attrs.some((a) => a.name === 'localId' && a.value === String(id))) found = n;
        n.children.forEach(visit);
      };
      visit(original);
      const n = found as XNode | null;
      if (!n) throw new Error(`no element ${id}`);
      return r.original.slice(n.start, n.end);
    };
    // Network 2: t2's Q assigned to a new variable, and a title given.
    const t2 = 21;
    r.step((m) => addSink(m, r.ids, 0, 1, t2, 'Q'));
    const q = r.id;
    r.step((m) => setSinkText(m, r.ids, 0, 1, q, 'pulse'));
    r.step((m) => setHeader(m, r.ids, 0, 1, 'title', 'The second timer'));
    r.step((m) => declareVariable(m, 0, { name: 'pulse', type: 'BOOL', section: 'localVars' }, r.source.pous[0]?.vars ?? []));
    expect(r.tree(1)).toBe('TON[t2](IN:=blink, PT:=T#40ms) Q => pulse');
    // The network above it, as the document had it, byte for byte.
    for (const id of [10, 11, 12]) expect(r.text, `element ${id} as found`).toContain(textOf(id));
    // The networks below it, the ones it keeps as found (the feedback path, the connector) among
    // them: each moved down whole (each element of a network by the same distance, none up), and
    // otherwise byte for byte.
    const doc = readFbd(r.text);
    const before = readFbd(r.original);
    if (!doc.ok || !before.ok) throw new Error('unreadable');
    const yOf = (pou: (typeof doc.pous)[number] | undefined, id: number) => pou?.elements.find((e) => e.localId === id)?.y ?? NaN;
    const unplaced = (t: string) => t.replace(/<position [^>]*\/>/g, '<position/>');
    for (const ids of [[30, 31, 34, 35, 36], [40, 41, 42], [50, 51, 52, 53, 54, 55], [60, 61], [70, 71, 72, 73, 74], [80]]) {
      const dy = yOf(doc.pous[0], ids[0] as number) - yOf(before.pous[0], ids[0] as number);
      expect(dy, `network of ${ids[0]} moved down`).toBeGreaterThan(0);
      for (const id of ids) {
        expect(yOf(doc.pous[0], id) - yOf(before.pous[0], id), `element ${id} moved with its network`).toBe(dy);
        expect(unplaced(r.text), `element ${id} as found`).toContain(unplaced(textOf(id)));
      }
    }
    // The XML comment at the top of the file, and the body's comment, as found.
    expect(r.text).toContain('A Function Block Diagram that drives itself');
    expect(r.text).toContain(textOf(1));
    // A kept network cannot take an instruction, and says why.
    const kept = insert(r.model, r.ids, network(3), { kind: 'box', type: 'AND' }, []);
    expect(kept.ok ? '' : kept.error).toMatch(/^Network 4 is kept as the document has it \(its lines run in a loop/);
    fixture('editor_blinker_edited.xml', r.text);
  });

  it('keeps a network with two lines into one input as found, byte for byte, and moves it whole', () => {
    const twice = TEMPLATE('Main').replace(
      '<FBD/>',
      `<FBD>
            <inVariable localId="1"><position x="0" y="0"/><connectionPointOut/><expression>a</expression></inVariable>
            <inVariable localId="2"><position x="0" y="40"/><connectionPointOut/><expression>b</expression></inVariable>
            <outVariable localId="3"><position x="200" y="0"/><connectionPointIn><connection refLocalId="1"/><connection refLocalId="2"/></connectionPointIn><expression>q</expression></outVariable>
          </FBD>`
    );
    const r = new Run(twice);
    expect(r.tree(0)).toBe('kept: the input of outVariable 3 takes 2 lines; an input takes one (IEC 61131-3 ed. 3 §8.3.2)');
    // A network added above it: the kept one moves down whole, its lines as they were.
    r.model = { ...r.model };
    const edited = addNetwork(r.model, 0, 0);
    if (!edited.ok) throw new Error(edited.error);
    r.model = edited.model;
    const ins = insert(r.model, r.ids, network(0), { kind: 'assign' }, []);
    if (!ins.ok) throw new Error(ins.error);
    r.model = ins.model;
    r.text = write(r.source, r.model, r.ids).text;
    expect(r.text).toContain('<connectionPointIn><connection refLocalId="1"/><connection refLocalId="2"/></connectionPointIn><expression>q</expression>');
    expect(r.tree(1)).toMatch(/^kept: the input of outVariable 3 takes 2 lines/);
  });
});

describe('the editor’s own check', () => {
  it('says what is wrong at the element: an undeclared operand, a missing input, an instance, a value of another family', () => {
    const r = new Run(TEMPLATE('Main').replace('<interface/>', '<interface><localVars><variable name="n"><type><INT/></type></variable></localVars></interface>'));
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'box', type: 'AND' }, []));
    const and = r.id;
    r.step((m) => setPinOperand(m, r.ids, pin(0, and, 'IN1'), 'nosuch'));
    r.step((m) => setPinOperand(m, r.ids, pin(0, and, 'IN2'), 'n'));
    r.step((m) => addNetwork(m, 0, 1));
    r.step((m) => insert(m, r.ids, network(1), { kind: 'box', type: 'TON' }, []));
    const ton = r.id;
    r.step((m) => addNetwork(m, 0, 2));
    r.step((m) => insert(m, r.ids, network(2), { kind: 'assign' }, []));
    const q = r.id;
    const opened = open(r.text);
    if (!opened.ok) throw new Error(opened.error);
    const problems = check(opened.model.pous[0] as DocModel['pous'][number], opened.source.pous[0]?.vars ?? [], []);
    expect(problems.elements.get(and)).toEqual(['IN1: nosuch is not declared.', 'IN2: n is INT; this takes a BOOL.']);
    expect(problems.elements.get(ton)).toEqual(['A TON needs an instance: name the variable that keeps its state.']);
    expect(problems.elements.get(q)).toEqual(['It has no operand.', 'It reads nothing: give it an operand, or put a box before it.']);
    expect(problems.networks.get(0)).toEqual(['Nothing uses what the network computes: assign the output of its last box.']);
  });
});
