// A program's bindings as the table shows them: one row per binding, whichever of the three
// lists it is stored in. Every change is made on the stored lists themselves, so a binding the
// table has no row for (made over the API, or naming a variable the program lost) is never
// dropped by an edit of another.

import type { InputBinding, OutputBinding, ProgramEntry, PublishedBinding, Variable } from './bridge';

/** `input`: a tag's value read into a variable; `good`: whether it is current, into a BOOL. */
export type Kind = 'input' | 'good' | 'output' | 'published';

export const KIND_LABEL: Record<Kind, string> = {
  input: 'Reads tag',
  good: 'Reads tag GOOD',
  output: 'Writes tag',
  published: 'Published as',
};

export interface BindingRow {
  kind: Kind;
  /** Its place in the list it is stored in (`inputs`, `outputs` or `published`). */
  index: number;
  variable: string;
  tag: string;
  /** An output's safe value; `null` when none has been given. */
  safe?: number | boolean | null;
  /** Whether an operator may write a published variable. */
  writable?: boolean;
}

type Lists = Pick<ProgramEntry, 'inputs' | 'outputs' | 'published'>;

export function bindingsOf(entry: Lists): BindingRow[] {
  return [
    ...entry.inputs.map((b, index): BindingRow => ({
      kind: b.of === 'good' ? 'good' : 'input',
      index,
      variable: b.variable,
      tag: b.tag,
    })),
    ...entry.outputs.map((b, index): BindingRow => ({
      kind: 'output',
      index,
      variable: b.variable,
      tag: b.tag,
      // A stored binding without a usable safe value is shown as having none, not as zero.
      safe: typeof b.safe === 'number' || typeof b.safe === 'boolean' ? b.safe : null,
    })),
    ...entry.published.map((b, index): BindingRow => ({
      kind: 'published',
      index,
      variable: b.variable,
      tag: b.tag,
      writable: b.writable === true,
    })),
  ];
}

const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** The bindings of `variable` (names in IEC 61131-3 do not differ by case). */
export function bindingsOfVariable(rows: BindingRow[], variable: string): BindingRow[] {
  return rows.filter((row) => same(row.variable, variable));
}

/** The bindings that name no variable of the program as last checked. */
export function orphans(rows: BindingRow[], variables: Variable[]): BindingRow[] {
  return rows.filter((row) => !variables.some((v) => same(v.path, row.variable)));
}

/**
 * A new binding of `variable`, with no tag. An output's safe value is left out: there is no
 * default, because no value is safe for every plant, and the module refuses to save one
 * without it.
 */
export function addBinding(entry: Lists, kind: Kind, variable: string): Lists {
  switch (kind) {
    case 'input':
      return { ...entry, inputs: [...entry.inputs, { variable, tag: '' }] };
    case 'good':
      return { ...entry, inputs: [...entry.inputs, { variable, tag: '', of: 'good' }] };
    case 'output':
      return {
        ...entry,
        outputs: [...entry.outputs, { variable, tag: '', safe: null }],
      };
    case 'published':
      return { ...entry, published: [...entry.published, { variable, tag: '', writable: false }] };
  }
}

const replace = <T,>(list: T[], index: number, edit: (item: T) => T): T[] =>
  list.map((item, i) => (i === index ? edit(item) : item));

export function setTag(entry: Lists, row: BindingRow, tag: string): Lists {
  switch (row.kind) {
    case 'input':
    case 'good':
      return { ...entry, inputs: replace<InputBinding>(entry.inputs, row.index, (b) => ({ ...b, tag })) };
    case 'output':
      return { ...entry, outputs: replace<OutputBinding>(entry.outputs, row.index, (b) => ({ ...b, tag })) };
    case 'published':
      return {
        ...entry,
        published: replace<PublishedBinding>(entry.published, row.index, (b) => ({ ...b, tag })),
      };
  }
}

export function setSafe(entry: Lists, row: BindingRow, safe: number | boolean | null): Lists {
  if (row.kind !== 'output') return entry;
  return {
    ...entry,
    outputs: replace<OutputBinding>(entry.outputs, row.index, (b) => ({ ...b, safe })),
  };
}

export function setWritable(entry: Lists, row: BindingRow, writable: boolean): Lists {
  if (row.kind !== 'published') return entry;
  return {
    ...entry,
    published: replace<PublishedBinding>(entry.published, row.index, (b) => ({ ...b, writable })),
  };
}

export function removeBinding(entry: Lists, row: BindingRow): Lists {
  const without = <T,>(list: T[]) => list.filter((_item, i) => i !== row.index);
  switch (row.kind) {
    case 'input':
    case 'good':
      return { ...entry, inputs: without(entry.inputs) };
    case 'output':
      return { ...entry, outputs: without(entry.outputs) };
    case 'published':
      return { ...entry, published: without(entry.published) };
  }
}

/**
 * What a safe value typed as text is: a number, or `null` when it is not one. Empty text is
 * not zero.
 */
export function parseSafeNumber(text: string): number | null {
  const trimmed = text.trim();
  if (trimmed === '') return null;
  const value = Number(trimmed);
  return Number.isFinite(value) ? value : null;
}

/** What is missing from a binding before it can be saved, in words; `null` when nothing is. */
export function problemOf(row: BindingRow): string | null {
  if (row.tag.trim() === '') return 'No tag is given.';
  if (row.kind === 'published' && !row.tag.startsWith('internal.')) {
    return "A published tag starts with 'internal.'.";
  }
  if (row.kind === 'output' && (row.safe === null || row.safe === undefined)) {
    return 'A safe value is required: what the tag is written whenever the program is not running.';
  }
  return null;
}
