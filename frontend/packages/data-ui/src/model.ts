// What the data UI derives from the module's answers, as pure functions: whether a diagnostic
// is about the type being edited, what a block's layout reads as, how a block compares with the
// one a runtime of the project is configured with, and whether two texts are the same text.

import type { BlockAnswer, Diagnostic, RuntimeBlock, RuntimeInstance } from './bridge';

/** The text shown for a value the module did not answer. */
export const NOT_KNOWN = 'NOT KNOWN';

/** Why a block's layout, size and members are not known, when they are not. */
export const NOT_CHECKING = "the project's types and data blocks do not check";

/**
 * Whether `d` is about the text of type `type`: the data module names a type's source
 * `data/types/<Name>.st`. One of another file, or of the whole (no source, line 0), is never a
 * place in this type's text.
 */
export function isOfThisType(d: Diagnostic, type: string): boolean {
  return d.source === `data/types/${type}.st`;
}

/** Where a diagnostic is, said beside its message: in this type's text, another file, or the whole. */
export function placeOf(d: Diagnostic, type: string): string {
  if (isOfThisType(d, type)) return `${d.line}:${d.col}`;
  const source = d.source ?? '';
  if (d.line === 0) return source === '' ? 'about the whole' : source;
  return source === '' ? `${d.line}:${d.col} (no file named)` : `${source} ${d.line}:${d.col}`;
}

/** A block's layout as answered, or NOT KNOWN when the answer has none (the state does not check). */
export function layoutText(block: BlockAnswer): string {
  return block.layout ?? 'NOT KNOWN';
}

/** Whether a runtime keeps the block's values: absent is false, the configuration's own default. */
export const retainOf = (b: RuntimeBlock): boolean => b.retain === true;

/** One thing a runtime's configuration has otherwise than the project saved it. */
export type Difference =
  | { what: 'layout'; there: string; here: string }
  | { what: 'type'; there: string; here: string }
  | { what: 'retain'; there: boolean; here: boolean };

/**
 * How a block a runtime is configured with compares with the project's saved one:
 * `gone`, the project no longer has it; `not-known`, the saved state does not check, so its
 * layout is not known and nothing is compared; `compared`, with each difference (none: laid out
 * as saved, which is not "up to date": an initial value is outside the layout).
 */
export type Comparison =
  | { kind: 'gone' }
  | { kind: 'not-known' }
  | { kind: 'compared'; differences: Difference[] };

export function compareBlock(saved: BlockAnswer | null, savedOk: boolean, configured: RuntimeBlock): Comparison {
  if (saved === null) return { kind: 'gone' };
  if (!savedOk || saved.layout === undefined) return { kind: 'not-known' };
  const differences: Difference[] = [];
  if (configured.layout !== saved.layout) differences.push({ what: 'layout', there: configured.layout, here: saved.layout });
  // Type names are one name in any case, as the module compares them.
  if (configured.type.toUpperCase() !== saved.type.toUpperCase()) {
    differences.push({ what: 'type', there: configured.type, here: saved.type });
  }
  if (retainOf(configured) !== saved.retain) differences.push({ what: 'retain', there: retainOf(configured), here: saved.retain });
  return { kind: 'compared', differences };
}

/** Two names are one name in any case, as the module and IEC 61131-3 compare them. */
export const sameName = (a: string, b: string): boolean => a.toUpperCase() === b.toUpperCase();

/** Block `name` of a list the module answered: by its stored spelling, else in another case. */
export function findBlock<B extends { name: string }>(blocks: readonly B[], name: string): B | null {
  return blocks.find((b) => b.name === name) ?? blocks.find((b) => sameName(b.name, name)) ?? null;
}

/** What one difference reads as: both values, the runtime's first. */
export function differenceText(d: Difference): string {
  switch (d.what) {
    case 'layout':
      return `built with layout ${d.there}, saved as ${d.here}`;
    case 'type':
      return `of type ${d.there} there, ${d.here} here`;
    case 'retain':
      return d.there ? 'retained there, not here' : 'not retained there, retained here';
  }
}

/**
 * What a comparison reads as. The most it says is "laid out as saved": an initial value changed
 * in the type is outside the layout, so it never says the runtime is up to date.
 */
export function comparisonText(c: Comparison): string {
  if (c.kind === 'gone') return 'the project no longer has this data block';
  if (c.kind === 'not-known') return `the saved layout is ${NOT_KNOWN}: ${NOT_CHECKING}, so nothing is compared`;
  if (c.differences.length === 0) return 'laid out as saved';
  return c.differences.map(differenceText).join('; ');
}

/** The block named `name` in a runtime's configuration, if it carries one. */
export const carried = (instance: RuntimeInstance, name: string): RuntimeBlock | null =>
  findBlock(instance.data ?? [], name);

/** Each block a configured runtime carries that the project's saved blocks do not have. */
export function goneBlocks(blocks: readonly BlockAnswer[], instances: readonly RuntimeInstance[]): Array<{ runtime: string; block: string }> {
  const gone: Array<{ runtime: string; block: string }> = [];
  for (const instance of instances) {
    for (const configured of instance.data ?? []) {
      if (compareBlock(findBlock(blocks, configured.name), true, configured).kind === 'gone') {
        gone.push({ runtime: instance.id, block: configured.name });
      }
    }
  }
  return gone;
}

/** A text with each `\r\n` read as `\n`: a textarea gives `\n`, a file saved on Windows has `\r\n`. */
export const lineEnds = (text: string): string => text.replace(/\r\n/g, '\n');

/** Whether two texts are the same text, whatever their line ends. */
export function sameText(a: string, b: string): boolean {
  return lineEnds(a) === lineEnds(b);
}
