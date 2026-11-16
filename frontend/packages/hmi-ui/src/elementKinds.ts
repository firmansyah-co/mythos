// The kinds of element a screen can hold: what each is called, how big a new one is, and how it
// is read from what the controller stores.
//
// This is the one list of element kinds. The toolbox offers what is here, the canvas reads what
// is here, and `editors/elementViews` must give each a drawing and a properties panel (the type
// checker refuses a kind without them). Adding a kind is adding an entry here and its views
// there; nothing else changes.

import type { Box, IoFieldElement, KnownElement, TextElement } from './screenModel';

export interface ElementKind<E extends KnownElement = KnownElement> {
  type: E['type'];
  /** What the engineer calls it. */
  label: string;
  /** One line on what it is for. */
  summary: string;
  /** The size of a new one. */
  width: number;
  height: number;
  /** A new element of this kind in `box`. */
  create: (box: Box) => E;
  /** The element a stored component describes, or `null` when a field it needs is missing. */
  parse: (raw: Record<string, unknown>, box: Box) => E | null;
}

const text: ElementKind<TextElement> = {
  type: 'text',
  label: 'Text field',
  summary: 'A fixed label',
  width: 160,
  height: 32,
  create: (box) => ({ ...box, type: 'text', text: 'Text' }),
  parse: (raw, box) => (typeof raw.text === 'string' ? { ...box, type: 'text', text: raw.text } : null),
};

const ioField: ElementKind<IoFieldElement> = {
  type: 'io_field',
  label: 'I/O field',
  summary: 'Shows the value of a tag',
  width: 200,
  height: 56,
  // Unbound until the engineer binds it: a new field shows NOT BOUND, never a value.
  create: (box) => ({ ...box, type: 'io_field', tag: '', label: '', unit: '' }),
  parse: (raw, box) =>
    typeof raw.tag === 'string' && typeof raw.label === 'string' && typeof raw.unit === 'string'
      ? { ...box, type: 'io_field', tag: raw.tag, label: raw.label, unit: raw.unit }
      : null,
};

export const ELEMENT_KINDS: readonly ElementKind[] = [text, ioField] as readonly ElementKind[];

export function elementKind(type: unknown): ElementKind | undefined {
  return ELEMENT_KINDS.find((kind) => kind.type === type);
}
