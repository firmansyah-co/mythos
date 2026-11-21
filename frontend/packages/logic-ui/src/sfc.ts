// A Sequential Function Chart as its PLCopen TC6 XML v2.01 document holds it (ADR-013 §2.17),
// read for drawing: each POU's SFC elements with their `localId`, their position (and size where
// the document gives one) and the elements each is linked from. Nothing here compiles or checks
// the chart: the logic module does, and what it says is drawn at the element it names.
//
// The reader takes what the front-end takes and draws it; an element it does not know (a macro
// step, which is not in IEC 61131-3 ed. 3, or an element of LD or FBD) is listed as not drawn,
// never left out silently. A POU's own actions and transitions are read with their text, so the
// view can show what an action block or a transition names.

import { child, children, flag, number, tc6Pous } from './ladder';

export type SfcKind =
  | 'step'
  | 'transition'
  | 'selectionDivergence'
  | 'selectionConvergence'
  | 'simultaneousDivergence'
  | 'simultaneousConvergence'
  | 'jumpStep'
  | 'actionBlock'
  | 'comment';

const KINDS: readonly SfcKind[] = [
  'step',
  'transition',
  'selectionDivergence',
  'selectionConvergence',
  'simultaneousDivergence',
  'simultaneousConvergence',
  'jumpStep',
  'actionBlock',
  'comment',
];

/** A text of the chart: ST as written, or the name of the language it is in when it is not ST. */
export interface SfcText {
  /** The ST text, or `null` when the body is in another language. */
  st: string | null;
  /** The body's language as the document names it (`ST`, `LD`, `FBD`, …), `null` when there is none. */
  language: string | null;
}

/** One association of an action block (§6.7.4.5): a qualifier, a duration, and what it runs. */
export interface SfcAction {
  /** The action's own `localId`, by which the check says where its state is kept. */
  localId: number | null;
  qualifier: string;
  duration: string | null;
  /** The name of an action of the POU, or of a BOOL variable. */
  reference: string | null;
  /** A body written in the block. */
  inline: SfcText | null;
}

/** A transition's condition: inline, or a transition of the POU by its name. */
export type SfcCondition =
  | { kind: 'inline'; text: SfcText; negated: boolean }
  | { kind: 'reference'; name: string; negated: boolean };

export interface SfcElement {
  kind: SfcKind;
  localId: number;
  x: number;
  y: number;
  /** As the document gives them; `null` when it does not. */
  width: number | null;
  height: number | null;
  /** A step's name. */
  name: string | null;
  initial: boolean;
  /** A jump's target step. */
  target: string | null;
  condition: SfcCondition | null;
  priority: number | null;
  actions: SfcAction[];
  /** A comment's text. */
  text: string | null;
  /** The elements it is linked from, by `localId`, in the document's order. */
  from: number[];
}

/** An action or a transition of the POU (its `actions` and `transitions`), with its text. */
export interface SfcNamed {
  name: string;
  body: SfcText;
}

export interface SfcPou {
  name: string;
  pouType: string;
  elements: SfcElement[];
  actions: SfcNamed[];
  transitions: SfcNamed[];
  /** Elements that are not drawn, each with why. */
  notDrawn: string[];
  /** Whether the body is SFC; one in another language is not drawn. */
  sfc: boolean;
  /** The body's language when it is not SFC (`LD`, `FBD`, `ST`, …). */
  language: string | null;
}

export type Sfc = { ok: true; pous: SfcPou[] } | { ok: false; error: string };

/** The language of a body (`<body>` or `<inline>`): its one element's name, ST's text kept. */
function textOf(holder: Element | null): SfcText {
  const body = holder ? children(holder)[0] : undefined;
  if (!body) return { st: null, language: null };
  if (body.localName !== 'ST') return { st: null, language: body.localName };
  return { st: (body.textContent ?? '').trim(), language: 'ST' };
}

function element(node: Element, kind: SfcKind): SfcElement | string {
  const localId = number(node.getAttribute('localId'));
  if (localId === null) return `a ${kind} without a localId`;
  const position = child(node, 'position');
  const x = number(position?.getAttribute('x') ?? null);
  const y = number(position?.getAttribute('y') ?? null);
  if (x === null || y === null) return `${kind} ${localId} has no position`;
  const from: number[] = [];
  for (const point of children(node, 'connectionPointIn')) {
    for (const c of children(point, 'connection')) {
      const id = number(c.getAttribute('refLocalId'));
      if (id !== null) from.push(id);
    }
  }
  const e: SfcElement = {
    kind,
    localId,
    x,
    y,
    width: number(node.getAttribute('width')),
    height: number(node.getAttribute('height')),
    name: null,
    initial: false,
    target: null,
    condition: null,
    priority: null,
    actions: [],
    text: null,
    from,
  };
  switch (kind) {
    case 'step':
      e.name = node.getAttribute('name');
      e.initial = flag(node, 'initialStep');
      break;
    case 'jumpStep':
      e.target = node.getAttribute('targetName');
      break;
    case 'transition': {
      e.priority = number(node.getAttribute('priority'));
      const condition = child(node, 'condition');
      if (condition) {
        const negated = flag(condition, 'negated');
        const reference = child(condition, 'reference');
        const inline = child(condition, 'inline');
        if (reference) e.condition = { kind: 'reference', name: reference.getAttribute('name') ?? '', negated };
        else if (inline) e.condition = { kind: 'inline', text: textOf(inline), negated };
      }
      break;
    }
    case 'actionBlock':
      e.actions = children(node, 'action').map((a) => {
        const reference = child(a, 'reference');
        const inline = child(a, 'inline');
        return {
          localId: number(a.getAttribute('localId')),
          qualifier: a.getAttribute('qualifier') ?? 'N',
          duration: a.getAttribute('duration')?.trim() || null,
          reference: reference ? (reference.getAttribute('name') ?? '') : null,
          inline: inline ? textOf(inline) : null,
        };
      });
      break;
    case 'comment':
      e.text = (child(node, 'content')?.textContent ?? '').trim();
      break;
    default:
      break;
  }
  return e;
}

/** The POU's `actions` or `transitions`, each by its name with its body. */
function named(pou: Element, list: 'actions' | 'transitions'): SfcNamed[] {
  const holder = child(pou, list);
  if (!holder) return [];
  return children(holder, list === 'actions' ? 'action' : 'transition').map((n) => ({
    name: n.getAttribute('name') ?? '',
    body: textOf(child(n, 'body')),
  }));
}

/** Read a TC6 document's Sequential Function Charts for drawing. What is not a TC6 project says why. */
export function readSfc(text: string): Sfc {
  const read = tc6Pous(text);
  if (!read.ok) return read;
  const out: SfcPou[] = [];
  for (const pou of read.pous) {
    const body = child(pou, 'body');
    const sfc = body ? child(body, 'SFC') : null;
    const found: SfcPou = {
      name: pou.getAttribute('name') ?? '',
      pouType: pou.getAttribute('pouType') ?? '',
      elements: [],
      actions: named(pou, 'actions'),
      transitions: named(pou, 'transitions'),
      notDrawn: [],
      sfc: sfc !== null,
      language: sfc === null && body ? (children(body)[0]?.localName ?? null) : null,
    };
    for (const node of sfc ? children(sfc) : []) {
      if (node.localName === 'addData' || node.localName === 'documentation') continue;
      const kind = KINDS.find((k) => k === node.localName);
      if (!kind) {
        found.notDrawn.push(`${node.localName} ${node.getAttribute('localId') ?? ''}`.trim());
        continue;
      }
      const e = element(node, kind);
      if (typeof e === 'string') found.notDrawn.push(e);
      // Two elements with one localId: the first is drawn, the second listed, never dropped
      // silently (the compiler refuses the document; the drawing would not know which is linked).
      else if (found.elements.some((other) => other.localId === e.localId)) {
        found.notDrawn.push(`${kind} ${e.localId}: another element has its localId`);
      } else found.elements.push(e);
    }
    out.push(found);
  }
  return { ok: true, pous: out };
}

/** A condition as it is written at its transition: its ST text, or the transition it names. */
export function conditionText(condition: SfcCondition | null): string {
  if (condition === null) return '';
  const text =
    condition.kind === 'reference'
      ? condition.name
      : (condition.text.st ?? `(in ${condition.text.language ?? 'no language'})`);
  return condition.negated ? `NOT (${text})` : text;
}

/** What an association runs, as written in its block: the action or variable it names, or its body. */
export function actionText(action: SfcAction): string {
  if (action.reference !== null) return action.reference;
  if (action.inline === null) return '';
  return action.inline.st ?? `(in ${action.inline.language ?? 'no language'})`;
}
