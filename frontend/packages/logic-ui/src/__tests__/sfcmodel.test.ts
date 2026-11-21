/// <reference types="node" />
// The Sequential Function Chart editor's model and edits (ADR-013 L3d-3), asserted on the TC6 text
// they write. Each step is checked for what online change and another tool rely on: every element
// the step did not delete keeps its localId and its kind; a new element takes an id no element of
// the document has had in this run, anywhere in it (ADR-013 §2.17: an inline body's values are
// carried by `__SFC_<localId>__…`); the bytes outside the edited body are the document's own;
// what is written opens again, well formed (steps and transitions alternate, every branch
// closed), and the view reads it.
//
// The documents the editor writes here are fixtures the compiler's tests compile and run
// (crates/mythos-logic-compiler/tests/sfc/editor_*.xml, `sfc_from_the_editor.rs`), so what is
// written is what is compiled and run. Set MYTHOS_UPDATE_FIXTURES=1 to write them anew.

import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { Ids } from '../ldmodel';
import { parseXml, type XNode } from '../ldxml';
import { readSfc } from '../sfc';
import {
  addAction,
  addChart,
  addJump,
  altBranch,
  check,
  declareVariable,
  endSequence,
  insertPair,
  moveBranch,
  remove,
  removeBranch,
  renameStep,
  setAction,
  setCondition,
  setInitial,
  setJumpTarget,
  simBranch,
  type Edited,
  type Sel,
} from '../sfcedit';
import { open, wellFormed, write, type DocModel, type Item, type PouModel, type Source } from '../sfcmodel';

const GRAPHIC = resolve(__dirname, '../../../../../crates/mythos-logic-graphic/tests/sfc');
const COMPILER = resolve(__dirname, '../../../../../crates/mythos-logic-compiler/tests');
const FIXTURES = resolve(COMPILER, 'sfc');

/** The logic module's template for a new SFC program (plugins/logic `template`), its version and date fixed. */
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

/**
 * A corpus document with each CDATA section written as escaped text, the same text to an XML
 * reader: the test DOM's DOMParser (the view's reader) refuses CDATA, where a browser's reads it.
 */
const corpus = (name: string) =>
  readFileSync(resolve(GRAPHIC, name), 'utf8').replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, (_, text: string) =>
    text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  );

function parse(text: string): XNode {
  const parsed = parseXml(text);
  if (!parsed.ok) throw new Error(parsed.error);
  return parsed.root;
}

const attrOf = (n: XNode, name: string) => n.attrs.find((a) => a.name === name)?.value;

/** Every element of each chart (an action of a block among them), by its POU and localId (`Main:12`), with its kind. */
function chartIds(text: string): Map<string, string> {
  const out = new Map<string, string>();
  const pous = parse(text).children.find((c) => c.name === 'types')?.children.find((c) => c.name === 'pous')?.children ?? [];
  for (const pou of pous.filter((p) => p.name === 'pou')) {
    const name = attrOf(pou, 'name') ?? '';
    const sfc = pou.children.find((c) => c.name === 'body')?.children.find((c) => c.name === 'SFC');
    for (const e of sfc?.children ?? []) {
      const id = attrOf(e, 'localId');
      if (id === undefined) continue;
      if (out.has(`${name}:${id}`)) throw new Error(`two elements of ${name} have the localId ${id}`);
      out.set(`${name}:${id}`, e.name);
      for (const a of e.name === 'actionBlock' ? e.children.filter((c) => c.name === 'action') : []) {
        const aid = attrOf(a, 'localId') ?? '';
        if (out.has(`${name}:${aid}`)) throw new Error(`two elements of ${name} have the localId ${aid}`);
        out.set(`${name}:${aid}`, 'action');
      }
    }
  }
  return out;
}

/** Every localId anywhere in the text (an inline body's too), by POU. */
function everyId(text: string): Set<string> {
  const out = new Set<string>();
  const visit = (n: XNode, pou: string) => {
    const inside = n.name === 'pou' ? (attrOf(n, 'name') ?? '') : pou;
    const id = attrOf(n, 'localId');
    if (id !== undefined) out.add(`${inside}:${id}`);
    n.children.forEach((c) => visit(c, inside));
  };
  visit(parse(text), '');
  return out;
}

const idOfKey = (key: string) => Number(key.slice(key.lastIndexOf(':') + 1));

/** The text outside `<types>`: the file and content headers, never the editor's. */
function header(text: string): string {
  const start = text.search(/<(\w+:)?types>/);
  const end = text.search(/<\/(\w+:)?types>/);
  return text.slice(0, start) + text.slice(end);
}

/** A sequence as text: `Start* -FALSE- Step1 sel{ -a- S2 | -b- ->Start } S3`. */
function show(items: readonly Item[]): string {
  return items
    .map((i) => {
      switch (i.t) {
        case 'step':
          return `${i.step.name}${i.step.initial ? '*' : ''}${i.step.actions.length > 0 ? `[${i.step.actions.map((a) => `${a.qualifier}${a.duration ? ` ${a.duration}` : ''} ${a.kind === 'kept' ? `(${a.text})` : a.text}`).join('; ')}]` : ''}`;
        case 'trans':
          return `-${i.trans.negated ? 'NOT ' : ''}${i.trans.kind === 'reference' ? `@${i.trans.text}` : i.trans.kind === 'st' ? i.trans.text : `(${i.trans.kind})`}-`;
        case 'jump':
          return `->${i.target}`;
        default:
          return `${i.t}{ ${i.branches.map(show).join(' | ')} }`;
      }
    })
    .join(' ');
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
  /** The POUs whose chart was kept as found when the document was opened. */
  readonly keptAtStart: Set<string>;

  constructor(text: string) {
    const opened = open(text);
    if (!opened.ok) throw new Error(opened.error);
    this.source = opened.source;
    this.ids = { next: opened.source.firstFree, used: new Set() };
    this.model = opened.model;
    this.text = text;
    this.original = text;
    for (const id of everyId(text)) this.ever.add(id);
    this.keptAtStart = new Set(opened.model.pous.filter((p) => p.why !== null).map((p) => p.name));
  }

  /** Undo: back to a model the run passed through (no check: what it deleted is the edit's own). */
  reset(model: DocModel): void {
    this.model = model;
    this.text = write(this.source, model, this.ids);
  }

  step(edit: (m: DocModel, ids: Ids) => Edited, deleted: number[] = []): this {
    const before = chartIds(this.text);
    const prior = new Set(this.ever);
    const edited = edit(this.model, this.ids);
    if (!edited.ok) throw new Error(edited.error);
    this.model = edited.model;
    if (edited.sel) this.sel = edited.sel;
    this.text = write(this.source, this.model, this.ids);
    const after = chartIds(this.text);
    for (const [id, kind] of before) {
      if (deleted.includes(idOfKey(id))) {
        expect(after.has(id), `element ${id} was deleted`).toBe(false);
        continue;
      }
      expect(after.get(id), `element ${id} (${kind}) keeps its localId`).toBe(kind);
    }
    for (const id of after.keys()) {
      if (!before.has(id)) expect(prior.has(id), `the new localId ${id} was never used before in this run`).toBe(false);
    }
    for (const id of everyId(this.text)) this.ever.add(id);
    expect(header(this.text)).toBe(header(this.original));
    const again = open(this.text);
    if (!again.ok) throw new Error(again.error);
    for (const p of again.model.pous.filter((x) => x.sfc && !this.keptAtStart.has(x.name))) {
      expect(p.why, `${p.name} is read again as the editor wrote it`).toBeNull();
      for (const c of p.charts) expect(wellFormed(c.items, 'top')).toBeNull();
    }
    expect(readSfc(this.text).ok).toBe(true);
    return this;
  }

  /** The selected element's id. */
  get id(): number {
    if (this.sel?.kind !== 'el') throw new Error('nothing is selected');
    return this.sel.id;
  }

  get row(): number {
    if (this.sel?.kind !== 'el' || this.sel.row === undefined) throw new Error('no row is selected');
    return this.sel.row;
  }

  /** POU `pou`'s charts as the document says when read again. */
  chart(pou = 0): string {
    const opened = open(this.text);
    if (!opened.ok) throw new Error(opened.error);
    const p = opened.model.pous[pou] as PouModel;
    if (p.why !== null) return `kept: ${p.why}`;
    return p.charts.map((c) => show(c.items)).join(' || ');
  }

  pou(index = 0): PouModel {
    return this.model.pous[index] as PouModel;
  }
}

const el = (id: number, row?: number): Sel => ({ kind: 'el', pou: 0, id, ...(row === undefined ? {} : { row }) });
const chart0: Sel = { kind: 'chart', pou: 0, chart: 0 };
const vars = (r: Run) => r.source.pous[0]?.vars ?? [];

function fixture(name: string, text: string): void {
  const path = resolve(FIXTURES, name);
  // The fixture is what the Rust tests compile; one that is missing is written only on request,
  // so deleting it cannot turn into a test that compares a document with itself.
  if (process.env.MYTHOS_UPDATE_FIXTURES === '1') writeFileSync(path, text);
  expect(existsSync(path), `${path} is missing; MYTHOS_UPDATE_FIXTURES=1 writes it`).toBe(true);
  expect(text).toBe(readFileSync(path, 'utf8'));
}

const declareAll = (r: Run, list: ReadonlyArray<readonly [string, string, 'inputVars' | 'outputVars' | 'localVars']>) => {
  for (const [name, type, section] of list) r.step((m) => declareVariable(m, 0, { name, type, section }, vars(r)));
};

describe('reading and writing back', () => {
  it('writes every document of the corpus back byte for byte when nothing changed', () => {
    const files = [
      ...readdirSync(GRAPHIC).map((f) => resolve(GRAPHIC, f)),
      ...['ladder', 'fbd', 'sfc'].flatMap((d) => (existsSync(resolve(COMPILER, d)) ? readdirSync(resolve(COMPILER, d)).map((f) => resolve(COMPILER, d, f)) : [])),
    ].filter((f) => f.endsWith('.xml'));
    expect(files.map((f) => f.slice(f.lastIndexOf('/') + 1))).toEqual(expect.arrayContaining(['bottling.xml', 'doors.xml', 'mixer.xml']));
    for (const f of files) {
      const text = readFileSync(f, 'utf8');
      const opened = open(text);
      if (!opened.ok) throw new Error(`${f}: ${opened.error}`);
      expect(write(opened.source, opened.model, { next: opened.source.firstFree, used: new Set() }), f).toBe(text);
    }
  });

  it('reads every chart of the corpus as the sequences it draws, in the order the compiler tests the branches', () => {
    const r = new Run(corpus('bottling.xml'));
    expect(r.chart()).toBe(
      'Idle* -Idle.T >= T#60ms- Convey[N conveyor; P CountBottle] sel{ -bottles MOD 3 = 0 AND Convey.T >= T#40ms- Reject[D T#20ms alarm; P Discard] -Reject.T >= T#60ms- | -Convey.T >= T#80ms- sim{ Fill[S valve; DS T#40ms label_ok; N fill_time := Fill.T;] -@Filled- FillDone | Cap[L T#60ms capper] -Cap.T >= T#70ms- CapDone } -TRUE- Release[R valve; R label_ok] -Release.T >= T#20ms- } ->Idle'
    );
    const doors = new Run(corpus('doors.xml'));
    expect(doors.pou(0).why).toBeNull();
    expect(doors.pou(1).sfc).toBe(false);
    // Bodies in LD and FBD are named with their language and kept; a network drawn in the chart too.
    const mixer = new Run(corpus('mixer.xml'));
    expect(mixer.chart()).toContain('Idle*[N (FBD)] -(wired)- Fill[N Open; P1 Count] -(kept)- Stir[L T#50ms motor; N (LD)] -(wired)- ->Idle');
  });

  it('keeps a chart it cannot draw as found, says why, and writes it back byte for byte while another chart of the document is edited', () => {
    const kept = `<pou name="Odd" pouType="functionBlock"><interface/><body><SFC>
            <step localId="1" name="A" initialStep="true"><position x="0" y="0"/></step>
            <transition localId="2"><position x="0" y="40"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn><condition><inline name=""><ST><xhtml:p>TRUE</xhtml:p></ST></inline></condition></transition>
            <step localId="3" name="B"><position x="0" y="80"/><connectionPointIn><connection refLocalId="2"/></connectionPointIn></step>
            <transition localId="4"><position x="0" y="120"/><connectionPointIn><connection refLocalId="3"/></connectionPointIn><condition><inline name=""><ST><xhtml:p>TRUE</xhtml:p></ST></inline></condition></transition>
            <selectionConvergence localId="5"><position x="0" y="150"/><connectionPointIn><connection refLocalId="4"/></connectionPointIn></selectionConvergence>
          </SFC></body></pou>`;
    const text = TEMPLATE('Main').replace('<pous>', `<pous>\n      ${kept}`).replace('<project xmlns="http://www.plcopen.org/xml/tc6_0201">', '<project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">');
    const r = new Run(text);
    expect(r.pou(0).why).toBe('the sequence from step 1 runs into selectionConvergence 5 outside any divergence');
    // Every edit of the chart kept is refused with why, a new sequence in it among them.
    const why = 'This chart is kept as the document has it (the sequence from step 1 runs into selectionConvergence 5 outside any divergence): nothing is put into it.';
    for (const tried of [insertPair(r.model, r.ids, 0, el(1), []), addChart(r.model, r.ids, 0, []), addAction(r.model, r.ids, 0, el(1)), remove(r.model, 0, el(1))]) {
      expect(tried.ok ? 'taken' : tried.error).toBe(why);
    }
    r.step((m, ids) => insertPair(m, ids, 1, { kind: 'chart', pou: 1, chart: 0 }, []));
    expect(r.text).toContain(kept);
    expect(r.chart(1)).toBe('Start* -FALSE- Step1');
  });

  it('says why for each chart it keeps', () => {
    const body = (inner: string) =>
      TEMPLATE('Main').replace(/<SFC>[\s\S]*<\/SFC>/, `<SFC>${inner}</SFC>`);
    const why = (inner: string) => {
      const opened = open(body(inner));
      if (!opened.ok) throw new Error(opened.error);
      return opened.model.pous[0]?.why;
    };
    const step = (id: number, name: string, from: number[] = [], initial = false) =>
      `<step localId="${id}" name="${name}"${initial ? ' initialStep="true"' : ''}><position x="0" y="${id * 40}"/>${from.map((f) => `<connectionPointIn><connection refLocalId="${f}"/></connectionPointIn>`).join('')}</step>`;
    const trans = (id: number, from: number) =>
      `<transition localId="${id}"><position x="0" y="${id * 40}"/><connectionPointIn><connection refLocalId="${from}"/></connectionPointIn><condition><inline name=""><ST><p>TRUE</p></ST></inline></condition></transition>`;
    expect(why(`${step(1, 'A', [], true)}<macroStep localId="2"><position x="0" y="0"/></macroStep>`)).toBe('it holds a macroStep (2), which this editor does not draw');
    expect(why(`${step(1, 'A', [], true)}${step(1, 'B')}`)).toBe('two elements have the localId 1');
    // A link back that is not a jump.
    expect(why(`${step(1, 'A', [3], true)}${trans(2, 1)}<step localId="3" name="B"><position x="0" y="80"/><connectionPointIn><connection refLocalId="2"/></connectionPointIn></step>${trans(4, 3)}`)).toMatch(/is not reached from a step that starts a sequence|go back/);
    // Two action blocks of one step.
    const block = (id: number, from: number) => `<actionBlock localId="${id}"><position x="100" y="0"/><connectionPointIn><connection refLocalId="${from}"/></connectionPointIn><action localId="${id + 50}" qualifier="N"><reference name="x"/></action></actionBlock>`;
    expect(why(`${step(1, 'A', [], true)}${block(5, 1)}${block(6, 1)}`)).toBe('step 1 has 2 action blocks; this editor draws one per step');
    // Two steps linked directly.
    expect(why(`${step(1, 'A', [], true)}${step(2, 'B', [1])}`)).toBe('step A is followed by step B: steps and transitions alternate');
    // A transition that leads to no step.
    expect(why(`${step(1, 'A', [], true)}${trans(2, 1)}`)).toBe('the chart ends with transition 2, which leads to no step');
  });

  it('writes a chart it laid out with every element, link, condition and action the document had, and the view draws the same elements', () => {
    const before = corpus('bottling.xml');
    const r = new Run(before);
    // An edit that changes nothing of the chart but its layout: a step renamed and named back.
    r.step((m) => renameStep(m, 0, 30, 'Released', vars(r)));
    r.step((m) => renameStep(m, 0, 30, 'Release', vars(r)));
    expect(r.chart()).toBe(new Run(before).chart());
    const drawn = (text: string) => {
      const read = readSfc(text);
      if (!read.ok) throw new Error(read.error);
      return read.pous[0]?.elements.map((e) => `${e.kind}:${e.localId}<${[...e.from].sort((a, b) => a - b).join(',')}`).sort();
    };
    expect(drawn(r.text)).toEqual(drawn(before));
    // The comment is written as found, where it was.
    expect(r.text).toContain('<comment localId="90" height="20" width="200">\n              <position x="500" y="0"/>\n              <content><xhtml:p>Every element of the chart</xhtml:p></content>\n            </comment>');
    // The selection's priority is its order: left to right, the order the compiler tests them.
    expect(r.text).not.toContain('priority=');
  });

  it('keeps an inline body in LD or FBD byte for byte and moves a network drawn in the chart with its transition', () => {
    const before = corpus('mixer.xml');
    const r = new Run(before);
    r.step((m, ids) => addAction(m, ids, 0, el(3)));
    r.step((m) => setAction(m, 0, r.row, { text: 'filling' }));
    const inline = (text: string, from: string) => text.slice(text.indexOf(from), text.indexOf('</inline>', text.indexOf(from)) + 9);
    for (const at of ['<inline>\n                  <FBD>', '<inline name="Filled">', '<inline>\n                  <LD>']) {
      expect(inline(r.text, at), at).toBe(inline(before, at));
    }
    // The contact of the rung drawn in the chart moved by as much as transition 2 did.
    const posOf = (text: string, tag: string) => {
      const at = text.indexOf(tag);
      const m = /<position x="(-?[\d.]+)" y="(-?[\d.]+)"\/>/.exec(text.slice(at));
      return m ? { x: Number(m[1]), y: Number(m[2]) } : null;
    };
    const t0 = posOf(before, '<transition localId="2">');
    const t1 = posOf(r.text, '<transition localId="2">');
    const c0 = posOf(before, '<contact localId="21">');
    const c1 = posOf(r.text, '<contact localId="21">');
    expect(c1 && c0 && t1 && t0 && { x: c1.x - c0.x, y: c1.y - c0.y }).toEqual(t1 && t0 && { x: t1.x - t0.x, y: t1.y - t0.y });
  });
});

describe('the structure keeps steps and transitions alternating, every branch closed', () => {
  it('puts a step and a transition after a step, and a step and a transition after a transition', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    expect(r.chart()).toBe('Start* -FALSE- Step1');
    const s1 = r.id;
    r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    expect(r.chart()).toBe('Start* -FALSE- Step2 -FALSE- Step1');
    const t = r.pou().charts[0]?.items[1];
    if (t?.t !== 'trans') throw new Error('no transition');
    r.step((m, ids) => insertPair(m, ids, 0, el(t.trans.id), []));
    expect(r.chart()).toBe('Start* -FALSE- Step3 -FALSE- Step2 -FALSE- Step1');
    // With only the chart selected, at its end.
    r.step((m, ids) => insertPair(m, ids, 0, chart0, []));
    expect(r.chart()).toBe('Start* -FALSE- Step3 -FALSE- Step2 -FALSE- Step1 -FALSE- Step4');
    expect(s1).toBeGreaterThan(1);
  });

  it('opens an alternative branch at a transition, at a step, at the first transition of a branch, and at the end of a sequence', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    r.step((m, ids) => insertPair(m, ids, 0, el(r.id), []));
    expect(r.chart()).toBe('Start* -FALSE- Step1 -FALSE- Step2');
    r.step((m, ids) => altBranch(m, ids, 0, el(1), []));
    expect(r.chart()).toBe('Start* sel{ -FALSE- | -FALSE- } Step1 -FALSE- Step2');
    const second = r.id;
    r.step((m) => setCondition(m, 0, second, 'st', 'b'));
    r.step((m, ids) => altBranch(m, ids, 0, el(second), []));
    expect(r.chart()).toBe('Start* sel{ -FALSE- | -b- | -FALSE- } Step1 -FALSE- Step2');
    // The last step: two transitions after it, meeting at a new step.
    r.step((m, ids) => altBranch(m, ids, 0, chart0, []));
    expect(r.chart()).toBe('Start* sel{ -FALSE- | -b- | -FALSE- } Step1 -FALSE- Step2 sel{ -FALSE- | -FALSE- } Step3');
  });

  it('opens a simultaneous branch at a step between two transitions, and refuses it where no transition is before or after', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    const s1 = r.id;
    expect(simBranch(r.model, r.ids, 0, el(1), []).ok).toBe(false);
    const last = simBranch(r.model, r.ids, 0, el(s1), []);
    expect(last.ok ? 'taken' : last.error).toBe('A simultaneous branch closes before a transition: put a transition after this step first.');
    const head = simBranch(r.model, r.ids, 0, el(1), []);
    expect(head.ok ? 'taken' : head.error).toBe('A simultaneous branch opens right after a transition: no transition is right before this step.');
    r.step((m, ids) => insertPair(m, ids, 0, el(s1), []));
    r.step((m, ids) => simBranch(m, ids, 0, el(s1), []));
    expect(r.chart()).toBe('Start* -FALSE- sim{ Step1 | Step3 } -FALSE- Step2');
    // At its first step, one more branch beside it.
    r.step((m, ids) => simBranch(m, ids, 0, el(r.id), []));
    expect(r.chart()).toBe('Start* -FALSE- sim{ Step1 | Step3 | Step4 } -FALSE- Step2');
    // A step and a transition inside a branch keep it a branch of steps.
    r.step((m, ids) => insertPair(m, ids, 0, el(s1), []));
    expect(r.chart()).toBe('Start* -FALSE- sim{ Step1 -FALSE- Step5 | Step3 | Step4 } -FALSE- Step2');
  });

  it('puts a jump after the last step, ends an alternative branch in one, and never jumps out of a simultaneous branch', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    const s1 = r.id;
    r.step((m, ids) => addJump(m, ids, 0, el(s1)));
    expect(r.chart()).toBe('Start* -FALSE- Step1 -FALSE- ->Start');
    expect(insertPair(r.model, r.ids, 0, el(r.id), []).ok).toBe(false);
    r.step((m, ids) => altBranch(m, ids, 0, el(1), []));
    const b = r.id;
    // The branch that ends in a jump: the other still meets the convergence.
    r.step((m, ids) => addJump(m, ids, 0, el(b)));
    expect(r.chart()).toBe('Start* sel{ -FALSE- | -FALSE- ->Start } Step1 -FALSE- ->Start');
    const first = r.pou().charts[0]?.items[1];
    if (first?.t !== 'sel') throw new Error('no selection');
    const t0 = first.branches[0]?.[0];
    if (t0?.t !== 'trans') throw new Error('no transition');
    const refused = addJump(r.model, r.ids, 0, el(t0.trans.id));
    expect(refused.ok ? 'taken' : refused.error).toBe('This branch is the last that meets the convergence, and steps follow it: it cannot end in a jump.');
    // In a simultaneous branch.
    const s = new Run(TEMPLATE('Main'));
    s.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    const a = s.id;
    s.step((m, ids) => insertPair(m, ids, 0, el(a), []));
    s.step((m, ids) => simBranch(m, ids, 0, el(a), []));
    const inSim = addJump(s.model, s.ids, 0, el(s.id));
    expect(inSim.ok ? 'taken' : inSim.error).toBe('A jump out of a simultaneous branch would leave its other branches active: a jump goes after the convergence.');
  });

  it('ends a sequence at a step in place of a jump', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    r.step((m, ids) => addJump(m, ids, 0, el(r.id)));
    const jump = r.id;
    r.step((m, ids) => endSequence(m, ids, 0, el(jump), []), [jump]);
    expect(r.chart()).toBe('Start* -FALSE- Step1 -FALSE- Step2');
  });

  it('deletes a step with the transition before it, a jump with its, a branch, and a divergence down to its first branch that meets', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    const s1 = r.id;
    r.step((m, ids) => insertPair(m, ids, 0, el(s1), []));
    const s2 = r.id;
    const items = () => r.pou().charts[0]?.items ?? [];
    const transAt = (i: number) => {
      const t = items()[i];
      if (t?.t !== 'trans') throw new Error('no transition');
      return t.trans.id;
    };
    // A step goes with the transition before it: the pair put after Start is taken out.
    const t1 = transAt(1);
    r.step((m) => remove(m, 0, el(s1)), [s1, t1]);
    expect(r.chart()).toBe('Start* -FALSE- Step2');
    r.step((m, ids) => altBranch(m, ids, 0, el(1), []));
    const b2 = r.id;
    r.step((m, ids) => insertPair(m, ids, 0, el(b2), []));
    // Step1's name is free again (a name is not an id: only ids are never given out twice).
    expect(r.chart()).toBe('Start* sel{ -FALSE- | -FALSE- Step1 -FALSE- } Step2');
    const sel = items()[1];
    if (sel?.t !== 'sel') throw new Error('no selection');
    // Delete the branch: one branch left becomes the sequence again.
    const s3 = r.id;
    const inner = sel.branches[1]?.[2];
    r.step((m) => removeBranch(m, 0, el(s3)), [b2, s3, inner?.t === 'trans' ? inner.trans.id : -1, sel.div, sel.conv ?? -1]);
    expect(r.chart()).toBe('Start* -FALSE- Step2');
    // A divergence deleted keeps its first branch that meets its convergence.
    r.step((m, ids) => altBranch(m, ids, 0, el(1), []));
    const other = r.id;
    r.step((m) => setCondition(m, 0, other, 'st', 'other'));
    r.step((m, ids) => addJump(m, ids, 0, el(other)));
    r.step((m) => moveBranch(m, 0, el(other), -1));
    expect(r.chart()).toBe('Start* sel{ -other- ->Start | -FALSE- } Step2');
    const sel2 = items()[1];
    if (sel2?.t !== 'sel') throw new Error('no selection');
    const jump = sel2.branches[0]?.[1];
    r.step((m) => remove(m, 0, el(sel2.div)), [sel2.div, sel2.conv ?? -1, other, jump?.t === 'jump' ? jump.id : -1]);
    expect(r.chart()).toBe('Start* -FALSE- Step2');
    // The initial step at the head goes with its transition; the step after is initial.
    const t = transAt(1);
    r.step((m) => remove(m, 0, el(1)), [1, t]);
    expect(r.chart()).toBe('Step2*');
    const last = remove(r.model, 0, el(s2));
    expect(last.ok ? 'taken' : last.error).toBe('A chart keeps its initial step: make another step initial first, or add one.');
  });

  it('renames a step and the jumps to it, moves the initial step, and refuses a name another has', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    const s1 = r.id;
    r.step((m, ids) => addJump(m, ids, 0, el(s1)));
    const jump = r.id;
    r.step((m) => renameStep(m, 0, 1, 'Idle', []));
    expect(r.chart()).toBe('Idle* -FALSE- Step1 -FALSE- ->Idle');
    const taken = renameStep(r.model, 0, s1, 'idle', []);
    expect(taken.ok ? 'taken' : taken.error).toBe('idle is already the name of a step, an action, a transition or a variable of Main.');
    expect(renameStep(r.model, 0, s1, 'two__bars', []).ok).toBe(false);
    r.step((m) => setInitial(m, 0, s1));
    expect(r.chart()).toBe('Idle -FALSE- Step1* -FALSE- ->Idle');
    r.step((m) => setJumpTarget(m, 0, jump, 'Step1'));
    expect(r.chart()).toBe('Idle -FALSE- Step1* -FALSE- ->Step1');
  });

  it('writes a step’s actions: qualifiers, durations, a variable, statements; refuses SD, SL and P0', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m, ids) => addAction(m, ids, 0, el(1)));
    const a = r.row;
    r.step((m) => setAction(m, 0, a, { text: 'lamp' }));
    r.step((m, ids) => addAction(m, ids, 0, el(1), 'L'));
    const b = r.row;
    r.step((m) => setAction(m, 0, b, { duration: 'T#50ms', text: 'horn' }));
    r.step((m, ids) => addAction(m, ids, 0, el(1)));
    const c = r.row;
    r.step((m) => setAction(m, 0, c, { kind: 'st', text: 'n := n + 1;' }));
    r.step((m) => setAction(m, 0, c, { qualifier: 'P1' }));
    expect(r.chart()).toBe('Start*[N lamp; L T#50ms horn; P1 n := n + 1;]');
    for (const q of ['SD', 'SL', 'P0']) {
      const refused = setAction(r.model, 0, a, { qualifier: q });
      expect(refused.ok ? 'taken' : refused.error).toBe(`${q} is not offered: SD, SL and P0 are refused until the owner decides (ADR-013 §2.17).`);
    }
    // A qualifier that takes no duration drops it.
    r.step((m) => setAction(m, 0, b, { qualifier: 'S' }));
    expect(r.chart()).toBe('Start*[N lamp; S horn; P1 n := n + 1;]');
    // The block goes with its last action.
    r.step((m) => remove(m, 0, el(1, a)), [a]);
    r.step((m) => remove(m, 0, el(1, b)), [b]);
    const block = r.pou().charts[0]?.items[0];
    const blockId = block?.t === 'step' ? (block.step.block ?? -1) : -1;
    r.step((m) => remove(m, 0, el(1, c)), [c, blockId]);
    expect(r.chart()).toBe('Start*');
    expect(r.text).not.toContain('actionBlock');
  });
});

describe('localIds', () => {
  it('never gives an id out twice, after an undo or a deletion, nor one an inline body has', () => {
    const r = new Run(corpus('mixer.xml'));
    // The mixer's inline bodies number their elements 1–6; the chart's highest id is 21.
    const before = r.model;
    r.step((m, ids) => insertPair(m, ids, 0, el(6), []));
    const first = r.id;
    expect(first).toBeGreaterThan(21);
    // Undo: the model before; the same edit again takes new ids.
    r.reset(before);
    r.step((m, ids) => insertPair(m, ids, 0, el(6), []));
    expect(r.id).toBeGreaterThan(first);
    // A new session on what was written continues past the record, not from the chart's highest id.
    const again = new Run(r.text);
    again.step((m) => remove(m, 0, el(r.id)), [r.id, r.id + 1]);
    const fresh = new Run(again.text);
    fresh.step((m, ids) => insertPair(m, ids, 0, el(6), []));
    expect(fresh.id).toBeGreaterThan(r.id + 1);
  });

  it('gives out no id an action of a block or an inline body has, though no element of the chart has it', () => {
    // The highest localId of this chart is an action's, inside its block; an inline body's elements
    // number from 1 in their own body. A new element takes an id past both.
    const text = TEMPLATE('Main').replace(
      /<SFC>[\s\S]*<\/SFC>/,
      `<SFC>
            <step localId="1" name="Start" initialStep="true"><position x="0" y="0"/><connectionPointOutAction formalParameter=""/></step>
            <actionBlock localId="2"><position x="100" y="0"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn><action localId="40" qualifier="N"><reference name="lamp"/></action></actionBlock>
          </SFC>`
    );
    const r = new Run(text);
    r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    expect(r.id).toBeGreaterThan(40);
  });

  it('keeps every id through a long run of random edits, each written chart well formed', () => {
    // A seeded walk: whatever an edit leaves is well formed (Run.step checks it), or the edit is refused.
    let seed = 20261122;
    const rand = (n: number) => {
      seed = (seed * 1103515245 + 12345) % 2147483648;
      return seed % n;
    };
    const r = new Run(TEMPLATE('Main'));
    let taken = 0;
    for (let i = 0; i < 300; i += 1) {
      const ids = [...chartIds(r.text).entries()].filter(([, k]) => k !== 'actionBlock' && k !== 'action' && k !== 'selectionConvergence' && k !== 'simultaneousConvergence').map(([k]) => idOfKey(k));
      const at: Sel = rand(8) === 0 ? chart0 : el(ids[rand(ids.length)] as number);
      const ops: Array<(m: DocModel, x: Ids) => Edited> = [
        (m, x) => insertPair(m, x, 0, at, []),
        (m, x) => insertPair(m, x, 0, at, []),
        (m, x) => altBranch(m, x, 0, at, []),
        (m, x) => simBranch(m, x, 0, at, []),
        (m, x) => addJump(m, x, 0, at),
        (m, x) => endSequence(m, x, 0, at, []),
        (m) => remove(m, 0, at),
        (m) => removeBranch(m, 0, at),
        (m) => moveBranch(m, 0, at, rand(2) === 0 ? -1 : 1),
      ];
      const op = ops[rand(ops.length)] as (m: DocModel, x: Ids) => Edited;
      const tried = op(r.model, r.ids);
      if (!tried.ok) continue;
      // What it deleted: the ids of the chart before that are not in the model after.
      const after = JSON.stringify(tried.model);
      const gone = [...chartIds(r.text).keys()].map(idOfKey).filter((id) => !new RegExp(`"(id|div|conv|block)":${id}[,}]`).test(after));
      r.step(() => tried, gone);
      taken += 1;
    }
    expect(taken).toBeGreaterThan(80);
  }, 60_000);
});

describe('the editor’s own check', () => {
  it('says what is wrong at the element, before the compiler', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
    const s1 = r.id;
    r.step((m, ids) => addJump(m, ids, 0, el(s1)));
    const jump = r.id;
    r.step((m) => setJumpTarget(m, 0, jump, 'Nowhere'));
    r.step((m, ids) => addAction(m, ids, 0, el(1), 'D'));
    const a = r.row;
    r.step((m) => setAction(m, 0, a, { duration: '', text: 'speed' }));
    r.step((m, ids) => addAction(m, ids, 0, el(1)));
    const b = r.row;
    r.step((m) => setAction(m, 0, b, { text: 'ghost' }));
    const t = r.pou().charts[0]?.items[1];
    if (t?.t !== 'trans') throw new Error('no transition');
    r.step((m) => setCondition(m, 0, t.trans.id, 'st', '  '));
    const problems = check(r.pou(), [{ name: 'speed', type: 'INT', section: 'outputVars' }]);
    expect(problems.elements.get(jump)).toEqual(['Main has no step Nowhere to jump to.']);
    expect(problems.elements.get(a)).toEqual(['D takes a duration (as T#5s).', 'speed is a INT: an action that is a variable is a BOOL (Table 56 feature 1).']);
    expect(problems.elements.get(b)).toEqual(['ghost is neither an action of Main nor a variable.']);
    expect(problems.elements.get(t.trans.id)).toEqual(['The transition has no condition.']);
  });
});

/**
 * The sequence the compiler's test runs (`sfc_from_the_editor.rs`), built as an engineer builds it
 * from the module's template: Idle until start; Fill (N valve, P1 counting batches in ST); then
 * either, left first, after 50 ms of Fill both Mix (L 30 ms mixer) and Heat (D 20 ms heater) at
 * once until both have lasted 40 ms, then Drain (R alarm) and back to Idle; or, on stop, Abort
 * (S alarm) until stop is released, and back to Idle.
 */
function sequence(): Run {
  const r = new Run(TEMPLATE('Sequence'));
  r.step((m) => renameStep(m, 0, 1, 'Idle', []));
  r.step((m, ids) => insertPair(m, ids, 0, el(1), []));
  const fill = r.id;
  r.step((m) => renameStep(m, 0, fill, 'Fill', []));
  const items = () => r.pou().charts[0]?.items ?? [];
  const transAfter = (i: number) => {
    const t = items()[i];
    if (t?.t !== 'trans') throw new Error(`no transition at ${i}`);
    return t.trans.id;
  };
  r.step((m) => setCondition(m, 0, transAfter(1), 'st', 'start'));
  r.step((m, ids) => addAction(m, ids, 0, el(fill)));
  r.step((m) => setAction(m, 0, r.row, { text: 'valve' }));
  r.step((m, ids) => addAction(m, ids, 0, el(fill), 'P1'));
  r.step((m) => setAction(m, 0, r.row, { kind: 'st', text: 'batches := batches + 1;' }));
  // Fill -> Mix, then Drain, and back.
  r.step((m, ids) => insertPair(m, ids, 0, el(fill), []));
  const mix = r.id;
  r.step((m) => renameStep(m, 0, mix, 'Mix', []));
  r.step((m) => setCondition(m, 0, transAfter(3), 'st', 'Fill.T >= T#50ms'));
  r.step((m, ids) => insertPair(m, ids, 0, el(mix), []));
  const drain = r.id;
  r.step((m) => renameStep(m, 0, drain, 'Drain', []));
  r.step((m) => setCondition(m, 0, transAfter(5), 'st', 'Mix.T >= T#40ms AND Heat.T >= T#40ms'));
  r.step((m, ids) => addJump(m, ids, 0, el(drain)));
  r.step((m) => setCondition(m, 0, transAfter(7), 'st', 'Drain.T >= T#20ms'));
  r.step((m, ids) => addAction(m, ids, 0, el(drain), 'R'));
  r.step((m) => setAction(m, 0, r.row, { text: 'alarm' }));
  // Heat beside Mix.
  r.step((m, ids) => simBranch(m, ids, 0, el(mix), []));
  const heat = r.id;
  r.step((m) => renameStep(m, 0, heat, 'Heat', []));
  r.step((m, ids) => addAction(m, ids, 0, el(mix), 'L'));
  r.step((m) => setAction(m, 0, r.row, { duration: 'T#30ms', text: 'mixer' }));
  r.step((m, ids) => addAction(m, ids, 0, el(heat), 'D'));
  r.step((m) => setAction(m, 0, r.row, { duration: 'T#20ms', text: 'heater' }));
  // An alternative from Fill on stop: Abort, and back to Idle.
  r.step((m, ids) => altBranch(m, ids, 0, el(transAfter(3)), []));
  const onStop = r.id;
  r.step((m) => setCondition(m, 0, onStop, 'st', 'stop'));
  r.step((m, ids) => insertPair(m, ids, 0, el(onStop), []));
  const abort = r.id;
  r.step((m) => renameStep(m, 0, abort, 'Abort', []));
  r.step((m, ids) => addAction(m, ids, 0, el(abort), 'S'));
  r.step((m) => setAction(m, 0, r.row, { text: 'alarm' }));
  const branch = (items()[3] as Extract<Item, { t: 'sel' }>).branches[1] as Item[];
  const released = branch[2];
  if (released?.t !== 'trans') throw new Error('no transition after Abort');
  r.step((m) => setCondition(m, 0, released.trans.id, 'st', 'NOT stop'));
  r.step((m, ids) => addJump(m, ids, 0, el(released.trans.id)));
  declareAll(r, [
    ['start', 'BOOL', 'inputVars'],
    ['stop', 'BOOL', 'inputVars'],
    ['valve', 'BOOL', 'outputVars'],
    ['mixer', 'BOOL', 'outputVars'],
    ['heater', 'BOOL', 'outputVars'],
    ['alarm', 'BOOL', 'outputVars'],
    ['batches', 'INT', 'outputVars'],
  ]);
  return r;
}

describe('documents the compiler runs', () => {
  it('builds the sequence the compiler tests run (editor_sequence.xml)', () => {
    const r = sequence();
    expect(r.chart()).toBe(
      'Idle* -start- Fill[N valve; P1 batches := batches + 1;] sel{ -Fill.T >= T#50ms- sim{ Mix[L T#30ms mixer] | Heat[D T#20ms heater] } -Mix.T >= T#40ms AND Heat.T >= T#40ms- | -stop- Abort[S alarm] -NOT stop- ->Idle } Drain[R alarm] -Drain.T >= T#20ms- ->Idle'
    );
    expect(check(r.pou(), [...vars(r), ...r.pou().declared]).elements.size).toBe(0);
    fixture('editor_sequence.xml', r.text);
  });

  it('edits another tool’s chart (bottling.xml): one action more, the rest as it runs (editor_bottling_edited.xml)', () => {
    const r = new Run(corpus('bottling.xml'));
    r.step((m, ids) => addAction(m, ids, 0, el(1)));
    r.step((m) => setAction(m, 0, r.row, { text: 'idle_lamp' }));
    declareAll(r, [['idle_lamp', 'BOOL', 'outputVars']]);
    fixture('editor_bottling_edited.xml', r.text);
  });

  it('edits a chart with bodies in LD and FBD and networks drawn in it (mixer.xml), keeping them (editor_mixer_edited.xml)', () => {
    const r = new Run(corpus('mixer.xml'));
    r.step((m, ids) => addAction(m, ids, 0, el(3)));
    r.step((m) => setAction(m, 0, r.row, { text: 'filling' }));
    declareAll(r, [['filling', 'BOOL', 'localVars']]);
    fixture('editor_mixer_edited.xml', r.text);
  });

  it('adds a sequence of its own beside the first', () => {
    const r = new Run(TEMPLATE('Main'));
    r.step((m, ids) => addChart(m, ids, 0, []));
    expect(r.chart()).toBe('Start* || Step1*');
  });
});
