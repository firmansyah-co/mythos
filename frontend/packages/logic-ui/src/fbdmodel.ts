// A Function Block Diagram as its editor holds it, read from its PLCopen TC6 document and written
// back into it (ADR-013 §2.8, §2.16, L3c-2). The Ladder Diagram editor's model (ldmodel.ts) is
// the template: the same splicing, the same record of localIds, the same network headers.
//
// **Networks.** A body is a list of networks in the order they are evaluated: the front-end's
// rule (connected elements, a connector joining its continuations, top to bottom by the
// topmost, then the leftmost; IEC 61131-3 ed. 3 §8.3.3, ADR-013 §2.13). TC6 has no element for a
// network, so a network is its connected elements: **one tree**. Two trees side by side would be
// read again as two networks; a second one goes into a network of its own. A network's title,
// comment and label are kept as LD's are (a TC6 `comment` above it with the title in its
// `addData`, and a `label`), under the same `addData` names, so either editor reads the other's.
//
// **Trees** (TIA's FBD: auto-laid-out, not free placement). A network the editor draws is read
// left to right: each input of a box takes one operand (an `inVariable` wired to it, or an
// `expression` at the pin as another tool may write it) or the output of a nested box; a box's
// output may go into one further box and to any number of assignments (`outVariable`, negated
// or not), jumps and returns, as TIA branches an output. A network that is only an assignment,
// a jump or a return reads an operand. What the tree cannot express (a connector and its
// continuation, an in-out variable, a line drawn back, an output into two boxes, a second line
// into one input, an element it does not know) is kept as found: it can be moved, deleted,
// titled, labelled and commented, and its elements are written back byte for byte.
//
// **No second line into one input** (§8.3.2, no wired-OR): an input of the model holds one
// source, and the writer gives each input at most one `connection`. An input another tool wired
// twice keeps its network as found (the front-end refuses it, and says so at its element).
//
// **Writing.** As LD's: the body of each POU changed is written anew, the networks the editor
// did not change byte for byte (moved down only when one above grew), the others laid out on a
// grid with TC6 positions, sizes and `relPosition`s, so other tools and the view read them. An
// element keeps its `localId` while it exists; a new one takes an id no element of the document
// has had (the record in the POU's `addData`, `urn:mythos:logic:ld-editor`, as LD's). Online
// change carries `__FBD_<localId>` by its path, so an element kept keeps its value.

import { findFbdBox, mainOutput } from "./fbdcatalog";
import type { BoxDef } from "./ldcatalog";
import {
  CHAR,
  commentHeight,
  commentPart,
  declare,
  deepText,
  extraAttrs,
  extrasOf,
  flag,
  idsRecord,
  indentOf,
  intOf,
  labelPart,
  numOf,
  ownBoxes,
  posOf,
  record,
  titleOf,
  up,
  varsOf,
  type Declared,
  type Ids,
  type PouHead,
  type VarInfo,
} from "./ldmodel";
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
} from "./ldxml";

/** An operand at an input: a variable or a value, written by an `inVariable` or as the pin's `expression`. */
export interface Operand {
  t: "operand";
  text: string;
  /** The `inVariable`'s negation (§6.6.1.4.3 rule 9); always false for an expression at a pin. */
  negated: boolean;
  /** The `inVariable` that writes it; `null` for an `expression` at the pin, as another tool wrote it. */
  varId: number | null;
}

/** What an input takes: an operand, or the output of a nested box. */
export type Src = Operand | { t: "box"; box: Box; output: string };

export interface Pin {
  formal: string;
  /** The type its declaration gives it; `ANY` when that is not known. */
  type: string;
  negated: boolean;
  /** One source, never two (§8.3.2). */
  src: Src | null;
}

/** What an output writes: an assignment (`outVariable`), a jump or a return. */
export interface Sink {
  id: number;
  kind: "assign" | "jump" | "return";
  /** An assignment's variable; a jump's label. */
  text: string;
  /** An assignment's negation. */
  negated: boolean;
}

export interface Out {
  formal: string;
  type: string;
  negated: boolean;
  to: Sink[];
}

export interface Box {
  id: number;
  typeName: string;
  instance: string;
  inputs: Pin[];
  outputs: Out[];
}

/** The tree of a network: a box, or an assignment, jump or return reading an operand (or nothing yet). */
export type Root =
  { t: "box"; box: Box } | { t: "sink"; sink: Sink; src: Operand | null };

export interface Network {
  /** Stable within a session, for React. */
  key: number;
  title: string;
  comment: string;
  label: string;
  commentId: number | null;
  labelId: number | null;
  /** Its tree; `null` while it has none, or when it is kept as found. */
  root: Root | null;
  /** Why it is kept as found; `null` when the editor draws it. */
  why: string | null;
  /** Every element of the document it held when it was read, comments and labels aside. */
  ids: number[];
  /** Comments and labels beyond its own, written as they were. */
  extras: number[];
  /** Its tree changed: it is laid out and written anew. */
  dirty: boolean;
  /** Its title, comment or label changed. */
  headerDirty: boolean;
}

export interface PouModel {
  name: string;
  pouType: string;
  networks: Network[];
  declared: Declared[];
  touched: boolean;
}

export interface DocModel {
  pous: PouModel[];
  rev: number;
}

interface SourcePou extends PouHead {
  node: XNode;
  /** The `FBD` body; `null` for a POU in another language, which is never touched. */
  fbd: XNode | null;
  byId: Map<number, XNode>;
  /** Children of the body that are no element (its addData, documentation, XML comments), in order. */
  others: XNode[];
}

/** A document as it was opened: its text and what was read from it. Never changed. */
export interface Source {
  text: string;
  root: XNode;
  pous: SourcePou[];
  /** The first localId no element has had: past every id of every body and the record. */
  firstFree: number;
}

export type Opened =
  { ok: true; source: Source; model: DocModel } | { ok: false; error: string };

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

let keys = 0;
const nextKey = () => {
  keys += 1;
  return keys;
};

/** A connection into an input. */
interface Conn {
  from: number;
  formal: string | null;
}
const connsOf = (point: XNode | null): Conn[] =>
  point
    ? childrenNamed(point, "connection")
        .map((c) => ({
          from: intOf(attr(c, "refLocalId")),
          formal: attr(c, "formalParameter"),
        }))
        .filter((c): c is Conn => c.from !== null)
    : [];
const exprOf = (point: XNode | null): string | null => {
  const e = point ? child(point, "expression") : null;
  return e ? e.text.trim() : null;
};

/** Each input point of an element: a block's by formal parameter, with its negation. */
function inputsOf(
  node: XNode,
): Array<{
  formal: string | null;
  negated: boolean;
  point: XNode | null;
  conns: Conn[];
}> {
  if (node.name === "block") {
    const iv = child(node, "inputVariables");
    return (iv ? childrenNamed(iv, "variable") : []).map((v) => {
      const point = child(v, "connectionPointIn");
      return {
        formal: attr(v, "formalParameter") ?? "",
        negated: flag(v, "negated"),
        point,
        conns: connsOf(point),
      };
    });
  }
  return childrenNamed(node, "connectionPointIn").map((point) => ({
    formal: null,
    negated: false,
    point,
    conns: connsOf(point),
  }));
}

const ELEMENT = new Set([
  "block",
  "inVariable",
  "outVariable",
  "inOutVariable",
  "connector",
  "continuation",
  "jump",
  "return",
]);

/** Open a TC6 document for the FBD editor. */
export function open(text: string): Opened {
  const parsed = parseXml(text);
  if (!parsed.ok) return { ok: false, error: parsed.error };
  const root = parsed.root;
  if (root.name !== "project")
    return { ok: false, error: `its root is ${root.name}, not a TC6 project` };
  const types = child(root, "types");
  const pousNode = types ? child(types, "pous") : null;
  let maxId = 0;
  const sources: SourcePou[] = [];
  for (const node of pousNode ? childrenNamed(pousNode, "pou") : []) {
    const body = child(node, "body");
    // The ids of every body count, whatever its language: none is given out twice in the document.
    for (const b of body?.children ?? []) {
      for (const c of b.children) {
        const id = intOf(attr(c, "localId"));
        if (id !== null) maxId = Math.max(maxId, id);
      }
    }
    const fbd = body ? child(body, "FBD") : null;
    const iface = child(node, "interface");
    const sp: SourcePou = {
      node,
      name: attr(node, "name") ?? "",
      pouType: attr(node, "pouType") ?? "",
      fbd,
      iface,
      vars: varsOf(iface),
      byId: new Map(),
      others: [],
    };
    for (const c of fbd?.children ?? []) {
      const id = intOf(attr(c, "localId"));
      if (id === null) {
        sp.others.push(c);
        continue;
      }
      if (sp.byId.has(id))
        return {
          ok: false,
          error: `two elements of ${sp.name} have the localId ${id}`,
        };
      sp.byId.set(id, c);
    }
    const rec = idsRecord(node);
    if (rec !== null) maxId = Math.max(maxId, rec - 1);
    sources.push(sp);
  }
  const source: Source = { text, root, pous: sources, firstFree: maxId + 1 };
  return {
    ok: true,
    source,
    model: { pous: sources.map((sp) => readPou(source, sp)), rev: 0 },
  };
}

const idOf = (n: XNode) => intOf(attr(n, "localId")) as number;

function readPou(source: Source, sp: SourcePou): PouModel {
  const model: PouModel = {
    name: sp.name,
    pouType: sp.pouType,
    networks: [],
    declared: [],
    touched: false,
  };
  if (!sp.fbd) return model;
  const elements = [...sp.byId.values()].filter(
    (n) => n.name !== "comment" && n.name !== "label",
  );
  // Networks: connected elements, a connector with each continuation of its name.
  const parent = new Map<number, number>(
    elements.map((n) => [idOf(n), idOf(n)]),
  );
  const find = (k: number): number => {
    let at = k;
    while (parent.get(at) !== at) at = parent.get(at) as number;
    return at;
  };
  const join = (a: number, b: number) => {
    if (parent.has(a) && parent.has(b)) parent.set(find(a), find(b));
  };
  const connectors = new Map<string, number>();
  for (const n of elements)
    if (n.name === "connector")
      connectors.set((attr(n, "name") ?? "").toUpperCase(), idOf(n));
  for (const n of elements) {
    for (const input of inputsOf(n))
      for (const c of input.conns) join(idOf(n), c.from);
    if (n.name === "continuation") {
      const to = connectors.get((attr(n, "name") ?? "").toUpperCase());
      if (to !== undefined) join(idOf(n), to);
    }
  }
  const groups = new Map<number, XNode[]>();
  for (const n of elements)
    groups.set(find(idOf(n)), [...(groups.get(find(idOf(n))) ?? []), n]);
  const topOf = (ns: XNode[]) =>
    ns
      .map((n) => ({ ...posOf(n), id: idOf(n) }))
      .reduce((a, b) => (b.y < a.y || (b.y === a.y && b.x < a.x) ? b : a));
  type Entry = {
    y: number;
    rank: number;
    x: number;
    id: number;
    group?: XNode[];
    node?: XNode;
  };
  const entries: Entry[] = [...groups.values()].map((g) => {
    const top = topOf(g);
    return { y: top.y, rank: 2, x: top.x, id: top.id, group: g };
  });
  for (const n of sp.byId.values()) {
    if (n.name === "comment" || n.name === "label") {
      const p = posOf(n);
      entries.push({
        y: p.y,
        rank: n.name === "comment" ? 0 : 1,
        x: p.x,
        id: idOf(n),
        node: n,
      });
    }
  }
  entries.sort(
    (a, b) => a.y - b.y || a.rank - b.rank || a.x - b.x || a.id - b.id,
  );
  let pending: XNode[] = [];
  const networkOf = (group: XNode[] | null): Network => {
    const comments = pending.filter((n) => n.name === "comment");
    const labels = pending.filter((n) => n.name === "label");
    const comment = comments[0] ?? null;
    const label = labels[0] ?? null;
    const net: Network = {
      key: nextKey(),
      title: comment ? (titleOf(comment) ?? "") : "",
      comment: comment
        ? deepText(child(comment, "content") ?? comment).trim()
        : "",
      label: label ? (attr(label, "label") ?? "") : "",
      commentId: comment ? idOf(comment) : null,
      labelId: label ? idOf(label) : null,
      root: null,
      why: null,
      ids: group ? group.map(idOf) : [],
      extras: [...comments.slice(1), ...labels.slice(1)].map(idOf),
      dirty: false,
      headerDirty: false,
    };
    if (group) {
      const read = treeOf(source, sp, group);
      if (typeof read === "string") net.why = read;
      else net.root = read;
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
    const header = n.name === "comment" && titleOf(n) !== null;
    const secondLabel =
      n.name === "label" && pending.some((p) => p.name === "label");
    if (pending.length > 0 && (header || secondLabel))
      model.networks.push(networkOf(null));
    pending.push(n);
  }
  if (pending.length > 0) model.networks.push(networkOf(null));
  return model;
}

/** The type of a box's pin as its declaration gives it, else `ANY` (EN and ENO are BOOL). */
function pinType(
  def: BoxDef | null,
  formal: string,
  which: "inputs" | "outputs",
): string {
  const upper = formal.toUpperCase();
  if (upper === "EN" || upper === "ENO") return "BOOL";
  const found = def?.[which].find((p) => p.name === upper)?.type;
  if (found) return found;
  // AND, OR and XOR take IN3, IN4, … of the type of IN1.
  if (def && which === "inputs" && /^IN\d+$/.test(upper))
    return def.inputs[0]?.type ?? "ANY";
  return "ANY";
}

/** A network's tree, or why it is kept as found. */
function treeOf(source: Source, sp: SourcePou, group: XNode[]): Root | string {
  const byId = sp.byId;
  const members = new Set(group.map(idOf));
  const own = ownBoxes(source).filter(
    (d) => d.name.toLowerCase() !== sp.name.toLowerCase(),
  );
  for (const n of group) {
    if (!ELEMENT.has(n.name))
      return `it holds a ${n.name} (${idOf(n)}), which this editor does not draw`;
    if (n.name === "connector" || n.name === "continuation") {
      return `it carries a line through the connector ${attr(n, "name") ?? ""} (${idOf(n)}), which this editor does not draw`;
    }
    if (n.name === "inOutVariable")
      return `it holds an in-out variable (${idOf(n)}), which this editor does not draw`;
    const inOut = n.name === "block" ? child(n, "inOutVariables") : null;
    if (inOut && childrenNamed(inOut, "variable").length > 0) {
      return `block ${idOf(n)} has in-out parameters, which this editor does not draw`;
    }
    // A pin's edge, storage or hidden attribute would be lost when the network is written again:
    // the editor writes a pin's formal parameter and negation only.
    if (n.name === "block") {
      for (const list of ["inputVariables", "outputVariables"]) {
        for (const v of childrenNamed(child(n, list) ?? n, "variable")) {
          const other = v.attrs.find(
            (a) =>
              a.name !== "formalParameter" &&
              a.name !== "negated" &&
              !a.qname.startsWith("xmlns"),
          );
          if (other !== undefined) {
            return `the pin ${attr(v, "formalParameter") ?? ""} of block ${idOf(n)} has ${other.qname}="${other.value}", which this editor does not draw`;
          }
        }
      }
    }
  }
  // Every input: one line or one expression, or nothing.
  for (const n of group) {
    for (const input of inputsOf(n)) {
      const where =
        input.formal !== null
          ? `the input ${input.formal} of block ${idOf(n)}`
          : `the input of ${n.name} ${idOf(n)}`;
      if (input.conns.length > 1)
        return `${where} takes ${input.conns.length} lines; an input takes one (IEC 61131-3 ed. 3 §8.3.2)`;
      if (input.conns.length === 1 && exprOf(input.point) !== null)
        return `${where} takes a line and an expression at once`;
      const from = input.conns[0]?.from;
      if (from !== undefined && !members.has(from))
        return `${where} is wired to ${from}, which is not in this network`;
    }
  }
  // Who reads each output.
  const readers = new Map<
    number,
    Array<{ by: XNode; formal: string | null; from: string | null }>
  >();
  for (const n of group) {
    for (const input of inputsOf(n)) {
      const c = input.conns[0];
      if (c)
        readers.set(c.from, [
          ...(readers.get(c.from) ?? []),
          { by: n, formal: input.formal, from: c.formal },
        ]);
    }
  }
  const outputsOf = (block: XNode) =>
    childrenNamed(child(block, "outputVariables") ?? block, "variable").map(
      (v) => attr(v, "formalParameter") ?? "",
    );
  const outputOf = (block: XNode, formal: string | null): string | null => {
    const outs = outputsOf(block);
    if (formal === null) return mainOutput(outs.map((f) => ({ formal: f })));
    return outs.find((f) => f.toUpperCase() === formal.toUpperCase()) ?? null;
  };
  // Operands: each input variable feeds exactly one input.
  const operandOf = (n: XNode): Operand => ({
    t: "operand",
    text: (child(n, "expression")?.text ?? "").trim(),
    negated: flag(n, "negated"),
    varId: idOf(n),
  });
  for (const n of group) {
    if (n.name !== "inVariable") continue;
    const rs = readers.get(idOf(n)) ?? [];
    if (rs.length !== 1)
      return `the variable ${idOf(n)} feeds ${rs.length === 0 ? "nothing" : `${rs.length} inputs`}; this editor draws a variable at one input`;
  }
  // Boxes: at most one further box reads a box; assignments, jumps and returns any number.
  const parentOf = new Map<number, number>();
  for (const n of group) {
    if (n.name !== "block") continue;
    for (const r of readers.get(idOf(n)) ?? []) {
      if (outputOf(n, r.from) === null)
        return `${r.by.name} ${idOf(r.by)} is wired to the output ${r.from} of block ${idOf(n)}, which the block does not show`;
      if (r.by.name === "block") {
        if (parentOf.has(idOf(n)))
          return `the outputs of block ${idOf(n)} go into more than one box; this editor draws a box's output into one`;
        parentOf.set(idOf(n), idOf(r.by));
      }
    }
  }
  const sinkOf = (n: XNode): Sink => ({
    id: idOf(n),
    kind:
      n.name === "outVariable"
        ? "assign"
        : n.name === "jump"
          ? "jump"
          : "return",
    text:
      n.name === "outVariable"
        ? (child(n, "expression")?.text ?? "").trim()
        : n.name === "jump"
          ? (attr(n, "label") ?? "")
          : "",
    negated: n.name === "outVariable" && flag(n, "negated"),
  });
  const boxes = new Map<number, Box>();
  const building = new Set<number>();
  const boxOf = (n: XNode): Box | string => {
    const id = idOf(n);
    const made = boxes.get(id);
    if (made) return made;
    if (building.has(id))
      return `its lines run in a loop through block ${id} (a line drawn back), which this editor does not draw`;
    building.add(id);
    const typeName = attr(n, "typeName") ?? "";
    const def = findFbdBox(typeName, own);
    const box: Box = {
      id,
      typeName,
      instance: attr(n, "instanceName") ?? "",
      inputs: [],
      outputs: [],
    };
    for (const input of inputsOf(n)) {
      const formal = input.formal ?? "";
      const pin: Pin = {
        formal,
        type: pinType(def, formal, "inputs"),
        negated: input.negated,
        src: null,
      };
      const expr = exprOf(input.point);
      const c = input.conns[0];
      if (expr !== null)
        pin.src = { t: "operand", text: expr, negated: false, varId: null };
      else if (c) {
        const from = byId.get(c.from) as XNode;
        if (from.name === "inVariable") pin.src = operandOf(from);
        else if (from.name === "block") {
          const nested = boxOf(from);
          if (typeof nested === "string") return nested;
          pin.src = {
            t: "box",
            box: nested,
            output: outputOf(from, c.formal) as string,
          };
        } else
          return `the input ${formal} of block ${id} is wired from a ${from.name}, which gives no value`;
      }
      box.inputs.push(pin);
    }
    for (const v of childrenNamed(
      child(n, "outputVariables") ?? n,
      "variable",
    )) {
      const formal = attr(v, "formalParameter") ?? "";
      box.outputs.push({
        formal,
        type: pinType(def, formal, "outputs"),
        negated: flag(v, "negated"),
        to: [],
      });
    }
    for (const r of readers.get(id) ?? []) {
      if (r.by.name === "block") continue;
      if (
        r.by.name !== "outVariable" &&
        r.by.name !== "jump" &&
        r.by.name !== "return"
      ) {
        return `block ${id} is read by ${r.by.name} ${idOf(r.by)}, which this editor does not draw`;
      }
      const formal = outputOf(n, r.from) as string;
      const out = box.outputs.find((o) => o.formal === formal);
      out?.to.push(sinkOf(r.by));
    }
    building.delete(id);
    boxes.set(id, box);
    return box;
  };
  const tops = group.filter(
    (n) =>
      (n.name === "block" && !parentOf.has(idOf(n))) ||
      ((n.name === "outVariable" || n.name === "jump" || n.name === "return") &&
        !readsBlock(n)),
  );
  function readsBlock(n: XNode): boolean {
    const c = inputsOf(n)[0]?.conns[0];
    return c !== undefined && byId.get(c.from)?.name === "block";
  }
  if (tops.length === 0) {
    // Every box is read by another: the lines go round.
    const first = group.find((n) => n.name === "block");
    return `its lines run in a loop${first ? ` through block ${idOf(first)}` : ""} (a line drawn back), which this editor does not draw`;
  }
  if (tops.length > 1) {
    // Connected yet more than one end: what joins them is told above; this is what is left.
    return `it ends in ${tops.length} places (${tops.map(idOf).join(", ")}) joined in a way this editor does not draw`;
  }
  const top = tops[0] as XNode;
  let root: Root;
  if (top.name === "block") {
    const b = boxOf(top);
    if (typeof b === "string") return b;
    root = { t: "box", box: b };
  } else {
    const input = inputsOf(top)[0];
    const c = input?.conns[0];
    const expr = exprOf(input?.point ?? null);
    const src: Operand | null = c
      ? operandOf(byId.get(c.from) as XNode)
      : expr !== null
        ? { t: "operand", text: expr, negated: false, varId: null }
        : null;
    root = { t: "sink", sink: sinkOf(top), src };
  }
  // Every element of the group is in the tree, or it is not a tree.
  const seen = new Set<number>(idsOfRoot(root));
  for (const n of group) {
    if (!seen.has(idOf(n)))
      return `its lines run in a loop through ${n.name} ${idOf(n)} (a line drawn back), which this editor does not draw`;
  }
  return root;
}

/** Every localId of a tree: its boxes, operands, assignments, jumps and returns. */
export function idsOfRoot(root: Root): number[] {
  if (root.t === "sink")
    return [root.sink.id, ...(root.src?.varId != null ? [root.src.varId] : [])];
  return idsOfBox(root.box);
}

export function idsOfBox(box: Box): number[] {
  const out = [box.id];
  for (const p of box.inputs) {
    if (p.src?.t === "operand" && p.src.varId !== null) out.push(p.src.varId);
    if (p.src?.t === "box") out.push(...idsOfBox(p.src.box));
  }
  for (const o of box.outputs) out.push(...o.to.map((s) => s.id));
  return out;
}

// ---------------------------------------------------------------------------------------------
// Layout and writing
// ---------------------------------------------------------------------------------------------

/**
 * Room between an operand and its box, a box and the box it feeds, a box and its assignments: the
 * view writes each output's live value there (`2000 ms`, `-1234.5`), so it must not reach the
 * next element.
 */
const OPERAND_GAP = 70;
const BOX_GAP = 90;
const OUT_GAP = 90;
/**
 * An operand's or an assignment's height, and how far apart one is from the next: the view writes
 * a variable's own value under it, which needs the room below it.
 */
const VAR_H = 20;
const VAR_PITCH = 40;

/** The size of a box as written: its pins 20 apart from 30 below its top, as the view draws them. */
export function boxSize(box: Box): { w: number; h: number } {
  const inText = Math.max(
    0,
    ...box.inputs.map(
      (p) =>
        p.formal.length +
        (p.src?.t === "operand" && p.src.varId === null
          ? ` := ${p.src.text}`.length
          : 0),
    ),
  );
  const outText = Math.max(0, ...box.outputs.map((p) => p.formal.length));
  const w = up(
    Math.max(
      80,
      CHAR * (inText + outText) + 30,
      CHAR * box.typeName.length + 20,
      CHAR * box.instance.length + 4,
    ),
  );
  const h = 20 + 20 * Math.max(1, box.inputs.length, box.outputs.length);
  return { w, h };
}

/** A variable's width, as the view sizes one. */
export const varWidth = (text: string) =>
  up(Math.max(60, 8 * text.length + 16));
const sinkWidth = (s: Sink) =>
  s.kind === "assign"
    ? varWidth(s.text)
    : s.kind === "jump"
      ? up(Math.max(80, CHAR * (s.text.length + 4)))
      : 80;

/** Where an element of a laid-out tree is. */
interface Placed {
  id: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

function layoutRoot(root: Root): Placed[] {
  const out: Placed[] = [];
  const sinksAt = (
    x: number,
    outY: number,
    sinks: Sink[],
    cursor: number,
  ): number => {
    let at = cursor;
    for (const s of sinks) {
      const y = Math.max(at, outY - VAR_H / 2);
      out.push({ id: s.id, x, y, w: sinkWidth(s), h: VAR_H });
      at = y + VAR_PITCH;
    }
    return at;
  };
  /** Lay `box` with its right edge at `right` and its top at `top`; the bottom of what it took. */
  const layBox = (box: Box, right: number, top: number): number => {
    const { w, h } = boxSize(box);
    const x = right - w;
    out.push({ id: box.id, x, y: top, w, h });
    let cursor = top;
    box.inputs.forEach((pin, i) => {
      const pinY = top + 30 + 20 * i;
      const src = pin.src;
      if (src?.t === "operand" && src.varId !== null) {
        const ow = varWidth(src.text);
        const y = Math.max(cursor, pinY - VAR_H / 2);
        out.push({
          id: src.varId,
          x: x - OPERAND_GAP - ow,
          y,
          w: ow,
          h: VAR_H,
        });
        cursor = y + VAR_PITCH;
      } else if (src?.t === "box") {
        const index = Math.max(
          0,
          src.box.outputs.findIndex((o) => o.formal === src.output),
        );
        const y = Math.max(cursor, pinY - (30 + 20 * index));
        cursor = layBox(src.box, x - BOX_GAP, y) + 10;
      }
    });
    let sinks = top;
    box.outputs.forEach((o, j) => {
      sinks = sinksAt(x + w + OUT_GAP, top + 30 + 20 * j, o.to, sinks);
    });
    return Math.max(top + h, cursor - 10, sinks - 10);
  };
  if (root.t === "box") layBox(root.box, 0, 0);
  else {
    const ow = root.src?.varId != null ? varWidth(root.src.text) : 0;
    if (root.src?.varId != null)
      out.push({
        id: root.src.varId,
        x: -OPERAND_GAP - ow,
        y: 0,
        w: ow,
        h: VAR_H,
      });
    out.push({
      id: root.sink.id,
      x: 0,
      y: 0,
      w: sinkWidth(root.sink),
      h: VAR_H,
    });
  }
  // Moved so that its leftmost element is at the left of the body.
  const dx = -Math.min(...out.map((p) => p.x));
  return out.map((p) => ({ ...p, x: p.x + dx }));
}

/** What the editor needs to draw a network it wrote: its elements and its band. */
export interface Drawn {
  key: number;
  ids: number[];
  top: number;
  bottom: number;
}

export interface Written {
  text: string;
  /** Per POU, per network, in order. */
  drawn: Drawn[][];
}

/** Write the document from the model: the source's text where nothing changed. */
export function write(source: Source, model: DocModel, ids: Ids): Written {
  const src = source.text;
  const edits: Splice[] = [];
  const drawn: Drawn[][] = [];
  model.pous.forEach((pou, index) => {
    const sp = source.pous[index] as SourcePou;
    const body = layoutPou(source, sp, pou);
    drawn.push(body.drawn);
    if (pou.touched && sp.fbd) {
      if (sp.fbd.selfClosing) {
        const qname = sp.fbd.qname;
        edits.push({
          start: sp.fbd.start,
          end: sp.fbd.end,
          text:
            body.parts.length === 0
              ? `<${qname}/>`
              : `<${qname}>${body.parts.map((p) => `\n${p}`).join("")}\n</${qname}>`,
        });
      } else {
        const { inner, close } = indentOf(src, sp.fbd);
        edits.push({
          start: sp.fbd.openEnd,
          end: sp.fbd.closeStart,
          text:
            body.parts.map((p) => inner + p).join("") +
            (body.parts.length > 0 ? close : ""),
        });
      }
    }
    if (pou.declared.length > 0) edits.push(...declare(sp, pou.declared));
    if (pou.touched || ids.used.has(index))
      edits.push(record(sp, Math.max(ids.next, idsRecord(sp.node) ?? 0)));
  });
  return { text: edits.length === 0 ? src : splice(src, edits), drawn };
}

const ELEMENT_ATTRS = [
  "localId",
  "width",
  "height",
  "negated",
  "typeName",
  "instanceName",
  "label",
];

function layoutPou(
  source: Source,
  sp: SourcePou,
  pou: PouModel,
): { parts: string[]; drawn: Drawn[] } {
  const src = source.text;
  const prefix =
    sp.fbd && sp.fbd.qname.includes(":")
      ? `${sp.fbd.qname.split(":")[0]}:`
      : "";
  const q = (name: string) => prefix + name;
  const sizeOfNode = (n: XNode) => {
    const w = numOf(attr(n, "width"));
    const h = numOf(attr(n, "height"));
    if (n.name === "block") {
      const count = (k: string) =>
        childrenNamed(child(n, k) ?? n, "variable").length;
      return {
        w: w ?? 100,
        h:
          h ??
          20 +
            20 * Math.max(1, count("inputVariables"), count("outputVariables")),
      };
    }
    return { w: w ?? 80, h: h ?? 20 };
  };
  const parts: string[] = [];
  const drawn: Drawn[] = [];
  let cursor = Math.min(
    0,
    ...(sp.fbd?.children ?? [])
      .filter((c) => child(c, "position"))
      .map((c) => posOf(c).y),
  );
  for (const net of pou.networks) {
    const comment =
      net.commentId !== null ? sp.byId.get(net.commentId) : undefined;
    const label = net.labelId !== null ? sp.byId.get(net.labelId) : undefined;
    const hasComment = net.title !== "" || net.comment !== "";
    const hasLabel = net.label !== "" && net.labelId !== null;
    if (!net.dirty && net.ids.length > 0) {
      // Written as it was, moved down when what is above it grew past it.
      const own = [...net.ids, ...net.extras]
        .map((id) => sp.byId.get(id))
        .filter((n): n is XNode => n !== undefined);
      const found = [
        ...own,
        ...[comment, label].filter((n): n is XNode => n !== undefined),
      ];
      const top = Math.min(...found.map((n) => posOf(n).y));
      const elementsTop = Math.min(
        ...net.ids.map((id) => posOf(sp.byId.get(id) as XNode).y),
      );
      const room =
        (hasComment && !comment ? commentHeight(net) + 10 : 0) +
        (hasLabel && !label ? 20 : 0);
      const dy = Math.max(0, cursor + room - top);
      const bottom = Math.max(
        ...found.map((n) => posOf(n).y + sizeOfNode(n).h),
      );
      if (net.headerDirty) {
        const cAt = comment ? posOf(comment) : { x: 0, y: top - room };
        const c = commentPart(src, sp, net, q, cAt.x, cAt.y + dy);
        if (c) parts.push(c.text);
        const lAt = label ? posOf(label) : { x: 0, y: elementsTop - 20 };
        const l = labelPart(src, sp, net, q, lAt.x, lAt.y + dy);
        if (l) parts.push(l);
      } else {
        for (const n of [comment, label])
          if (n) parts.push(shifted(src, n, dy));
      }
      for (const n of own) parts.push(shifted(src, n, dy));
      drawn.push({
        key: net.key,
        ids: net.ids,
        top: Math.min(top + dy, cursor),
        bottom: bottom + dy,
      });
      cursor = up(bottom + dy + 30);
      continue;
    }
    // Laid out anew: the comment, then the label, then the tree. A comment or label that did not
    // change is written as found, only moved.
    const top = cursor;
    let at = top;
    const c =
      !net.headerDirty && comment
        ? {
            text: shifted(src, comment, at - posOf(comment).y),
            h: sizeOfNode(comment).h,
          }
        : commentPart(src, sp, net, q, 0, at);
    if (c) {
      parts.push(c.text);
      at += c.h + 10;
    }
    const l =
      !net.headerDirty && label
        ? shifted(src, label, at - posOf(label).y)
        : labelPart(src, sp, net, q, 0, at);
    if (l) {
      parts.push(l);
      at += 20;
    }
    for (const id of net.extras) {
      const n = sp.byId.get(id);
      if (!n) continue;
      parts.push(shifted(src, n, at - posOf(n).y));
      at += sizeOfNode(n).h + 10;
    }
    if (net.root === null) {
      drawn.push({ key: net.key, ids: [], top, bottom: at });
      cursor = up(at + 20);
      continue;
    }
    const placed = layoutRoot(net.root).map((p) => ({
      ...p,
      y: p.y + up(at + 10),
    }));
    const written = emitRoot(src, sp, net.root, placed, q);
    parts.push(...written);
    const bottom = Math.max(...placed.map((p) => p.y + p.h));
    drawn.push({ key: net.key, ids: idsOfRoot(net.root), top, bottom });
    cursor = up(bottom + 30);
  }
  return {
    parts: [...sp.others.map((n) => src.slice(n.start, n.end)), ...parts],
    drawn,
  };
}

/** The elements of a tree laid out at `placed`, each input wired to its one source. */
function emitRoot(
  src: string,
  sp: SourcePou,
  root: Root,
  placed: Placed[],
  q: (name: string) => string,
): string[] {
  const at = new Map(placed.map((p) => [p.id, p]));
  const parts: string[] = [];
  const position = (p: Placed) =>
    `<${q("position")} x="${num(p.x)}" y="${num(p.y)}"/>`;
  const rel = (x: number, y: number) =>
    `<${q("relPosition")} x="${num(x)}" y="${num(y)}"/>`;
  const sized = (
    id: number,
    p: Placed,
    found: XNode | undefined,
    more: Array<[string, string]>,
  ): Array<[string, string]> => [
    ["localId", String(id)],
    ["width", num(p.w)],
    ["height", num(p.h)],
    ...more,
    ...extraAttrs(found, ELEMENT_ATTRS),
  ];
  /** The point an input takes its source at: one connection, an expression, or nothing. */
  const pointIn = (s: Src | null, y: number) => {
    let inner = "";
    if (s?.t === "operand")
      inner =
        s.varId === null
          ? `<${q("expression")}>${escapeText(s.text)}</${q("expression")}>`
          : `<${q("connection")} refLocalId="${s.varId}"/>`;
    else if (s?.t === "box")
      inner = `<${q("connection")} refLocalId="${s.box.id}" formalParameter="${escapeAttr(s.output)}"/>`;
    return `<${q("connectionPointIn")}>${rel(0, y)}${inner}</${q("connectionPointIn")}>`;
  };
  const operand = (o: Operand) => {
    if (o.varId === null) return;
    const p = at.get(o.varId) as Placed;
    const found = sp.byId.get(o.varId);
    const name = found?.qname ?? q("inVariable");
    parts.push(
      `${openTag(name, sized(o.varId, p, found, o.negated ? [["negated", "true"]] : []))}${position(p)}<${q("connectionPointOut")}>${rel(p.w, p.h / 2)}</${q("connectionPointOut")}><${q("expression")}>${escapeText(o.text)}</${q("expression")}>${extrasOf(src, found, ["position", "connectionPointOut", "expression"])}</${name}>`,
    );
  };
  const sink = (s: Sink, from: Src | null) => {
    const p = at.get(s.id) as Placed;
    const found = sp.byId.get(s.id);
    if (s.kind === "assign") {
      const name = found?.qname ?? q("outVariable");
      parts.push(
        `${openTag(name, sized(s.id, p, found, s.negated ? [["negated", "true"]] : []))}${position(p)}${pointIn(from, p.h / 2)}<${q("expression")}>${escapeText(s.text)}</${q("expression")}>${extrasOf(src, found, ["position", "connectionPointIn", "expression"])}</${name}>`,
      );
      return;
    }
    const name = found?.qname ?? q(s.kind);
    parts.push(
      `${openTag(name, sized(s.id, p, found, s.kind === "jump" ? [["label", s.text]] : []))}${position(p)}${pointIn(from, p.h / 2)}${extrasOf(src, found, ["position", "connectionPointIn"])}</${name}>`,
    );
  };
  const box = (b: Box) => {
    const p = at.get(b.id) as Placed;
    const found = sp.byId.get(b.id);
    const name = found?.qname ?? q("block");
    const more: Array<[string, string]> = [["typeName", b.typeName]];
    if (b.instance !== "") more.push(["instanceName", b.instance]);
    const pinExtras = (which: string, formal: string) => {
      const list = found ? child(found, which) : null;
      const v = list
        ? childrenNamed(list, "variable").find(
            (x) => attr(x, "formalParameter") === formal,
          )
        : undefined;
      return extrasOf(src, v, ["connectionPointIn", "connectionPointOut"]);
    };
    const inputs = b.inputs
      .map(
        (pin, i) =>
          `<${q("variable")} formalParameter="${escapeAttr(pin.formal)}"${pin.negated ? ' negated="true"' : ""}>${pointIn(pin.src, 30 + 20 * i)}${pinExtras("inputVariables", pin.formal)}</${q("variable")}>`,
      )
      .join("");
    const outputs = b.outputs
      .map(
        (o, i) =>
          `<${q("variable")} formalParameter="${escapeAttr(o.formal)}"${o.negated ? ' negated="true"' : ""}><${q("connectionPointOut")}>${rel(p.w, 30 + 20 * i)}</${q("connectionPointOut")}>${pinExtras("outputVariables", o.formal)}</${q("variable")}>`,
      )
      .join("");
    parts.push(
      `${openTag(name, sized(b.id, p, found, more))}${position(p)}<${q("inputVariables")}>${inputs}</${q("inputVariables")}><${q("inOutVariables")}/><${q("outputVariables")}>${outputs}</${q("outputVariables")}>${extrasOf(src, found, ["position", "inputVariables", "inOutVariables", "outputVariables"])}</${name}>`,
    );
    for (const pin of b.inputs) {
      if (pin.src?.t === "operand") operand(pin.src);
      else if (pin.src?.t === "box") box(pin.src.box);
    }
    for (const o of b.outputs)
      for (const s of o.to) sink(s, { t: "box", box: b, output: o.formal });
  };
  if (root.t === "box") box(root.box);
  else {
    if (root.src) operand(root.src);
    sink(root.sink, root.src);
  }
  return parts;
}

/** The variables of a POU of the source, and those the editor declared in it. */
export function varsOfPou(
  source: Source,
  model: DocModel,
  index: number,
): VarInfo[] {
  const sp = source.pous[index];
  const pou = model.pous[index];
  return [
    ...(sp?.vars ?? []),
    ...(pou?.declared ?? []).map((d) => ({
      name: d.name,
      type: d.type,
      section: d.section,
    })),
  ];
}
