// What an HMI screen is made of, and the conversion to and from what the controller stores.
//
// The stored canvas is an object with a `components` array. The kinds of component this
// workbench knows are listed in `elementKinds`. Anything else in the array — a component written by another tool, or by an earlier
// workbench — is carried through untouched: it is neither dropped nor reinterpreted, and the
// editor says that it is there.

import { elementKind } from './elementKinds';

export const DEFAULT_CANVAS_WIDTH = 1280;
export const DEFAULT_CANVAS_HEIGHT = 720;
export const GRID = 8;

export interface Box {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface TextElement extends Box {
  type: 'text';
  text: string;
}

/** Shows one tag's value. `tag` is empty until the engineer binds it. */
export interface IoFieldElement extends Box {
  type: 'io_field';
  tag: string;
  label: string;
  unit: string;
}

/** A stored component this workbench cannot draw. `raw` is exactly what was stored. */
export interface UnknownElement {
  type: 'unknown';
  id: string;
  raw: unknown;
  /** What the component says it is, for the editor to name it. */
  declaredType: string;
}

export type KnownElement = TextElement | IoFieldElement;
export type ScreenElement = KnownElement | UnknownElement;

export interface ScreenCanvas {
  width: number;
  height: number;
  elements: ScreenElement[];
  /** Every other top-level key of the stored canvas, kept as stored. */
  extra: Record<string, unknown>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);

function box(raw: Record<string, unknown>): Box | null {
  const { id, x, y, width, height } = raw;
  if (typeof id !== 'string' || id === '') return null;
  if (!finite(x) || !finite(y) || !finite(width) || !finite(height)) return null;
  if (width <= 0 || height <= 0) return null;
  return { id, x, y, width, height };
}

let unknownSeq = 1;

function parseElement(raw: unknown): ScreenElement {
  if (isRecord(raw)) {
    const geometry = box(raw);
    const kind = elementKind(raw.type);
    const element = geometry && kind ? kind.parse(raw, geometry) : null;
    if (element) return element;
  }
  // Not one of ours, or one of ours with a field missing. Either way it is not drawn from a
  // guess: it is kept as it is.
  const declaredType = isRecord(raw) && typeof raw.type === 'string' ? raw.type : 'untyped';
  const id = isRecord(raw) && typeof raw.id === 'string' && raw.id !== '' ? raw.id : `unknown-${unknownSeq++}`;
  return { type: 'unknown', id, raw, declaredType };
}

/** Read a stored canvas. A value that is not a canvas at all reads as an empty one. */
export function parseCanvas(raw: unknown): ScreenCanvas {
  const record = isRecord(raw) ? raw : {};
  const { components, width, height, ...extra } = record;
  return {
    // A canvas that does not state its size gets the standard one, which is then saved with it.
    width: finite(width) && width > 0 ? width : DEFAULT_CANVAS_WIDTH,
    height: finite(height) && height > 0 ? height : DEFAULT_CANVAS_HEIGHT,
    elements: Array.isArray(components) ? components.map(parseElement) : [],
    extra,
  };
}

/** The canvas as it is stored. */
export function serializeCanvas(canvas: ScreenCanvas): Record<string, unknown> {
  return {
    ...canvas.extra,
    width: canvas.width,
    height: canvas.height,
    components: canvas.elements.map((element) => (element.type === 'unknown' ? element.raw : element)),
  };
}

const snap = (value: number) => Math.round(value / GRID) * GRID;

/** Keep a box on the grid and inside the canvas. */
export function place<T extends Box>(element: T, canvas: Pick<ScreenCanvas, 'width' | 'height'>): T {
  const width = Math.max(GRID, Math.min(snap(element.width), canvas.width));
  const height = Math.max(GRID, Math.min(snap(element.height), canvas.height));
  return {
    ...element,
    width,
    height,
    x: Math.max(0, Math.min(snap(element.x), canvas.width - width)),
    y: Math.max(0, Math.min(snap(element.y), canvas.height - height)),
  };
}

let elementSeq = 1;

function newId(existing: ScreenElement[]): string {
  const taken = new Set(existing.map((element) => element.id));
  let id: string;
  do {
    id = `el-${Date.now().toString(36)}-${elementSeq++}`;
  } while (taken.has(id));
  return id;
}

/** Where a new element goes: stepped down the canvas so new ones do not stack exactly. */
function slot(existing: ScreenElement[]): { x: number; y: number } {
  const step = existing.length % 12;
  return { x: 40 + step * 24, y: 40 + step * 24 };
}

/** A new element of the kind named `type`, or `null` when there is no such kind. */
export function newElement(canvas: ScreenCanvas, type: string): KnownElement | null {
  const kind = elementKind(type);
  if (!kind) return null;
  const box = { id: newId(canvas.elements), ...slot(canvas.elements), width: kind.width, height: kind.height };
  return place(kind.create(box), canvas);
}

export function newTextElement(canvas: ScreenCanvas): TextElement {
  return newElement(canvas, 'text') as TextElement;
}

export function newIoField(canvas: ScreenCanvas): IoFieldElement {
  return newElement(canvas, 'io_field') as IoFieldElement;
}
