// The edits of the Ladder Diagram editor, as functions from one model to the next (ADR-013
// L3b-2b), and its own check of what is written in it. Each edit returns a new model and
// leaves the one it was given as it was: the editor keeps those it passed through for undo.
//
// **Where an instruction goes** (as an editor of the TIA Portal kind inserts): after the
// selected element, in its series; at a selected point between two elements; or, with only
// the network selected, a contact or box before the coils at the end of its rung and a coil,
// jump or return at the end. A jump or a return ends its rung: nothing is put after one.
//
// **A parallel branch** is opened at the first element of a selection and closed after the
// last (both in one series): the selection becomes the upper branch, and a new, empty branch
// below it gets the insertion point. An empty branch is a wire, so what is in parallel with it
// is bypassed until something is put into it, and the editor's check says so.

import { family, findBox, pinsOf, type BoxDef, STANDARD } from './ldcatalog';
import { declaredType, LIBRARY_GROUP, pinProblems, unknownBoxWords, type LibraryBoxes } from './libraryblocks';
import type { BoxPin, Declared, DocModel, El, Ids, Item, Network, PouModel, Section, VarInfo } from './ldmodel';

/** What is selected: a network, an element (or a run of elements of one series), or a point between two. */
export type Sel =
  | { kind: 'network'; pou: number; net: number }
  | { kind: 'element'; pou: number; net: number; id: number; to: number | null }
  | { kind: 'gap'; pou: number; net: number; path: number[]; index: number };

/** An instruction of the palette. */
export type Spec =
  | { kind: 'contact'; negated?: boolean; edge?: 'rising' | 'falling' }
  | { kind: 'coil'; negated?: boolean; storage?: 'set' | 'reset'; edge?: 'rising' | 'falling' }
  | { kind: 'box'; type: string }
  | { kind: 'jump' }
  | { kind: 'return' };

export type Edited = { ok: true; model: DocModel; sel: Sel | null } | { ok: false; error: string };

const clone = (model: DocModel): DocModel => ({ ...(JSON.parse(JSON.stringify(model)) as DocModel), rev: model.rev + 1 });

/** The series at `path` in a rung: item and branch indices taken in turn. */
export function seriesAt(rung: Item[], path: number[]): Item[] | null {
  let at: Item[] = rung;
  for (let k = 0; k < path.length; k += 2) {
    const item = at[path[k] as number];
    if (!item || item.t !== 'par') return null;
    const branch = item.branches[path[k + 1] as number];
    if (!branch) return null;
    at = branch;
  }
  return at;
}

/** Where element `id` is: the path of its series and its index in it. */
export function locate(rung: Item[], id: number, path: number[] = []): { path: number[]; index: number } | null {
  for (let i = 0; i < rung.length; i += 1) {
    const item = rung[i] as Item;
    if (item.t === 'el') {
      if (item.el.id === id) return { path, index: i };
    } else {
      for (let b = 0; b < item.branches.length; b += 1) {
        const found = locate(item.branches[b] as Item[], id, [...path, i, b]);
        if (found) return found;
      }
    }
  }
  return null;
}

/** Every element of a rung, in order. */
export function elementsOf(rung: Item[]): El[] {
  return rung.flatMap((item) => (item.t === 'el' ? [item.el] : item.branches.flatMap(elementsOf)));
}

export function findEl(model: DocModel, pou: number, id: number): { net: number; el: El } | null {
  const p = model.pous[pou];
  for (const [n, net] of (p?.networks ?? []).entries()) {
    const el = net.rung ? elementsOf(net.rung).find((e) => e.id === id) : undefined;
    if (el) return { net: n, el };
  }
  return null;
}

const isOutput = (item: Item | undefined) => item?.t === 'el' && ['coil', 'jump', 'return'].includes(item.el.kind);
const ends = (item: Item | undefined) => item?.t === 'el' && (item.el.kind === 'jump' || item.el.kind === 'return');

/** Whether the end of the series at `path` is the end of the rung. */
function atRungEnd(rung: Item[], path: number[]): boolean {
  let at = rung;
  for (let k = 0; k < path.length; k += 2) {
    if (path[k] !== at.length - 1) return false;
    const item = at[path[k] as number];
    if (item?.t !== 'par') return false;
    at = item.branches[path[k + 1] as number] ?? [];
  }
  return true;
}

function touch(pou: PouModel, net: Network): void {
  pou.touched = true;
  net.dirty = true;
}

/** A new element of `spec`. */
export function newElement(spec: Spec, id: number, own: readonly BoxDef[]): El {
  const el: El = {
    id,
    kind: spec.kind === 'box' ? 'block' : spec.kind,
    operand: '',
    negated: false,
    edge: null,
    storage: null,
    typeName: '',
    instance: '',
    inputs: [],
    outputs: [],
    powerIn: 'EN',
    powerOut: 'ENO',
  };
  if (spec.kind === 'contact') {
    el.negated = spec.negated === true;
    el.edge = spec.edge ?? null;
  }
  if (spec.kind === 'coil') {
    el.negated = spec.negated === true;
    el.storage = spec.storage ?? null;
    el.edge = spec.edge ?? null;
  }
  if (spec.kind === 'box') retype(el, spec.type, own);
  return el;
}

/** A box given type `type`: its pins as the type declares them, operands of pins it keeps kept. */
function retype(el: El, type: string, own: readonly BoxDef[]): void {
  const def = findBox(type, own);
  el.typeName = def?.name ?? type.trim();
  if (!def) {
    el.inputs = [{ formal: 'EN', type: 'BOOL', operand: '', negated: false, varId: null }];
    el.outputs = [{ formal: 'ENO', type: 'BOOL', operand: '', negated: false, varId: null }];
    el.powerIn = 'EN';
    el.powerOut = 'ENO';
    return;
  }
  const pins = pinsOf(def);
  const keep = (list: BoxPin[], formal: string) => list.find((p) => p.formal.toUpperCase() === formal.toUpperCase());
  el.inputs = pins.inputs.map((p) => {
    const was = keep(el.inputs, p.name);
    return { formal: p.name, type: p.type, operand: was?.operand ?? '', negated: was?.negated ?? false, varId: was?.varId ?? null };
  });
  el.outputs = pins.outputs.map((p) => {
    const was = keep(el.outputs, p.name);
    return { formal: p.name, type: p.type, operand: was?.operand ?? '', negated: was?.negated ?? false, varId: was?.varId ?? null };
  });
  el.powerIn = pins.powerIn;
  el.powerOut = pins.powerOut;
  for (const p of [...el.inputs.filter((x) => x.formal === el.powerIn), ...el.outputs.filter((x) => x.formal === el.powerOut)]) {
    p.operand = '';
    p.varId = null;
  }
  if (def.kind === 'function') el.instance = '';
}

/** Put an instruction at the selection. */
export function insert(model: DocModel, ids: Ids, sel: Sel | null, spec: Spec, own: readonly BoxDef[]): Edited {
  if (sel === null) return { ok: false, error: 'Select a network, an element or a point on a rung first.' };
  const m = clone(model);
  const pou = m.pous[sel.pou];
  const net = pou?.networks[sel.net];
  if (!pou || !net) return { ok: false, error: 'The selection is no longer there.' };
  if (net.rung === null) return { ok: false, error: `Network ${sel.net + 1} is kept as the document has it (${net.why}); it is not edited here.` };
  let path: number[] = [];
  let index: number;
  const output = spec.kind === 'coil' || spec.kind === 'jump' || spec.kind === 'return';
  if (sel.kind === 'network') {
    index = net.rung.length;
    if (!output) while (index > 0 && isOutput(net.rung[index - 1])) index -= 1;
  } else if (sel.kind === 'element') {
    const at = locate(net.rung, sel.to ?? sel.id);
    if (!at) return { ok: false, error: 'The selected element is no longer there.' };
    path = at.path;
    index = at.index + 1;
  } else {
    path = sel.path;
    index = sel.index;
  }
  const series = seriesAt(net.rung, path);
  if (!series) return { ok: false, error: 'The selected point is no longer there.' };
  if (ends(series[index - 1])) return { ok: false, error: 'Nothing follows a jump or a return on a rung.' };
  if ((spec.kind === 'jump' || spec.kind === 'return') && !(index === series.length && atRungEnd(net.rung, path))) {
    return { ok: false, error: `A ${spec.kind} ends its rung: put it at the end.` };
  }
  const el = newElement(spec, ids.next, own);
  ids.next += 1;
  ids.used.add(sel.pou);
  series.splice(index, 0, { t: 'el', el });
  touch(pou, net);
  return { ok: true, model: m, sel: { kind: 'element', pou: sel.pou, net: sel.net, id: el.id, to: null } };
}

/** The range of a selection in its series: first and last index. */
function rangeOf(rung: Item[], sel: Extract<Sel, { kind: 'element' }>): { path: number[]; from: number; to: number } | string {
  const a = locate(rung, sel.id);
  const b = locate(rung, sel.to ?? sel.id);
  if (!a || !b) return 'The selected element is no longer there.';
  if (a.path.join() !== b.path.join()) return 'A selection runs along one series: its first and last element are in one branch.';
  return { path: a.path, from: Math.min(a.index, b.index), to: Math.max(a.index, b.index) };
}

/** Put whatever is left of a parallel in order: an emptied branch goes, a single branch is a series. */
function tidy(series: Item[]): void {
  for (let i = 0; i < series.length; i += 1) {
    const item = series[i] as Item;
    if (item.t !== 'par') continue;
    item.branches.forEach(tidy);
    if (item.branches.length === 1) {
      series.splice(i, 1, ...(item.branches[0] as Item[]));
      i -= 1;
    } else if (item.branches.length === 0) {
      series.splice(i, 1);
      i -= 1;
    }
  }
}

/** Delete what is selected: elements, an empty branch, or a network. */
export function remove(model: DocModel, sel: Sel | null): Edited {
  if (sel === null) return { ok: false, error: 'Nothing is selected.' };
  const m = clone(model);
  const pou = m.pous[sel.pou];
  const net = pou?.networks[sel.net];
  if (!pou || !net) return { ok: false, error: 'The selection is no longer there.' };
  if (sel.kind === 'network') {
    pou.networks.splice(sel.net, 1);
    pou.touched = true;
    const next = Math.min(sel.net, pou.networks.length - 1);
    return { ok: true, model: m, sel: next >= 0 ? { kind: 'network', pou: sel.pou, net: next } : null };
  }
  if (net.rung === null) return { ok: false, error: `Network ${sel.net + 1} is kept as the document has it; delete it as a whole.` };
  if (sel.kind === 'gap') {
    // A point in an empty branch: the branch goes.
    const series = seriesAt(net.rung, sel.path);
    if (sel.path.length === 0 || !series || series.length > 0) return { ok: false, error: 'Select an element, or a point in an empty branch, to delete.' };
    const parent = seriesAt(net.rung, sel.path.slice(0, -2)) as Item[];
    const par = parent[sel.path[sel.path.length - 2] as number];
    if (par?.t === 'par') par.branches.splice(sel.path[sel.path.length - 1] as number, 1);
    tidy(net.rung);
    touch(pou, net);
    return { ok: true, model: m, sel: { kind: 'network', pou: sel.pou, net: sel.net } };
  }
  const range = rangeOf(net.rung, sel);
  if (typeof range === 'string') return { ok: false, error: range };
  const series = seriesAt(net.rung, range.path) as Item[];
  series.splice(range.from, range.to - range.from + 1);
  // A branch emptied by the deletion goes with it; a branch that was empty before stays.
  if (series.length === 0 && range.path.length > 0) {
    const parent = seriesAt(net.rung, range.path.slice(0, -2)) as Item[];
    const par = parent[range.path[range.path.length - 2] as number];
    if (par?.t === 'par') par.branches.splice(range.path[range.path.length - 1] as number, 1);
  }
  tidy(net.rung);
  touch(pou, net);
  const still = seriesAt(net.rung, range.path);
  return {
    ok: true,
    model: m,
    sel: still ? { kind: 'gap', pou: sel.pou, net: sel.net, path: range.path, index: Math.min(range.from, still.length) } : { kind: 'network', pou: sel.pou, net: sel.net },
  };
}

/** A parallel branch around the run of elements from `start` to the selection's last. */
export function branch(model: DocModel, sel: Sel | null, start: number | null = null): Edited {
  if (sel === null || sel.kind !== 'element') return { ok: false, error: 'Select the element, or the run of elements, the branch goes around.' };
  const m = clone(model);
  const pou = m.pous[sel.pou] as PouModel;
  const net = pou.networks[sel.net];
  if (!net || net.rung === null) return { ok: false, error: 'This network is kept as the document has it; it is not edited here.' };
  const range = rangeOf(net.rung, { ...sel, id: start ?? sel.id, to: sel.to ?? sel.id });
  if (typeof range === 'string') return { ok: false, error: start === null ? range : 'A branch opens and closes in one series: select its end in the series it opened in.' };
  const series = seriesAt(net.rung, range.path) as Item[];
  const run = series.slice(range.from, range.to + 1);
  if (run.some(ends) && range.to < series.length - 1) return { ok: false, error: 'Nothing follows a jump or a return on a rung.' };
  let target: { path: number[]; index: number };
  const only = run[0];
  if (run.length === 1 && only?.t === 'par') {
    only.branches.push([]);
    target = { path: [...range.path, range.from, only.branches.length - 1], index: 0 };
  } else {
    series.splice(range.from, run.length, { t: 'par', branches: [run, []] });
    target = { path: [...range.path, range.from, 1], index: 0 };
  }
  touch(pou, net);
  return { ok: true, model: m, sel: { kind: 'gap', pou: sel.pou, net: sel.net, ...target } };
}

/** Change an element: its operand, its kind of contact or coil, a box's type, instance or pin operands. */
export function change(
  model: DocModel,
  ids: Ids,
  pouIndex: number,
  id: number,
  edit: (el: El, alloc: () => number) => void
): Edited {
  const m = clone(model);
  const pou = m.pous[pouIndex] as PouModel;
  const found = findEl(m, pouIndex, id);
  if (!found) return { ok: false, error: 'The element is no longer there.' };
  edit(found.el, () => {
    const next = ids.next;
    ids.next += 1;
    ids.used.add(pouIndex);
    return next;
  });
  touch(pou, pou.networks[found.net] as Network);
  return { ok: true, model: m, sel: null };
}

export const setOperand = (model: DocModel, ids: Ids, pou: number, id: number, text: string) =>
  change(model, ids, pou, id, (el) => {
    el.operand = text.trim();
  });

export const setVariant = (model: DocModel, ids: Ids, pou: number, id: number, v: Pick<El, 'negated' | 'edge' | 'storage'>) =>
  change(model, ids, pou, id, (el) => {
    el.negated = v.negated;
    el.edge = v.edge;
    el.storage = el.kind === 'coil' ? v.storage : null;
  });

export const setBoxType = (model: DocModel, ids: Ids, pou: number, id: number, type: string, own: readonly BoxDef[]) =>
  change(model, ids, pou, id, (el) => retype(el, type, own));

export const setInstance = (model: DocModel, ids: Ids, pou: number, id: number, text: string) =>
  change(model, ids, pou, id, (el) => {
    el.instance = text.trim();
  });

/** A pin's operand. An output's is written by an output variable, which gets an id of its own while it has one. */
export const setPin = (model: DocModel, ids: Ids, pou: number, id: number, which: 'inputs' | 'outputs', formal: string, text: string) =>
  change(model, ids, pou, id, (el, alloc) => {
    const pin = el[which].find((p) => p.formal === formal);
    if (!pin) return;
    pin.operand = text.trim();
    if (pin.operand === '') pin.varId = null;
    else if (which === 'outputs' && pin.varId === null) pin.varId = alloc();
  });

function header(model: DocModel, ids: Ids, pouIndex: number, n: number, edit: (net: Network, alloc: () => number) => void): Edited {
  const m = clone(model);
  const pou = m.pous[pouIndex];
  const net = pou?.networks[n];
  if (!pou || !net) return { ok: false, error: 'The network is no longer there.' };
  edit(net, () => {
    const next = ids.next;
    ids.next += 1;
    ids.used.add(pouIndex);
    return next;
  });
  net.headerDirty = true;
  pou.touched = true;
  return { ok: true, model: m, sel: null };
}

/** A network's title or comment: kept in a TC6 comment above it, which gets an id when it first has one. */
export const setHeader = (model: DocModel, ids: Ids, pou: number, n: number, what: 'title' | 'comment', text: string) =>
  header(model, ids, pou, n, (net, alloc) => {
    net[what] = text;
    if (net.commentId === null && (net.title !== '' || net.comment !== '')) net.commentId = alloc();
  });

/** A network's label (IEC 61131-3 ed. 3 §8.1.4): what a jump names. */
export const setLabel = (model: DocModel, ids: Ids, pou: number, n: number, text: string) =>
  header(model, ids, pou, n, (net, alloc) => {
    net.label = text.trim();
    if (net.labelId === null && net.label !== '') net.labelId = alloc();
  });

let keys = 1_000_000;

/** A new, empty network at `at`. */
export function addNetwork(model: DocModel, pouIndex: number, at: number): Edited {
  const m = clone(model);
  const pou = m.pous[pouIndex];
  if (!pou) return { ok: false, error: 'There is no such POU.' };
  keys += 1;
  const net: Network = {
    key: keys,
    title: '',
    comment: '',
    label: '',
    commentId: null,
    labelId: null,
    rung: [],
    why: null,
    ids: [],
    extras: [],
    left: null,
    right: null,
    dirty: true,
    headerDirty: false,
  };
  const index = Math.max(0, Math.min(at, pou.networks.length));
  pou.networks.splice(index, 0, net);
  pou.touched = true;
  return { ok: true, model: m, sel: { kind: 'network', pou: pouIndex, net: index } };
}

/** Move a network one place up (-1) or down (+1) in the order of evaluation. */
export function moveNetwork(model: DocModel, pouIndex: number, n: number, by: -1 | 1): Edited {
  const m = clone(model);
  const pou = m.pous[pouIndex];
  const to = n + by;
  if (!pou || !pou.networks[n] || to < 0 || to >= pou.networks.length) return { ok: false, error: 'The network cannot move further.' };
  const [moved] = pou.networks.splice(n, 1);
  pou.networks.splice(to, 0, moved as Network);
  pou.touched = true;
  return { ok: true, model: m, sel: { kind: 'network', pou: pouIndex, net: to } };
}

/** Declare a variable in the POU's interface. */
export function declareVariable(model: DocModel, pouIndex: number, d: Declared, existing: readonly VarInfo[]): Edited {
  const m = clone(model);
  const pou = m.pous[pouIndex];
  if (!pou) return { ok: false, error: 'There is no such POU.' };
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(d.name) || /__/.test(d.name) || d.name.endsWith('_')) {
    return { ok: false, error: `${d.name} is not a name a variable can have.` };
  }
  const lower = d.name.toLowerCase();
  if (existing.some((v) => v.name.toLowerCase() === lower) || pou.declared.some((v) => v.name.toLowerCase() === lower)) {
    return { ok: false, error: `${d.name} is already declared.` };
  }
  if (d.type.trim() === '') return { ok: false, error: `Choose a type for ${d.name}.` };
  pou.declared.push({ ...d, type: d.type.trim() });
  return { ok: true, model: m, sel: null };
}

// ---------------------------------------------------------------------------------------------
// The editor's own check
// ---------------------------------------------------------------------------------------------

/** A name, members and subscripts: a variable an element can read or write. */
export const PLACE = /^[A-Za-z_]\w*(\[[^\]]+\])?(\.[A-Za-z_]\w*(\[[^\]]+\])?)*$/;
const LITERAL_BOOL = /^(TRUE|FALSE)$/i;
const LITERAL_TIME = /^(T|TIME)#[-+]?[0-9_.a-z]+$/i;
const LITERAL_NUM = /^[-+]?(\d[\d_]*(\.\d[\d_]*)?([eE][-+]?\d+)?|(2|8|16)#[0-9A-Fa-f_]+)$/;
const LITERAL_STRING = /^'.*'$/s;

/** The family of a literal, or `null` when the text is not one. */
function literalFamily(text: string): 'BOOL' | 'TIME' | 'NUM' | 'STRING' | null {
  if (LITERAL_BOOL.test(text)) return 'BOOL';
  if (LITERAL_TIME.test(text)) return 'TIME';
  if (LITERAL_NUM.test(text)) return 'NUM';
  if (LITERAL_STRING.test(text)) return 'STRING';
  return null;
}

/** The variables an operand may name: each declared one, and each output of a function block instance. */
export function operands(vars: readonly VarInfo[], own: readonly BoxDef[]): VarInfo[] {
  const out: VarInfo[] = [];
  for (const v of vars) {
    out.push(v);
    const def = findBox(v.type, own);
    if (def?.kind === 'functionBlock') {
      for (const o of def.outputs) out.push({ name: `${v.name}.${o.name}`, type: o.type, section: 'member' });
    }
  }
  return out;
}

/** What is wrong with an operand at a place that takes `wanted` (`null`: any), and writes when `writes`. */
export function operandProblem(
  text: string,
  wanted: string | null,
  writes: boolean,
  vars: readonly VarInfo[],
  own: readonly BoxDef[]
): string | null {
  const t = text.trim();
  if (t === '') return 'It has no operand.';
  const literal = literalFamily(t);
  if (literal !== null) {
    if (writes) return `${t} is a value, not a variable to write.`;
    const want = wanted === null ? null : family(wanted);
    return want !== null && want !== literal ? `${t} is a ${literal === 'NUM' ? 'number' : literal}; this takes a ${wanted}.` : null;
  }
  if (!PLACE.test(t)) return writes ? `${t} is not a variable to write.` : null;
  const root = (t.split(/[.[]/)[0] as string).toLowerCase();
  const declared = vars.find((v) => v.name.toLowerCase() === root);
  if (!declared) return `${t.split(/[.[]/)[0]} is not declared.`;
  if (writes && declared.section === 'inputVars' && !t.includes('.')) {
    return `${declared.name} is an input of the POU; it cannot be written inside it.`;
  }
  let type: string | null = /^[A-Za-z_]\w*$/.test(t) ? declared.type : null;
  const member = /^([A-Za-z_]\w*)\.([A-Za-z_]\w*)$/.exec(t);
  if (member) {
    const def = findBox(declared.type, own);
    if (def?.kind === 'functionBlock') {
      const name = (member[2] as string).toUpperCase();
      const out = def.outputs.find((o) => o.name === name);
      const inp = def.inputs.find((o) => o.name === name);
      if (!out && !inp) return `${declared.type} has no ${member[2]}.`;
      if (inp && !out) return `${t} is an input of the block; it is set from outside, not read there.`;
      if (writes) return `${t} is an output of the block; it cannot be written.`;
      type = out?.type ?? null;
    }
  }
  const want = wanted === null ? null : family(wanted);
  const have = type === null ? null : family(type);
  if (want !== null && have !== null && want !== have) return `${t} is ${type}; this takes a ${wanted}.`;
  if (want !== null && type !== null && have === null && findBox(type, own)) return `${t} is a ${type} instance; this takes a ${wanted}.`;
  return null;
}

/** What the editor finds wrong in a POU, at once: per element, and per network. */
export interface Problems {
  elements: Map<number, string[]>;
  networks: Map<number, string[]>;
}

/**
 * `own`: the boxes this program may call besides the standard ones (the document's own POUs, then
 * the library's blocks). `library`: whether the library's blocks are known, and why not; left
 * out, the editor says only what it does not know.
 */
export function check(pou: PouModel, vars: readonly VarInfo[], own: readonly BoxDef[], library?: LibraryBoxes): Problems {
  const elements = new Map<number, string[]>();
  const networks = new Map<number, string[]>();
  const say = (map: Map<number, string[]>, key: number, text: string) => map.set(key, [...(map.get(key) ?? []), text]);
  const labels = new Set(pou.networks.map((n) => n.label.toUpperCase()).filter((l) => l !== ''));
  pou.networks.forEach((net, n) => {
    if (net.rung === null) return;
    const instances = new Map<string, number>();
    const visit = (series: Item[], branch: boolean) => {
      if (branch && series.length === 0) say(networks, n, 'An empty branch is a wire: what is in parallel with it is bypassed.');
      for (const item of series) {
        if (item.t === 'par') {
          item.branches.forEach((b) => visit(b, true));
          continue;
        }
        const el = item.el;
        const problem = (text: string | null) => text !== null && say(elements, el.id, text);
        if (el.kind === 'contact') problem(operandProblem(el.operand, 'BOOL', false, vars, own));
        if (el.kind === 'coil') problem(operandProblem(el.operand, 'BOOL', true, vars, own));
        if (el.kind === 'jump') {
          if (el.operand === '') problem('A jump names the label of a network.');
          else if (!labels.has(el.operand.toUpperCase())) problem(`No network has the label ${el.operand}.`);
        }
        if (el.kind === 'block') {
          if (el.typeName === '') {
            problem('The box has no type: name a function or a function block.');
            continue;
          }
          const def = findBox(el.typeName, own);
          if (!def) {
            for (const words of unknownBoxWords(el.typeName, library)) problem(words);
            continue;
          }
          // A block whose declaration can change under the diagram: said per pin, never re-pinned.
          if (def.group === LIBRARY_GROUP || def.group === 'This document') {
            for (const words of pinProblems(el.inputs, el.outputs, def)) problem(words);
          }
          if (def.kind === 'functionBlock') {
            if (el.instance === '') problem(`A ${def.name} needs an instance: name the variable that keeps its state.`);
            else {
              const v = vars.find((x) => x.name.toLowerCase() === el.instance.toLowerCase());
              if (!v) problem(`${el.instance} is not declared.`);
              else if (v.type.toUpperCase() !== def.name.toUpperCase()) problem(`${el.instance} is ${v.type}, not a ${def.name}.`);
              const key = el.instance.toLowerCase();
              if (instances.has(key)) problem(`${el.instance} is called by another box of this network; an instance is called once in a network.`);
              instances.set(key, el.id);
            }
          }
          for (const pin of el.inputs) {
            if (pin.formal === el.powerIn) continue;
            if (pin.operand === '') {
              if (def.kind === 'function') problem(`${pin.formal} has no operand.`);
              continue;
            }
            const p = operandProblem(pin.operand, declaredType(def, 'inputs', pin.formal, pin.type), false, vars, own);
            if (p) problem(`${pin.formal}: ${p}`);
          }
          for (const pin of el.outputs) {
            if (pin.formal === el.powerOut || pin.operand === '') continue;
            const p = operandProblem(pin.operand, declaredType(def, 'outputs', pin.formal, pin.type), true, vars, own);
            if (p) problem(`${pin.formal}: ${p}`);
          }
        }
      }
    };
    visit(net.rung, false);
    const last = net.rung[net.rung.length - 1];
    const lastEls = (item: Item | undefined): El[] =>
      !item ? [] : item.t === 'el' ? [item.el] : item.branches.flatMap((b) => lastEls(b[b.length - 1]));
    if (net.rung.length > 0 && lastEls(last).some((e) => e.kind === 'contact')) {
      say(networks, n, 'The rung ends in a contact: nothing uses what it computes. End it with a coil, a box, a jump or a return.');
    }
  });
  return { elements, networks };
}

/**
 * The type a new variable named at an element should be declared with, and where. A pin's type
 * is the one its box's block declares now (`own`), else the one the model took when it was read.
 */
export function declarationFor(
  el: El,
  where: { pin?: string; which?: 'inputs' | 'outputs'; instance?: boolean },
  own: readonly BoxDef[] = []
): { type: string; section: Section } {
  if (where.instance) return { type: el.typeName, section: 'localVars' };
  if (where.pin) {
    const which = where.which ?? 'inputs';
    const pin = el[which].find((p) => p.formal === where.pin);
    const t = pin ? declaredType(el.kind === 'block' ? findBox(el.typeName, own) : null, which, pin.formal, pin.type) : '';
    return { type: family(t) === null || t.startsWith('ANY') ? '' : t, section: 'localVars' };
  }
  return { type: 'BOOL', section: el.kind === 'coil' ? 'outputVars' : 'inputVars' };
}

/** The standard boxes the palette offers. */
export const STANDARD_BOXES = STANDARD;
