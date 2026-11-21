// A Sequential Function Chart as its editor holds it, read from its PLCopen TC6 document and
// written back into it (ADR-013 §2.8, §2.17, L3d-3). The Ladder Diagram editor's model
// (ldmodel.ts) is the template: the same splicing, the same record of localIds.
//
// **A chart is a sequence** (IEC 61131-3 ed. 3 §6.7.5, Table 61; S7-GRAPH's sequence, by the
// owner's decision of 2026-11-18 that the editors follow TIA Portal): a list of items, each a
// step, a transition, a selection (alternative branches, each a sequence that starts with a
// transition), a simultaneous construct (branches that each start with a step), or a jump.
// Steps and transitions always alternate, a selection stands where a transition would and a
// simultaneous construct where a step would, and every branch is closed: a selection's branches
// meet in its convergence or end (in a jump, or at a step no transition follows), a simultaneous
// construct's branches all meet in its convergence. `wellFormed` says so of a sequence, and
// every edit of `sfcedit.ts` is refused unless what it leaves is well formed: the editor never
// writes two steps or two transitions linked directly, or a branch that leads nowhere.
//
// A chart another tool wrote that is not such a sequence (a link back that is not a jump, a
// step with two action blocks, an element the editor does not know, two elements with one
// localId) is **kept as found**: its body is not written again, byte for byte, and the editor
// says why. A chart's comments are written as found, where they are; a network of LD or FBD
// drawn in the chart that gives one transition its condition (ADR-013 §2.17, L3d-1b) is written
// as found, moved by as much as its transition moved, and deleted with its transition.
//
// **Writing.** A POU the editor changed has its chart laid out anew on a grid and written with
// TC6 positions (and a step's size), so other tools and the view read them. Every element keeps
// its `localId` while it exists, and a new one takes an id no element of the document has had:
// the record in the POU's `addData` (`urn:mythos:logic:ld-editor`, as LD's and FBD's) keeps undo
// and a new session from giving one out again. ADR-013 §2.17 asks this of the SFC editor in
// particular: an inline body's values are carried across an online change by
// `__SFC_<localId>__…`, so an id given to another action or transition would carry one body's
// values into another. A condition or an action body the editor did not change is written as
// found (an inline body in LD or FBD is never written by the editor at all); a selection's
// `priority` attributes are dropped once its branches are ordered by them, since the editor draws
// the branches in the order they are tested, left to right (Table 61 feature 2a, as S7-GRAPH).

import {
  declare,
  extraAttrs,
  extrasOf,
  flag,
  idsRecord,
  indentOf,
  intOf,
  posOf,
  record,
  up,
  varsOf,
  type Declared,
  type Ids,
  type VarInfo,
} from './ldmodel';
import { attr, child, childrenNamed, escapeAttr, escapeText, num, openTag, parseXml, splice, type Splice, type XNode } from './ldxml';

const XHTML = 'http://www.w3.org/1999/xhtml';

/** The qualifiers the editor offers (Table 59 without SD, SL and P0, which the front-end refuses until the owner decides, ADR-013 §2.17). */
export const QUALIFIERS = ['N', 'R', 'S', 'L', 'D', 'P', 'P1', 'DS'] as const;
/** The qualifiers that take a duration. */
export const TIMED = new Set(['L', 'D', 'DS', 'SD', 'SL']);

/** One association of a step's action block (§6.7.4.5): a qualifier, a duration, and what it runs. */
export interface Assoc {
  /** The action's own `localId`: the check keeps its Q by it. */
  id: number;
  qualifier: string;
  /** '' when it has none. */
  duration: string;
  /** `reference`: an action of the POU or a BOOL variable; `st`: statements in the block; `kept`: a body in another language, written as found. */
  kind: 'reference' | 'st' | 'kept';
  /** The name, the ST text, or the language of a body kept. */
  text: string;
  /** What it runs changed: written anew. Otherwise its body is written as found. */
  bodyDirty: boolean;
}

export interface Step {
  id: number;
  name: string;
  initial: boolean;
  /** Its action block's `localId`; `null` while it has none. */
  block: number | null;
  actions: Assoc[];
}

/**
 * A transition's condition: an ST expression written in it, a transition of the POU by name, a
 * body in LD or FBD written in it (`kept`), a network of LD or FBD drawn in the chart (`wired`),
 * or none.
 */
export type CondKind = 'st' | 'reference' | 'kept' | 'wired' | 'none';

export interface Trans {
  id: number;
  kind: CondKind;
  /** The ST text, the transition's name, or the language of a body kept. */
  text: string;
  negated: boolean;
  /** Changed in the editor: written anew. Otherwise the `condition` is written as found. */
  condDirty: boolean;
}

export type Item =
  | { t: 'step'; step: Step }
  | { t: 'trans'; trans: Trans }
  /** Alternative branches (selection divergence and convergence); `conv` is `null` when every branch ends. */
  | { t: 'sel'; div: number; conv: number | null; branches: Item[][] }
  /** Simultaneous branches; they all meet in `conv`. */
  | { t: 'sim'; div: number; conv: number; branches: Item[][] }
  | { t: 'jump'; id: number; target: string };

export interface Chart {
  /** Stable within a session, for React. */
  key: number;
  items: Item[];
}

export interface PouModel {
  name: string;
  pouType: string;
  /** Whether its body is SFC; one in another language is never touched. */
  sfc: boolean;
  /** Its charts (connected sequences), left to right; empty when it is kept as found. */
  charts: Chart[];
  /** Why its chart is kept as found; `null` when the editor draws it. */
  why: string | null;
  /** The names of the POU's own actions and transitions (`actions`, `transitions`). */
  actions: string[];
  transitions: string[];
  /** Variables declared in the editor, written into the interface. */
  declared: Declared[];
  /** Changed in the editor: its chart is laid out and written anew. */
  touched: boolean;
}

export interface DocModel {
  pous: PouModel[];
  rev: number;
}

interface SourcePou {
  node: XNode;
  name: string;
  pouType: string;
  iface: XNode | null;
  vars: VarInfo[];
  /** The `SFC` body; `null` for a POU in another language. */
  sfc: XNode | null;
  /** Each element of the chart's body by its localId. */
  byId: Map<number, XNode>;
  /** Children of the body that are no element (addData, documentation, XML comments), in order. */
  others: XNode[];
  /** Elements written as found: comments, and the elements of networks of LD or FBD drawn in the chart. */
  kept: number[];
  /** An element of a network drawn in the chart, by the transition whose condition it gives (moved with it). */
  follows: Map<number, number>;
  /** Why the chart cannot be read as sequences, found while reading the document. */
  refused: string | null;
}

/** A document as it was opened: its text and what was read from it. Never changed. */
export interface Source {
  text: string;
  root: XNode;
  pous: SourcePou[];
  /** The first localId no element has had: past every id anywhere in the document and the record. */
  firstFree: number;
  /** How an XHTML paragraph's start tag is written in this document. */
  xhtml: string;
}

export type Opened = { ok: true; source: Source; model: DocModel } | { ok: false; error: string };

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

const CHART_KINDS = new Set([
  'step',
  'transition',
  'selectionDivergence',
  'selectionConvergence',
  'simultaneousDivergence',
  'simultaneousConvergence',
  'jumpStep',
  'actionBlock',
]);

/** The elements of LD and FBD a network drawn in a chart is made of (TC6). */
const NETWORK_KINDS = new Set([
  'leftPowerRail',
  'rightPowerRail',
  'contact',
  'coil',
  'block',
  'inVariable',
  'outVariable',
  'inOutVariable',
  'connector',
  'continuation',
  'label',
  'jump',
  'return',
]);

let keys = 0;
const nextKey = () => {
  keys += 1;
  return keys;
};

const idOf = (n: XNode) => intOf(attr(n, 'localId')) as number;

/** The character data of a formatted text as the front-end reads it: its own and its children's, in order. */
export function textOf(n: XNode): string {
  return n.text + n.children.filter((c) => !c.name.startsWith('#')).map(textOf).join('');
}

/** The ids a point's connections come from. */
const fromOf = (point: XNode | null): number[] =>
  point
    ? childrenNamed(point, 'connection')
        .map((c) => intOf(attr(c, 'refLocalId')))
        .filter((id): id is number => id !== null)
    : [];

/** The ids an element's own inputs come from (not its condition's). */
const inputsOf = (n: XNode): number[] => childrenNamed(n, 'connectionPointIn').flatMap(fromOf);

/** The start tag of an XHTML paragraph in this document: its prefix when the root binds one, else declared on the spot. */
function xhtmlOf(root: XNode): string {
  const bound = root.attrs.find((a) => a.qname.startsWith('xmlns:') && a.value === XHTML);
  return bound ? `${bound.qname.slice('xmlns:'.length)}:p` : `xhtml:p xmlns:xhtml="${XHTML}"`;
}

/** Open a TC6 document for the SFC editor. */
export function open(text: string): Opened {
  const parsed = parseXml(text);
  if (!parsed.ok) return { ok: false, error: parsed.error };
  const root = parsed.root;
  if (root.name !== 'project') return { ok: false, error: `its root is ${root.name}, not a TC6 project` };
  // Every localId anywhere counts, an inline body's and an action's among them: none is given out twice.
  let maxId = 0;
  const visit = (n: XNode) => {
    const id = intOf(attr(n, 'localId'));
    if (id !== null) maxId = Math.max(maxId, id);
    n.children.forEach(visit);
  };
  visit(root);
  const types = child(root, 'types');
  const pousNode = types ? child(types, 'pous') : null;
  const sources: SourcePou[] = [];
  for (const node of pousNode ? childrenNamed(pousNode, 'pou') : []) {
    const body = child(node, 'body');
    const sfc = body ? child(body, 'SFC') : null;
    const iface = child(node, 'interface');
    const sp: SourcePou = {
      node,
      name: attr(node, 'name') ?? '',
      pouType: attr(node, 'pouType') ?? '',
      iface,
      vars: varsOf(iface),
      sfc,
      byId: new Map(),
      others: [],
      kept: [],
      follows: new Map(),
      refused: null,
    };
    for (const c of sfc?.children ?? []) {
      const id = intOf(attr(c, 'localId'));
      if (id === null) {
        if (CHART_KINDS.has(c.name) || NETWORK_KINDS.has(c.name) || c.name === 'comment') {
          sp.refused ??= `a ${c.name} has no localId`;
        }
        sp.others.push(c);
        continue;
      }
      if (sp.byId.has(id)) sp.refused ??= `two elements have the localId ${id}`;
      else sp.byId.set(id, c);
    }
    const rec = idsRecord(node);
    if (rec !== null) maxId = Math.max(maxId, rec - 1);
    sources.push(sp);
  }
  const source: Source = { text, root, pous: sources, firstFree: maxId + 1, xhtml: xhtmlOf(root) };
  return { ok: true, source, model: { pous: sources.map((sp) => readPou(sp)), rev: 0 } };
}

/** The names of a POU's own actions or transitions. */
function namesOf(node: XNode, list: 'actions' | 'transitions'): string[] {
  const holder = child(node, list);
  return holder ? childrenNamed(holder, list === 'actions' ? 'action' : 'transition').map((n) => attr(n, 'name') ?? '') : [];
}

/** A body's language: the name of its one element. */
const languageOf = (holder: XNode | null): string | null =>
  holder?.children.find((c) => !c.name.startsWith('#') && c.name !== 'addData' && c.name !== 'documentation')?.name ?? null;

function readAssoc(a: XNode): Assoc | string {
  const id = intOf(attr(a, 'localId'));
  if (id === null) return 'an action of an action block has no localId';
  const reference = child(a, 'reference');
  const inline = child(a, 'inline');
  const language = languageOf(inline);
  const st = inline && language === 'ST' ? child(inline, 'ST') : null;
  return {
    id,
    qualifier: attr(a, 'qualifier') ?? 'N',
    duration: (attr(a, 'duration') ?? '').trim(),
    kind: reference ? 'reference' : st ? 'st' : 'kept',
    text: reference ? (attr(reference, 'name') ?? '') : st ? textOf(st).trim() : (language ?? 'no language'),
    bodyDirty: false,
  };
}

function readTrans(n: XNode): Trans {
  const condition = child(n, 'condition');
  const negated = condition ? flag(condition, 'negated') : false;
  const t: Trans = { id: idOf(n), kind: 'none', text: '', negated, condDirty: false };
  if (!condition) return t;
  const reference = child(condition, 'reference');
  const inline = child(condition, 'inline');
  if (reference) return { ...t, kind: 'reference', text: attr(reference, 'name') ?? '' };
  if (inline) {
    const language = languageOf(inline);
    const st = language === 'ST' ? child(inline, 'ST') : null;
    return st ? { ...t, kind: 'st', text: textOf(st).trim() } : { ...t, kind: 'kept', text: language ?? 'no language' };
  }
  if (child(condition, 'connectionPointIn')) return { ...t, kind: 'wired', text: 'a network drawn in the chart' };
  return t;
}

function readPou(sp: SourcePou): PouModel {
  const model: PouModel = {
    name: sp.name,
    pouType: sp.pouType,
    sfc: sp.sfc !== null,
    charts: [],
    why: null,
    actions: namesOf(sp.node, 'actions'),
    transitions: namesOf(sp.node, 'transitions'),
    declared: [],
    touched: false,
  };
  if (!sp.sfc) return model;
  const read = chartsOf(sp);
  if (typeof read === 'string') model.why = read;
  else model.charts = read;
  return model;
}

/** The chart's sequences, or why it is kept as found. */
function chartsOf(sp: SourcePou): Chart[] | string {
  if (sp.refused) return sp.refused;
  const chart = new Map<number, XNode>();
  const network: XNode[] = [];
  for (const [id, n] of sp.byId) {
    if (CHART_KINDS.has(n.name)) chart.set(id, n);
    else if (n.name === 'comment') sp.kept.push(id);
    else if (NETWORK_KINDS.has(n.name)) network.push(n);
    else return `it holds a ${n.name} (${id}), which this editor does not draw`;
  }
  // Each network drawn in the chart moves with the one transition whose condition it gives.
  followNetworks(sp, chart, network);
  // What follows each element, an action block aside.
  const next = new Map<number, number[]>();
  const blocks = new Map<number, number[]>();
  for (const [id, n] of chart) {
    for (const from of inputsOf(n)) {
      if (!chart.has(from)) return `${n.name} ${id} is linked from ${from}, which is no step, transition, divergence or convergence of the chart`;
      if (n.name === 'actionBlock') blocks.set(from, [...(blocks.get(from) ?? []), id]);
      else next.set(from, [...(next.get(from) ?? []), id]);
    }
  }
  for (const [id, n] of chart) {
    if (n.name !== 'actionBlock') continue;
    const from = inputsOf(n);
    if (from.length !== 1 || chart.get(from[0] as number)?.name !== 'step') {
      return `action block ${id} is not linked to one step`;
    }
  }
  const seen = new Set<number>();
  const kindOf = (id: number) => chart.get(id)?.name ?? '';
  const nexts = (id: number) => next.get(id) ?? [];
  /** What the compiler tests first comes first: by `priority` where every branch gives one, else left to right (Table 61). */
  const ordered = (ids: number[]) => {
    const prio = (id: number) => intOf(attr(chart.get(id) as XNode, 'priority'));
    const all = ids.every((id) => prio(id) !== null);
    return [...ids].sort((a, b) => {
      if (all) return (prio(a) as number) - (prio(b) as number) || a - b;
      const pa = posOf(chart.get(a) as XNode);
      const pb = posOf(chart.get(b) as XNode);
      return pa.x - pb.x || pa.y - pb.y || a - b;
    });
  };
  type Read = { items: Item[]; conv: number | null; last: number | null };
  const fail = (why: string): never => {
    throw new KeptAsFound(why);
  };
  const stepOf = (id: number): Step => {
    const n = chart.get(id) as XNode;
    const own = blocks.get(id) ?? [];
    if (own.length > 1) fail(`step ${id} has ${own.length} action blocks; this editor draws one per step`);
    const block = own[0] ?? null;
    const actions: Assoc[] = [];
    if (block !== null) {
      for (const a of childrenNamed(chart.get(block) as XNode, 'action')) {
        const read = readAssoc(a);
        if (typeof read === 'string') fail(`${read} (${block})`);
        else actions.push(read);
      }
    }
    return { id, name: attr(n, 'name') ?? '', initial: flag(n, 'initialStep'), block, actions };
  };
  /** A sequence from `start` to the convergence it reaches, a jump, or its end. */
  const readSeq = (start: number): Read => {
    const items: Item[] = [];
    let at: number | null = start;
    let last: number | null = null;
    while (at !== null) {
      const kind = kindOf(at);
      if (kind === 'selectionConvergence' || kind === 'simultaneousConvergence') return { items, conv: at, last };
      if (seen.has(at)) fail(`its links go back to ${kind} ${at} without a jump, which this editor does not draw`);
      seen.add(at);
      const after = nexts(at);
      switch (kind) {
        case 'step':
        case 'transition':
        case 'jumpStep': {
          if (after.length > 1) fail(`${kind} ${at} is followed by ${after.length} elements without a divergence`);
          if (kind === 'step') items.push({ t: 'step', step: stepOf(at) });
          else if (kind === 'transition') items.push({ t: 'trans', trans: readTrans(chart.get(at) as XNode) });
          else {
            if (after.length > 0) fail(`jump ${at} is followed by another element`);
            items.push({ t: 'jump', id: at, target: attr(chart.get(at) as XNode, 'targetName') ?? '' });
          }
          last = at;
          at = after[0] ?? null;
          break;
        }
        case 'selectionDivergence':
        case 'simultaneousDivergence': {
          const sel = kind === 'selectionDivergence';
          if (after.length < 2) fail(`${kind} ${at} has ${after.length} branch${after.length === 1 ? '' : 'es'}`);
          const branches = (sel ? ordered(after) : [...after].sort((a, b) => posOf(chart.get(a) as XNode).x - posOf(chart.get(b) as XNode).x || a - b)).map(readSeq);
          const meeting = [...new Set(branches.filter((b) => b.conv !== null).map((b) => b.conv as number))];
          if (meeting.length > 1) fail(`the branches of ${kind} ${at} meet in ${meeting.length} convergences`);
          const conv = meeting[0] ?? null;
          const want = sel ? 'selectionConvergence' : 'simultaneousConvergence';
          if (conv !== null && kindOf(conv) !== want) fail(`the branches of ${kind} ${at} meet in ${kindOf(conv)} ${conv}`);
          if (!sel && branches.some((b) => b.conv === null)) fail(`a branch of ${kind} ${at} does not reach its convergence`);
          // A selection's branch meets the convergence from a transition, and one that ends with a
          // transition meets it: otherwise writing the chart again would link what was not linked.
          if (sel && branches.some((b) => (b.conv !== null) !== converges(b.items))) {
            fail(`a branch of ${kind} ${at} ends otherwise than at a transition that meets its convergence`);
          }
          if (conv !== null) {
            const into = inputsOf(chart.get(conv) as XNode);
            const lasts = branches.filter((b) => b.conv !== null).map((b) => b.last);
            if (into.length !== lasts.length || lasts.some((l) => l === null || !into.includes(l))) {
              fail(`${kindOf(conv)} ${conv} is reached otherwise than by the ends of the branches of ${kind} ${at}`);
            }
            if (seen.has(conv)) fail(`${kindOf(conv)} ${conv} closes two divergences`);
            seen.add(conv);
          }
          const div = at;
          if (sel) items.push({ t: 'sel', div, conv, branches: branches.map((b) => b.items) });
          else items.push({ t: 'sim', div, conv: conv as number, branches: branches.map((b) => b.items) });
          last = conv;
          if (conv === null) at = null;
          else {
            const out = nexts(conv);
            if (out.length > 1) fail(`${kindOf(conv)} ${conv} is followed by ${out.length} elements`);
            at = out[0] ?? null;
          }
          break;
        }
        default:
          fail(`${kind} ${at} cannot be reached the way this editor draws a chart`);
      }
    }
    return { items, conv: null, last };
  };
  try {
    // A chart starts at a step nothing links to: left to right, as they are drawn.
    const heads = [...chart.entries()]
      .filter(([, n]) => n.name === 'step' && inputsOf(n).length === 0)
      .map(([id]) => id)
      .sort((a, b) => posOf(chart.get(a) as XNode).x - posOf(chart.get(b) as XNode).x || posOf(chart.get(a) as XNode).y - posOf(chart.get(b) as XNode).y || a - b);
    const charts: Chart[] = [];
    for (const head of heads) {
      const read = readSeq(head);
      if (read.conv !== null) fail(`the sequence from step ${head} runs into ${kindOf(read.conv)} ${read.conv} outside any divergence`);
      charts.push({ key: nextKey(), items: read.items });
    }
    for (const [id, n] of chart) {
      if (n.name !== 'actionBlock' && !seen.has(id)) fail(`${n.name} ${id} is not reached from a step that starts a sequence`);
    }
    for (const c of charts) {
      const why = wellFormed(c.items, 'top');
      if (why) fail(why);
    }
    return charts;
  } catch (e) {
    if (e instanceof KeptAsFound) return e.why;
    throw e;
  }
}

class KeptAsFound {
  constructor(readonly why: string) {}
}

/** Each element of a network drawn in the chart by the transition it gives its condition to, where that is one transition. */
function followNetworks(sp: SourcePou, chart: Map<number, XNode>, network: XNode[]): void {
  const ids = new Set(network.map(idOf));
  const parent = new Map<number, number>([...ids].map((id) => [id, id]));
  const find = (k: number): number => {
    let at = k;
    while (parent.get(at) !== at) at = parent.get(at) as number;
    return at;
  };
  const join = (a: number, b: number) => {
    if (ids.has(a) && ids.has(b)) parent.set(find(a), find(b));
  };
  const connectors = new Map<string, number>();
  for (const n of network) if (n.name === 'connector') connectors.set((attr(n, 'name') ?? '').toUpperCase(), idOf(n));
  const allInputs = (n: XNode): number[] => {
    const out: number[] = [];
    const walk = (x: XNode) => {
      if (x.name === 'connectionPointIn') out.push(...fromOf(x));
      x.children.forEach(walk);
    };
    walk(n);
    return out;
  };
  for (const n of network) {
    for (const from of allInputs(n)) join(idOf(n), from);
    if (n.name === 'continuation') {
      const to = connectors.get((attr(n, 'name') ?? '').toUpperCase());
      if (to !== undefined) join(idOf(n), to);
    }
  }
  const reaches = new Map<number, Set<number>>();
  for (const [id, n] of chart) {
    if (n.name !== 'transition') continue;
    const condition = child(n, 'condition');
    const point = condition ? child(condition, 'connectionPointIn') : null;
    for (const from of fromOf(point)) {
      if (!ids.has(from)) continue;
      const g = find(from);
      reaches.set(g, new Set([...(reaches.get(g) ?? []), id]));
    }
  }
  for (const n of network) {
    sp.kept.push(idOf(n));
    const to = reaches.get(find(idOf(n)));
    if (to && to.size === 1) sp.follows.set(idOf(n), [...to][0] as number);
  }
}

// ---------------------------------------------------------------------------------------------
// The shape of a sequence
// ---------------------------------------------------------------------------------------------

/** Whether an item stands as a step (`S`) or a transition (`T`) in the alternation. */
const role = (item: Item): 'S' | 'T' => (item.t === 'step' || item.t === 'sim' || item.t === 'jump' ? 'S' : 'T');
const itemName = (item: Item): string =>
  item.t === 'step'
    ? `step ${item.step.name || item.step.id}`
    : item.t === 'trans'
      ? `transition ${item.trans.id}`
      : item.t === 'jump'
        ? `jump ${item.id}`
        : `${item.t === 'sel' ? 'selection' : 'simultaneous'} divergence ${item.div}`;

/** Whether a selection's branch meets its convergence: it ends with a transition, not in a jump or at a last step. */
export const converges = (branch: readonly Item[]): boolean => branch[branch.length - 1]?.t === 'trans';

/**
 * Why a sequence is not well formed (§6.7.5, Table 61), or `null` when it is: steps and
 * transitions alternate; a chart starts at a step, a selection's branch with a transition and a
 * simultaneous branch with a step; a selection follows a step and, where a branch meets its
 * convergence, is followed by a step (or a jump); a simultaneous construct follows a transition and is
 * followed by one, every branch ending at a step that meets its convergence; a jump ends its
 * sequence, after a transition; a chart does not end at a transition, which must lead to a step.
 */
export function wellFormed(items: readonly Item[], where: 'top' | 'sel' | 'sim'): string | null {
  if (items.length === 0) return where === 'top' ? 'a chart has no step' : 'a branch is empty';
  const first = items[0] as Item;
  if (where !== 'sel' && first.t !== 'step') return `${where === 'top' ? 'a chart' : 'a simultaneous branch'} starts with ${itemName(first)}, not a step`;
  if (where === 'sel' && first.t !== 'trans') return `an alternative branch starts with ${itemName(first)}, not a transition`;
  for (let i = 0; i < items.length; i += 1) {
    const item = items[i] as Item;
    const prev = items[i - 1];
    const after = items[i + 1];
    if (prev && role(prev) === role(item)) return `${itemName(prev)} is followed by ${itemName(item)}: steps and transitions alternate`;
    if (item.t === 'jump' && after) return `${itemName(item)} is followed by ${itemName(after)}: a jump ends its sequence`;
    if (item.t === 'sel') {
      if (prev?.t !== 'step') return `${itemName(item)} does not follow a step`;
      if (item.branches.length < 2) return `${itemName(item)} has fewer than two branches`;
      for (const b of item.branches) {
        const why = wellFormed(b, 'sel');
        if (why) return why;
      }
      const meets = item.branches.some(converges);
      if (meets && item.conv === null) return `the branches of ${itemName(item)} have no convergence to meet in`;
      if (!meets && item.conv !== null) return `no branch of ${itemName(item)} reaches its convergence`;
      if (meets && after?.t !== 'step' && after?.t !== 'jump') return `the convergence of ${itemName(item)} is not followed by a step or a jump`;
      if (!meets && after) return `every branch of ${itemName(item)} ends, yet ${itemName(after)} follows it`;
    }
    if (item.t === 'sim') {
      if (prev?.t !== 'trans') return `${itemName(item)} does not follow a transition`;
      if (after?.t !== 'trans') return `the convergence of ${itemName(item)} is not followed by a transition`;
      if (item.branches.length < 2) return `${itemName(item)} has fewer than two branches`;
      for (const b of item.branches) {
        const why = wellFormed(b, 'sim');
        if (why) return why;
        const end = b[b.length - 1] as Item;
        if (end.t !== 'step') return `a branch of ${itemName(item)} ends with ${itemName(end)}, not a step that meets its convergence`;
      }
    }
  }
  const last = items[items.length - 1] as Item;
  if (where === 'top' && last.t === 'trans') return `the chart ends with ${itemName(last)}, which leads to no step`;
  return null;
}

/** Every item of a sequence and of its branches, depth first, in the order they are drawn. */
export function itemsOf(items: readonly Item[]): Item[] {
  const out: Item[] = [];
  for (const item of items) {
    out.push(item);
    if (item.t === 'sel' || item.t === 'sim') for (const b of item.branches) out.push(...itemsOf(b));
  }
  return out;
}

export const stepsOf = (items: readonly Item[]): Step[] => itemsOf(items).flatMap((i) => (i.t === 'step' ? [i.step] : []));

// ---------------------------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------------------------

/** The view's scale of the document's units (`SfcView.SCALE`), and its measures in drawn pixels. */
const SCALE = 2.5;
const AX = 22; // a step's axis, 55 px right of its x
const CHAR = 7;
const SHOWN = 40;
/** The live column the view adds to an action block, room for `UNKNOWN`. */
const VALUE_PX = CHAR * 'UNKNOWN'.length + 10;

/** A step's box as the view sizes it, in the document's units. */
export const stepWidth = (name: string) => Math.ceil(Math.max(110, CHAR * name.length + 24) / SCALE);
export const STEP_HEIGHT = 16;

const shown = (text: string) => Math.min(SHOWN, text.replace(/\s+/g, ' ').trim().length);

/** A condition as the view writes it at its transition. */
export function conditionShown(t: Trans): string {
  const text =
    t.kind === 'reference' || t.kind === 'st' ? t.text : t.kind === 'wired' ? '(network in the chart)' : t.kind === 'kept' ? `(in ${t.text})` : '';
  return t.negated ? `NOT (${text})` : text;
}

const assocShown = (a: Assoc) => (a.kind === 'kept' ? `(in ${a.text})` : a.text);
const qualifierShown = (a: Assoc) => (a.duration ? `${a.qualifier} ${a.duration}` : a.qualifier);

/** An action block's width as the view draws it live, in the document's units. */
function blockWidth(s: Step): number {
  const q = Math.max(30, ...s.actions.map((a) => CHAR * qualifierShown(a).length + 10));
  const t = Math.max(60, ...s.actions.map((a) => CHAR * shown(assocShown(a)) + 10));
  return Math.ceil((q + t + VALUE_PX) / SCALE);
}

export interface Placed {
  x: number;
  y: number;
}

/** Where each element of a chart goes, by its localId (action blocks included). */
export type Layout = Map<number, Placed>;

/** Lay out a sequence from left `left` and top `top`: what it took to the right, and where what follows goes. */
function laySeq(items: readonly Item[], left: number, top: number, out: Layout): { right: number; end: number } {
  const half = Math.max(AX, ...items.flatMap((i) => (i.t === 'step' ? [stepWidth(i.step.name) / 2] : [])));
  const x = up(left + half - AX);
  const axis = x + AX;
  let at = top;
  let right = left;
  for (const item of items) {
    switch (item.t) {
      case 'step': {
        const s = item.step;
        out.set(s.id, { x, y: at });
        right = Math.max(right, axis + stepWidth(s.name) / 2);
        if (s.block !== null && s.actions.length > 0) {
          const bx = up(axis + stepWidth(s.name) / 2 + 8);
          out.set(s.block, { x: bx, y: at });
          right = Math.max(right, bx + blockWidth(s));
        }
        at = up(at + Math.max(STEP_HEIGHT, 8 * s.actions.length) + 10);
        break;
      }
      case 'trans':
        out.set(item.trans.id, { x, y: at });
        // The condition, and under it the state the view writes live (`not evaluated FORCED`).
        right = Math.max(right, axis + (15 + 8 + CHAR * Math.max(shown(conditionShown(item.trans)), 20)) / SCALE);
        at += 20;
        break;
      case 'jump':
        out.set(item.id, { x, y: at });
        right = Math.max(right, axis + (12 + CHAR * item.target.length) / SCALE);
        at += 20;
        break;
      default: {
        out.set(item.div, { x, y: at });
        let l = left;
        let bottom = at + 10;
        for (const b of item.branches) {
          const r = laySeq(b, l, at + 10, out);
          l = up(r.right + 10);
          bottom = Math.max(bottom, r.end);
        }
        right = Math.max(right, l);
        if (item.conv !== null) {
          out.set(item.conv, { x, y: bottom });
          at = bottom + 10;
        } else at = bottom;
        break;
      }
    }
  }
  return { right, end: at };
}

/** Every chart of a POU laid out side by side, from the body's top left. */
export function layoutPou(pou: PouModel): Layout {
  const out: Layout = new Map();
  let left = 0;
  for (const c of pou.charts) {
    const r = laySeq(c.items, left, 0, out);
    left = up(r.right + 20);
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------------------------

/** Write the document from the model: the source's text where nothing changed. */
export function write(source: Source, model: DocModel, ids: Ids): string {
  const src = source.text;
  const edits: Splice[] = [];
  model.pous.forEach((pou, index) => {
    const sp = source.pous[index] as SourcePou;
    if (pou.touched && sp.sfc && pou.why === null) {
      const parts = emitPou(source, sp, pou);
      if (sp.sfc.selfClosing) {
        const qname = sp.sfc.qname;
        edits.push({ start: sp.sfc.start, end: sp.sfc.end, text: `<${qname}>${parts.map((p) => `\n${p}`).join('')}\n</${qname}>` });
      } else {
        const { inner, close } = indentOf(src, sp.sfc);
        edits.push({ start: sp.sfc.openEnd, end: sp.sfc.closeStart, text: parts.map((p) => inner + p).join('') + close });
      }
    }
    if (pou.declared.length > 0) edits.push(...declare(sp, pou.declared));
    if (pou.touched || ids.used.has(index)) edits.push(record(sp, Math.max(ids.next, idsRecord(sp.node) ?? 0)));
  });
  return edits.length === 0 ? src : splice(src, edits);
}

/** `node` as found with every `position` inside it moved by `dx`, `dy` (a `relPosition` is relative to its element and stays). */
function moved(src: string, node: XNode, dx: number, dy: number): string {
  const text = src.slice(node.start, node.end);
  if (dx === 0 && dy === 0) return text;
  const edits: Splice[] = [];
  const visit = (n: XNode) => {
    if (n.name === 'position') {
      const attrs: Array<[string, string]> = n.attrs.map((a) => {
        const by = a.name === 'x' ? dx : a.name === 'y' ? dy : null;
        const v = Number(a.value);
        return [a.qname, by !== null && Number.isFinite(v) ? num(v + by) : a.value];
      });
      edits.push({ start: n.start - node.start, end: n.openEnd - node.start, text: openTag(n.qname, attrs, n.selfClosing) });
    }
    n.children.forEach(visit);
  };
  visit(node);
  return splice(text, edits);
}

/**
 * An ST text in a TC6 body: one XHTML paragraph holding it as escaped character data, which an XML
 * reader takes as the same text as CDATA (and a DOM parser that refuses CDATA reads it too).
 */
function stBody(source: Source, q: (n: string) => string, text: string): string {
  const close = source.xhtml.split(' ')[0] as string;
  return `<${q('ST')}><${source.xhtml}>${escapeText(text)}</${close}></${q('ST')}>`;
}

const STEP_ATTRS = ['localId', 'name', 'initialStep', 'width', 'height'];
const TRANS_ATTRS = ['localId', 'width', 'height', 'priority'];
const PLAIN_ATTRS = ['localId', 'width', 'height'];
const JUMP_ATTRS = ['localId', 'width', 'height', 'targetName'];
const ACTION_ATTRS = ['localId', 'qualifier', 'duration'];

function emitPou(source: Source, sp: SourcePou, pou: PouModel): string[] {
  const src = source.text;
  const prefix = sp.sfc && sp.sfc.qname.includes(':') ? `${sp.sfc.qname.split(':')[0]}:` : '';
  const q = (name: string) => prefix + name;
  const layout = layoutPou(pou);
  const parts: string[] = [];
  const pos = (id: number) => {
    const p = layout.get(id) ?? { x: 0, y: 0 };
    return `<${q('position')} x="${num(p.x)}" y="${num(p.y)}"/>`;
  };
  const pointIn = (from: number | null) => (from === null ? '' : `<${q('connectionPointIn')}><${q('connection')} refLocalId="${from}"/></${q('connectionPointIn')}>`);
  const tag = (id: number, kind: string, attrs: Array<[string, string]>, written: string[]) => {
    const found = sp.byId.get(id);
    return { name: found?.qname ?? q(kind), open: openTag(found?.qname ?? q(kind), [...attrs, ...extraAttrs(found, written)]), found };
  };
  const emitStep = (s: Step, from: number | null) => {
    const t = tag(s.id, 'step', [
      ['localId', String(s.id)],
      ['name', s.name],
      ...(s.initial ? ([['initialStep', 'true']] as Array<[string, string]>) : []),
      ['width', num(stepWidth(s.name))],
      ['height', num(STEP_HEIGHT)],
    ], STEP_ATTRS);
    const hasBlock = s.block !== null && s.actions.length > 0;
    parts.push(
      `${t.open}${pos(s.id)}${pointIn(from)}<${q('connectionPointOut')} formalParameter=""/>${hasBlock ? `<${q('connectionPointOutAction')} formalParameter=""/>` : ''}${extrasOf(src, t.found, ['position', 'connectionPointIn', 'connectionPointOut', 'connectionPointOutAction'])}</${t.name}>`
    );
    if (hasBlock) emitBlock(s);
  };
  const emitBlock = (s: Step) => {
    const id = s.block as number;
    const t = tag(id, 'actionBlock', [['localId', String(id)]], PLAIN_ATTRS);
    const foundActions = new Map((t.found ? childrenNamed(t.found, 'action') : []).map((a) => [intOf(attr(a, 'localId')), a] as const));
    const rows = s.actions
      .map((a, i) => {
        const found = t.found ? foundActions.get(a.id) : undefined;
        const name = found?.qname ?? q('action');
        const attrs: Array<[string, string]> = [
          ['localId', String(a.id)],
          ['qualifier', a.qualifier],
          ...(a.duration !== '' ? ([['duration', a.duration]] as Array<[string, string]>) : []),
          ...extraAttrs(found, ACTION_ATTRS),
        ];
        const foundBody = found ? (child(found, 'reference') ?? child(found, 'inline')) : null;
        const body =
          !a.bodyDirty && foundBody
            ? src.slice(foundBody.start, foundBody.end)
            : a.kind === 'reference'
              ? `<${q('reference')} name="${escapeAttr(a.text)}"/>`
              : `<${q('inline')}>${stBody(source, q, a.text)}</${q('inline')}>`;
        return `${openTag(name, attrs)}<${q('relPosition')} x="0" y="${num(8 * i)}"/>${body}${extrasOf(src, found, ['relPosition', 'reference', 'inline'])}</${name}>`;
      })
      .join('');
    parts.push(`${t.open}${pos(id)}${pointIn(s.id)}${rows}${extrasOf(src, t.found, ['position', 'connectionPointIn', 'action'])}</${t.name}>`);
  };
  const emitTrans = (tr: Trans, from: number | null) => {
    const t = tag(tr.id, 'transition', [['localId', String(tr.id)]], TRANS_ATTRS);
    const foundCond = t.found ? child(t.found, 'condition') : null;
    let condition = '';
    if (!tr.condDirty && foundCond) condition = src.slice(foundCond.start, foundCond.end);
    else if (tr.kind === 'reference' || tr.kind === 'st') {
      const inner = tr.kind === 'reference' ? `<${q('reference')} name="${escapeAttr(tr.text)}"/>` : `<${q('inline')} name="">${stBody(source, q, tr.text)}</${q('inline')}>`;
      condition = `${openTag(foundCond?.qname ?? q('condition'), [...(tr.negated ? ([['negated', 'true']] as Array<[string, string]>) : []), ...extraAttrs(foundCond ?? undefined, ['negated'])])}${inner}</${foundCond?.qname ?? q('condition')}>`;
    } else if (foundCond) {
      // A condition kept as found whose negation changed.
      const attrs: Array<[string, string]> = [...(tr.negated ? ([['negated', 'true']] as Array<[string, string]>) : []), ...extraAttrs(foundCond, ['negated'])];
      condition = openTag(foundCond.qname, attrs) + src.slice(foundCond.openEnd, foundCond.end);
      if (foundCond.selfClosing) condition = openTag(foundCond.qname, attrs, true);
    }
    parts.push(
      `${t.open}${pos(tr.id)}${pointIn(from)}<${q('connectionPointOut')}/>${condition}${extrasOf(src, t.found, ['position', 'connectionPointIn', 'connectionPointOut', 'condition'])}</${t.name}>`
    );
  };
  const emitPlain = (id: number, kind: string, ins: number[], outs: number) => {
    const t = tag(id, kind, [['localId', String(id)]], PLAIN_ATTRS);
    const into = ins.map((f) => pointIn(f)).join('');
    const out = Array.from({ length: outs }, () => `<${q('connectionPointOut')} formalParameter=""/>`).join('');
    parts.push(`${t.open}${pos(id)}${into}${out}${extrasOf(src, t.found, ['position', 'connectionPointIn', 'connectionPointOut'])}</${t.name}>`);
  };
  /** Write a sequence after `from`; the id of its last element. */
  const walk = (items: readonly Item[], from: number | null): number | null => {
    let at = from;
    for (const item of items) {
      switch (item.t) {
        case 'step':
          emitStep(item.step, at);
          at = item.step.id;
          break;
        case 'trans':
          emitTrans(item.trans, at);
          at = item.trans.id;
          break;
        case 'jump': {
          const t = tag(item.id, 'jumpStep', [['localId', String(item.id)], ['targetName', item.target]], JUMP_ATTRS);
          parts.push(`${t.open}${pos(item.id)}${pointIn(at)}${extrasOf(src, t.found, ['position', 'connectionPointIn'])}</${t.name}>`);
          at = item.id;
          break;
        }
        default: {
          const sel = item.t === 'sel';
          emitPlain(item.div, sel ? 'selectionDivergence' : 'simultaneousDivergence', at === null ? [] : [at], item.branches.length);
          const ends: number[] = [];
          for (const b of item.branches) {
            const end = walk(b, item.div);
            if ((!sel || converges(b)) && end !== null) ends.push(end);
          }
          if (item.conv !== null) emitPlain(item.conv, sel ? 'selectionConvergence' : 'simultaneousConvergence', ends, 1);
          at = item.conv;
          break;
        }
      }
    }
    return at;
  };
  for (const c of pou.charts) walk(c.items, null);
  // Comments as found, where they are; a network drawn in the chart moved with its transition.
  for (const id of sp.kept) {
    const n = sp.byId.get(id) as XNode;
    const to = sp.follows.get(id);
    // A network drawn for a transition that was deleted goes with it: it was its condition.
    if (to !== undefined && !layout.has(to)) continue;
    const now = to !== undefined ? layout.get(to) : undefined;
    const was = to !== undefined ? sp.byId.get(to) : undefined;
    if (now && was) {
      const p = posOf(was);
      parts.push(moved(src, n, now.x - p.x, now.y - p.y));
    } else parts.push(src.slice(n.start, n.end));
  }
  return [...sp.others.map((n) => src.slice(n.start, n.end)), ...parts];
}

/** The variables of a POU of the source, and those the editor declared in it. */
export function varsOfPou(source: Source, model: DocModel, index: number): VarInfo[] {
  const sp = source.pous[index];
  const pou = model.pous[index];
  return [...(sp?.vars ?? []), ...(pou?.declared ?? []).map((d) => ({ name: d.name, type: d.type, section: d.section }))];
}

/** The chart's shape as online change judges it (ADR-013 §2.17, `__SFC_SHAPE`): its steps (names, initial) and each transition's steps before and after. */
export function shapeOf(pou: PouModel): string {
  const steps = pou.charts.flatMap((c) => stepsOf(c.items)).map((s) => `${s.name.toLowerCase()}${s.initial ? '*' : ''}`);
  const trans: string[] = [];
  /** The steps a sequence's first item is entered at, and those its last leaves from. */
  const firstSteps = (items: readonly Item[]): string[] => {
    const f = items[0];
    if (!f) return [];
    if (f.t === 'step') return [f.step.name.toLowerCase()];
    if (f.t === 'sim') return f.branches.flatMap(firstSteps);
    if (f.t === 'jump') return [f.target.toLowerCase()];
    return [];
  };
  const visit = (items: readonly Item[], before: string[]) => {
    let pre = before;
    items.forEach((item, i) => {
      if (item.t === 'step') pre = [item.step.name.toLowerCase()];
      else if (item.t === 'trans') {
        const post = firstSteps(items.slice(i + 1));
        trans.push(`${[...pre].sort().join(',')}>${[...post].sort().join(',')}`);
      } else if (item.t === 'sim') {
        const lasts: string[] = [];
        for (const b of item.branches) {
          visit(b, pre);
          const end = b[b.length - 1];
          if (end?.t === 'step') lasts.push(end.step.name.toLowerCase());
        }
        pre = lasts;
      } else if (item.t === 'sel') {
        const rest = items.slice(i + 1);
        for (const b of item.branches) {
          // A branch that meets the convergence leads to what follows the selection.
          visit(converges(b) ? [...b, ...rest.slice(0, 1)] : b, pre);
        }
      }
    });
  };
  for (const c of pou.charts) visit(c.items, []);
  return `${[...steps].sort().join(';')}|${[...trans].sort().join(';')}`;
}
