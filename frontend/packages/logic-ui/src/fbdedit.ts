// The edits of the Function Block Diagram editor, as functions from one model to the next (ADR-013
// §2.8, L3c-2), and its own check of what is written in it. Each edit returns a new model and
// leaves the one it was given as it was: the editor keeps those it passed through for undo.
//
// **Where an instruction goes** (as TIA's FBD editor inserts, in its terms: a box goes into the
// line the selection is on):
// - a box at a selected input takes that input's place: what the input read (an operand or a
//   box) goes into the new box's first input;
// - a box after a selected box: the selected box's output goes into the new box's first input,
//   the new box takes its place, and the assignments of that output move to the new box's;
// - a box at a selected assignment, jump or return that reads an operand: the operand goes into
//   the box, and the assignment is written from the box's output;
// - with only the network selected, as at the end of its tree (its last box, or its assignment),
//   or as the network's first element when it has none;
// - an assignment, a jump or a return at a selected box (or one of its assignments) is one more
//   written from that box's output, as TIA branches an output; with only an empty network
//   selected, the network's element, reading nothing yet.
//
// **One line into one input** (§8.3.2, no wired-OR). An input holds one source. An operand typed
// at an input that a box feeds is refused, and the editor offers what TIA would: an OR box
// joining both (`orAt`).

import { EXTENSIBLE, FBD_STANDARD, findFbdBox, mainOutput } from "./fbdcatalog";
import { family, type BoxDef } from "./ldcatalog";
import {
  declaredType,
  instanceOfFunctionWords,
  LIBRARY_GROUP,
  pinProblems,
  twinWords,
  unknownBoxWords,
  type LibraryBoxes,
} from "./libraryblocks";
import { operandProblem, operands, PLACE } from "./ldedit";
import type { Declared, Ids, Section, VarInfo } from "./ldmodel";
import type {
  Box,
  DocModel,
  Network,
  Operand,
  Out,
  Pin,
  PouModel,
  Root,
  Sink,
  Src,
} from "./fbdmodel";

const IN_PIN = /^IN\d+$/i;

export { operands, PLACE };

/** What is selected: a network, an element (a box, an assignment, a jump, a return), or an input. */
export type Sel =
  | { kind: "network"; pou: number; net: number }
  | { kind: "element"; pou: number; net: number; id: number }
  /** An input of box `box`; of an assignment, jump or return that is a network's whole tree, `formal` is `''`. */
  | { kind: "pin"; pou: number; net: number; box: number; formal: string };

/** An instruction of the palette. */
export type FbdSpec =
  | { kind: "box"; type: string }
  | { kind: "assign"; negated?: boolean }
  | { kind: "jump" }
  | { kind: "return" };

export type Edited =
  { ok: true; model: DocModel; sel: Sel | null } | { ok: false; error: string };

const clone = (model: DocModel): DocModel => ({
  ...(JSON.parse(JSON.stringify(model)) as DocModel),
  rev: model.rev + 1,
});

const alloc = (ids: Ids, pou: number) => {
  const next = ids.next;
  ids.next += 1;
  ids.used.add(pou);
  return next;
};

// ---------------------------------------------------------------------------------------------
// Finding things in a tree
// ---------------------------------------------------------------------------------------------

/** Every box of a tree, the root's first. */
export function boxesOf(root: Root | null): Box[] {
  if (!root || root.t !== "box") return [];
  const out: Box[] = [];
  const visit = (b: Box) => {
    out.push(b);
    for (const p of b.inputs) if (p.src?.t === "box") visit(p.src.box);
  };
  visit(root.box);
  return out;
}

/** Every assignment, jump and return of a tree. */
export function sinksOf(root: Root | null): Sink[] {
  if (!root) return [];
  if (root.t === "sink") return [root.sink];
  return boxesOf(root).flatMap((b) => b.outputs.flatMap((o) => o.to));
}

export const findBoxIn = (root: Root | null, id: number): Box | null =>
  boxesOf(root).find((b) => b.id === id) ?? null;

/** The input a box is nested at, with the box it is an input of; `null` for the root box. */
function parentPin(
  root: Root | null,
  id: number,
): { owner: Box; pin: Pin } | null {
  for (const b of boxesOf(root)) {
    for (const p of b.inputs)
      if (p.src?.t === "box" && p.src.box.id === id)
        return { owner: b, pin: p };
  }
  return null;
}

/** The output an assignment, jump or return is written from (`null`: it is the network's tree). */
function sinkPlace(
  root: Root | null,
  id: number,
): { box: Box | null; out: Out | null; sink: Sink } | null {
  if (!root) return null;
  if (root.t === "sink")
    return root.sink.id === id
      ? { box: null, out: null, sink: root.sink }
      : null;
  for (const b of boxesOf(root)) {
    for (const o of b.outputs) {
      const s = o.to.find((x) => x.id === id);
      if (s) return { box: b, out: o, sink: s };
    }
  }
  return null;
}

/** The network element `id` (a box, an assignment, a jump, a return, or an operand's variable) is in. */
export function networkOf(pou: PouModel, id: number): number {
  return pou.networks.findIndex(
    (n) => n.root !== null && idsIn(n.root).includes(id),
  );
}

function idsIn(root: Root): number[] {
  if (root.t === "sink")
    return [root.sink.id, ...(root.src?.varId != null ? [root.src.varId] : [])];
  return boxesOf(root).flatMap((b) => [
    b.id,
    ...b.inputs.flatMap((p) =>
      p.src?.t === "operand" && p.src.varId !== null ? [p.src.varId] : [],
    ),
    ...b.outputs.flatMap((o) => o.to.map((s) => s.id)),
  ]);
}

/** The input an operand's variable feeds, as a selection. */
export function pinOfOperand(
  root: Root | null,
  varId: number,
): { box: number; formal: string } | null {
  if (!root) return null;
  if (root.t === "sink")
    return root.src?.varId === varId ? { box: root.sink.id, formal: "" } : null;
  for (const b of boxesOf(root)) {
    for (const p of b.inputs)
      if (p.src?.t === "operand" && p.src.varId === varId)
        return { box: b.id, formal: p.formal };
  }
  return null;
}

/** The main output of a box: what a nested box gives its parent. */
const mainOf = (b: Box): Out | null =>
  b.outputs.find((o) => o.formal === mainOutput(b.outputs)) ?? null;

// ---------------------------------------------------------------------------------------------
// New boxes and their types
// ---------------------------------------------------------------------------------------------

/** A new box of `type`: its pins as its declaration gives them; an empty box (`??`) two inputs and an output, as TIA's. */
export function newBox(type: string, id: number, own: readonly BoxDef[]): Box {
  const box: Box = { id, typeName: "", instance: "", inputs: [], outputs: [] };
  // A new box has nothing to lose, so its type is always taken.
  retype(box, type, own);
  return box;
}

/**
 * A box given type `type`: each input keeps what it read by its formal parameter; what an input
 * the new type does not have read goes into the first free input of the new type, in order. An
 * input that would lose what it reads refuses the change.
 */
function retype(box: Box, type: string, own: readonly BoxDef[]): string | null {
  const def = findFbdBox(type, own);
  const pins = def
    ? { inputs: def.inputs, outputs: def.outputs }
    : {
        inputs: [
          { name: "IN1", type: "ANY" },
          { name: "IN2", type: "ANY" },
        ],
        outputs: [{ name: "OUT", type: "ANY" }],
      };
  const byFormal = (list: Array<{ formal: string }>, formal: string) =>
    list.findIndex((p) => p.formal.toUpperCase() === formal.toUpperCase());
  // EN and ENO another tool wrote stay, ahead of the declaration's pins.
  const en = box.inputs.filter((p) => p.formal.toUpperCase() === "EN");
  const eno = box.outputs.filter((p) => p.formal.toUpperCase() === "ENO");
  const inputs: Pin[] = [
    ...en,
    ...pins.inputs.map((p) => ({
      formal: p.name,
      type: p.type,
      negated: false,
      src: null as Src | null,
    })),
  ];
  // AND, OR and XOR keep the inputs they were given beyond two.
  if (def && EXTENSIBLE.has(def.name)) {
    const extra = box.inputs.filter(
      (p) =>
        /^IN\d+$/i.test(p.formal) &&
        Number(p.formal.slice(2)) > def.inputs.length,
    );
    for (const p of extra)
      inputs.push({
        formal: p.formal.toUpperCase(),
        type: def.inputs[0]?.type ?? "ANY",
        negated: false,
        src: null,
      });
  }
  const homeless: Pin[] = [];
  for (const old of box.inputs) {
    if (old.formal.toUpperCase() === "EN") continue;
    const k = byFormal(inputs, old.formal);
    if (k >= 0) {
      (inputs[k] as Pin).src = old.src;
      (inputs[k] as Pin).negated = old.negated;
    } else if (old.src) homeless.push(old);
  }
  for (const old of homeless) {
    const free = inputs.find(
      (p) => p.src === null && p.formal.toUpperCase() !== "EN",
    );
    if (!free)
      return `${def?.name ?? type} has no input left for what ${old.formal} reads: delete it first.`;
    free.src = old.src;
    free.negated = old.negated;
  }
  const outputs: Out[] = [
    ...eno,
    ...pins.outputs.map((p) => ({
      formal: p.name,
      type: p.type,
      negated: false,
      to: [] as Sink[],
    })),
  ];
  const lost: Sink[] = [];
  for (const old of box.outputs) {
    if (old.formal.toUpperCase() === "ENO") continue;
    const k = byFormal(outputs, old.formal);
    if (k >= 0) {
      (outputs[k] as Out).to = old.to;
      (outputs[k] as Out).negated = old.negated;
    } else lost.push(...old.to);
  }
  // What the old main output wrote goes to the new main output.
  const main = outputs.find((o) => o.formal === mainOutput(outputs));
  if (main) main.to.push(...lost);
  else if (lost.length > 0)
    return `${def?.name ?? type} has no output to write ${lost.map((s) => s.text || s.kind).join(", ")}.`;
  box.typeName = def?.name ?? type.trim();
  box.inputs = inputs;
  box.outputs = outputs;
  if (def?.kind === "function") box.instance = "";
  return null;
}

// ---------------------------------------------------------------------------------------------
// Edits
// ---------------------------------------------------------------------------------------------

function netOf(
  m: DocModel,
  sel: Sel,
): { pou: PouModel; net: Network } | string {
  const pou = m.pous[sel.pou];
  const net = pou?.networks[sel.net];
  if (!pou || !net) return "The selection is no longer there.";
  if (net.why !== null)
    return `Network ${sel.net + 1} is kept as the document has it (${net.why}); it is not edited here.`;
  return { pou, net };
}

function touch(pou: PouModel, net: Network): void {
  pou.touched = true;
  net.dirty = true;
}

/** Put an instruction at the selection. */
export function insert(
  model: DocModel,
  ids: Ids,
  sel: Sel | null,
  spec: FbdSpec,
  own: readonly BoxDef[],
): Edited {
  if (sel === null)
    return { ok: false, error: "Select a network, a box or an input first." };
  const m = clone(model);
  const found = netOf(m, sel);
  if (typeof found === "string") return { ok: false, error: found };
  const { pou, net } = found;
  // A network selected is its tree's end: its root box, or its assignment.
  let at: Sel = sel;
  if (sel.kind === "network" && net.root !== null) {
    at = {
      kind: "element",
      pou: sel.pou,
      net: sel.net,
      id: net.root.t === "box" ? net.root.box.id : net.root.sink.id,
    };
  }
  const done = (id: number, focus: Sel | null = null): Edited => {
    touch(pou, net);
    return {
      ok: true,
      model: m,
      sel: focus ?? { kind: "element", pou: sel.pou, net: sel.net, id },
    };
  };
  if (spec.kind === "box") {
    const box = newBox(spec.type, ids.next, own);
    const take = (src: Src | null): string | null => {
      if (src === null) return null;
      const first = box.inputs.find((p) => p.formal.toUpperCase() !== "EN");
      if (!first) return `${box.typeName} has no input to take what is there.`;
      first.src = src;
      return null;
    };
    if (at.kind === "network") {
      net.root = { t: "box", box };
    } else if (at.kind === "pin") {
      const root = net.root;
      if (root?.t === "sink" && root.sink.id === at.box) {
        const problem = take(root.src);
        if (problem) return { ok: false, error: problem };
        const main = mainOf(box);
        if (!main)
          return {
            ok: false,
            error: `${box.typeName} has no output to write ${root.sink.text || "it"}.`,
          };
        main.to.push(root.sink);
        net.root = { t: "box", box };
      } else {
        const pin = findBoxIn(root, at.box)?.inputs.find(
          (p) => p.formal === at.formal,
        );
        if (!pin)
          return { ok: false, error: "The selected input is no longer there." };
        const problem = take(pin.src);
        if (problem) return { ok: false, error: problem };
        const main = mainOf(box);
        if (!main)
          return {
            ok: false,
            error: `${box.typeName} has no output to give the input.`,
          };
        pin.src = { t: "box", box, output: main.formal };
      }
    } else {
      const root = net.root;
      const target = findBoxIn(root, at.id);
      if (target) {
        // After the box: its output goes into the new box, which takes its place.
        const out = mainOf(target);
        if (!out)
          return {
            ok: false,
            error: `${target.typeName} gives no output to go on with.`,
          };
        const problem = take({ t: "box", box: target, output: out.formal });
        if (problem) return { ok: false, error: problem };
        const main = mainOf(box);
        if (!main)
          return { ok: false, error: `${box.typeName} has no output.` };
        main.to.push(...out.to);
        out.to = [];
        const parent = parentPin(root, target.id);
        if (parent) parent.pin.src = { t: "box", box, output: main.formal };
        else net.root = { t: "box", box };
        // The target is now inside the new box: nest it there (the clone holds it once).
      } else {
        const place = sinkPlace(root, at.id);
        if (!place)
          return {
            ok: false,
            error: "The selected element is no longer there.",
          };
        const main = mainOf(box);
        if (!main)
          return {
            ok: false,
            error: `${box.typeName} has no output to write ${place.sink.text || "it"}.`,
          };
        if (place.box === null && root?.t === "sink") {
          const problem = take(root.src);
          if (problem) return { ok: false, error: problem };
          main.to.push(root.sink);
          net.root = { t: "box", box };
        } else if (
          place.box !== null &&
          place.out !== null &&
          root?.t === "box" &&
          root.box.id === place.box.id
        ) {
          // Between the root box and one of its assignments: the box reads that output.
          const problem = take({
            t: "box",
            box: place.box,
            output: place.out.formal,
          });
          if (problem) return { ok: false, error: problem };
          place.out.to = place.out.to.filter((s) => s.id !== place.sink.id);
          main.to.push(place.sink);
          net.root = { t: "box", box };
        } else {
          return {
            ok: false,
            error: `Select block ${place.box?.id ?? ""} to put a box after it: an output of a box inside the tree goes into one box.`,
          };
        }
      }
    }
    ids.next += 1;
    ids.used.add(sel.pou);
    return done(box.id);
  }
  // An assignment, a jump or a return.
  const sink: Sink = {
    id: ids.next,
    kind: spec.kind,
    text: "",
    negated: spec.kind === "assign" && spec.negated === true,
  };
  if (at.kind === "network") {
    net.root = { t: "sink", sink, src: null };
  } else if (at.kind === "pin" && !findBoxIn(net.root, at.box)) {
    return {
      ok: false,
      error: `An ${spec.kind === "assign" ? "assignment" : spec.kind} is written from a box's output: put a box before this one first.`,
    };
  } else {
    // An input selected: the box it belongs to.
    const box = findBoxIn(net.root, at.kind === "pin" ? at.box : at.id);
    const place = box || at.kind === "pin" ? null : sinkPlace(net.root, at.id);
    const out = box ? mainOf(box) : place?.out;
    if (!out) {
      return {
        ok: false,
        error:
          place?.box === null
            ? `Network ${sel.net + 1} writes its operand already: put a box before the ${place.sink.kind === "assign" ? "assignment" : place.sink.kind} to write more from it.`
            : "The selected element gives no output.",
      };
    }
    const index = place
      ? out.to.findIndex((s) => s.id === place.sink.id) + 1
      : out.to.length;
    out.to.splice(index, 0, sink);
  }
  ids.next += 1;
  ids.used.add(sel.pou);
  return done(sink.id);
}

/** One more assignment (or a jump, or a return) written from output `formal` of box `boxId`. */
export function addSink(
  model: DocModel,
  ids: Ids,
  pou: number,
  net: number,
  boxId: number,
  formal: string,
  kind: Sink["kind"] = "assign",
): Edited {
  const m = clone(model);
  const found = netOf(m, { kind: "element", pou, net, id: boxId });
  if (typeof found === "string") return { ok: false, error: found };
  const out = findBoxIn(found.net.root, boxId)?.outputs.find(
    (o) => o.formal === formal,
  );
  if (!out) return { ok: false, error: "The output is no longer there." };
  const sink: Sink = { id: alloc(ids, pou), kind, text: "", negated: false };
  out.to.push(sink);
  touch(found.pou, found.net);
  return {
    ok: true,
    model: m,
    sel: { kind: "element", pou, net, id: sink.id },
  };
}

/** One more input of an AND, OR or XOR (TIA's "insert input"). */
export function addInput(model: DocModel, sel: Sel | null): Edited {
  if (sel === null || sel.kind === "network")
    return {
      ok: false,
      error: "Select the AND, OR or XOR box, or one of its inputs.",
    };
  const m = clone(model);
  const found = netOf(m, sel);
  if (typeof found === "string") return { ok: false, error: found };
  const box = findBoxIn(found.net.root, sel.kind === "pin" ? sel.box : sel.id);
  if (!box || !EXTENSIBLE.has(box.typeName.toUpperCase()))
    return {
      ok: false,
      error: "Only an AND, an OR or an XOR takes more inputs.",
    };
  const n = box.inputs.filter((p) => IN_PIN.test(p.formal)).length + 1;
  box.inputs.push({
    formal: `IN${n}`,
    type: box.inputs.find((p) => IN_PIN.test(p.formal))?.type ?? "BOOL",
    negated: false,
    src: null,
  });
  touch(found.pou, found.net);
  return {
    ok: true,
    model: m,
    sel: {
      kind: "pin",
      pou: sel.pou,
      net: sel.net,
      box: box.id,
      formal: `IN${n}`,
    },
  };
}

/** Delete what is selected: a box (what its first input read takes its place), an assignment, what an input reads, an input added, or a network. */
export function remove(model: DocModel, sel: Sel | null): Edited {
  if (sel === null) return { ok: false, error: "Nothing is selected." };
  const m = clone(model);
  const pou = m.pous[sel.pou];
  if (!pou || !pou.networks[sel.net])
    return { ok: false, error: "The selection is no longer there." };
  if (sel.kind === "network") {
    pou.networks.splice(sel.net, 1);
    pou.touched = true;
    const next = Math.min(sel.net, pou.networks.length - 1);
    return {
      ok: true,
      model: m,
      sel: next >= 0 ? { kind: "network", pou: sel.pou, net: next } : null,
    };
  }
  const found = netOf(m, sel);
  if (typeof found === "string")
    return {
      ok: false,
      error: `Network ${sel.net + 1} is kept as the document has it; delete it as a whole.`,
    };
  const { net } = found;
  const root = net.root;
  const back: Sel = { kind: "network", pou: sel.pou, net: sel.net };
  if (sel.kind === "pin") {
    if (root?.t === "sink" && root.sink.id === sel.box) {
      if (root.src === null)
        return { ok: false, error: "The input reads nothing." };
      root.src = null;
    } else {
      const box = findBoxIn(root, sel.box);
      const index = box?.inputs.findIndex((p) => p.formal === sel.formal) ?? -1;
      const pin = box?.inputs[index];
      if (!box || !pin)
        return { ok: false, error: "The selected input is no longer there." };
      if (pin.src !== null) pin.src = null;
      else if (
        EXTENSIBLE.has(box.typeName.toUpperCase()) &&
        IN_PIN.test(pin.formal) &&
        box.inputs.filter((p) => IN_PIN.test(p.formal)).length > 2
      ) {
        // Only the INn inputs are numbered again; EN stays EN.
        box.inputs.splice(index, 1);
        let k = 0;
        for (const p of box.inputs)
          if (IN_PIN.test(p.formal)) p.formal = `IN${++k}`;
      } else
        return {
          ok: false,
          error: `${pin.formal} reads nothing, and ${box.typeName || "the box"} keeps it.`,
        };
    }
    touch(found.pou, net);
    return { ok: true, model: m, sel };
  }
  const box = findBoxIn(root, sel.id);
  if (box) {
    const first = box.inputs.find((p) => p.formal.toUpperCase() !== "EN");
    const taking = first?.src ?? null;
    const main = mainOf(box);
    const parent = parentPin(root, box.id);
    // What reads the box must not vanish with it: an output that stops being written keeps
    // its last value. Only the main output's assignments carry over, to what takes its place.
    const others = box.outputs.filter((o) => o !== main && o.to.length > 0);
    if (others.length > 0) {
      return {
        ok: false,
        error: `Delete the assignments of ${others.map((o) => o.formal).join(", ")} first.`,
      };
    }
    if (!parent && taking?.t !== "box" && (main?.to.length ?? 0) > 1) {
      return {
        ok: false,
        error: `${box.typeName || "The box"} is assigned to ${main?.to.length} variables; delete all but one first.`,
      };
    }
    if (parent) parent.pin.src = taking;
    else if (taking?.t === "box") {
      const out = taking.box.outputs.find((o) => o.formal === taking.output);
      out?.to.push(...(main?.to ?? []));
      net.root = { t: "box", box: taking.box };
    } else {
      const sink = main?.to[0];
      net.root = sink ? { t: "sink", sink, src: taking } : null;
    }
  } else {
    const place = sinkPlace(root, sel.id);
    if (!place)
      return { ok: false, error: "The selected element is no longer there." };
    if (place.out) place.out.to = place.out.to.filter((s) => s.id !== sel.id);
    else net.root = null;
  }
  touch(found.pou, net);
  return { ok: true, model: m, sel: back };
}

/** Change a box, an assignment, a jump or a return of a network the editor draws. */
function change(
  model: DocModel,
  ids: Ids,
  sel: Sel,
  edit: (root: Root, net: Network, alloc: () => number) => string | null,
): Edited {
  const m = clone(model);
  const found = netOf(m, sel);
  if (typeof found === "string") return { ok: false, error: found };
  if (!found.net.root)
    return { ok: false, error: "The network has nothing to change." };
  const problem = edit(found.net.root, found.net, () => alloc(ids, sel.pou));
  if (problem) return { ok: false, error: problem };
  touch(found.pou, found.net);
  return { ok: true, model: m, sel: null };
}

const at = (pou: number, net: number, id: number): Sel => ({
  kind: "element",
  pou,
  net,
  id,
});

export const setBoxType = (
  model: DocModel,
  ids: Ids,
  pou: number,
  net: number,
  id: number,
  type: string,
  own: readonly BoxDef[],
) =>
  change(model, ids, at(pou, net, id), (root) => {
    const box = findBoxIn(root, id);
    return box ? retype(box, type, own) : "The box is no longer there.";
  });

export const setInstance = (
  model: DocModel,
  ids: Ids,
  pou: number,
  net: number,
  id: number,
  text: string,
) =>
  change(model, ids, at(pou, net, id), (root) => {
    const box = findBoxIn(root, id);
    if (!box) return "The box is no longer there.";
    box.instance = text.trim();
    return null;
  });

/** An assignment's variable, or a jump's label. */
export const setSinkText = (
  model: DocModel,
  ids: Ids,
  pou: number,
  net: number,
  id: number,
  text: string,
) =>
  change(model, ids, at(pou, net, id), (root) => {
    const place = sinkPlace(root, id);
    if (!place) return "The element is no longer there.";
    place.sink.text = text.trim();
    return null;
  });

/** Negate an input, an output, an operand's variable or an assignment (the circle of §6.6.1.4.3 rule 9). */
export const setNegated = (
  model: DocModel,
  ids: Ids,
  pou: number,
  net: number,
  what: { box: number; input?: string; output?: string } | { sink: number },
  negated: boolean,
) =>
  change(
    model,
    ids,
    at(pou, net, "sink" in what ? what.sink : what.box),
    (root) => {
      if ("sink" in what) {
        const place = sinkPlace(root, what.sink);
        if (!place || place.sink.kind !== "assign")
          return "Only an assignment is negated.";
        place.sink.negated = negated;
        return null;
      }
      const box = findBoxIn(root, what.box);
      const pin =
        what.input !== undefined
          ? box?.inputs.find((p) => p.formal === what.input)
          : box?.outputs.find((p) => p.formal === what.output);
      if (!pin) return "The pin is no longer there.";
      pin.negated = negated;
      return null;
    },
  );

/** What an input reads, typed: an operand. Never a second line into an input a box feeds (§8.3.2). */
export const setPinOperand = (
  model: DocModel,
  ids: Ids,
  sel: Extract<Sel, { kind: "pin" }>,
  text: string,
) =>
  change(model, ids, sel, (root, _net, next) => {
    const t = text.trim();
    let holder: { src: Src | null } | null = null;
    if (root.t === "sink" && root.sink.id === sel.box)
      holder = root as unknown as { src: Src | null };
    else
      holder =
        findBoxIn(root, sel.box)?.inputs.find((p) => p.formal === sel.formal) ??
        null;
    if (!holder) return "The input is no longer there.";
    const src = holder.src;
    if (src?.t === "box") {
      const where = sel.formal === "" ? "The input" : sel.formal;
      return `${where} takes the output of block ${src.box.id} (${src.box.typeName || "??"}); an input takes one line (IEC 61131-3 ed. 3 §8.3.2). Join both with an OR box, or delete the block first.`;
    }
    if (t === "") holder.src = null;
    else if (src?.t === "operand") src.text = t;
    else
      holder.src = {
        t: "operand",
        text: t,
        negated: false,
        varId: next(),
      } satisfies Operand;
    return null;
  });

/**
 * Join what an input reads with a second signal in an OR box, as TIA offers where a second line
 * would go into one input: the OR takes the input's place, what it read goes into IN1, and IN2
 * is selected for the second.
 */
export function orAt(
  model: DocModel,
  ids: Ids,
  sel: Extract<Sel, { kind: "pin" }>,
  own: readonly BoxDef[],
): Edited {
  const m = clone(model);
  const found = netOf(m, sel);
  if (typeof found === "string") return { ok: false, error: found };
  const root = found.net.root;
  const box =
    root?.t === "sink" && root.sink.id === sel.box
      ? null
      : findBoxIn(root, sel.box);
  const pin = box?.inputs.find((p) => p.formal === sel.formal);
  // The type the box's block declares now (a pin read from the text before the library's
  // blocks were known is typed ANY), else the one the model took.
  const type =
    box && pin
      ? declaredType(findFbdBox(box.typeName, own), "inputs", pin.formal, pin.type)
      : "BOOL";
  if (family(type) !== null && family(type) !== "BOOL")
    return {
      ok: false,
      error: `An OR joins BOOL signals; ${sel.formal} takes a ${type}.`,
    };
  const edited = insert(model, ids, sel, { kind: "box", type: "OR" }, own);
  if (!edited.ok || edited.sel?.kind !== "element") return edited;
  return {
    ...edited,
    sel: {
      kind: "pin",
      pou: sel.pou,
      net: sel.net,
      box: edited.sel.id,
      formal: "IN2",
    },
  };
}

// Networks, as LD's.

function header(
  model: DocModel,
  ids: Ids,
  pouIndex: number,
  n: number,
  edit: (net: Network, alloc: () => number) => void,
): Edited {
  const m = clone(model);
  const pou = m.pous[pouIndex];
  const net = pou?.networks[n];
  if (!pou || !net)
    return { ok: false, error: "The network is no longer there." };
  edit(net, () => alloc(ids, pouIndex));
  net.headerDirty = true;
  pou.touched = true;
  return { ok: true, model: m, sel: null };
}

export const setHeader = (
  model: DocModel,
  ids: Ids,
  pou: number,
  n: number,
  what: "title" | "comment",
  text: string,
) =>
  header(model, ids, pou, n, (net, next) => {
    net[what] = text;
    if (net.commentId === null && (net.title !== "" || net.comment !== ""))
      net.commentId = next();
  });

export const setLabel = (
  model: DocModel,
  ids: Ids,
  pou: number,
  n: number,
  text: string,
) =>
  header(model, ids, pou, n, (net, next) => {
    net.label = text.trim();
    if (net.labelId === null && net.label !== "") net.labelId = next();
  });

let keys = 2_000_000;

export function addNetwork(
  model: DocModel,
  pouIndex: number,
  atIndex: number,
): Edited {
  const m = clone(model);
  const pou = m.pous[pouIndex];
  if (!pou) return { ok: false, error: "There is no such POU." };
  keys += 1;
  const net: Network = {
    key: keys,
    title: "",
    comment: "",
    label: "",
    commentId: null,
    labelId: null,
    root: null,
    why: null,
    ids: [],
    extras: [],
    dirty: true,
    headerDirty: false,
  };
  const index = Math.max(0, Math.min(atIndex, pou.networks.length));
  pou.networks.splice(index, 0, net);
  pou.touched = true;
  return {
    ok: true,
    model: m,
    sel: { kind: "network", pou: pouIndex, net: index },
  };
}

export function moveNetwork(
  model: DocModel,
  pouIndex: number,
  n: number,
  by: -1 | 1,
): Edited {
  const m = clone(model);
  const pou = m.pous[pouIndex];
  const to = n + by;
  if (!pou || !pou.networks[n] || to < 0 || to >= pou.networks.length)
    return { ok: false, error: "The network cannot move further." };
  const [moved] = pou.networks.splice(n, 1);
  pou.networks.splice(to, 0, moved as Network);
  pou.touched = true;
  return {
    ok: true,
    model: m,
    sel: { kind: "network", pou: pouIndex, net: to },
  };
}

export function declareVariable(
  model: DocModel,
  pouIndex: number,
  d: Declared,
  existing: readonly VarInfo[],
): Edited {
  const m = clone(model);
  const pou = m.pous[pouIndex];
  if (!pou) return { ok: false, error: "There is no such POU." };
  if (
    !/^[A-Za-z_][A-Za-z0-9_]*$/.test(d.name) ||
    /__/.test(d.name) ||
    d.name.endsWith("_")
  ) {
    return { ok: false, error: `${d.name} is not a name a variable can have.` };
  }
  const lower = d.name.toLowerCase();
  if (
    existing.some((v) => v.name.toLowerCase() === lower) ||
    pou.declared.some((v) => v.name.toLowerCase() === lower)
  ) {
    return { ok: false, error: `${d.name} is already declared.` };
  }
  if (d.type.trim() === "")
    return { ok: false, error: `Choose a type for ${d.name}.` };
  pou.declared.push({ ...d, type: d.type.trim() });
  return { ok: true, model: m, sel: null };
}

// ---------------------------------------------------------------------------------------------
// The editor's own check
// ---------------------------------------------------------------------------------------------

export interface Problems {
  /** By the localId of the box, assignment, jump or return the problem is told at. */
  elements: Map<number, string[]>;
  networks: Map<number, string[]>;
}

/** A type that says what a value is: elementary, not generic (ANY, ANY_NUM, …). */
export const concrete = (type: string) =>
  family(type) !== null && !type.toUpperCase().startsWith("ANY");

/**
 * `own`: the boxes this program may call besides the standard ones (the document's own POUs, then
 * the library's blocks). `library`: whether the library's blocks are known, and why not; left
 * out, the editor says only what it does not know.
 */
export function check(
  pou: PouModel,
  vars: readonly VarInfo[],
  own: readonly BoxDef[],
  library?: LibraryBoxes,
): Problems {
  const elements = new Map<number, string[]>();
  const networks = new Map<number, string[]>();
  const say = (map: Map<number, string[]>, key: number, text: string) =>
    map.set(key, [...(map.get(key) ?? []), text]);
  const labels = new Set(
    pou.networks.map((n) => n.label.toUpperCase()).filter((l) => l !== ""),
  );
  pou.networks.forEach((net, n) => {
    const root = net.root;
    if (net.why !== null || root === null) return;
    const instances = new Set<string>();
    const sink = (s: Sink, wanted: string | null, from: boolean) => {
      const problem = (text: string | null) =>
        text !== null && say(elements, s.id, text);
      if (s.kind === "assign")
        problem(operandProblem(s.text, wanted, true, vars, own));
      if (s.kind === "jump") {
        if (s.text === "") problem("A jump names the label of a network.");
        else if (!labels.has(s.text.toUpperCase()))
          problem(`No network has the label ${s.text}.`);
      }
      if (!from)
        problem(
          `It reads nothing: give it an operand, or put a box before it.`,
        );
    };
    if (root.t === "sink") {
      sink(
        root.sink,
        root.sink.kind === "assign" ? null : "BOOL",
        root.src !== null,
      );
      if (root.src) {
        const p = operandProblem(
          root.src.text,
          root.sink.kind === "assign" ? null : "BOOL",
          false,
          vars,
          own,
        );
        if (p) say(elements, root.sink.id, `Its input: ${p}`);
      }
      return;
    }
    for (const box of boxesOf(root)) {
      const problem = (text: string) => say(elements, box.id, text);
      const known = findFbdBox(box.typeName, own);
      for (const o of box.outputs) {
        const type = declaredType(known, "outputs", o.formal, o.type);
        for (const s of o.to)
          sink(
            s,
            s.kind === "assign"
              ? concrete(type)
                ? o.negated
                  ? "BOOL"
                  : type
                : null
              : "BOOL",
            true,
          );
      }
      if (box.typeName === "") {
        problem("The box has no type: name a function or a function block.");
        continue;
      }
      const def = known;
      if (!def) {
        for (const words of unknownBoxWords(box.typeName, library))
          problem(words);
        continue;
      }
      // A block whose declaration can change under the diagram: said per pin, never re-pinned.
      if (def.group === LIBRARY_GROUP || def.group === 'This document') {
        for (const words of pinProblems(box.inputs, box.outputs, def))
          problem(words);
      }
      if (
        def.group === "This document" &&
        library?.boxes?.some(
          (b) => b.name.toUpperCase() === def.name.toUpperCase(),
        )
      )
        problem(twinWords(def.name));
      if (def.kind === "function" && box.instance !== "")
        problem(instanceOfFunctionWords(def.name, box.instance));
      if (def.kind === "functionBlock") {
        if (box.instance === "")
          problem(
            `A ${def.name} needs an instance: name the variable that keeps its state.`,
          );
        else {
          const v = vars.find(
            (x) => x.name.toLowerCase() === box.instance.toLowerCase(),
          );
          if (!v) problem(`${box.instance} is not declared.`);
          else if (v.type.toUpperCase() !== def.name.toUpperCase())
            problem(`${box.instance} is ${v.type}, not a ${def.name}.`);
          const key = box.instance.toLowerCase();
          if (instances.has(key))
            problem(
              `${box.instance} is called by another box of this network; an instance is called once in a network.`,
            );
          instances.add(key);
        }
      }
      for (const pin of box.inputs) {
        const pinType = declaredType(def, "inputs", pin.formal, pin.type);
        if (pin.src === null) {
          if (def.kind === "function" && pin.formal.toUpperCase() !== "EN")
            problem(`${pin.formal} has no operand.`);
          continue;
        }
        if (pin.src.t === "box") {
          // A nested box's output is checked by the compiler, whose type it is (§2.16): the
          // editor says only what the declarations say for certain.
          const out = pin.src.box.outputs.find(
            (o) => o.formal === (pin.src as { output: string }).output,
          );
          const outType = out
            ? declaredType(
                findFbdBox(pin.src.box.typeName, own),
                "outputs",
                out.formal,
                out.type,
              )
            : "";
          if (
            out &&
            concrete(outType) &&
            concrete(pinType) &&
            family(out.negated ? "BOOL" : outType) !== family(pinType)
          ) {
            problem(
              `${pin.formal} takes a ${pinType}; block ${pin.src.box.id} gives ${out.formal} as ${outType}.`,
            );
          }
          continue;
        }
        const p = operandProblem(
          pin.src.text,
          pin.negated || pin.src.negated ? "BOOL" : pinType,
          false,
          vars,
          own,
        );
        if (p) problem(`${pin.formal}: ${p}`);
      }
    }
    if (sinksOf(root).length === 0) {
      const def = findFbdBox(root.box.typeName, own);
      if (def?.kind === "function")
        say(
          networks,
          n,
          "Nothing uses what the network computes: assign the output of its last box.",
        );
    }
  });
  return { elements, networks };
}

/** The type a new variable named at a place should be declared with, and where. */
export function declarationFor(where: {
  pinType?: string;
  instance?: string;
  assign?: string | null;
}): { type: string; section: Section } {
  if (where.instance !== undefined)
    return { type: where.instance, section: "localVars" };
  if (where.assign !== undefined) {
    const t = where.assign ?? "";
    return { type: concrete(t) ? t : "", section: "outputVars" };
  }
  const t = where.pinType ?? "";
  return {
    type: concrete(t) ? t : "",
    section: t.toUpperCase() === "BOOL" ? "inputVars" : "localVars",
  };
}

/** The standard boxes the palette offers. */
export const FBD_BOXES = FBD_STANDARD;
