// A Function Block Diagram as its PLCopen TC6 XML v2.01 document holds it (ADR-013 §2.16), read
// for drawing: each POU's FBD elements with their `localId`, their position and size as the
// document gives them, and their connections. Nothing here compiles or checks the diagram: the
// logic module does, and what it says is drawn at the element it names.
//
// The reader takes what the front-end takes and draws it; an element it does not know is listed
// as not drawn, never left out silently.

import { child, children, flag, inputOf, number, relPosition, tc6Pous, textOf, type LdInput, type LdOutput } from './ladder';

export type FbdKind =
  | 'block'
  | 'inVariable'
  | 'outVariable'
  | 'inOutVariable'
  | 'connector'
  | 'continuation'
  | 'label'
  | 'jump'
  | 'return'
  | 'comment';

const KINDS: readonly FbdKind[] = [
  'block',
  'inVariable',
  'outVariable',
  'inOutVariable',
  'connector',
  'continuation',
  'label',
  'jump',
  'return',
  'comment',
];

export interface FbdElement {
  kind: FbdKind;
  localId: number;
  x: number;
  y: number;
  /** As the document gives them; `null` when it does not. */
  width: number | null;
  height: number | null;
  /** An in, out or in-out variable's expression. */
  variable: string | null;
  /** An input or output variable's negation; an in-out variable's is on its input and output. */
  negated: boolean;
  typeName: string | null;
  instanceName: string | null;
  /** A label's or jump's label; a connector's or continuation's name. */
  label: string | null;
  /** A comment's text. */
  text: string | null;
  inputs: LdInput[];
  outputs: LdOutput[];
}

export interface FbdPou {
  name: string;
  pouType: string;
  elements: FbdElement[];
  /** Elements that are not drawn, each with why. */
  notDrawn: string[];
  /** Whether the body is FBD; one in another language is not drawn. */
  fbd: boolean;
}

export type Fbd = { ok: true; pous: FbdPou[] } | { ok: false; error: string };

function element(node: Element, kind: FbdKind): FbdElement | string {
  const localId = number(node.getAttribute('localId'));
  if (localId === null) return `a ${kind} without a localId`;
  const position = child(node, 'position');
  const x = number(position?.getAttribute('x') ?? null);
  const y = number(position?.getAttribute('y') ?? null);
  if (x === null || y === null) return `${kind} ${localId} has no position`;
  const e: FbdElement = {
    kind,
    localId,
    x,
    y,
    width: number(node.getAttribute('width')),
    height: number(node.getAttribute('height')),
    variable: null,
    negated: flag(node, 'negated'),
    typeName: node.getAttribute('typeName'),
    instanceName: node.getAttribute('instanceName'),
    label: node.getAttribute('label') ?? node.getAttribute('name'),
    text: null,
    inputs: [],
    outputs: [],
  };
  const pointIn = child(node, 'connectionPointIn');
  const pointOut = child(node, 'connectionPointOut');
  switch (kind) {
    case 'inVariable':
      e.variable = textOf(node, 'expression');
      e.outputs = [{ formal: '', negated: e.negated, at: relPosition(pointOut) }];
      break;
    case 'outVariable':
      e.variable = textOf(node, 'expression');
      e.inputs = [inputOf(pointIn, null, e.negated)];
      break;
    case 'inOutVariable':
      e.variable = textOf(node, 'expression');
      e.inputs = [inputOf(pointIn, null, flag(node, 'negatedIn'))];
      e.outputs = [{ formal: '', negated: flag(node, 'negatedOut'), at: relPosition(pointOut) }];
      break;
    case 'block': {
      const inputs = child(node, 'inputVariables');
      const outputs = child(node, 'outputVariables');
      e.inputs = (inputs ? children(inputs, 'variable') : []).map((v) =>
        inputOf(child(v, 'connectionPointIn'), v.getAttribute('formalParameter') ?? '', flag(v, 'negated'))
      );
      e.outputs = (outputs ? children(outputs, 'variable') : []).map((v) => ({
        formal: v.getAttribute('formalParameter') ?? '',
        negated: flag(v, 'negated'),
        at: relPosition(child(v, 'connectionPointOut')),
      }));
      break;
    }
    case 'connector':
    case 'jump':
    case 'return':
      e.inputs = [inputOf(pointIn, null, false)];
      break;
    case 'continuation':
      e.outputs = [{ formal: '', negated: false, at: relPosition(pointOut) }];
      break;
    case 'comment':
      e.text = (child(node, 'content')?.textContent ?? '').trim();
      break;
    case 'label':
      break;
  }
  return e;
}

/** Read a TC6 document's Function Block Diagrams for drawing. What is not a TC6 project says why. */
export function readFbd(text: string): Fbd {
  const read = tc6Pous(text);
  if (!read.ok) return read;
  const out: FbdPou[] = [];
  for (const pou of read.pous) {
    const body = child(pou, 'body');
    const fbd = body ? child(body, 'FBD') : null;
    const found: FbdPou = {
      name: pou.getAttribute('name') ?? '',
      pouType: pou.getAttribute('pouType') ?? '',
      elements: [],
      notDrawn: [],
      fbd: fbd !== null,
    };
    for (const node of fbd ? children(fbd) : []) {
      if (node.localName === 'addData' || node.localName === 'documentation') continue;
      const kind = KINDS.find((k) => k === node.localName);
      if (!kind) {
        found.notDrawn.push(`${node.localName} ${node.getAttribute('localId') ?? ''}`.trim());
        continue;
      }
      const e = element(node, kind);
      if (typeof e === 'string') found.notDrawn.push(e);
      else found.elements.push(e);
    }
    out.push(found);
  }
  return { ok: true, pous: out };
}

/**
 * Where a line from element `from` really starts: a continuation goes on with what its
 * connector's line comes from (§8.1.3), followed through connectors fed by continuations.
 * `null` when it starts nowhere (a continuation no connector names).
 */
export function lineSource(pou: FbdPou, from: number, formal: string | null): { from: number; formal: string | null } | null {
  const byId = new Map(pou.elements.map((e) => [e.localId, e]));
  let at = { from, formal };
  const seen = new Set<number>();
  for (;;) {
    const e = byId.get(at.from);
    if (!e || e.kind !== 'continuation') return at;
    if (seen.has(e.localId)) return null;
    seen.add(e.localId);
    const connector = pou.elements.find(
      (c) => c.kind === 'connector' && (c.label ?? '').toUpperCase() === (e.label ?? '').toUpperCase()
    );
    const c = connector?.inputs[0]?.connections[0];
    if (!c) return null;
    at = { from: c.from, formal: c.formal };
  }
}

/**
 * The networks of a body in the order they are evaluated, as the front-end orders them (ADR-013
 * §2.13 on §8.3.3): a network is a maximal set of connected elements, a connector and its
 * continuations one line; networks run top to bottom by their topmost element, then the
 * leftmost. Each is the `localId`s of its elements, by position; labels, comments, connectors
 * and continuations are in none.
 */
export function fbdNetworks(pou: FbdPou): number[][] {
  const counted = pou.elements.filter(
    (e) => e.kind !== 'comment' && e.kind !== 'label' && e.kind !== 'connector' && e.kind !== 'continuation'
  );
  const ids = new Set(counted.map((e) => e.localId));
  const parent = new Map<number, number>(counted.map((e) => [e.localId, e.localId]));
  const root = (k: number): number => {
    let at = k;
    while (parent.get(at) !== at) at = parent.get(at) as number;
    return at;
  };
  for (const e of counted) {
    for (const input of e.inputs) {
      for (const c of input.connections) {
        const source = lineSource(pou, c.from, c.formal);
        if (source && ids.has(source.from)) parent.set(root(e.localId), root(source.from));
      }
    }
  }
  const groups = new Map<number, FbdElement[]>();
  for (const e of counted) {
    const r = root(e.localId);
    groups.set(r, [...(groups.get(r) ?? []), e]);
  }
  const byPlace = (a: FbdElement, b: FbdElement) => a.y - b.y || a.x - b.x || a.localId - b.localId;
  return [...groups.values()]
    .map((members) => [...members].sort(byPlace))
    .sort((a, b) => byPlace(a[0] as FbdElement, b[0] as FbdElement))
    .map((members) => members.map((e) => e.localId));
}
