/// <reference types="node" />
// The Ladder Diagram editor's model (ADR-013 L3b-2b): every edit, asserted on the TC6 text it
// writes. Each step is checked for what an online change and another tool rely on: every
// element the step did not delete keeps its localId; a new element takes an id no element of
// the document has had in this edit's history; and the bytes outside the edited body, the
// file header and the content header among them, are the document's own.
//
// The documents the editor writes here are the fixtures the compiler's tests run
// (crates/mythos-logic-compiler/tests/ladder/editor_*.xml, `ladder_from_the_editor.rs`), so
// what is written is what is compiled and run. Set MYTHOS_UPDATE_FIXTURES=1 to write them anew.

import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  addNetwork,
  branch,
  declareVariable,
  elementsOf,
  findEl,
  insert,
  moveNetwork,
  remove,
  setBoxType,
  setHeader,
  setInstance,
  setLabel,
  setOperand,
  setPin,
  setVariant,
  type Edited,
  type Sel,
} from '../ldedit';
import { idsFor, NETWORK_DATA, open, ownBoxes, write, type DocModel, type Ids, type Item, type Source } from '../ldmodel';
import { parseXml, type XNode } from '../ldxml';
import { readLadder } from '../ladder';

const FIXTURES = resolve(__dirname, '../../../../../crates/mythos-logic-compiler/tests/ladder');

/** The logic module's template for a new LD program (plugins/logic `template`), with its version and date fixed. */
const TEMPLATE = (name: string) => `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <fileHeader companyName="" productName="MythOS logic" productVersion="0.3.0" creationDateTime="2026-11-18T00:00:00"/>
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

/** Every element of the document, by its POU and localId (`Main:12`), with its kind. A localId is the body's. */
function elementIds(text: string): Map<string, string> {
  const parsed = parseXml(text);
  if (!parsed.ok) throw new Error(parsed.error);
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
  visit(parsed.root, '');
  return out;
}

const idOfKey = (key: string) => Number(key.slice(key.lastIndexOf(':') + 1));

/** The bytes before `<types>` and after `</types>`: the header, as the document has them. */
function header(text: string): string {
  const start = text.search(/<(\w+:)?types>/);
  const end = text.search(/<\/(\w+:)?types>/);
  return text.slice(0, start) + text.slice(end);
}

/**
 * A run of edits on one document, as the editor makes them: each step's text checked against
 * the one before it.
 */
class Run {
  readonly source: Source;
  readonly ids: Ids;
  model: DocModel;
  text: string;
  sel: Sel | null = null;
  /** Every element any text of this run has held, by POU and localId. */
  readonly ever = new Set<string>();
  readonly original: string;

  constructor(text: string) {
    const opened = open(text);
    if (!opened.ok) throw new Error(opened.error);
    this.source = opened.source;
    this.ids = idsFor(opened.source);
    this.model = opened.model;
    this.text = text;
    this.original = text;
    for (const id of elementIds(text).keys()) this.ever.add(id);
  }

  /** Apply an edit; check what it wrote. `deleted`: the ids the step removes on purpose. */
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
      // Kept, with its id and its kind: never renumbered, never given to another element.
      expect(after.get(id), `element ${id} (${kind}) keeps its localId`).toBe(kind);
    }
    for (const id of after.keys()) {
      if (!before.has(id)) expect(this.ever.has(id), `the new localId ${id} was never used before in this edit`).toBe(false);
      this.ever.add(id);
    }
    expect(header(this.text)).toBe(header(this.original));
    // What is written opens again, and the viewer reads it.
    expect(open(this.text).ok).toBe(true);
    expect(readLadder(this.text).ok).toBe(true);
    return this;
  }

  /** The id of the selected element. */
  get id(): number {
    if (this.sel?.kind !== 'element') throw new Error('no element is selected');
    return this.sel.id;
  }

  /** The rung of network `n` as the document says when read again: kinds, operands, parallels. */
  rung(n: number, pou = 0): string {
    const opened = open(this.text);
    if (!opened.ok) throw new Error(opened.error);
    const net = opened.model.pous[pou]?.networks[n];
    if (!net) return 'none';
    if (net.rung === null) return `kept: ${net.why}`;
    const show = (items: Item[]): string =>
      items
        .map((i) =>
          i.t === 'par'
            ? `(${i.branches.map(show).join(' | ')})`
            : i.el.kind === 'block'
              ? `${i.el.typeName}[${i.el.instance}${i.el.inputs.filter((p) => p.operand).map((p) => ` ${p.formal}:=${p.operand}`).join('')}${i.el.outputs.filter((p) => p.operand).map((p) => ` ${p.formal}=>${p.operand}`).join('')}]`
              : `${i.el.kind}${i.el.negated ? '/' : ''}${i.el.edge ? `:${i.el.edge}` : ''}${i.el.storage ? `:${i.el.storage}` : ''}${i.el.operand ? ` ${i.el.operand}` : ''}`
        )
        .join(' ');
    return show(net.rung);
  }
}

const network = (n: number): Sel => ({ kind: 'network', pou: 0, net: n });
const element = (n: number, id: number, to: number | null = null): Sel => ({ kind: 'element', pou: 0, net: n, id, to });

/** The connections into element `id` of a text, each `from` or `from.formal`. */
function inputsOf(text: string, id: number): string[] {
  const parsed = parseXml(text);
  if (!parsed.ok) throw new Error(parsed.error);
  let found: XNode | null = null;
  const visit = (n: XNode) => {
    if (n.attrs.some((a) => a.name === 'localId' && a.value === String(id))) found = n;
    n.children.forEach(visit);
  };
  visit(parsed.root);
  const out: string[] = [];
  const conns = (n: XNode) => {
    if (n.name === 'connection') {
      const from = n.attrs.find((a) => a.name === 'refLocalId')?.value ?? '?';
      const formal = n.attrs.find((a) => a.name === 'formalParameter')?.value;
      out.push(formal ? `${from}.${formal}` : from);
    }
    n.children.forEach(conns);
  };
  if (found) conns(found);
  return out;
}

/** The seal-in rung and a timer rung, built as an engineer builds them. */
function sealIn(): Run {
  const r = new Run(TEMPLATE('Motor'));
  r.step((m) => addNetwork(m, 0, 0));
  r.step((m) => setHeader(m, r.ids, 0, 0, 'title', 'Motor seal-in'));
  r.step((m) => setHeader(m, r.ids, 0, 0, 'comment', 'Start latches the motor; stop drops it.'));
  r.step((m) => insert(m, r.ids, network(0), { kind: 'contact' }, []));
  const start = r.id;
  r.step((m) => setOperand(m, r.ids, 0, start, 'start'));
  r.step((m) => insert(m, r.ids, element(0, start), { kind: 'contact', negated: true }, []));
  const stop = r.id;
  r.step((m) => setOperand(m, r.ids, 0, stop, 'stop'));
  r.step((m) => insert(m, r.ids, element(0, stop), { kind: 'coil' }, []));
  const coil = r.id;
  r.step((m) => setOperand(m, r.ids, 0, coil, 'motor'));
  r.step((m) => branch(m, element(0, start)));
  r.step((m) => insert(m, r.ids, r.sel, { kind: 'contact' }, []));
  const seal = r.id;
  r.step((m) => setOperand(m, r.ids, 0, seal, 'motor'));
  r.step((m) => addNetwork(m, 0, 1));
  r.step((m) => setHeader(m, r.ids, 0, 1, 'title', 'Running lamp after 50 ms'));
  r.step((m) => insert(m, r.ids, network(1), { kind: 'contact' }, []));
  const running = r.id;
  r.step((m) => setOperand(m, r.ids, 0, running, 'motor'));
  r.step((m) => insert(m, r.ids, element(1, running), { kind: 'box', type: 'TON' }, []));
  const timer = r.id;
  r.step((m) => setInstance(m, r.ids, 0, timer, 'lampDelay'));
  r.step((m) => setPin(m, r.ids, 0, timer, 'inputs', 'PT', 'T#50ms'));
  r.step((m) => setPin(m, r.ids, 0, timer, 'outputs', 'ET', 'waited'));
  r.step((m) => insert(m, r.ids, element(1, timer), { kind: 'coil' }, []));
  const lamp = r.id;
  r.step((m) => setOperand(m, r.ids, 0, lamp, 'lamp'));
  for (const [name, type, section] of [
    ['start', 'BOOL', 'inputVars'],
    ['stop', 'BOOL', 'inputVars'],
    ['motor', 'BOOL', 'outputVars'],
    ['lamp', 'BOOL', 'outputVars'],
    ['lampDelay', 'TON', 'localVars'],
    ['waited', 'TIME', 'localVars'],
  ] as const) {
    r.step((m) => declareVariable(m, 0, { name, type, section }, r.source.pous[0]?.vars ?? []));
  }
  return r;
}

/** `text` equals the committed fixture `name`; written anew with MYTHOS_UPDATE_FIXTURES=1. */
function fixture(name: string, text: string): void {
  const path = resolve(FIXTURES, name);
  if (process.env.MYTHOS_UPDATE_FIXTURES === '1' || !existsSync(path)) writeFileSync(path, text);
  expect(text).toBe(readFileSync(path, 'utf8'));
}

describe('networks', () => {
  it('adds, titles, comments, labels, moves and deletes networks, each kept in TC6', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'coil' }, []));
    const first = r.id;
    r.step((m) => setOperand(m, r.ids, 0, first, 'one'));
    r.step((m) => addNetwork(m, 0, 1));
    r.step((m) => insert(m, r.ids, network(1), { kind: 'coil' }, []));
    const second = r.id;
    r.step((m) => setOperand(m, r.ids, 0, second, 'two'));
    r.step((m) => setHeader(m, r.ids, 0, 1, 'title', 'Second'));
    r.step((m) => setHeader(m, r.ids, 0, 1, 'comment', 'Line one\nLine <two> & "three"'));
    r.step((m) => setLabel(m, r.ids, 0, 1, 'NEXT'));
    // Inserted above the first: it comes first in the order of evaluation.
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'coil' }, []));
    const zero = r.id;
    r.step((m) => setOperand(m, r.ids, 0, zero, 'zero'));
    const opened = open(r.text);
    if (!opened.ok) throw new Error(opened.error);
    expect(opened.model.pous[0]?.networks.map((n) => [n.title, n.comment, n.label, n.rung?.length])).toEqual([
      ['', '', '', 1],
      ['', '', '', 1],
      ['Second', 'Line one\nLine <two> & "three"', 'NEXT', 1],
    ]);
    // The title is in the comment's addData, which other tools ignore, and the comment is TC6's.
    expect(r.text).toContain(`<data name="${NETWORK_DATA}" handleUnknown="preserve"><network xmlns="${NETWORK_DATA}" title="Second"/></data>`);
    expect(r.text).toContain('Line one\nLine &lt;two&gt; &amp; "three"</xhtml:p>');
    // The viewer and the front-end order the networks by height: zero, one, two.
    const ladder = readLadder(r.text);
    if (!ladder.ok) throw new Error(ladder.error);
    const y = (id: number) => ladder.pous[0]?.elements.find((e) => e.localId === id)?.y ?? NaN;
    expect(y(zero)).toBeLessThan(y(first));
    expect(y(first)).toBeLessThan(y(second));
    // Moved down, the first network runs after the second.
    r.step((m) => moveNetwork(m, 0, 0, 1));
    expect([r.rung(0), r.rung(1), r.rung(2)]).toEqual(['coil one', 'coil zero', 'coil two']);
    r.step((m) => moveNetwork(m, 0, 2, -1));
    expect([r.rung(0), r.rung(1), r.rung(2)]).toEqual(['coil one', 'coil two', 'coil zero']);
    // Deleted with its elements, and its comment and label with it.
    const header1 = open(r.text);
    if (!header1.ok) throw new Error(header1.error);
    const second1 = header1.model.pous[0]?.networks[1];
    r.step((m) => remove(m, network(1)), [second, second1?.commentId ?? -1, second1?.labelId ?? -1]);
    expect([r.rung(0), r.rung(1)]).toEqual(['coil one', 'coil zero']);
  });

  it('keeps a network that has only a title, and drops one with nothing in it', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => addNetwork(m, 0, 1));
    r.step((m) => setHeader(m, r.ids, 0, 1, 'title', 'To do'));
    const opened = open(r.text);
    if (!opened.ok) throw new Error(opened.error);
    expect(opened.model.pous[0]?.networks.map((n) => n.title)).toEqual(['To do']);
  });
});

describe('instructions', () => {
  it('puts every kind of contact and coil, and changes one kind into another', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    const kinds = [
      { kind: 'contact' },
      { kind: 'contact', negated: true },
      { kind: 'contact', edge: 'rising' },
      { kind: 'contact', edge: 'falling' },
      { kind: 'coil' },
      { kind: 'coil', negated: true },
      { kind: 'coil', storage: 'set' },
      { kind: 'coil', storage: 'reset' },
      { kind: 'coil', edge: 'rising' },
      { kind: 'coil', edge: 'falling' },
    ] as const;
    let at: Sel = network(0);
    kinds.forEach((spec, k) => {
      r.step((m) => insert(m, r.ids, at, spec, []));
      const id = r.id;
      r.step((m) => setOperand(m, r.ids, 0, id, `v${k}`));
      at = element(0, id);
    });
    expect(r.rung(0)).toBe(
      'contact v0 contact/ v1 contact:rising v2 contact:falling v3 coil v4 coil/ v5 coil:set v6 coil:reset v7 coil:rising v8 coil:falling v9'
    );
    // Written as TC6 says: negated, edge and storage attributes.
    expect(r.text).toMatch(/<contact localId="\d+" width="30" height="20" edge="rising">/);
    expect(r.text).toMatch(/<coil localId="\d+" width="30" height="20" storage="reset">/);
    // A normally open contact made normally closed keeps its id.
    const first = r.model.pous[0]?.networks[0]?.rung?.[0];
    const id = first?.t === 'el' ? first.el.id : -1;
    r.step((m) => setVariant(m, r.ids, 0, id, { negated: true, edge: null, storage: null }));
    expect(r.rung(0).startsWith('contact/ v0 contact/ v1')).toBe(true);
  });

  it('puts a contact before the coils when only the network is selected, and nothing after a jump', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'coil' }, []));
    const coil = r.id;
    r.step((m) => setOperand(m, r.ids, 0, coil, 'q'));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'contact' }, []));
    const a = r.id;
    r.step((m) => setOperand(m, r.ids, 0, a, 'a'));
    expect(r.rung(0)).toBe('contact a coil q');
    // A jump ends its rung; nothing is put after it; a label names the network it goes to.
    r.step((m) => addNetwork(m, 0, 1));
    r.step((m) => insert(m, r.ids, network(1), { kind: 'contact' }, []));
    const b = r.id;
    r.step((m) => setOperand(m, r.ids, 0, b, 'b'));
    r.step((m) => insert(m, r.ids, element(1, b), { kind: 'jump' }, []));
    const jump = r.id;
    r.step((m) => setOperand(m, r.ids, 0, jump, 'SKIP'));
    expect(insert(r.model, r.ids, element(1, jump), { kind: 'contact' }, []).ok).toBe(false);
    expect(insert(r.model, r.ids, element(1, b), { kind: 'return' }, []).ok).toBe(false);
    r.step((m) => addNetwork(m, 0, 2));
    r.step((m) => setLabel(m, r.ids, 0, 2, 'SKIP'));
    r.step((m) => insert(m, r.ids, network(2), { kind: 'return' }, []));
    expect(r.rung(1)).toBe('contact b jump SKIP');
    expect(r.rung(2)).toBe('return');
    expect(r.text).toMatch(/<jump localId="\d+" width="80" height="20" label="SKIP">/);
    expect(r.text).toMatch(/<label localId="\d+" height="20" width="80" label="SKIP">/);
  });

  it('puts boxes with their pins: a standard function block, a comparison, a block of the document', () => {
    const doc = TEMPLATE('Main').replace(
      '    </pous>',
      `      <pou name="Pulse" pouType="functionBlock">
        <interface>
          <inputVars><variable name="go"><type><BOOL/></type></variable><variable name="width"><type><TIME/></type></variable></inputVars>
          <outputVars><variable name="busy"><type><BOOL/></type></variable></outputVars>
        </interface>
        <body><LD><leftPowerRail localId="1"><position x="0" y="0"/><connectionPointOut formalParameter=""/></leftPowerRail></LD></body>
      </pou>
    </pous>`
    );
    const r = new Run(doc);
    const own = ownBoxes(r.source);
    expect(own.map((d) => `${d.name}(${d.inputs.map((p) => p.name).join(',')})`)).toEqual(['Pulse(GO,WIDTH)']);
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'box', type: 'CTU' }, own));
    const ctu = r.id;
    r.step((m) => setInstance(m, r.ids, 0, ctu, 'counter'));
    r.step((m) => setPin(m, r.ids, 0, ctu, 'inputs', 'R', 'reset'));
    r.step((m) => setPin(m, r.ids, 0, ctu, 'inputs', 'PV', '3'));
    r.step((m) => setPin(m, r.ids, 0, ctu, 'outputs', 'CV', 'count'));
    const cv = findEl(r.model, 0, ctu)?.el.outputs.find((p) => p.formal === 'CV')?.varId ?? -1;
    expect(cv).toBeGreaterThan(ctu);
    // The power goes in at CU and out at Q; the operands are at R, PV and as a variable at CV.
    expect(r.text).toContain('<variable formalParameter="R"><connectionPointIn><relPosition x="0" y="50"/><expression>reset</expression></connectionPointIn></variable>');
    expect(inputsOf(r.text, cv)).toEqual([`${ctu}.CV`]);
    // A comparison passes power at OUT, entered at EN.
    r.step((m) => insert(m, r.ids, element(0, ctu), { kind: 'box', type: 'GT' }, own));
    const gt = r.id;
    r.step((m) => setPin(m, r.ids, 0, gt, 'inputs', 'IN1', 'count'));
    r.step((m) => setPin(m, r.ids, 0, gt, 'inputs', 'IN2', '2'));
    expect(inputsOf(r.text, gt)).toEqual([`${ctu}.Q`]);
    r.step((m) => insert(m, r.ids, element(0, gt), { kind: 'box', type: 'Pulse' }, own));
    const pulse = r.id;
    r.step((m) => setInstance(m, r.ids, 0, pulse, 'p1'));
    expect(inputsOf(r.text, pulse)).toEqual([`${gt}.OUT`]);
    r.step((m) => insert(m, r.ids, element(0, pulse), { kind: 'coil' }, own));
    const coil = r.id;
    r.step((m) => setOperand(m, r.ids, 0, coil, 'done'));
    expect(inputsOf(r.text, coil)).toEqual([`${pulse}.BUSY`]);
    expect(r.rung(0)).toBe('CTU[counter R:=reset PV:=3 CV=>count] GT[ IN1:=count IN2:=2] Pulse[p1] coil done');
    // An empty box takes a type later; a type it is given keeps the pins it shares.
    r.step((m) => insert(m, r.ids, element(0, ctu), { kind: 'box', type: '' }, own));
    const box = r.id;
    r.step((m) => setBoxType(m, r.ids, 0, box, 'TOF', own));
    r.step((m) => setPin(m, r.ids, 0, box, 'inputs', 'PT', 'T#1s'));
    r.step((m) => setBoxType(m, r.ids, 0, box, 'TP', own));
    expect(findEl(r.model, 0, box)?.el.inputs.map((p) => `${p.formal}=${p.operand}`)).toEqual(['IN=', 'PT=T#1s']);
    // A cleared output operand takes its variable away; it is not given out again.
    r.step((m) => setPin(m, r.ids, 0, ctu, 'outputs', 'CV', ''), [cv]);
    r.step((m) => setPin(m, r.ids, 0, ctu, 'outputs', 'CV', 'count'));
    expect(findEl(r.model, 0, ctu)?.el.outputs.find((p) => p.formal === 'CV')?.varId).not.toBe(cv);
  });
});

describe('parallel branches', () => {
  it('writes a branch around a selection as parallel, never as series', () => {
    const r = sealIn();
    expect(r.rung(0)).toBe('(contact start | contact motor) contact/ stop coil motor');
    const rung = r.model.pous[0]?.networks[0]?.rung ?? [];
    const [start, seal, stop] = elementsOf(rung).map((e) => e.id);
    // Both branches start at the rail, and both end at the stop contact.
    expect(inputsOf(r.text, start as number)).toEqual(['1']);
    expect(inputsOf(r.text, seal as number)).toEqual(['1']);
    expect(inputsOf(r.text, stop as number).sort()).toEqual([String(start), String(seal)].sort());
    // The lower branch is drawn below the upper one.
    const ladder = readLadder(r.text);
    if (!ladder.ok) throw new Error(ladder.error);
    const y = (id: number) => ladder.pous[0]?.elements.find((e) => e.localId === id)?.y ?? NaN;
    expect(y(seal as number)).toBeGreaterThan(y(start as number));
  });

  it('opens a branch at one element and closes it after another, and parallels coils', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    const ids: number[] = [];
    let at: Sel = network(0);
    for (const v of ['a', 'b', 'c']) {
      r.step((m) => insert(m, r.ids, at, { kind: 'contact' }, []));
      const id = r.id;
      ids.push(id);
      r.step((m) => setOperand(m, r.ids, 0, id, v));
      at = element(0, id);
    }
    r.step((m) => insert(m, r.ids, at, { kind: 'coil' }, []));
    const q = r.id;
    r.step((m) => setOperand(m, r.ids, 0, q, 'q'));
    // Opened at a, closed after b: a and b in series, in parallel with what is put below.
    r.step((m) => branch(m, element(0, ids[1] as number), ids[0] as number));
    r.step((m) => insert(m, r.ids, r.sel, { kind: 'contact' }, []));
    const d = r.id;
    r.step((m) => setOperand(m, r.ids, 0, d, 'd'));
    // A coil in parallel with q.
    r.step((m) => branch(m, element(0, q)));
    r.step((m) => insert(m, r.ids, r.sel, { kind: 'coil' }, []));
    const p = r.id;
    r.step((m) => setOperand(m, r.ids, 0, p, 'p'));
    expect(r.rung(0)).toBe('(contact a contact b | contact d) contact c (coil q | coil p)');
    // Across two series it is refused.
    expect(branch(r.model, element(0, d), ids[2] as number).ok).toBe(false);
    // Deleting the only element of a branch takes the branch away.
    r.step((m) => remove(m, element(0, d)), [d]);
    expect(r.rung(0)).toBe('contact a contact b contact c (coil q | coil p)');
  });

  it('keeps an empty branch as a wire, and says so', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'contact' }, []));
    const a = r.id;
    r.step((m) => setOperand(m, r.ids, 0, a, 'a'));
    r.step((m) => insert(m, r.ids, element(0, a), { kind: 'contact' }, []));
    const b = r.id;
    r.step((m) => setOperand(m, r.ids, 0, b, 'b'));
    r.step((m) => insert(m, r.ids, element(0, b), { kind: 'coil' }, []));
    const q = r.id;
    r.step((m) => setOperand(m, r.ids, 0, q, 'q'));
    r.step((m) => branch(m, element(0, b)));
    // Read again, the wire around b is still there.
    expect(r.rung(0)).toBe('contact a (contact b | ) coil q');
    expect(inputsOf(r.text, q).sort()).toEqual([String(a), String(b)].sort());
  });
});

describe('localIds', () => {
  it('never gives out an id twice, after a deletion, an undo, or a save and a new session', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m) => addNetwork(m, 0, 0));
    r.step((m) => insert(m, r.ids, network(0), { kind: 'coil' }, []));
    const first = r.id;
    // Deleted: its id is not given to the next element.
    r.step((m) => remove(m, element(0, first)), [first]);
    r.step((m) => insert(m, r.ids, network(0), { kind: 'coil' }, []));
    expect(r.id).toBeGreaterThan(first);
    // An edit undone: the model goes back, the ids do not.
    const before = r.model;
    const undone = insert(before, r.ids, network(0), { kind: 'contact' }, []);
    const again = insert(before, r.ids, network(0), { kind: 'contact' }, []);
    expect(undone.ok && again.ok && undone.sel?.kind === 'element' && again.sel?.kind === 'element' && again.sel.id > undone.sel.id).toBe(true);
    // Undone, and saved, and opened again: the record carries the highest id used.
    const last = again.ok && again.sel?.kind === 'element' ? again.sel.id : 0;
    const reopened = open(write(r.source, before, r.ids).text);
    if (!reopened.ok) throw new Error(reopened.error);
    expect(reopened.source.firstFree).toBeGreaterThan(last);
    expect(r.text).toMatch(/<addData><data name="urn:mythos:logic:ld-editor" handleUnknown="preserve"><localIds xmlns="urn:mythos:logic:ld-editor" next="\d+"\/><\/data><\/addData>/);
  });

  it('writes the document back byte for byte when nothing changed', () => {
    const r = new Run(TEMPLATE('Main'));
    expect(write(r.source, r.model, r.ids).text).toBe(TEMPLATE('Main'));
  });
});

describe('variables', () => {
  it('declares a variable from an operand in the section chosen, in the interface', () => {
    const r = sealIn();
    expect(r.text).toContain(
      '<interface><inputVars><variable name="start"><type><BOOL/></type></variable><variable name="stop"><type><BOOL/></type></variable></inputVars><outputVars><variable name="motor"><type><BOOL/></type></variable><variable name="lamp"><type><BOOL/></type></variable></outputVars><localVars><variable name="lampDelay"><type><derived name="TON"/></type></variable><variable name="waited"><type><TIME/></type></variable></localVars></interface>'
    );
    expect(declareVariable(r.model, 0, { name: 'Start', type: 'BOOL', section: 'inputVars' }, r.source.pous[0]?.vars ?? []).ok).toBe(false);
    expect(declareVariable(r.model, 0, { name: 'a__b', type: 'BOOL', section: 'inputVars' }, []).ok).toBe(false);
  });
});

describe('the compiler runs what the editor writes', () => {
  it('writes the seal-in and timer rungs the compiler tests run (editor_seal_in.xml)', () => {
    fixture('editor_seal_in.xml', sealIn().text);
  });
});

describe('a document from another tool', () => {
  const foreign = () => readFileSync(resolve(FIXTURES, 'foreign.xml'), 'utf8');

  it('reads its rungs, and keeps a network it does not draw as found', () => {
    const r = new Run(foreign());
    expect([r.rung(0), r.rung(1), r.rung(2)]).toEqual([
      'contact a contact/ b coil q',
      'contact c TON[t PT:=T#50ms ET=>elapsed] coil r',
      expect.stringMatching(/^kept: the variable 30[12] is wired as part of the rung/),
    ]);
    expect(r.model.pous[0]?.networks[0]?.comment).toBe('Rung one: a and not b drive q.');
  });

  it('opens, edits and saves without losing an element, and leaves what it did not change as it was', () => {
    const r = new Run(foreign());
    const before = elementIds(r.original);
    // Title the first network (its comment is the other tool's), put c after a in it, move the
    // kept network up, and add a network of its own at the end.
    r.step((m) => setHeader(m, r.ids, 0, 0, 'title', 'Interlock'));
    r.step((m) => insert(m, r.ids, element(0, 101), { kind: 'contact' }, []));
    const c = r.id;
    r.step((m) => setOperand(m, r.ids, 0, c, 'c'));
    r.step((m) => moveNetwork(m, 0, 2, -1));
    r.step((m) => addNetwork(m, 0, 3));
    r.step((m) => insert(m, r.ids, network(3), { kind: 'contact' }, []));
    const a = r.id;
    r.step((m) => setOperand(m, r.ids, 0, a, 'a'));
    r.step((m) => insert(m, r.ids, element(3, a), { kind: 'coil' }, []));
    const extra = r.id;
    r.step((m) => setOperand(m, r.ids, 0, extra, 'extra'));
    r.step((m) => declareVariable(m, 0, { name: 'extra', type: 'BOOL', section: 'outputVars' }, r.source.pous[0]?.vars ?? []));
    const after = elementIds(r.text);
    for (const [key, kind] of before) expect(after.get(key), `element ${key}`).toBe(kind);
    expect([r.rung(0), r.rung(1), r.rung(2), r.rung(3)]).toEqual([
      'contact a contact c contact/ b coil q',
      expect.stringMatching(/^kept: /),
      'contact c TON[t PT:=T#50ms ET=>elapsed] coil r',
      'contact a coil extra',
    ]);
    // What the editor did not draw or change is the other tool's text: the edited network's
    // contact keeps its addData, the block its documentation, the body its XML comment, and the
    // unchanged timer rung each of its elements, moved down whole.
    expect(r.text).toContain('<ppx:addData><ppx:data name="https://example.org/another-tool/contact" handleUnknown="preserve"><colour xmlns="https://example.org/another-tool/contact" value="blue"/></ppx:data></ppx:addData>');
    expect(r.text).toContain('<ppx:documentation><xhtml:p>The timer of rung two.</xhtml:p></ppx:documentation>');
    expect(r.text).toContain('<!-- Written by another tool: a rail pair for each rung, wires with their points. -->');
    expect(r.text).toContain('<ppx:documentation><xhtml:p>A program another tool wrote.</xhtml:p></ppx:documentation>');
    expect(r.text).toContain('<ppx:content><xhtml:p>Rung one: a and not b drive q.</xhtml:p></ppx:content>');
    expect(r.text).toContain('<ppx:expression>level &gt; 10</ppx:expression>');
    // The other tool's prefix on everything written into its document.
    expect(r.text).toMatch(/<ppx:contact localId="\d+" width="30" height="20"><ppx:position/);
    fixture('editor_foreign_edited.xml', r.text);
  });

  it('opens the oscillator the emulators run, and writes it back unchanged after an edit undone', () => {
    const text = readFileSync(resolve(FIXTURES, 'oscillator.xml'), 'utf8');
    const r = new Run(text);
    expect(r.model.pous[0]?.networks.map((n) => (n.rung === null ? `kept: ${n.why}` : n.rung.length))).toEqual([
      3,
      2,
      3,
      2,
      2,
      2,
      // steps := ADD(steps, 1) is a function fed by variables, not a rung: kept as found.
      'kept: block 71 takes no power from the rung',
      // The label DONE after the last rung: a network with no element.
      0,
    ]);
    const titled = setHeader(r.model, r.ids, 0, 1, 'title', 'x');
    expect(titled.ok).toBe(true);
    // Back to the model as read, with no id given out: the text as read.
    expect(write(r.source, r.model, idsFor(r.source)).text).toBe(text);
  });
});
