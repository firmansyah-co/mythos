// A Ladder Diagram as the editor holds it, read from its PLCopen TC6 document and written
// back into it (ADR-013 §2.15, L3b-2b).
//
// **Networks.** A body is a list of networks in the order they are evaluated: the front-end's
// rule (connected elements without the rails, top to bottom by the topmost, then the leftmost;
// IEC 61131-3 ed. 3 §8.1.4, §8.2.7). TC6 has no element for a network, so a network is its
// elements; its comment is a TC6 `comment` placed above them, and its label a `label`. Its
// title is kept in that comment's `addData`, as `<data name="urn:mythos:logic:ld-network"
// handleUnknown="preserve"><network title="…"/></data>`: the schema's place for what one tool
// keeps and others ignore (and are asked to preserve). A network with no element, title,
// comment or label is not written, so it is gone when the document is read again.
//
// **Rungs.** A network the editor draws is a series of items from the left rail, each an
// element or a parallel of branches, each branch a series again; what is left at its end goes
// to the right rail when it is a coil. A network of another tool that is not such a series
// (an element it does not draw, a wire into a second pin of a block, a bridge between
// branches) is kept as it is: it can be moved, deleted, titled and commented, and its elements
// are written back byte for byte.
//
// **Writing.** The document is changed by splicing (ldxml.ts): the body of each POU the editor
// changed, its interface where a variable was declared, and its `addData` where the editor
// records the localIds it has used. Every other byte is the document's own, the file header
// and the content header among them. A network the editor did not change is written as it
// was, moved down only when a network above it grew; one it changed is laid out on a grid
// (positions and sizes written as TC6 `x`, `y`, `width`, `height`, so other tools read them).
// An element keeps its `localId` while it exists, and a new one takes an id no element of the
// document has had: the highest the document or its record shows, plus one, counted up and
// never back, so undo does not give an id out twice. Online change follows those ids (the
// hidden `__LD_<localId>` and `__EDGE_<localId>` variables are carried by their path, §2.5).

import { pinsOf, findBox, type BoxDef } from './ldcatalog';
import {
  attr,
  child,
  childrenNamed,
  escapeAttr,
  escapeText,
  num,
  openTag,
  parseXml,
  shifted,
  splice,
  type Splice,
  type XNode,
} from './ldxml';

export const TC6_NAMESPACE = 'http://www.plcopen.org/xml/tc6_0201';
/** The `addData` name of a network's title, on its comment. */
export const NETWORK_DATA = 'urn:mythos:logic:ld-network';
/** The `addData` name of the record of localIds used, on a POU. */
export const IDS_DATA = 'urn:mythos:logic:ld-editor';
const XHTML = 'http://www.w3.org/1999/xhtml';

export type ElKind = 'contact' | 'coil' | 'block' | 'jump' | 'return';

/** A pin of a box. The power pins take no operand. */
export interface BoxPin {
  formal: string;
  /** The type the box's declaration gives it; `ANY` when that is not known. */
  type: string;
  /** What is written at it; `''` for none. */
  operand: string;
  negated: boolean;
  /**
   * For an output: the `outVariable` that writes its operand. For an input of another tool's
   * document: the `inVariable` that feeds it, kept as it is.
   */
  varId: number | null;
}

export interface El {
  id: number;
  kind: ElKind;
  /** A contact's or coil's variable; a jump's label. */
  operand: string;
  negated: boolean;
  edge: 'rising' | 'falling' | null;
  storage: 'set' | 'reset' | null;
  typeName: string;
  instance: string;
  inputs: BoxPin[];
  outputs: BoxPin[];
  /** The block's pins that carry the rung's power. */
  powerIn: string;
  powerOut: string;
}

export type Item = { t: 'el'; el: El } | { t: 'par'; branches: Item[][] };

export interface Network {
  /** Stable within a session, for React. */
  key: number;
  title: string;
  comment: string;
  label: string;
  commentId: number | null;
  labelId: number | null;
  /** The rung; `null` for a network kept as it was found. */
  rung: Item[] | null;
  /** Why a network is kept as it was found. */
  why: string | null;
  /** Every element of the document it held when it was read, comments and labels aside. */
  ids: number[];
  /** Comments and labels beyond its own, written as they were. */
  extras: number[];
  /** The rails its rung is connected to. */
  left: number | null;
  right: number | null;
  /** Its rung changed: it is laid out and written anew. */
  dirty: boolean;
  /** Its title, comment or label changed. */
  headerDirty: boolean;
}

export type Section = 'inputVars' | 'outputVars' | 'localVars';

export interface Declared {
  name: string;
  type: string;
  section: Section;
}

export interface PouModel {
  name: string;
  pouType: string;
  networks: Network[];
  /** Variables declared in the editor, written into the interface. */
  declared: Declared[];
  /** Changed in the editor: its body is written anew. */
  touched: boolean;
}

export interface DocModel {
  pous: PouModel[];
  /** 0 for the model as read. */
  rev: number;
}

/** A variable of a POU's interface. */
export interface VarInfo {
  name: string;
  type: string;
  section: string;
}

/** What a POU's head gives: enough to offer it as a box. */
export interface PouHead {
  name: string;
  pouType: string;
  iface: XNode | null;
  vars: VarInfo[];
}

/** What the writers of a POU's interface, record and network headers need of it. */
export interface PouParts {
  node: XNode;
  iface: XNode | null;
  byId: Map<number, XNode>;
}

/** A network's header as it is written: its title, comment and label, with their ids. */
export type Header = Pick<Network, 'title' | 'comment' | 'label' | 'commentId' | 'labelId'>;

interface SourcePou {
  node: XNode;
  name: string;
  pouType: string;
  ld: XNode | null;
  iface: XNode | null;
  vars: VarInfo[];
  /** Each element of the body by its localId. */
  byId: Map<number, XNode>;
  left: XNode[];
  right: XNode[];
  /** Children of the body that are not elements of a network, in order. */
  others: XNode[];
}

/** A document as it was opened: its text and what was read from it. Never changed. */
export interface Source {
  text: string;
  root: XNode;
  pous: SourcePou[];
  /** The first localId no element has had: past every id and the record of used ones. */
  firstFree: number;
}

export type Opened = { ok: true; source: Source; model: DocModel } | { ok: false; error: string };

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

export const intOf = (text: string | null): number | null => {
  if (text === null || !/^\s*\d+\s*$/.test(text)) return null;
  return Number(text.trim());
};
export const numOf = (text: string | null): number | null => {
  if (text === null || text.trim() === '') return null;
  const v = Number(text);
  return Number.isFinite(v) ? v : null;
};
export const flag = (node: XNode, name: string) => {
  const v = attr(node, name);
  return v === 'true' || v === '1';
};
export const posOf = (node: XNode) => {
  const p = child(node, 'position');
  return { x: numOf(p ? attr(p, 'x') : null) ?? 0, y: numOf(p ? attr(p, 'y') : null) ?? 0 };
};
/** All the text inside an element: each paragraph of a comment on its own line. */
export function deepText(node: XNode): string {
  const inner = node.children.filter((c) => !c.name.startsWith('#'));
  if (inner.length === 0) return node.text;
  return inner.map(deepText).join('\n');
}

export function typeOf(typeNode: XNode | null): string {
  const t = typeNode?.children.find((c) => c.name !== 'addData' && c.name !== 'documentation' && !c.name.startsWith('#'));
  if (!t) return 'UNKNOWN';
  if (t.name === 'derived') return attr(t, 'name') ?? 'UNKNOWN';
  if (t.name === 'string') return 'STRING';
  if (t.name === 'array') return 'ARRAY';
  return t.name;
}

export function varsOf(iface: XNode | null): VarInfo[] {
  if (!iface) return [];
  const out: VarInfo[] = [];
  for (const section of iface.children) {
    if (section.name === 'returnType') continue;
    for (const v of childrenNamed(section, 'variable')) {
      out.push({ name: attr(v, 'name') ?? '', type: typeOf(child(v, 'type')), section: section.name });
    }
  }
  return out;
}

/** A POU of the document as a box another POU may call. */
export function boxOfPou(source: { pous: readonly PouHead[] }, index: number): BoxDef | null {
  const p = source.pous[index];
  if (!p || (p.pouType !== 'functionBlock' && p.pouType !== 'function')) return null;
  const pins = (section: string) =>
    p.vars.filter((v) => v.section === section).map((v) => ({ name: v.name.toUpperCase(), type: v.type.toUpperCase() }));
  const outputs = pins('outputVars');
  if (p.pouType === 'function') {
    const ret = p.iface ? child(p.iface, 'returnType') : null;
    return { name: p.name, kind: 'function', inputs: pins('inputVars'), outputs: [{ name: 'OUT', type: typeOf(ret) }], group: 'This document' };
  }
  return { name: p.name, kind: 'functionBlock', inputs: pins('inputVars'), outputs, group: 'This document' };
}

/** The document's own function and function block POUs as boxes. */
export const ownBoxes = (source: { pous: readonly PouHead[] }): BoxDef[] =>
  source.pous.map((_, i) => boxOfPou(source, i)).filter((d): d is BoxDef => d !== null);

/** A connection into an input. */
interface Conn {
  from: number;
  formal: string | null;
}
const connsOf = (point: XNode | null): Conn[] =>
  point
    ? childrenNamed(point, 'connection')
        .map((c) => ({ from: intOf(attr(c, 'refLocalId')), formal: attr(c, 'formalParameter') }))
        .filter((c): c is Conn => c.from !== null)
    : [];

/** Each input point of an element with its connections (a block's by formal parameter). */
function inputsOf(node: XNode): Array<{ formal: string | null; conns: Conn[] }> {
  if (node.name === 'block') {
    const iv = child(node, 'inputVariables');
    return (iv ? childrenNamed(iv, 'variable') : []).map((v) => ({
      formal: attr(v, 'formalParameter'),
      conns: connsOf(child(v, 'connectionPointIn')),
    }));
  }
  return childrenNamed(node, 'connectionPointIn').map((cp) => ({ formal: null, conns: connsOf(cp) }));
}

let keys = 0;
const nextKey = () => {
  keys += 1;
  return keys;
};

/** Open a TC6 document. */
export function open(text: string): Opened {
  const parsed = parseXml(text);
  if (!parsed.ok) return { ok: false, error: parsed.error };
  const root = parsed.root;
  if (root.name !== 'project') return { ok: false, error: `its root is ${root.name}, not a TC6 project` };
  const types = child(root, 'types');
  const pousNode = types ? child(types, 'pous') : null;
  let maxId = 0;
  const sources: SourcePou[] = [];
  for (const node of pousNode ? childrenNamed(pousNode, 'pou') : []) {
    const body = child(node, 'body');
    const ld = body ? child(body, 'LD') : null;
    const iface = child(node, 'interface');
    const sp: SourcePou = {
      node,
      name: attr(node, 'name') ?? '',
      pouType: attr(node, 'pouType') ?? '',
      ld,
      iface,
      vars: varsOf(iface),
      byId: new Map(),
      left: [],
      right: [],
      others: [],
    };
    for (const c of ld?.children ?? []) {
      const id = intOf(attr(c, 'localId'));
      if (id !== null) maxId = Math.max(maxId, id);
      if (c.name === 'leftPowerRail') sp.left.push(c);
      else if (c.name === 'rightPowerRail') sp.right.push(c);
      if (id === null || c.name === 'leftPowerRail' || c.name === 'rightPowerRail') {
        if (c.name !== 'leftPowerRail' && c.name !== 'rightPowerRail') sp.others.push(c);
        if (id !== null) sp.byId.set(id, c);
        continue;
      }
      if (sp.byId.has(id)) {
        // Two elements of one id: the front-end refuses the document, and so does the editor.
        return { ok: false, error: `two elements of ${sp.name} have the localId ${id}` };
      }
      sp.byId.set(id, c);
    }
    const record = idsRecord(node);
    if (record !== null) maxId = Math.max(maxId, record - 1);
    sources.push(sp);
  }
  const source: Source = { text, root, pous: sources, firstFree: maxId + 1 };
  return { ok: true, source, model: { pous: sources.map((sp) => readPou(source, sp)), rev: 0 } };
}

/** The `next` of the editor's record of used localIds on a POU, when it has one. */
export function idsRecord(pou: XNode): number | null {
  const add = child(pou, 'addData');
  const data = add ? childrenNamed(add, 'data').find((d) => attr(d, 'name') === IDS_DATA) : undefined;
  const ids = data ? child(data, 'localIds') : null;
  return ids ? intOf(attr(ids, 'next')) : null;
}

/** The title a comment keeps in its `addData`, or `null` when it keeps none. */
export function titleOf(comment: XNode): string | null {
  const add = child(comment, 'addData');
  const data = add ? childrenNamed(add, 'data').find((d) => attr(d, 'name') === NETWORK_DATA) : undefined;
  if (!data) return null;
  return attr(child(data, 'network') ?? data, 'title') ?? '';
}

function readPou(source: Source, sp: SourcePou): PouModel {
  const model: PouModel = { name: sp.name, pouType: sp.pouType, networks: [], declared: [], touched: false };
  if (!sp.ld) return model;
  const elements = [...sp.byId.values()].filter(
    (n) => !['leftPowerRail', 'rightPowerRail', 'comment', 'label'].includes(n.name) && intOf(attr(n, 'localId')) !== null
  );
  const idOf = (n: XNode) => intOf(attr(n, 'localId')) as number;
  // Networks: connected elements, the rails aside (as the front-end groups them).
  const parent = new Map<number, number>(elements.map((n) => [idOf(n), idOf(n)]));
  const find = (k: number): number => {
    let at = k;
    while (parent.get(at) !== at) at = parent.get(at) as number;
    return at;
  };
  for (const n of elements) {
    for (const input of inputsOf(n)) {
      for (const c of input.conns) {
        if (parent.has(c.from)) parent.set(find(idOf(n)), find(c.from));
      }
    }
  }
  const groups = new Map<number, XNode[]>();
  for (const n of elements) groups.set(find(idOf(n)), [...(groups.get(find(idOf(n))) ?? []), n]);
  const topOf = (ns: XNode[]) =>
    ns.map((n) => ({ ...posOf(n), id: idOf(n) })).reduce((a, b) => (b.y < a.y || (b.y === a.y && b.x < a.x) ? b : a));
  // One sequence of networks, comments and labels, top to bottom; a label before the network
  // at its height, as the front-end orders them.
  type Entry = { y: number; rank: number; x: number; id: number; group?: XNode[]; node?: XNode };
  const entries: Entry[] = [...groups.values()].map((g) => {
    const top = topOf(g);
    return { y: top.y, rank: 2, x: top.x, id: top.id, group: g };
  });
  for (const n of sp.byId.values()) {
    if (n.name === 'comment' || n.name === 'label') {
      const p = posOf(n);
      entries.push({ y: p.y, rank: n.name === 'comment' ? 0 : 1, x: p.x, id: idOf(n), node: n });
    }
  }
  entries.sort((a, b) => a.y - b.y || a.rank - b.rank || a.x - b.x || a.id - b.id);
  let pending: XNode[] = [];
  const networkOf = (group: XNode[] | null): Network => {
    const comments = pending.filter((n) => n.name === 'comment');
    const labels = pending.filter((n) => n.name === 'label');
    const comment = comments[0] ?? null;
    const label = labels[0] ?? null;
    const net: Network = {
      key: nextKey(),
      title: comment ? (titleOf(comment) ?? '') : '',
      comment: comment ? deepText(child(comment, 'content') ?? comment).trim() : '',
      label: label ? (attr(label, 'label') ?? '') : '',
      commentId: comment ? idOf(comment) : null,
      labelId: label ? idOf(label) : null,
      rung: [],
      why: null,
      ids: group ? group.map(idOf) : [],
      extras: [...comments.slice(1), ...labels.slice(1)].map(idOf),
      left: null,
      right: null,
      dirty: false,
      headerDirty: false,
    };
    if (group) {
      const read = rungOf(source, sp, group);
      if (typeof read === 'string') {
        net.rung = null;
        net.why = read;
      } else {
        net.rung = read.rung;
        net.left = read.left;
        net.right = read.right;
      }
    }
    pending = [];
    return net;
  };
  for (const e of entries) {
    if (e.group) {
      model.networks.push(networkOf(e.group));
      continue;
    }
    const n = e.node as XNode;
    // A comment of this editor's starts a network; so does a second label: what was pending
    // before it was a network with no element.
    const header = n.name === 'comment' && titleOf(n) !== null;
    const secondLabel = n.name === 'label' && pending.some((p) => p.name === 'label');
    if (pending.length > 0 && (header || secondLabel)) model.networks.push(networkOf(null));
    pending.push(n);
  }
  if (pending.length > 0) model.networks.push(networkOf(null));
  return model;
}

/** A tree of series and parallels, while a network is reduced. */
type Tree = { t: 'el'; id: number } | { t: 'ser'; items: Tree[] } | { t: 'par'; items: Tree[] } | { t: 'wire' };

const POWER = new Set(['contact', 'coil', 'block', 'jump', 'return']);

/**
 * A network's rung as series and parallels, or why it is not one. The graph is reduced as a
 * series-parallel graph: each element an edge from the junction its input is on to the one its
 * output is on, the left rail one terminal and what ends the rung the other.
 */
function rungOf(source: Source, sp: SourcePou, group: XNode[]): { rung: Item[]; left: number | null; right: number | null } | string {
  const byId = sp.byId;
  const idOf = (n: XNode) => intOf(attr(n, 'localId')) as number;
  const members = new Set(group.map(idOf));
  const leftIds = new Set(sp.left.map(idOf));
  const rightIds = new Set(sp.right.map(idOf));
  for (const n of group) {
    if (!POWER.has(n.name) && n.name !== 'inVariable' && n.name !== 'outVariable') {
      return `it holds ${n.name === 'inOutVariable' ? 'an in-out variable' : `a ${n.name}`} (${idOf(n)}), which this editor does not draw as part of a rung`;
    }
  }
  // Who reads each output: [consumer id, its input's formal or null].
  const readers = new Map<number, Array<{ by: number; formal: string | null; from: string | null }>>();
  for (const n of [...group, ...sp.right]) {
    for (const input of inputsOf(n)) {
      for (const c of input.conns) {
        readers.set(c.from, [...(readers.get(c.from) ?? []), { by: idOf(n), formal: input.formal, from: c.formal }]);
      }
    }
  }
  const els = new Map<number, El>();
  const operandVars = new Set<number>();
  let left: number | null = null;
  let right: number | null = null;
  const own = ownBoxes(source);
  // The formal of a block's output a connection takes: its own, or the first that is not ENO.
  const outputOf = (block: XNode, formal: string | null): string => {
    if (formal) return formal.toUpperCase();
    const ov = child(block, 'outputVariables');
    const first = (ov ? childrenNamed(ov, 'variable') : []).map((v) => (attr(v, 'formalParameter') ?? '').toUpperCase()).find((f) => f !== 'ENO');
    return first ?? '';
  };
  const isPower = (id: number) => members.has(id) && POWER.has(byId.get(id)?.name ?? '');
  for (const n of group) {
    if (!POWER.has(n.name)) continue;
    const id = idOf(n);
    const el: El = {
      id,
      kind: n.name as ElKind,
      operand: '',
      negated: flag(n, 'negated'),
      edge: attr(n, 'edge') === 'rising' ? 'rising' : attr(n, 'edge') === 'falling' ? 'falling' : null,
      storage: attr(n, 'storage') === 'set' ? 'set' : attr(n, 'storage') === 'reset' ? 'reset' : null,
      typeName: '',
      instance: '',
      inputs: [],
      outputs: [],
      powerIn: '',
      powerOut: '',
    };
    if (n.name === 'contact' || n.name === 'coil') el.operand = (child(n, 'variable')?.text ?? '').trim();
    if (n.name === 'jump') el.operand = attr(n, 'label') ?? '';
    if (n.name === 'block') {
      el.typeName = attr(n, 'typeName') ?? '';
      el.instance = attr(n, 'instanceName') ?? '';
      const def = findBox(el.typeName, own);
      const typed = (formal: string, which: 'inputs' | 'outputs') => {
        const pins = def ? pinsOf(def) : null;
        return pins?.[which].find((p) => p.name === formal.toUpperCase())?.type ?? (formal.toUpperCase().startsWith('EN') ? 'BOOL' : 'ANY');
      };
      const iv = child(n, 'inputVariables');
      for (const v of iv ? childrenNamed(iv, 'variable') : []) {
        const formal = attr(v, 'formalParameter') ?? '';
        const cp = child(v, 'connectionPointIn');
        const conns = connsOf(cp);
        const pin: BoxPin = {
          formal,
          type: typed(formal, 'inputs'),
          operand: (cp ? child(cp, 'expression')?.text : null)?.trim() ?? '',
          negated: flag(v, 'negated'),
          varId: null,
        };
        const fromPower = conns.filter((c) => leftIds.has(c.from) || isPower(c.from));
        if (fromPower.length > 0) {
          if (fromPower.length !== conns.length) return `block ${id} takes ${formal} from the rung and from a variable at once`;
          if (el.powerIn !== '') return `block ${id} takes the rung's power at two pins (${el.powerIn}, ${formal})`;
          el.powerIn = formal;
        } else if (conns.length === 1) {
          const src = byId.get(conns[0]?.from ?? -1);
          if (!src || src.name !== 'inVariable' || !members.has(idOf(src)) || flag(src, 'negated')) {
            return `the pin ${formal} of block ${id} is wired to what this editor does not draw at a pin`;
          }
          if ((readers.get(idOf(src)) ?? []).length !== 1) return `the variable ${idOf(src)} feeds more than one pin`;
          pin.operand = (child(src, 'expression')?.text ?? '').trim();
          pin.varId = idOf(src);
          operandVars.add(idOf(src));
        } else if (conns.length > 1) {
          return `the pin ${formal} of block ${id} joins several wires`;
        }
        el.inputs.push(pin);
      }
      const ov = child(n, 'outputVariables');
      for (const v of ov ? childrenNamed(ov, 'variable') : []) {
        const formal = attr(v, 'formalParameter') ?? '';
        el.outputs.push({ formal, type: typed(formal, 'outputs'), operand: '', negated: flag(v, 'negated'), varId: null });
      }
      if (el.powerIn === '') return `block ${id} takes no power from the rung`;
      const powerOuts = new Set<string>();
      for (const r of readers.get(id) ?? []) {
        const formal = outputOf(n, r.from);
        const by = byId.get(r.by);
        if (by && (isPower(r.by) || rightIds.has(r.by))) {
          powerOuts.add(formal);
          continue;
        }
        if (!by || by.name !== 'outVariable' || !members.has(r.by) || flag(by, 'negated') || inputsOf(by)[0]?.conns.length !== 1) {
          return `the output ${formal} of block ${id} is wired to what this editor does not draw at a pin`;
        }
        const pin = el.outputs.find((o) => o.formal.toUpperCase() === formal);
        if (!pin || pin.varId !== null) return `the output ${formal} of block ${id} writes more than one variable`;
        pin.operand = (child(by, 'expression')?.text ?? '').trim();
        pin.varId = r.by;
        operandVars.add(r.by);
      }
      if (powerOuts.size > 1) return `block ${id} gives the rung's power from two outputs (${[...powerOuts].join(', ')})`;
      const defaultOut = def ? pinsOf(def).powerOut : null;
      el.powerOut =
        [...powerOuts][0] ??
        el.outputs.find((o) => o.formal.toUpperCase() === defaultOut)?.formal ??
        el.outputs.find((o) => o.type === 'BOOL')?.formal ??
        el.outputs[0]?.formal ??
        '';
      // The power pins are named as the block names them.
      el.powerOut = el.outputs.find((o) => o.formal.toUpperCase() === el.powerOut.toUpperCase())?.formal ?? el.powerOut;
    }
    els.set(id, el);
  }
  for (const n of group) {
    if (!POWER.has(n.name) && !operandVars.has(idOf(n))) {
      return `the variable ${idOf(n)} is wired as part of the rung, which this editor draws only at a pin of a box`;
    }
  }
  // The junctions: each input and output point, joined by the connections.
  const parent = new Map<string, string>();
  const find = (k: string): string => {
    if (!parent.has(k)) parent.set(k, k);
    let at = k;
    while (parent.get(at) !== at) at = parent.get(at) as string;
    return at;
  };
  const join = (a: string, b: string) => parent.set(find(a), find(b));
  for (const [id, el] of els) {
    const n = byId.get(id) as XNode;
    const input = el.kind === 'block' ? inputsOf(n).find((i) => i.formal === el.powerIn) : inputsOf(n)[0];
    const conns = input?.conns ?? [];
    if (conns.length === 0) return `${el.kind} ${id} has no input`;
    for (const c of conns) {
      if (leftIds.has(c.from)) {
        join(`in:${id}`, 'S');
        left = left ?? c.from;
      } else if (els.has(c.from)) {
        const src = els.get(c.from) as El;
        if (src.kind === 'jump' || src.kind === 'return') return `${el.kind} ${id} is wired from a ${src.kind}`;
        if (src.kind === 'block' && outputOf(byId.get(c.from) as XNode, c.formal) !== src.powerOut.toUpperCase()) {
          return `${el.kind} ${id} takes the output ${c.formal} of block ${c.from}, which does not carry the rung`;
        }
        join(`in:${id}`, `out:${c.from}`);
      } else {
        return `${el.kind} ${id} is wired from ${c.from}, which is not on the rung`;
      }
    }
    const rs = readers.get(id) ?? [];
    const toRail = rs.filter((r) => rightIds.has(r.by));
    if (toRail.length > 0) right = right ?? (toRail[0]?.by as number);
    const onward = rs.filter((r) => els.has(r.by));
    if (el.kind === 'jump' || el.kind === 'return' || onward.length === 0 || toRail.length > 0) join(`out:${id}`, 'T');
  }
  if (find('S') === find('T')) return 'a wire runs from the left rail to the end of the rung with nothing on it';
  // Reduce: parallels of one pair of junctions, series through a junction of one edge in and
  // one out, a branch that starts and ends at one junction (a wire around it) kept apart.
  let edges = [...els.keys()].map((id) => ({ a: find(`in:${id}`), b: find(`out:${id}`), tree: { t: 'el', id } as Tree }));
  const loops = new Map<string, Tree[]>();
  const S = find('S');
  const T = find('T');
  const minY = (t: Tree): number =>
    t.t === 'el' ? posOf(byId.get(t.id) as XNode).y : t.t === 'wire' ? Infinity : Math.min(...t.items.map(minY));
  const par = (items: Tree[]): Tree => ({
    t: 'par',
    items: items.flatMap((x) => (x.t === 'par' ? x.items : [x])).sort((p, q) => minY(p) - minY(q)),
  });
  const ser = (items: Tree[]): Tree => ({ t: 'ser', items: items.flatMap((x) => (x.t === 'ser' ? x.items : [x])) });
  const withLoops = (node: string): Tree[] => {
    const at = loops.get(node);
    if (!at) return [];
    loops.delete(node);
    return [par([...at, { t: 'wire' }])];
  };
  for (let changed = true; changed; ) {
    changed = false;
    for (const e of edges.filter((x) => x.a === x.b)) {
      loops.set(e.a, [...(loops.get(e.a) ?? []), e.tree]);
      changed = true;
    }
    edges = edges.filter((x) => x.a !== x.b);
    const byPair = new Map<string, typeof edges>();
    for (const e of edges) byPair.set(`${e.a}>${e.b}`, [...(byPair.get(`${e.a}>${e.b}`) ?? []), e]);
    if ([...byPair.values()].some((g) => g.length > 1)) {
      edges = [...byPair.values()].map((g) => (g.length === 1 ? (g[0] as (typeof edges)[0]) : { a: g[0]?.a as string, b: g[0]?.b as string, tree: par(g.map((x) => x.tree)) }));
      changed = true;
      continue;
    }
    const nodes = new Set(edges.flatMap((e) => [e.a, e.b]));
    for (const n of nodes) {
      if (n === S || n === T) continue;
      const ins = edges.filter((e) => e.b === n);
      const outs = edges.filter((e) => e.a === n);
      if (ins.length === 1 && outs.length === 1) {
        const [i, o] = [ins[0] as (typeof edges)[0], outs[0] as (typeof edges)[0]];
        edges = edges.filter((e) => e !== i && e !== o);
        edges.push({ a: i.a, b: o.b, tree: ser([i.tree, ...withLoops(n), o.tree]) });
        changed = true;
        break;
      }
    }
  }
  if (edges.length !== 1 || edges[0]?.a !== S || edges[0]?.b !== T || [...loops.keys()].some((k) => k !== S && k !== T)) {
    return 'its wiring is not a series of contacts, boxes and coils with parallel branches (it crosses between branches)';
  }
  const whole = ser([...withLoops(S), (edges[0] as (typeof edges)[0]).tree, ...withLoops(T)]);
  const items = (t: Tree): Item[] => {
    switch (t.t) {
      case 'el':
        return [{ t: 'el', el: els.get(t.id) as El }];
      case 'ser':
        return t.items.flatMap(items);
      case 'wire':
        return [];
      case 'par':
        return [{ t: 'par', branches: t.items.map(items) }];
    }
  };
  return { rung: items(whole), left, right };
}

// ---------------------------------------------------------------------------------------------
// Layout and writing
// ---------------------------------------------------------------------------------------------

/** The grid every position the editor writes is on. */
export const GRID = 10;
export const up = (v: number) => Math.ceil(v / GRID) * GRID;
/** About the width of one character of the diagram's small monospace text. */
export const CHAR = 7;
const GAP = 30;

/** The size of an element as written, and where its power pins are from its top. */
export function elementSize(el: El): { w: number; h: number; pinIn: number; pinOut: number } {
  if (el.kind === 'block') {
    const inText = Math.max(0, ...el.inputs.map((p) => (p.formal + (p.operand && p.varId === null ? ` := ${p.operand}` : '')).length));
    const outText = Math.max(0, ...el.outputs.map((p) => p.formal.length));
    const w = up(Math.max(80, CHAR * (inText + outText) + 30, CHAR * el.typeName.length + 20));
    const h = 20 + 20 * Math.max(1, el.inputs.length, el.outputs.length);
    const iIn = Math.max(0, el.inputs.findIndex((p) => p.formal === el.powerIn));
    const iOut = Math.max(0, el.outputs.findIndex((p) => p.formal === el.powerOut));
    return { w, h, pinIn: 30 + 20 * iIn, pinOut: 30 + 20 * iOut };
  }
  if (el.kind === 'jump' || el.kind === 'return') return { w: 80, h: 20, pinIn: 10, pinOut: 10 };
  return { w: 30, h: 20, pinIn: 10, pinOut: 10 };
}

/** An operand variable's width, as the viewer sizes one. */
const varWidth = (text: string) => up(Math.max(60, 8 * text.length + 16));

interface Measure {
  w: number;
  asc: number;
  desc: number;
}

function measureEl(el: El): Measure & { left: number; inner: number } {
  const s = elementSize(el);
  if (el.kind === 'block') {
    const left = Math.max(0, ...el.inputs.filter((p) => p.varId !== null).map((p) => varWidth(p.operand) + 20));
    const right = Math.max(0, ...el.outputs.filter((p) => p.operand !== '' && p.formal !== el.powerOut).map((p) => varWidth(p.operand) + 20));
    const inner = Math.max(s.w, up(CHAR * el.instance.length + 4));
    return { w: left + inner + right + GAP, asc: s.pinIn + 20, desc: s.h - s.pinIn + 20, left, inner };
  }
  const inner = up(Math.max(s.w, CHAR * el.operand.length + 4, el.kind === 'jump' ? CHAR * (el.operand.length + 3) : 0));
  return { w: inner + GAP, asc: 30, desc: 40, left: 0, inner };
}

const EMPTY: Measure = { w: 40, asc: 20, desc: 10 };
const PAD = 20;

function measureSeries(items: Item[]): Measure {
  if (items.length === 0) return EMPTY;
  const ms = items.map(measureItem);
  return { w: ms.reduce((a, m) => a + m.w, 0), asc: Math.max(...ms.map((m) => m.asc)), desc: Math.max(...ms.map((m) => m.desc)) };
}

function branchLines(branches: Item[][]): { lines: number[]; ms: Measure[] } {
  const ms = branches.map(measureSeries);
  const lines: number[] = [];
  ms.forEach((m, k) => {
    const prev = ms[k - 1];
    lines.push(k === 0 ? 0 : (lines[k - 1] as number) + (prev?.desc ?? 0) + m.asc);
  });
  return { lines, ms };
}

function measureItem(item: Item): Measure {
  if (item.t === 'el') return measureEl(item.el);
  const { lines, ms } = branchLines(item.branches);
  return {
    w: PAD + Math.max(...ms.map((m) => m.w)) + PAD,
    asc: ms[0]?.asc ?? 20,
    desc: (lines[lines.length - 1] ?? 0) + (ms[ms.length - 1]?.desc ?? 10),
  };
}

/** Where each element of a laid-out rung is. */
interface Placed {
  el: El;
  x: number;
  y: number;
}

function placeSeries(items: Item[], x: number, line: number, out: Placed[]): void {
  let at = x;
  for (const item of items) {
    const m = measureItem(item);
    if (item.t === 'el') {
      const me = measureEl(item.el);
      const s = elementSize(item.el);
      const ex = item.el.kind === 'block' ? at + me.left : at + up((me.inner - s.w) / 2);
      out.push({ el: item.el, x: ex, y: line - s.pinIn });
    } else {
      const { lines } = branchLines(item.branches);
      item.branches.forEach((b, k) => placeSeries(b, at + PAD, line + (lines[k] as number), out));
    }
    at += m.w;
  }
}

/** A source of power for what follows: the left rail, or an element's power output. */
type Src = { rail: true } | { rail: false; id: number; formal: string | null };

/** What the editor needs to draw a network it wrote: its elements and its band. */
export interface Drawn {
  key: number;
  /** The elements of the network in the written document, comments and labels aside. */
  ids: number[];
  /** The top of its band, where its comment is. */
  top: number;
  bottom: number;
}

export interface Written {
  text: string;
  /** Per POU, per network, in order. */
  drawn: Drawn[][];
}

/** Allocates localIds: counted up from the first free one, never back. */
export interface Ids {
  next: number;
  /** POUs an id was given out in, by index: their record is written. */
  used: Set<number>;
}

export const idsFor = (source: Source): Ids => ({ next: source.firstFree, used: new Set() });

export const indentOf = (src: string, ld: XNode): { inner: string; close: string } => {
  const first = ld.children[0];
  if (!first) return { inner: '\n  ', close: '\n' };
  const inner = src.slice(ld.openEnd, first.start);
  const last = ld.children[ld.children.length - 1] as XNode;
  return { inner: /^\s*$/.test(inner) ? inner : '\n', close: src.slice(last.end, ld.closeStart).replace(/\S/g, '') };
};

/** The children of an element the editor does not write (its addData, documentation, …), as found. */
export function extrasOf(src: string, node: XNode | undefined, written: string[]): string {
  if (!node) return '';
  return node.children
    .filter((c) => !written.includes(c.name))
    .map((c) => src.slice(c.start, c.end))
    .join('');
}

/** Its attributes the editor does not write, as found. */
export function extraAttrs(node: XNode | undefined, written: string[]): Array<[string, string]> {
  return node ? node.attrs.filter((a) => !written.includes(a.name)).map((a) => [a.qname, a.value]) : [];
}

const ELEMENT_ATTRS = ['localId', 'width', 'height', 'negated', 'edge', 'storage', 'typeName', 'instanceName', 'label'];

/** Write the document from the model: the source's text where nothing changed. */
export function write(source: Source, model: DocModel, ids: Ids): Written {
  const src = source.text;
  const edits: Splice[] = [];
  const drawn: Drawn[][] = [];
  model.pous.forEach((pou, index) => {
    const sp = source.pous[index] as SourcePou;
    const body = layoutPou(source, sp, pou);
    drawn.push(body.drawn);
    if (pou.touched && sp.ld) {
      if (sp.ld.selfClosing) {
        const qname = sp.ld.qname;
        edits.push({ start: sp.ld.start, end: sp.ld.end, text: `<${qname}>${body.parts.map((p) => `\n${p}`).join('')}\n</${qname}>` });
      } else {
        const { inner, close } = indentOf(src, sp.ld);
        edits.push({ start: sp.ld.openEnd, end: sp.ld.closeStart, text: body.parts.map((p) => inner + p).join('') + close });
      }
    }
    if (pou.declared.length > 0) edits.push(...declare(sp, pou.declared));
    if (pou.touched || ids.used.has(index)) edits.push(record(sp, Math.max(ids.next, idsRecord(sp.node) ?? 0)));
  });
  return { text: edits.length === 0 ? src : splice(src, edits), drawn };
}

/** The variables declared in the editor, put into the interface: each in its section. */
export function declare(sp: PouParts, declared: Declared[]): Splice[] {
  const prefix = sp.node.qname.includes(':') ? `${sp.node.qname.split(':')[0]}:` : '';
  const variable = (d: Declared) => {
    const elementary = ['BOOL', 'SINT', 'INT', 'DINT', 'LINT', 'REAL', 'LREAL', 'TIME', 'STRING'];
    const upper = d.type.toUpperCase();
    const type =
      upper === 'STRING' ? `<${prefix}string/>` : elementary.includes(upper) ? `<${prefix}${upper}/>` : `<${prefix}derived name="${escapeAttr(d.type)}"/>`;
    return `<${prefix}variable name="${escapeAttr(d.name)}"><${prefix}type>${type}</${prefix}type></${prefix}variable>`;
  };
  const order: Section[] = ['inputVars', 'outputVars', 'localVars'];
  const iface = sp.iface;
  if (!iface || iface.selfClosing) {
    const sections = order
      .filter((s) => declared.some((d) => d.section === s))
      .map((s) => `<${prefix}${s}>${declared.filter((d) => d.section === s).map(variable).join('')}</${prefix}${s}>`)
      .join('');
    if (iface) return [{ start: iface.start, end: iface.end, text: `<${iface.qname}>${sections}</${iface.qname}>` }];
    // A POU with no interface: one is put before its body, where the schema has it.
    const at = child(sp.node, 'body')?.start ?? sp.node.openEnd;
    return [{ start: at, end: at, text: `<${prefix}interface>${sections}</${prefix}interface>` }];
  }
  const out: Splice[] = [];
  const plain = (s: XNode) => !['retain', 'nonretain', 'constant', 'persistent'].some((a) => flag(s, a));
  const tail = iface.children.find((c) => c.name === 'addData' || c.name === 'documentation')?.start ?? iface.closeStart;
  for (const s of order) {
    const these = declared.filter((d) => d.section === s);
    if (these.length === 0) continue;
    const section = childrenNamed(iface, s).find(plain);
    const text = these.map(variable).join('');
    if (!section) out.push({ start: tail, end: tail, text: `<${prefix}${s}>${text}</${prefix}${s}>` });
    else if (section.selfClosing) out.push({ start: section.start, end: section.end, text: `<${section.qname}>${text}</${section.qname}>` });
    else {
      const at = section.children.find((c) => c.name === 'addData' || c.name === 'documentation')?.start ?? section.closeStart;
      out.push({ start: at, end: at, text });
    }
  }
  return out;
}

/** The record of the localIds the editor has used, in the POU's `addData`. */
export function record(sp: PouParts, next: number): Splice {
  const prefix = sp.node.qname.includes(':') ? `${sp.node.qname.split(':')[0]}:` : '';
  const data = `<${prefix}data name="${IDS_DATA}" handleUnknown="preserve"><localIds xmlns="${IDS_DATA}" next="${next}"/></${prefix}data>`;
  const add = child(sp.node, 'addData');
  if (!add) {
    const bodies = childrenNamed(sp.node, 'body');
    const at = bodies[bodies.length - 1]?.end ?? sp.node.closeStart;
    return { start: at, end: at, text: `<${prefix}addData>${data}</${prefix}addData>` };
  }
  const mine = childrenNamed(add, 'data').find((d) => attr(d, 'name') === IDS_DATA);
  if (mine) return { start: mine.start, end: mine.end, text: data };
  if (add.selfClosing) return { start: add.start, end: add.end, text: `<${add.qname}>${data}</${add.qname}>` };
  return { start: add.closeStart, end: add.closeStart, text: data };
}

/** Lay out a POU's body: the text of each child, and where each network went. */
function layoutPou(source: Source, sp: SourcePou, pou: PouModel): { parts: string[]; drawn: Drawn[] } {
  const src = source.text;
  const prefix = sp.ld && sp.ld.qname.includes(':') ? `${sp.ld.qname.split(':')[0]}:` : '';
  const q = (name: string) => prefix + name;
  const idOf = (n: XNode) => intOf(attr(n, 'localId')) as number;
  const leftRail = sp.left[0];
  const railX = leftRail ? posOf(leftRail).x + (numOf(attr(leftRail, 'width')) ?? 3) : 3;
  const sizeOfNode = (n: XNode) => {
    const w = numOf(attr(n, 'width'));
    const h = numOf(attr(n, 'height'));
    if (w !== null && h !== null) return { w, h };
    if (n.name === 'block') {
      const count = (k: string) => childrenNamed(child(n, k) ?? n, 'variable').length;
      return { w: w ?? 100, h: h ?? 20 + 20 * Math.max(1, count('inputVariables'), count('outputVariables')) };
    }
    if (n.name === 'contact' || n.name === 'coil') return { w: w ?? 30, h: h ?? 20 };
    return { w: w ?? 80, h: h ?? 20 };
  };
  const parts: string[] = [];
  const drawn: Drawn[] = [];
  const networkParts: string[] = [];
  const toRight = new Map<number, number[]>();
  // Networks are laid out from the top of the body: the highest thing in it, or 0. One written
  // as found is never moved up, so starting high moves nothing that was there.
  let cursor = Math.min(0, ...(sp.ld?.children ?? []).filter((c) => child(c, 'position')).map((c) => posOf(c).y));
  let maxRight = 0;
  const keptIds = new Set<number>();
  for (const net of pou.networks) {
    const comment = net.commentId !== null ? sp.byId.get(net.commentId) : undefined;
    const label = net.labelId !== null ? sp.byId.get(net.labelId) : undefined;
    const hasComment = net.title !== '' || net.comment !== '';
    const hasLabel = net.label !== '' && net.labelId !== null;
    if (!net.dirty && net.ids.length > 0) {
      // Written as it was, moved down when what is above it grew past it. A comment or label
      // it did not have is put above it, with room made for it.
      const own = [...net.ids, ...net.extras].map((id) => sp.byId.get(id)).filter((n): n is XNode => n !== undefined);
      const found = [...own, ...[comment, label].filter((n): n is XNode => n !== undefined)];
      const top = Math.min(...found.map((n) => posOf(n).y));
      const elementsTop = Math.min(...net.ids.map((id) => posOf(sp.byId.get(id) as XNode).y));
      const room = (hasComment && !comment ? commentHeight(net) + 10 : 0) + (hasLabel && !label ? 20 : 0);
      const dy = Math.max(0, cursor + room - top);
      const bottom = Math.max(...found.map((n) => posOf(n).y + sizeOfNode(n).h));
      if (net.headerDirty) {
        const cAt = comment ? posOf(comment) : { x: railX + 10, y: top - room };
        const c = commentPart(src, sp, net, q, cAt.x, cAt.y + dy);
        if (c) networkParts.push(c.text);
        const lAt = label ? posOf(label) : { x: up(railX + 6), y: elementsTop - 20 };
        const l = labelPart(src, sp, net, q, lAt.x, lAt.y + dy);
        if (l) networkParts.push(l);
      } else {
        for (const n of [comment, label]) if (n) networkParts.push(shifted(src, n, dy));
      }
      for (const n of own) networkParts.push(shifted(src, n, dy));
      for (const id of net.ids) {
        keptIds.add(id);
        const n = sp.byId.get(id) as XNode;
        maxRight = Math.max(maxRight, posOf(n).x + sizeOfNode(n).w);
      }
      drawn.push({ key: net.key, ids: net.ids, top: Math.min(top + dy, cursor), bottom: bottom + dy });
      cursor = up(bottom + dy + 30);
      continue;
    }
    // Laid out anew: the comment, then the label, then the rung.
    const top = cursor;
    let at = top;
    const c = commentPart(src, sp, net, q, railX + 10, at);
    if (c) {
      networkParts.push(c.text);
      at += c.h + 10;
    }
    const l = labelPart(src, sp, net, q, up(railX + 6), at);
    if (l) {
      networkParts.push(l);
      at += 20;
    }
    for (const id of net.extras) {
      const n = sp.byId.get(id);
      if (!n) continue;
      networkParts.push(shifted(src, n, at - posOf(n).y));
      at += sizeOfNode(n).h + 10;
    }
    const rung = net.rung ?? [];
    const m = measureSeries(rung);
    const line = up(at + m.asc);
    const placed: Placed[] = [];
    placeSeries(rung, up(railX + GAP), line, placed);
    const written = emitRung(src, sp, net, rung, placed, q, leftRail ? idOf(leftRail) : null);
    networkParts.push(...written.parts);
    for (const id of written.toRail) {
      const rail = net.right ?? (sp.right[0] ? idOf(sp.right[0]) : null);
      if (rail !== null) toRight.set(rail, [...(toRight.get(rail) ?? []), id]);
    }
    maxRight = Math.max(maxRight, written.right);
    const bottom = rung.length > 0 ? line + m.desc : at;
    drawn.push({ key: net.key, ids: written.ids, top, bottom });
    cursor = up(bottom + 20);
  }
  // The rails: as found, the right one moved clear of the rungs, its connections those of the
  // networks written as found and of the ones written anew. A rail is not stretched or moved
  // down: the editor draws them per network, and TC6 gives their length only for drawing.
  const railPart = (n: XNode, right: boolean): string => {
    const p = posOf(n);
    const newX = right && maxRight + 30 > p.x ? up(maxRight + 30) : p.x;
    const id = idOf(n);
    const conns = right ? inputsOf(n) : [];
    const keep = right
      ? childrenNamed(n, 'connectionPointIn').filter((cp) => connsOf(cp).length > 0 && connsOf(cp).every((c) => keptIds.has(c.from)))
      : [];
    const added = right ? (toRight.get(id) ?? []) : [];
    const sameConns = !right || (keep.length === conns.length && added.length === 0);
    if (newX === p.x && sameConns) return src.slice(n.start, n.end);
    const attrs: Array<[string, string]> = n.attrs.map((a) => [a.qname, a.value]);
    const position = child(n, 'position');
    const posText = position
      ? openTag(position.qname, position.attrs.map((a) => [a.qname, a.name === 'x' ? num(newX) : a.value]), true)
      : `<${q('position')} x="${num(newX)}" y="0"/>`;
    const others = n.children.filter((c) => c.name !== 'position' && c.name !== 'connectionPointIn').map((c) => src.slice(c.start, c.end));
    const cps = right
      ? [...keep.map((cp) => src.slice(cp.start, cp.end)), ...added.map((from) => `<${q('connectionPointIn')}><${q('connection')} refLocalId="${from}"/></${q('connectionPointIn')}>`)]
      : childrenNamed(n, 'connectionPointIn').map((cp) => src.slice(cp.start, cp.end));
    const outs = others.filter((t) => /^<([\w-]+:)?connectionPointOut\b/.test(t));
    const rest = others.filter((t) => !/^<([\w-]+:)?connectionPointOut\b/.test(t));
    return `${openTag(n.qname, attrs)}${posText}${cps.join('')}${outs.join('')}${rest.join('')}</${n.qname}>`;
  };
  for (const n of sp.left) parts.push(railPart(n, false));
  for (const n of sp.others) parts.push(src.slice(n.start, n.end));
  parts.push(...networkParts);
  for (const n of sp.right) parts.push(railPart(n, true));
  return { parts, drawn };
}

export const commentHeight = (net: Pick<Network, 'comment'>) => 16 * Math.max(1, net.comment.split('\n').length) + 8;

/** A network's comment: as found when nothing of it changed, else written with its title. */
export function commentPart(
  src: string,
  sp: PouParts,
  net: Header,
  q: (name: string) => string,
  x: number,
  y: number
): { text: string; h: number } | null {
  // A network with neither title nor comment has no comment element: one it had is removed.
  if (net.commentId === null || (net.title === '' && net.comment === '')) return null;
  const found = sp.byId.get(net.commentId);
  const h = commentHeight(net);
  const w = up(Math.max(200, CHAR * Math.max(net.title.length, ...net.comment.split('\n').map((l) => l.length)) + 16));
  const sameText = found ? deepText(child(found, 'content') ?? found).trim() === net.comment : false;
  const content =
    found && sameText && child(found, 'content')
      ? src.slice((child(found, 'content') as XNode).start, (child(found, 'content') as XNode).end)
      : `<${q('content')}><xhtml:p xmlns:xhtml="${XHTML}">${escapeText(net.comment)}</xhtml:p></${q('content')}>`;
  const mine = `<${q('data')} name="${NETWORK_DATA}" handleUnknown="preserve"><network xmlns="${NETWORK_DATA}" title="${escapeAttr(net.title)}"/></${q('data')}>`;
  const add = found ? child(found, 'addData') : null;
  const addText = add
    ? `<${add.qname}>${add.children.filter((d) => !(d.name === 'data' && attr(d, 'name') === NETWORK_DATA)).map((d) => src.slice(d.start, d.end)).join('')}${mine}</${add.qname}>`
    : `<${q('addData')}>${mine}</${q('addData')}>`;
  const rest = found ? extrasOf(src, found, ['position', 'content', 'addData']) : '';
  const attrs: Array<[string, string]> = [
    ['localId', String(net.commentId)],
    ['height', num(h)],
    ['width', num(w)],
    ...extraAttrs(found, ['localId', 'height', 'width']),
  ];
  return { text: `${openTag(found?.qname ?? q('comment'), attrs)}<${q('position')} x="${num(x)}" y="${num(y)}"/>${content}${addText}${rest}</${found?.qname ?? q('comment')}>`, h };
}

export function labelPart(src: string, sp: PouParts, net: Header, q: (name: string) => string, x: number, y: number): string | null {
  if (net.labelId === null || net.label === '') return null;
  const found = sp.byId.get(net.labelId);
  const attrs: Array<[string, string]> = [
    ['localId', String(net.labelId)],
    ['height', '20'],
    ['width', num(up(Math.max(80, CHAR * (net.label.length + 2))))],
    ['label', net.label],
    ...extraAttrs(found, ['localId', 'height', 'width', 'label']),
  ];
  const rest = extrasOf(src, found, ['position']);
  return `${openTag(found?.qname ?? q('label'), attrs)}<${q('position')} x="${num(x)}" y="${num(y)}"/>${rest}</${found?.qname ?? q('label')}>`;
}

/** The elements of a rung laid out at `placed`, wired in series and parallel. */
function emitRung(
  src: string,
  sp: SourcePou,
  net: Network,
  rung: Item[],
  placed: Placed[],
  q: (name: string) => string,
  mainLeft: number | null
): { parts: string[]; ids: number[]; toRail: number[]; right: number } {
  const at = new Map(placed.map((p) => [p.el.id, p]));
  const parts: string[] = [];
  const ids: number[] = [];
  let right = 0;
  const leftId = net.left ?? mainLeft;
  const cpIn = (sources: Src[], rel: { x: number; y: number }) => {
    const conns = sources
      .map((s) =>
        s.rail
          ? leftId === null
            ? ''
            : `<${q('connection')} refLocalId="${leftId}"/>`
          : `<${q('connection')} refLocalId="${s.id}"${s.formal ? ` formalParameter="${escapeAttr(s.formal)}"` : ''}/>`
      )
      .join('');
    return `<${q('connectionPointIn')}><${q('relPosition')} x="${num(rel.x)}" y="${num(rel.y)}"/>${conns}</${q('connectionPointIn')}>`;
  };
  const cpOut = (rel: { x: number; y: number }) =>
    `<${q('connectionPointOut')}><${q('relPosition')} x="${num(rel.x)}" y="${num(rel.y)}"/></${q('connectionPointOut')}>`;
  const position = (x: number, y: number) => `<${q('position')} x="${num(x)}" y="${num(y)}"/>`;
  const element = (el: El, sources: Src[]) => {
    const p = at.get(el.id) as Placed;
    const s = elementSize(el);
    const found = sp.byId.get(el.id);
    const name = found?.qname ?? q(el.kind);
    const base: Array<[string, string]> = [
      ['localId', String(el.id)],
      ['width', num(s.w)],
      ['height', num(s.h)],
    ];
    right = Math.max(right, p.x + s.w);
    ids.push(el.id);
    if (el.kind === 'contact' || el.kind === 'coil') {
      const attrs: Array<[string, string]> = [...base];
      if (el.negated) attrs.push(['negated', 'true']);
      if (el.edge) attrs.push(['edge', el.edge]);
      if (el.kind === 'coil' && el.storage) attrs.push(['storage', el.storage]);
      attrs.push(...extraAttrs(found, ELEMENT_ATTRS));
      parts.push(
        `${openTag(name, attrs)}${position(p.x, p.y)}${cpIn(sources, { x: 0, y: s.pinIn })}${cpOut({ x: s.w, y: s.pinOut })}<${q('variable')}>${escapeText(el.operand)}</${q('variable')}>${extrasOf(src, found, ['position', 'connectionPointIn', 'connectionPointOut', 'variable'])}</${name}>`
      );
      return;
    }
    if (el.kind === 'jump' || el.kind === 'return') {
      const attrs: Array<[string, string]> = [...base];
      if (el.kind === 'jump') attrs.push(['label', el.operand]);
      attrs.push(...extraAttrs(found, ELEMENT_ATTRS));
      parts.push(`${openTag(name, attrs)}${position(p.x, p.y)}${cpIn(sources, { x: 0, y: s.pinIn })}${extrasOf(src, found, ['position', 'connectionPointIn'])}</${name}>`);
      return;
    }
    // A block: its pins in order, an operand written at each input, a variable at each output.
    const attrs: Array<[string, string]> = [...base, ['typeName', el.typeName]];
    if (el.instance !== '') attrs.push(['instanceName', el.instance]);
    attrs.push(...extraAttrs(found, ELEMENT_ATTRS));
    const pinExtras = (which: string, formal: string) => {
      const list = found ? child(found, which) : null;
      const v = list ? childrenNamed(list, 'variable').find((x) => attr(x, 'formalParameter') === formal) : undefined;
      return extrasOf(src, v, ['connectionPointIn', 'connectionPointOut']);
    };
    const inputs = el.inputs
      .map((pin, i) => {
        const rel = { x: 0, y: 30 + 20 * i };
        const neg = pin.negated ? ' negated="true"' : '';
        let point: string;
        if (pin.formal === el.powerIn) point = cpIn(sources, rel);
        else if (pin.varId !== null) {
          // Another tool's input variable, kept: it stays where it is wired from.
          const vw = varWidth(pin.operand);
          const vx = p.x - 20 - vw;
          const vy = p.y + rel.y - 10;
          const vfound = sp.byId.get(pin.varId);
          ids.push(pin.varId);
          parts.push(
            `${openTag(vfound?.qname ?? q('inVariable'), [['localId', String(pin.varId)], ['width', num(vw)], ['height', '20'], ...extraAttrs(vfound, ['localId', 'width', 'height'])])}${position(vx, vy)}${cpOut({ x: vw, y: 10 })}<${q('expression')}>${escapeText(pin.operand)}</${q('expression')}>${extrasOf(src, vfound, ['position', 'connectionPointOut', 'expression'])}</${vfound?.qname ?? q('inVariable')}>`
          );
          point = `<${q('connectionPointIn')}><${q('relPosition')} x="0" y="${num(rel.y)}"/><${q('connection')} refLocalId="${pin.varId}"/></${q('connectionPointIn')}>`;
        } else if (pin.operand !== '') {
          point = `<${q('connectionPointIn')}><${q('relPosition')} x="0" y="${num(rel.y)}"/><${q('expression')}>${escapeText(pin.operand)}</${q('expression')}></${q('connectionPointIn')}>`;
        } else point = `<${q('connectionPointIn')}><${q('relPosition')} x="0" y="${num(rel.y)}"/></${q('connectionPointIn')}>`;
        return `<${q('variable')} formalParameter="${escapeAttr(pin.formal)}"${neg}>${point}${pinExtras('inputVariables', pin.formal)}</${q('variable')}>`;
      })
      .join('');
    const outputs = el.outputs
      .map((pin, i) => {
        const neg = pin.negated ? ' negated="true"' : '';
        return `<${q('variable')} formalParameter="${escapeAttr(pin.formal)}"${neg}>${cpOut({ x: s.w, y: 30 + 20 * i })}${pinExtras('outputVariables', pin.formal)}</${q('variable')}>`;
      })
      .join('');
    parts.push(
      `${openTag(name, attrs)}${position(p.x, p.y)}<${q('inputVariables')}>${inputs}</${q('inputVariables')}><${q('inOutVariables')}/><${q('outputVariables')}>${outputs}</${q('outputVariables')}>${extrasOf(src, found, ['position', 'inputVariables', 'inOutVariables', 'outputVariables'])}</${name}>`
    );
    // Each output's operand: an output variable wired to it, beside the box.
    el.outputs.forEach((pin, i) => {
      if (pin.formal === el.powerOut || pin.operand === '' || pin.varId === null) return;
      const vw = varWidth(pin.operand);
      const vx = p.x + s.w + 20;
      const vy = p.y + 30 + 20 * i - 10;
      const vfound = sp.byId.get(pin.varId);
      right = Math.max(right, vx + vw);
      ids.push(pin.varId);
      parts.push(
        `${openTag(vfound?.qname ?? q('outVariable'), [['localId', String(pin.varId)], ['width', num(vw)], ['height', '20'], ...extraAttrs(vfound, ['localId', 'width', 'height'])])}${position(vx, vy)}<${q('connectionPointIn')}><${q('relPosition')} x="0" y="10"/><${q('connection')} refLocalId="${el.id}" formalParameter="${escapeAttr(pin.formal)}"/></${q('connectionPointIn')}><${q('expression')}>${escapeText(pin.operand)}</${q('expression')}>${extrasOf(src, vfound, ['position', 'connectionPointIn', 'expression'])}</${vfound?.qname ?? q('outVariable')}>`
      );
    });
  };
  const series = (items: Item[], sources: Src[]): Src[] => {
    let now = sources;
    for (const item of items) {
      if (item.t === 'el') {
        element(item.el, now);
        now =
          item.el.kind === 'jump' || item.el.kind === 'return'
            ? []
            : [{ rail: false, id: item.el.id, formal: item.el.kind === 'block' ? item.el.powerOut : null }];
      } else {
        const ends: Src[] = [];
        for (const b of item.branches) {
          for (const s of series(b, now)) {
            if (!ends.some((e) => (e.rail && s.rail) || (!e.rail && !s.rail && e.id === s.id && e.formal === s.formal))) ends.push(s);
          }
        }
        now = ends;
      }
    }
    return now;
  };
  const ends = series(rung, [{ rail: true }]);
  const coils = new Set(placed.filter((p) => p.el.kind === 'coil').map((p) => p.el.id));
  const toRail = ends.filter((s): s is Extract<Src, { rail: false }> => !s.rail && coils.has(s.id)).map((s) => s.id);
  return { parts, ids, toRail, right };
}
