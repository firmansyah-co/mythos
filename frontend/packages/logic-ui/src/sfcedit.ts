// The edits of the Sequential Function Chart editor, as functions from one model to the next
// (ADR-013 §2.8, §2.17, L3d-3), and its own check of what is written in it. Each edit returns a
// new model and leaves the one it was given as it was: the editor keeps those it passed through
// for undo.
//
// **The structure operations are S7-GRAPH's** (the owner's decision that the editors follow TIA
// Portal): a step and a transition put after the selection as a pair; an alternative branch (a
// selection) and a simultaneous branch opened at the selection, closed at once; a jump at the
// end of a sequence; the end of a sequence, which IEC 61131-3 draws as a step no transition
// follows (a transition must lead to a step, §6.7.5); deleting a step with its transition, a
// branch, or a divergence with all but one of its branches. Each keeps the chart well formed
// (`wellFormed`: steps and transitions alternate, every branch closed), and what would not is
// refused with why: the editor never writes a chart the compiler would refuse for its structure.
// Whether a chart is safe and every step reachable (§6.7.5, Figure 26) is the compiler's to say,
// drawn at the element it names.
//
// **localIds**: every new element (step, transition, divergence, convergence, jump, action
// block, action) takes the next id of `Ids`, counted up and never back, so no id an element has
// had is given to another (ADR-013 §2.17: an inline body's values are carried by
// `__SFC_<localId>__…`).

import { converges, itemsOf, QUALIFIERS, stepsOf, TIMED, wellFormed, type Assoc, type Chart, type DocModel, type Item, type PouModel, type Step, type Trans } from './sfcmodel';
import type { Declared, Ids, VarInfo } from './ldmodel';

/** What is selected: a chart (nothing in it), or an element by its localId (a row of a step's actions by the action's). */
export type Sel = { kind: 'chart'; pou: number; chart: number } | { kind: 'el'; pou: number; id: number; row?: number };

export type Edited = { ok: true; model: DocModel; sel: Sel | null } | { ok: false; error: string };

const clone = (model: DocModel): DocModel => ({ ...(JSON.parse(JSON.stringify(model)) as DocModel), rev: model.rev + 1 });

const alloc = (ids: Ids, pou: number) => {
  const next = ids.next;
  ids.next += 1;
  ids.used.add(pou);
  return next;
};

// ---------------------------------------------------------------------------------------------
// Finding things
// ---------------------------------------------------------------------------------------------

/** Where an item is: its sequence and index, and the branch construct holding that sequence. */
export interface Loc {
  chart: number;
  seq: Item[];
  index: number;
  item: Item;
  where: 'top' | 'sel' | 'sim';
  /** The selection or simultaneous construct whose branch `seq` is; `null` at the top. */
  owner: Extract<Item, { t: 'sel' | 'sim' }> | null;
  /** Where the owner is. */
  ownerLoc: Loc | null;
  branch: number;
}

const idsOfItem = (item: Item): number[] =>
  item.t === 'step' ? [item.step.id] : item.t === 'trans' ? [item.trans.id] : item.t === 'jump' ? [item.id] : [item.div, ...(item.conv === null ? [] : [item.conv])];

/** Where the element `id` is in a POU's charts (a divergence's convergence finds the divergence). */
export function locate(pou: PouModel, id: number): Loc | null {
  const search = (seq: Item[], chart: number, where: Loc['where'], owner: Loc['owner'], ownerLoc: Loc | null, branch: number): Loc | null => {
    for (let index = 0; index < seq.length; index += 1) {
      const item = seq[index] as Item;
      const loc: Loc = { chart, seq, index, item, where, owner, ownerLoc, branch };
      if (idsOfItem(item).includes(id)) return loc;
      if (item.t === 'sel' || item.t === 'sim') {
        for (let b = 0; b < item.branches.length; b += 1) {
          const found = search(item.branches[b] as Item[], chart, item.t, item, loc, b);
          if (found) return found;
        }
      }
    }
    return null;
  };
  for (let c = 0; c < pou.charts.length; c += 1) {
    const found = search((pou.charts[c] as Chart).items, c, 'top', null, null, 0);
    if (found) return found;
  }
  return null;
}

/** The step whose action block (or one of whose actions) has `id`. */
export function stepOfBlock(pou: PouModel, id: number): Step | null {
  for (const c of pou.charts) {
    const found = stepsOf(c.items).find((s) => s.block === id || s.actions.some((a) => a.id === id));
    if (found) return found;
  }
  return null;
}

const allNames = (pou: PouModel, vars: readonly VarInfo[]) =>
  new Set([...pou.charts.flatMap((c) => stepsOf(c.items)).map((s) => s.name.toLowerCase()), ...pou.actions, ...pou.transitions, ...vars.map((v) => v.name)].map((n) => n.toLowerCase()));

/** A step name nothing of the POU has: `Step1`, `Step2`, … as S7-GRAPH numbers them. */
function stepName(pou: PouModel, vars: readonly VarInfo[], taken: Set<string> = new Set()): string {
  const names = allNames(pou, vars);
  for (let n = 1; ; n += 1) {
    const name = `Step${n}`;
    if (!names.has(name.toLowerCase()) && !taken.has(name.toLowerCase())) {
      taken.add(name.toLowerCase());
      return name;
    }
  }
}

/**
 * A new transition's condition: FALSE, so a sequence never moves on through a transition the
 * engineer has not written yet (a safety HMI: nothing clears that nobody wrote).
 */
const NEW_CONDITION = 'FALSE';

const newStep = (ids: Ids, pi: number, name: string): Item => ({ t: 'step', step: { id: alloc(ids, pi), name, initial: false, block: null, actions: [] } });
const newTrans = (ids: Ids, pi: number): Item => ({
  t: 'trans',
  trans: { id: alloc(ids, pi), kind: 'st', text: NEW_CONDITION, negated: false, condDirty: true },
});

/** The chart's initial step's name, the target a new jump starts with. */
function initialOf(chart: Chart): string {
  const steps = stepsOf(chart.items);
  return (steps.find((s) => s.initial) ?? steps[0])?.name ?? '';
}

/** Close an edit: refused unless every chart of the POU is still well formed. */
function done(m: DocModel, pi: number, sel: Sel | null): Edited {
  const pou = m.pous[pi] as PouModel;
  for (const c of pou.charts) {
    const why = wellFormed(c.items, 'top');
    if (why) return { ok: false, error: `That would leave the chart not well formed: ${why}.` };
  }
  pou.touched = true;
  return { ok: true, model: m, sel };
}

interface Ctx {
  m: DocModel;
  pi: number;
  pou: PouModel;
}

function begin(model: DocModel, pi: number): Ctx | string {
  const m = clone(model);
  const pou = m.pous[pi];
  if (!pou || !pou.sfc) return 'There is no chart to edit here.';
  if (pou.why !== null) return `This chart is kept as the document has it (${pou.why}): nothing is put into it.`;
  return { m, pi, pou };
}

/** The item a selection names, or the last of the chart's sequence when only the chart is selected. */
function target(ctx: Ctx, sel: Sel | null): Loc | string {
  if (!sel) return 'Select a step or a transition first.';
  if (sel.kind === 'chart') {
    const chart = ctx.pou.charts[sel.chart];
    if (!chart) return 'There is no such chart.';
    const index = chart.items.length - 1;
    return { chart: sel.chart, seq: chart.items, index, item: chart.items[index] as Item, where: 'top', owner: null, ownerLoc: null, branch: 0 };
  }
  const loc = locate(ctx.pou, sel.id);
  return loc ?? 'The selection is not in the chart any more.';
}

// ---------------------------------------------------------------------------------------------
// Structure
// ---------------------------------------------------------------------------------------------

/**
 * A step and a transition, as S7-GRAPH inserts them: after a step (or a simultaneous construct)
 * a transition and then a step; after a transition (or a selection) a step and then a transition.
 * The new step is selected.
 */
export function insertPair(model: DocModel, ids: Ids, pi: number, sel: Sel | null, vars: readonly VarInfo[]): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const loc = target(ctx, sel);
  if (typeof loc === 'string') return { ok: false, error: loc };
  const { item, seq, index } = loc;
  if (item.t === 'jump') return { ok: false, error: 'A jump ends its sequence: select the step or transition before it.' };
  if (item.t === 'sel' && item.conv === null) return { ok: false, error: 'Every branch of this selection ends: nothing follows it.' };
  const name = stepName(ctx.pou, vars);
  const step = newStep(ids, pi, name);
  const trans = newTrans(ids, pi);
  const pair = item.t === 'step' || item.t === 'sim' ? [trans, step] : [step, trans];
  seq.splice(index + 1, 0, ...pair);
  return done(ctx.m, pi, { kind: 'el', pou: pi, id: (step as Extract<Item, { t: 'step' }>).step.id });
}

/** One more alternative branch of selection `sel`, put at index `at`: a transition (and a jump when the selection's branches all end). */
function addSelBranch(ctx: Ctx, ids: Ids, owner: Extract<Item, { t: 'sel' }>, chart: number, at: number): Item {
  const trans = newTrans(ids, ctx.pi);
  const branch: Item[] = [trans];
  if (owner.conv === null) branch.push({ t: 'jump', id: alloc(ids, ctx.pi), target: initialOf(ctx.pou.charts[chart] as Chart) });
  owner.branches.splice(at, 0, branch);
  return trans;
}

/**
 * An alternative branch (a selection divergence, Table 61 feature 2), opened at the selection
 * and closed at once: at a transition, a branch beside it from the step before it to the next
 * step after it (beside the branch it starts, when it starts one); at a step, at the transition after
 * it; at a selection, one more branch on its right. The new transition is selected.
 */
export function altBranch(model: DocModel, ids: Ids, pi: number, sel: Sel | null, vars: readonly VarInfo[]): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const loc = target(ctx, sel);
  if (typeof loc === 'string') return { ok: false, error: loc };
  let { item, index } = loc;
  const { seq } = loc;
  if (item.t === 'step') {
    const after = seq[index + 1];
    if (!after) {
      // The last step of a sequence: a selection of two transitions after it, meeting at a new step.
      const sel2: Item = { t: 'sel', div: alloc(ids, pi), conv: alloc(ids, pi), branches: [[newTrans(ids, pi)], [newTrans(ids, pi)]] };
      seq.push(sel2, newStep(ids, pi, stepName(ctx.pou, vars)));
      const second = (sel2 as Extract<Item, { t: 'sel' }>).branches[1]?.[0] as Extract<Item, { t: 'trans' }>;
      return done(ctx.m, pi, { kind: 'el', pou: pi, id: second.trans.id });
    }
    item = after;
    index += 1;
  }
  if (item.t === 'sel') {
    const t = addSelBranch(ctx, ids, item, loc.chart, item.branches.length) as Extract<Item, { t: 'trans' }>;
    return done(ctx.m, pi, { kind: 'el', pou: pi, id: t.trans.id });
  }
  if (item.t !== 'trans') return { ok: false, error: 'An alternative branch opens at a step or a transition.' };
  // The first transition of a branch of a selection: one more branch of it, beside this one.
  const inBranch = seq === loc.seq && loc.where === 'sel' && index === 0 && loc.owner?.t === 'sel';
  if (inBranch && loc.owner?.t === 'sel') {
    const t = addSelBranch(ctx, ids, loc.owner, loc.chart, loc.branch + 1) as Extract<Item, { t: 'trans' }>;
    return done(ctx.m, pi, { kind: 'el', pou: pi, id: t.trans.id });
  }
  // The branch beside runs from the step before to the next step (or jump): what stands in
  // between (a transition, or a transition, a simultaneous construct and a transition) is the
  // first branch, so the convergence is followed by a step, as Table 61 draws it.
  let end = index;
  while (end < seq.length && seq[end]?.t !== 'step' && seq[end]?.t !== 'jump') end += 1;
  if (end === seq.length) {
    return { ok: false, error: 'This transition ends its branch: an alternative to it would meet nowhere. Put one at the branch’s first transition.' };
  }
  const t = newTrans(ids, pi) as Extract<Item, { t: 'trans' }>;
  const run = seq.slice(index, end);
  seq.splice(index, end - index, { t: 'sel', div: alloc(ids, pi), conv: alloc(ids, pi), branches: [run, [t]] });
  return done(ctx.m, pi, { kind: 'el', pou: pi, id: t.trans.id });
}

/**
 * A simultaneous branch (Table 61 feature 3), opened at the selection and closed at once: at a
 * step after a transition, a step beside it up to the next transition (beside the branch it
 * starts, when it starts one); at a transition, at the step after it; at a simultaneous construct, one more branch on
 * its right. The new step is selected.
 */
export function simBranch(model: DocModel, ids: Ids, pi: number, sel: Sel | null, vars: readonly VarInfo[]): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const loc = target(ctx, sel);
  if (typeof loc === 'string') return { ok: false, error: loc };
  let { item, index } = loc;
  const { seq } = loc;
  if (item.t === 'trans') {
    const after = seq[index + 1];
    if (!after) return { ok: false, error: 'A simultaneous branch opens at a step: this transition ends its branch.' };
    item = after;
    index += 1;
  }
  const name = stepName(ctx.pou, vars);
  if (item.t === 'sim') {
    const s = newStep(ids, pi, name) as Extract<Item, { t: 'step' }>;
    item.branches.push([s]);
    return done(ctx.m, pi, { kind: 'el', pou: pi, id: s.step.id });
  }
  if (item.t !== 'step') return { ok: false, error: 'A simultaneous branch opens at a step or a transition.' };
  if (loc.where === 'sim' && index === 0 && seq === loc.seq && loc.owner?.t === 'sim') {
    const s = newStep(ids, pi, name) as Extract<Item, { t: 'step' }>;
    loc.owner.branches.splice(loc.branch + 1, 0, [s]);
    return done(ctx.m, pi, { kind: 'el', pou: pi, id: s.step.id });
  }
  if (seq[index - 1]?.t !== 'trans') {
    return { ok: false, error: 'A simultaneous branch opens right after a transition: no transition is right before this step.' };
  }
  // The branch beside runs from the transition before to the next transition: what stands in
  // between (a step, or steps with alternative branches between them) is the first branch.
  let end = index;
  while (end < seq.length && seq[end]?.t !== 'trans') end += 1;
  if (end === seq.length) {
    return { ok: false, error: 'A simultaneous branch closes before a transition: put a transition after this step first.' };
  }
  const s = newStep(ids, pi, name) as Extract<Item, { t: 'step' }>;
  const run = seq.slice(index, end);
  seq.splice(index, end - index, { t: 'sim', div: alloc(ids, pi), conv: alloc(ids, pi), branches: [run, [s]] });
  return done(ctx.m, pi, { kind: 'el', pou: pi, id: s.step.id });
}

/**
 * A jump (Table 61 feature 4c's arrow, S7-GRAPH's jump): after the last step of a sequence, a
 * transition and a jump to the chart's initial step; at the last transition of an alternative
 * branch, the branch ends in a jump instead of meeting the convergence. Never out of a
 * simultaneous branch (its other branches would stay active: unsafe, §6.7.5). The jump is selected.
 */
export function addJump(model: DocModel, ids: Ids, pi: number, sel: Sel | null): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const loc = target(ctx, sel);
  if (typeof loc === 'string') return { ok: false, error: loc };
  const { item, seq, index } = loc;
  const last = index === seq.length - 1;
  const to = initialOf(ctx.pou.charts[loc.chart] as Chart);
  if (loc.where === 'sim') return { ok: false, error: 'A jump out of a simultaneous branch would leave its other branches active: a jump goes after the convergence.' };
  if (item.t === 'step' && last) {
    const jump: Item = { t: 'jump', id: alloc(ids, pi), target: to };
    seq.push(newTrans(ids, pi), jump);
    return done(ctx.m, pi, { kind: 'el', pou: pi, id: (jump as Extract<Item, { t: 'jump' }>).id });
  }
  if (item.t === 'trans' && last && loc.owner?.t === 'sel' && loc.ownerLoc) {
    const owner = loc.owner;
    const jump: Item = { t: 'jump', id: alloc(ids, pi), target: to };
    seq.push(jump);
    if (!owner.branches.some(converges)) {
      if (loc.ownerLoc.index !== loc.ownerLoc.seq.length - 1) {
        return { ok: false, error: 'This branch is the last that meets the convergence, and steps follow it: it cannot end in a jump.' };
      }
      owner.conv = null;
    }
    return done(ctx.m, pi, { kind: 'el', pou: pi, id: (jump as Extract<Item, { t: 'jump' }>).id });
  }
  return { ok: false, error: 'A jump goes at the end of a sequence: select its last step, or the last transition of an alternative branch.' };
}

/**
 * The end of a sequence in place of a jump: IEC 61131-3 has no element for it, so the sequence
 * ends at a step no transition follows (a transition must lead to a step, §6.7.5), which stays
 * active once reached.
 */
export function endSequence(model: DocModel, ids: Ids, pi: number, sel: Sel | null, vars: readonly VarInfo[]): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const loc = target(ctx, sel);
  if (typeof loc === 'string') return { ok: false, error: loc };
  if (loc.item.t !== 'jump') {
    return { ok: false, error: 'Select a jump: the sequence then ends at a step that no transition follows. A sequence that ends at a step ends there already.' };
  }
  const s = newStep(ids, pi, stepName(ctx.pou, vars)) as Extract<Item, { t: 'step' }>;
  loc.seq.splice(loc.index, 1, s);
  return done(ctx.m, pi, { kind: 'el', pou: pi, id: s.step.id });
}

/** A new chart (a sequence of its own, with its initial step), right of the others. */
export function addChart(model: DocModel, ids: Ids, pi: number, vars: readonly VarInfo[]): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const s = newStep(ids, pi, stepName(ctx.pou, vars)) as Extract<Item, { t: 'step' }>;
  s.step.initial = true;
  ctx.pou.charts.push({ key: Date.now() + ids.next, items: [s] });
  return done(ctx.m, pi, { kind: 'el', pou: pi, id: s.step.id });
}

/** Replace a branch construct by its branch `keep`, inlined where it stood. */
function dissolve(loc: Loc, keep: Item[]): void {
  loc.seq.splice(loc.index, 1, ...keep);
}

/** Remove branch `b` of the construct at `ownerLoc`; one branch left is inlined. */
function dropBranch(ownerLoc: Loc, b: number): string | null {
  const owner = ownerLoc.item as Extract<Item, { t: 'sel' | 'sim' }>;
  owner.branches.splice(b, 1);
  if (owner.branches.length === 1) {
    dissolve(ownerLoc, owner.branches[0] as Item[]);
    return null;
  }
  if (owner.t === 'sel' && !owner.branches.some(converges)) {
    if (ownerLoc.index !== ownerLoc.seq.length - 1) return 'Every branch left would end, yet steps follow the selection: delete those first.';
    owner.conv = null;
  }
  return null;
}

/**
 * Delete what is selected, keeping the chart well formed: a step or a transition with the
 * element before it (the inverse of putting a pair after it), or with the one after it at the head
 * of a sequence or where the one before is the initial step; a jump with the transition before it; a branch whose only element it was goes with it, and a
 * construct left with one branch becomes that branch. A divergence keeps its first branch that
 * meets its convergence, the others deleted. A row of a step's actions, that action. An initial
 * step deleted at the head of its chart makes the step after it initial; a chart's last step is
 * not deleted.
 */
export function remove(model: DocModel, pi: number, sel: Sel | null): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  if (!sel || sel.kind === 'chart') {
    if (sel?.kind === 'chart' && ctx.pou.charts.length > 1) {
      ctx.pou.charts.splice(sel.chart, 1);
      return done(ctx.m, pi, null);
    }
    return { ok: false, error: sel ? 'A POU’s only chart is not deleted: delete its elements.' : 'Select what to delete.' };
  }
  if (sel.row !== undefined) {
    const step = stepOfBlock(ctx.pou, sel.row);
    if (!step) return { ok: false, error: 'The action is not in the chart any more.' };
    step.actions = step.actions.filter((a) => a.id !== sel.row);
    if (step.actions.length === 0) step.block = null;
    return done(ctx.m, pi, { kind: 'el', pou: pi, id: step.id });
  }
  const loc = locate(ctx.pou, sel.id);
  if (!loc) return { ok: false, error: 'The selection is not in the chart any more.' };
  const { seq, index, item } = loc;
  const prev = seq[index - 1];
  const next = seq[index + 1];
  const goneSel = (id: number | undefined): Sel | null => (id === undefined ? null : { kind: 'el', pou: pi, id });
  if (item.t === 'sel' || item.t === 'sim') {
    const keep = item.t === 'sel' ? (item.branches.find(converges) ?? item.branches[0]) : item.branches[0];
    dissolve(loc, keep as Item[]);
    return done(ctx.m, pi, goneSel(idsOfItem((keep as Item[])[0] as Item)[0]));
  }
  // The only element of a branch: the branch goes.
  if (seq.length === 1 && loc.ownerLoc) {
    const why = dropBranch(loc.ownerLoc, loc.branch);
    if (why) return { ok: false, error: why };
    return done(ctx.m, pi, goneSel(idsOfItem(loc.ownerLoc.item)[0]));
  }
  if (item.t === 'jump') {
    if (prev?.t !== 'trans') return { ok: false, error: 'This jump cannot be deleted alone.' };
    if (seq.length === 2 && loc.ownerLoc) {
      const why = dropBranch(loc.ownerLoc, loc.branch);
      if (why) return { ok: false, error: why };
      return done(ctx.m, pi, null);
    }
    seq.splice(index - 1, 2);
    return done(ctx.m, pi, goneSel(seq[index - 2] ? idsOfItem(seq[index - 2] as Item)[0] : undefined));
  }
  const plain = item.t === 'step' ? 'trans' : 'step';
  if (item.t === 'step' && item.step.initial && loc.where === 'top' && index === 0) {
    if (seq.length < 3) return { ok: false, error: 'A chart keeps its initial step: make another step initial first, or add one.' };
  }
  // With the element before it, as a pair is put after it (so deleting what was just put takes
  // out what was put); at the head of a sequence, or where the one before is the initial step,
  // with the element after it.
  const initialBefore = prev?.t === 'step' && prev.step.initial;
  let from: number;
  if (prev?.t === plain && !initialBefore) from = index - 1;
  else if (next?.t === plain) from = index;
  else return { ok: false, error: `Delete the branches next to this ${item.t === 'step' ? 'step' : 'transition'} first.` };
  // A pair at the head of a chart takes its initial step with it: the step after becomes initial.
  const removed = seq.splice(from, 2);
  const wasInitial = removed.some((r) => r.t === 'step' && r.step.initial);
  if (wasInitial) {
    const head = seq[0];
    if (head?.t === 'step') head.step.initial = true;
  }
  if (seq.length === 0 && loc.ownerLoc) {
    const why = dropBranch(loc.ownerLoc, loc.branch);
    if (why) return { ok: false, error: why };
  }
  const near = seq[Math.min(from, seq.length - 1)];
  return done(ctx.m, pi, goneSel(near ? idsOfItem(near)[0] : undefined));
}

/** Delete the branch the selected element is in. */
export function removeBranch(model: DocModel, pi: number, sel: Sel | null): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  if (sel?.kind !== 'el') return { ok: false, error: 'Select an element of the branch.' };
  const loc = locate(ctx.pou, sel.id);
  if (!loc?.ownerLoc) return { ok: false, error: 'The selection is not in a branch.' };
  const why = dropBranch(loc.ownerLoc, loc.branch);
  if (why) return { ok: false, error: why };
  return done(ctx.m, pi, { kind: 'el', pou: pi, id: idsOfItem(loc.ownerLoc.item)[0] as number });
}

/** Move the branch the selected element is in one place left or right: in a selection, the order its transitions are tested (Table 61 feature 2a). */
export function moveBranch(model: DocModel, pi: number, sel: Sel | null, by: -1 | 1): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  if (sel?.kind !== 'el') return { ok: false, error: 'Select an element of the branch.' };
  const loc = locate(ctx.pou, sel.id);
  if (!loc?.owner) return { ok: false, error: 'The selection is not in a branch.' };
  const to = loc.branch + by;
  if (to < 0 || to >= loc.owner.branches.length) return { ok: false, error: 'The branch cannot move further.' };
  const [b] = loc.owner.branches.splice(loc.branch, 1);
  loc.owner.branches.splice(to, 0, b as Item[]);
  return done(ctx.m, pi, sel);
}

// ---------------------------------------------------------------------------------------------
// Fields
// ---------------------------------------------------------------------------------------------

/** A name a step, an action or a variable may have (§6.1.2: no two underscores together, none at the end). */
export const isName = (name: string) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(name) && !/__/.test(name) && !name.endsWith('_');

function stepAt(ctx: Ctx, id: number): Step | string {
  const loc = locate(ctx.pou, id);
  return loc?.item.t === 'step' ? loc.item.step : 'The step is not in the chart any more.';
}

function transAt(ctx: Ctx, id: number): Trans | string {
  const loc = locate(ctx.pou, id);
  return loc?.item.t === 'trans' ? loc.item.trans : 'The transition is not in the chart any more.';
}

/**
 * A step's name. A jump to the step follows it. Its state is carried across an online change by
 * its name (ADR-013 §2.17), so a renamed step starts from its initial value.
 */
export function renameStep(model: DocModel, pi: number, id: number, name: string, vars: readonly VarInfo[]): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const step = stepAt(ctx, id);
  if (typeof step === 'string') return { ok: false, error: step };
  const text = name.trim();
  if (text === step.name) return { ok: true, model, sel: { kind: 'el', pou: pi, id } };
  if (!isName(text)) return { ok: false, error: `${text || 'An empty name'} is not a name a step can have.` };
  const others = allNames(ctx.pou, vars);
  others.delete(step.name.toLowerCase());
  if (others.has(text.toLowerCase())) return { ok: false, error: `${text} is already the name of a step, an action, a transition or a variable of ${ctx.pou.name}.` };
  const old = step.name.toLowerCase();
  step.name = text;
  for (const c of ctx.pou.charts) {
    const walk = (items: Item[]) => {
      for (const item of items) {
        if (item.t === 'jump' && item.target.toLowerCase() === old) item.target = text;
        if (item.t === 'sel' || item.t === 'sim') item.branches.forEach(walk);
      }
    };
    walk(c.items);
  }
  return done(ctx.m, pi, { kind: 'el', pou: pi, id });
}

/** Make a step its chart's initial step (§6.7.2: one per chart): the one that was is not any more. */
export function setInitial(model: DocModel, pi: number, id: number): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const loc = locate(ctx.pou, id);
  if (loc?.item.t !== 'step') return { ok: false, error: 'Select a step.' };
  for (const s of stepsOf((ctx.pou.charts[loc.chart] as Chart).items)) s.initial = s.id === id;
  return done(ctx.m, pi, { kind: 'el', pou: pi, id });
}

/** A transition's condition: an ST expression written in it, or a transition of the POU by its name. */
export function setCondition(model: DocModel, pi: number, id: number, kind: 'st' | 'reference', text: string): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const t = transAt(ctx, id);
  if (typeof t === 'string') return { ok: false, error: t };
  if (t.kind === 'wired') return { ok: false, error: 'This condition is a network drawn in the chart, kept as found: the editor does not change it.' };
  t.kind = kind;
  t.text = kind === 'reference' ? text.trim() : text;
  t.condDirty = true;
  return done(ctx.m, pi, { kind: 'el', pou: pi, id });
}

export function setConditionNegated(model: DocModel, pi: number, id: number, negated: boolean): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const t = transAt(ctx, id);
  if (typeof t === 'string') return { ok: false, error: t };
  t.negated = negated;
  t.condDirty = true;
  return done(ctx.m, pi, { kind: 'el', pou: pi, id });
}

/** A jump's target step. */
export function setJumpTarget(model: DocModel, pi: number, id: number, to: string): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const loc = locate(ctx.pou, id);
  if (loc?.item.t !== 'jump') return { ok: false, error: 'The jump is not in the chart any more.' };
  loc.item.target = to.trim();
  return done(ctx.m, pi, { kind: 'el', pou: pi, id });
}

/** One more row of a step's actions (the step's action block is made when it has none), with `qualifier`. The row is selected. */
export function addAction(model: DocModel, ids: Ids, pi: number, sel: Sel | null, qualifier = 'N'): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  if (sel?.kind !== 'el') return { ok: false, error: 'Select the step the action is for.' };
  const step = sel.row !== undefined ? stepOfBlock(ctx.pou, sel.row) : stepAt(ctx, sel.id);
  if (step === null || typeof step === 'string') return { ok: false, error: 'An action is a step’s: select a step.' };
  if (step.block === null) step.block = alloc(ids, pi);
  const a: Assoc = { id: alloc(ids, pi), qualifier, duration: TIMED.has(qualifier) ? 'T#1s' : '', kind: 'reference', text: '', bodyDirty: true };
  step.actions.push(a);
  return done(ctx.m, pi, { kind: 'el', pou: pi, id: step.id, row: a.id });
}

/** A row of a step's actions changed: its qualifier, duration, or what it runs (a name, or ST statements). */
export function setAction(
  model: DocModel,
  pi: number,
  actionId: number,
  patch: Partial<Pick<Assoc, 'qualifier' | 'duration' | 'kind' | 'text'>>
): Edited {
  const ctx = begin(model, pi);
  if (typeof ctx === 'string') return { ok: false, error: ctx };
  const step = stepOfBlock(ctx.pou, actionId);
  const a = step?.actions.find((x) => x.id === actionId);
  if (!step || !a) return { ok: false, error: 'The action is not in the chart any more.' };
  if (patch.qualifier !== undefined) {
    if (!(QUALIFIERS as readonly string[]).includes(patch.qualifier)) {
      return { ok: false, error: `${patch.qualifier} is not offered: SD, SL and P0 are refused until the owner decides (ADR-013 §2.17).` };
    }
    a.qualifier = patch.qualifier;
    // A qualifier that takes no duration drops it; one that takes one keeps it, or starts at 1 s.
    if (!TIMED.has(a.qualifier)) a.duration = '';
    else if (a.duration === '') a.duration = 'T#1s';
  }
  if (patch.duration !== undefined) a.duration = patch.duration.trim();
  if (patch.kind !== undefined && patch.kind !== a.kind) {
    if (patch.kind === 'kept') return { ok: false, error: 'A body in LD or FBD is not written by this editor.' };
    a.kind = patch.kind;
    a.text = '';
    a.bodyDirty = true;
  }
  if (patch.text !== undefined) {
    if (a.kind === 'kept') return { ok: false, error: 'This action’s body is in another language and kept as found.' };
    a.text = a.kind === 'reference' ? patch.text.trim() : patch.text;
    a.bodyDirty = true;
  }
  return done(ctx.m, pi, { kind: 'el', pou: pi, id: step.id, row: a.id });
}

/** A variable declared in the editor (offered where an action names one nothing declares). */
export function declareVariable(model: DocModel, pi: number, d: Declared, existing: readonly VarInfo[]): Edited {
  const m = clone(model);
  const pou = m.pous[pi];
  if (!pou) return { ok: false, error: 'There is no such POU.' };
  if (!isName(d.name)) return { ok: false, error: `${d.name} is not a name a variable can have.` };
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

export interface Problems {
  /** By element (a step, a transition, a jump) or by action row, the localId. */
  elements: Map<number, string[]>;
}

const TIME_LITERAL = /^(T|TIME|LT|LTIME)#[-+]?[0-9_.a-zA-Z]+$/i;

/**
 * What is wrong in a POU's chart that the editor sees at once, before the compiler's check: a
 * step's name (not a name, two steps of one name, the name of a variable, an action or a
 * transition of the POU); a condition with nothing written, or naming a transition the POU does
 * not have; a jump to a step the POU does not have; an action that names nothing, names neither
 * an action of the POU nor a variable, or names a variable that is not a BOOL; a qualifier that
 * takes a duration without one, or one that takes none with one; SD, SL and P0, refused until
 * the owner decides.
 */
export function check(pou: PouModel, vars: readonly VarInfo[]): Problems {
  const elements = new Map<number, string[]>();
  const say = (id: number, text: string) => elements.set(id, [...(elements.get(id) ?? []), text]);
  const steps = pou.charts.flatMap((c) => stepsOf(c.items));
  const count = new Map<string, number>();
  for (const s of steps) count.set(s.name.toLowerCase(), (count.get(s.name.toLowerCase()) ?? 0) + 1);
  const lower = (list: readonly string[]) => new Set(list.map((n) => n.toLowerCase()));
  const actions = lower(pou.actions);
  const transitions = lower(pou.transitions);
  const varOf = (name: string) => vars.find((v) => v.name.toLowerCase() === name.toLowerCase());
  for (const s of steps) {
    const n = s.name.toLowerCase();
    if (!isName(s.name)) say(s.id, `${s.name || 'An empty name'} is not a name a step can have.`);
    if ((count.get(n) ?? 0) > 1) say(s.id, `Two steps are named ${s.name}: a step’s name is its own (§6.7.2).`);
    if (varOf(s.name)) say(s.id, `${s.name} is a variable of ${pou.name}: a step’s name is apart from the variables.`);
    if (actions.has(n) || transitions.has(n)) say(s.id, `${s.name} is the name of an action or a transition of ${pou.name}.`);
    for (const a of s.actions) {
      if (!TIMED.has(a.qualifier) && a.duration !== '') say(a.id, `${a.qualifier} takes no duration.`);
      if (TIMED.has(a.qualifier) && a.duration === '') say(a.id, `${a.qualifier} takes a duration (as T#5s).`);
      if (TIMED.has(a.qualifier) && a.duration !== '' && !TIME_LITERAL.test(a.duration) && !isName(a.duration)) {
        say(a.id, `${a.duration} is not a duration: a TIME literal (T#5s) or a variable.`);
      }
      if (['SD', 'SL', 'P0'].includes(a.qualifier.toUpperCase())) {
        say(a.id, `${a.qualifier} is refused by the compiler until the owner decides (ADR-013 §2.17): choose another qualifier.`);
      } else if (!(QUALIFIERS as readonly string[]).includes(a.qualifier)) say(a.id, `${a.qualifier} is not a qualifier of IEC 61131-3 this compiler takes.`);
      if (a.kind === 'reference') {
        if (a.text === '') say(a.id, 'The action names nothing: an action of the POU or a BOOL variable.');
        else if (!actions.has(a.text.toLowerCase())) {
          const v = varOf(a.text);
          if (!v) say(a.id, `${a.text} is neither an action of ${pou.name} nor a variable.`);
          else if (v.type.toUpperCase() !== 'BOOL') say(a.id, `${a.text} is a ${v.type}: an action that is a variable is a BOOL (Table 56 feature 1).`);
        }
      } else if (a.kind === 'st' && a.text.trim() === '') say(a.id, 'The action has no statements.');
    }
  }
  const names = lower(steps.map((s) => s.name));
  for (const c of pou.charts) {
    for (const item of itemsOf(c.items)) {
      if (item.t === 'trans') {
        const t = item.trans;
        if (t.kind === 'none' || (t.kind === 'st' && t.text.trim() === '')) say(t.id, 'The transition has no condition.');
        if (t.kind === 'reference' && !transitions.has(t.text.toLowerCase())) say(t.id, `${pou.name} has no transition ${t.text}.`);
      }
      if (item.t === 'jump' && !names.has(item.target.toLowerCase())) say(item.id, `${pou.name} has no step ${item.target || '(none named)'} to jump to.`);
    }
  }
  return { elements };
}
