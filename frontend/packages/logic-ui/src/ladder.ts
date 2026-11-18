// A Ladder Diagram as its PLCopen TC6 XML v2.01 document holds it (ADR-013 §2.15), read for
// drawing: each POU's LD elements with their `localId`, their position and size as the
// document gives them, and their connections. Nothing here compiles or checks the diagram:
// the logic module does, and what it says is drawn at the element it names.
//
// The reader takes what the front-end takes and draws it; an element it does not know is
// listed as not drawn, never left out silently.

export const TC6_NAMESPACE = 'http://www.plcopen.org/xml/tc6_0201';

export type LdKind =
  | 'leftPowerRail'
  | 'rightPowerRail'
  | 'contact'
  | 'coil'
  | 'block'
  | 'inVariable'
  | 'outVariable'
  | 'inOutVariable'
  | 'label'
  | 'jump'
  | 'return'
  | 'comment';

const KINDS: readonly LdKind[] = [
  'leftPowerRail',
  'rightPowerRail',
  'contact',
  'coil',
  'block',
  'inVariable',
  'outVariable',
  'inOutVariable',
  'label',
  'jump',
  'return',
  'comment',
];

/** A connection into an input: from element `from`, from its output `formal` (a block's) or its one output. */
export interface LdConnection {
  from: number;
  formal: string | null;
}

/** An input of an element: what is connected to it, or an expression written there. */
export interface LdInput {
  /** A block's formal parameter; `null` for the one input of any other element. */
  formal: string | null;
  connections: LdConnection[];
  expression: string | null;
  negated: boolean;
  /** Where the pin is, from the element's position, when the document says. */
  at: { x: number; y: number } | null;
}

export interface LdOutput {
  /** A block's formal parameter; `''` for the one output of any other element. */
  formal: string;
  negated: boolean;
  at: { x: number; y: number } | null;
}

export interface LdElement {
  kind: LdKind;
  localId: number;
  x: number;
  y: number;
  /** As the document gives them; `null` when it does not. */
  width: number | null;
  height: number | null;
  /** A contact's or coil's variable; an in, out or in-out variable's expression. */
  variable: string | null;
  negated: boolean;
  edge: 'rising' | 'falling' | null;
  storage: 'set' | 'reset' | null;
  typeName: string | null;
  instanceName: string | null;
  /** A label's or jump's label. */
  label: string | null;
  /** A comment's text. */
  text: string | null;
  inputs: LdInput[];
  outputs: LdOutput[];
}

export interface LdPou {
  name: string;
  pouType: string;
  elements: LdElement[];
  /** Elements that are not drawn, each with why. */
  notDrawn: string[];
  /** Whether the body is LD; one in another language is not drawn. */
  ld: boolean;
}

export type Ladder = { ok: true; pous: LdPou[] } | { ok: false; error: string };

export const children = (node: Element, name?: string): Element[] =>
  [...node.children].filter((child) => name === undefined || child.localName === name);
export const child = (node: Element, name: string): Element | null => children(node, name)[0] ?? null;

export function number(text: string | null): number | null {
  if (text === null || text.trim() === '') return null;
  const value = Number(text);
  return Number.isFinite(value) ? value : null;
}

/** A `true`/`false` attribute (xsd:boolean, so also `1`/`0`). */
export const flag = (node: Element, name: string) => {
  const value = node.getAttribute(name);
  return value === 'true' || value === '1';
};

export function relPosition(point: Element | null): { x: number; y: number } | null {
  const rel = point ? child(point, 'relPosition') : null;
  if (!rel) return null;
  const x = number(rel.getAttribute('x'));
  const y = number(rel.getAttribute('y'));
  return x === null || y === null ? null : { x, y };
}

function connectionsOf(point: Element): LdConnection[] {
  const out: LdConnection[] = [];
  for (const c of children(point, 'connection')) {
    const from = number(c.getAttribute('refLocalId'));
    if (from !== null) out.push({ from, formal: c.getAttribute('formalParameter') || null });
  }
  return out;
}

export function inputOf(point: Element | null, formal: string | null, negated: boolean): LdInput {
  const expression = point ? child(point, 'expression') : null;
  return {
    formal,
    connections: point ? connectionsOf(point) : [],
    expression: expression ? (expression.textContent ?? '') : null,
    negated,
    at: relPosition(point),
  };
}

/** The text of an element's `<variable>` or `<expression>` child. */
export const textOf = (node: Element, name: string): string | null => {
  const found = child(node, name);
  return found ? (found.textContent ?? '').trim() : null;
};

function element(node: Element, kind: LdKind): LdElement | string {
  const localId = number(node.getAttribute('localId'));
  if (localId === null) return `a ${kind} without a localId`;
  const position = child(node, 'position');
  const x = number(position?.getAttribute('x') ?? null);
  const y = number(position?.getAttribute('y') ?? null);
  if (x === null || y === null) return `${kind} ${localId} has no position`;
  const edge = node.getAttribute('edge');
  const storage = node.getAttribute('storage');
  const e: LdElement = {
    kind,
    localId,
    x,
    y,
    width: number(node.getAttribute('width')),
    height: number(node.getAttribute('height')),
    variable: null,
    negated: flag(node, 'negated'),
    edge: edge === 'rising' || edge === 'falling' ? edge : null,
    storage: storage === 'set' || storage === 'reset' ? storage : null,
    typeName: node.getAttribute('typeName'),
    instanceName: node.getAttribute('instanceName'),
    label: node.getAttribute('label'),
    text: null,
    inputs: [],
    outputs: [],
  };
  const pointsIn = children(node, 'connectionPointIn');
  const pointOut = child(node, 'connectionPointOut');
  switch (kind) {
    case 'leftPowerRail':
      // One output per connectionPointOut; a link from the rail is drawn from where it ends.
      e.outputs = [{ formal: '', negated: false, at: null }];
      break;
    case 'rightPowerRail':
      e.inputs = pointsIn.map((point) => inputOf(point, null, false));
      break;
    case 'contact':
    case 'coil':
      e.variable = textOf(node, 'variable');
      e.inputs = [inputOf(pointsIn[0] ?? null, null, false)];
      e.outputs = [{ formal: '', negated: false, at: relPosition(pointOut) }];
      break;
    case 'inVariable':
      e.variable = textOf(node, 'expression');
      e.outputs = [{ formal: '', negated: false, at: relPosition(pointOut) }];
      break;
    case 'outVariable':
      e.variable = textOf(node, 'expression');
      e.inputs = [inputOf(pointsIn[0] ?? null, null, false)];
      break;
    case 'inOutVariable':
      e.variable = textOf(node, 'expression');
      e.inputs = [inputOf(pointsIn[0] ?? null, null, flag(node, 'negatedIn'))];
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
    case 'jump':
    case 'return':
      e.inputs = [inputOf(pointsIn[0] ?? null, null, false)];
      break;
    case 'comment':
      e.text = (child(node, 'content')?.textContent ?? '').trim();
      break;
    case 'label':
      break;
  }
  return e;
}

/** The POUs of a TC6 document, or why it is not one (LD and FBD read it alike). */
export function tc6Pous(text: string): { ok: true; pous: Element[] } | { ok: false; error: string } {
  let doc: Document;
  try {
    doc = new DOMParser().parseFromString(text, 'application/xml');
  } catch (cause) {
    return { ok: false, error: `the document cannot be read as XML: ${String(cause)}` };
  }
  const root = doc.documentElement;
  if (!root || doc.getElementsByTagName('parsererror').length > 0) {
    return { ok: false, error: 'the document is not well-formed XML' };
  }
  if (root.localName !== 'project' || root.namespaceURI !== TC6_NAMESPACE) {
    return {
      ok: false,
      error: `the document is not a PLCopen TC6 XML v2.01 project: its root is ${root.localName} in the namespace ${root.namespaceURI ?? 'of none'}`,
    };
  }
  const pous = child(root, 'types') ? child(child(root, 'types') as Element, 'pous') : null;
  return { ok: true, pous: pous ? children(pous, 'pou') : [] };
}

/** Read a TC6 document for drawing. What is not a TC6 project says why. */
export function readLadder(text: string): Ladder {
  const read = tc6Pous(text);
  if (!read.ok) return read;
  const out: LdPou[] = [];
  for (const pou of read.pous) {
    const body = child(pou, 'body');
    const ld = body ? child(body, 'LD') : null;
    const read: LdPou = {
      name: pou.getAttribute('name') ?? '',
      pouType: pou.getAttribute('pouType') ?? '',
      elements: [],
      notDrawn: [],
      ld: ld !== null,
    };
    for (const node of ld ? children(ld) : []) {
      if (node.localName === 'addData' || node.localName === 'documentation') continue;
      const kind = KINDS.find((k) => k === node.localName);
      if (!kind) {
        read.notDrawn.push(`${node.localName} ${node.getAttribute('localId') ?? ''}`.trim());
        continue;
      }
      const e = element(node, kind);
      if (typeof e === 'string') read.notDrawn.push(e);
      else read.elements.push(e);
    }
    out.push(read);
  }
  return { ok: true, pous: out };
}

/**
 * The networks of a body in the order they are evaluated (IEC 61131-3 ed. 3 §8.1.4, §8.2.7, as
 * the front-end orders them, ADR-013 §2.13): a network is a maximal set of connected elements,
 * the rails excluded; networks run top to bottom by their topmost element, then the leftmost.
 * Each is the `localId`s of its elements.
 */
export function networks(pou: LdPou): number[][] {
  const counted = pou.elements.filter(
    (e) => e.kind !== 'leftPowerRail' && e.kind !== 'rightPowerRail' && e.kind !== 'comment' && e.kind !== 'label'
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
        if (ids.has(c.from)) parent.set(root(e.localId), root(c.from));
      }
    }
  }
  const groups = new Map<number, LdElement[]>();
  for (const e of counted) {
    const r = root(e.localId);
    groups.set(r, [...(groups.get(r) ?? []), e]);
  }
  const top = (members: LdElement[]) =>
    members.reduce((a, b) => (b.y < a.y || (b.y === a.y && b.x < a.x) ? b : a));
  const byPlace = (a: LdElement, b: LdElement) => a.y - b.y || a.x - b.x || a.localId - b.localId;
  return [...groups.values()]
    .map((members) => [...members].sort(byPlace))
    .sort((a, b) => byPlace(top(a), top(b)))
    .map((members) => members.map((e) => e.localId));
}

/** Whether the body has a jump or a return: a rung the scan skips is drawn as not executed. */
export const transfers = (pou: LdPou): boolean =>
  pou.elements.some((e) => e.kind === 'jump' || e.kind === 'return');
