// The project library's functions and function blocks as the Ladder Diagram and Function Block
// Diagram editors offer and check them (ADR-014 §2.8): the blocks the logic module answered in
// its check of the library as it is stored (`POST /library/check`, `blocks`), as boxes.
//
// Nothing here is the UI's own knowledge of a block. Its pins are those the module answered, in
// its order, never read from the Structured Text and never completed with a default type. A
// list the module did not answer is said with why, never shown as an empty list or a partial
// one: one entry of another shape makes the whole list unknown. The pin names are upper-cased,
// as the document's own POUs' are (ldmodel.ts `boxOfPou`), and the types kept as answered.

import { findFbdBox } from './fbdcatalog';
import type { BoxDef, PinDef } from './ldcatalog';
import type { Pin } from './library';
import type { StoredCheck } from './librarystore';

/** The palette group of the library's blocks, and the `group` of each of their boxes. */
export const LIBRARY_GROUP = 'Library';

/** The library's blocks as boxes, or why they are not known here. */
export type LibraryBoxes = { boxes: BoxDef[]; why: null } | { boxes: null; why: string };

export const BEING_CHECKED = 'The library is being checked; its blocks are offered once it answers.';
/** The list of the library's sources could not be read, so its check was never asked. */
export const notReadWords = (error: string) => `The library could not be read: ${error}; its blocks are not offered.`;
export const notCheckedWords = (error: string) => `The library could not be checked: ${error}; its blocks are not offered.`;
export const doesNotCheckWords = (n: number) =>
  `The library does not check (${n} ${n === 1 ? 'diagnostic' : 'diagnostics'}); its blocks are offered once it does.`;
export const NOT_ANSWERED = "This logic module does not answer the library's blocks.";
export const OTHER_SHAPE = "The library's blocks were answered in a shape this workbench does not read.";
/**
 * An answer that names a block twice (in any case) or names one like a standard block: every
 * lookup of the editors would take one for the other, so none is offered.
 */
export const NAMES_UNREAD = "The library's blocks were answered with a name twice or with a standard block's name; this workbench does not offer them.";
/** Said in the palette's group when the library answered that it declares none. */
export const NO_BLOCKS = "The project's library declares no function or function block.";

/** A block entry of the shape `check_logic_library` gives, its pin names upper-cased; `null` for any other. */
type Checked =
  | { name: string; kind: 'function'; inputs: PinDef[]; outputs: PinDef[]; result: string }
  | { name: string; kind: 'function_block'; inputs: PinDef[]; outputs: PinDef[]; result: null };

const isText = (x: unknown): x is string => typeof x === 'string' && x !== '';

function pinsOf(list: unknown): PinDef[] | null {
  if (!Array.isArray(list)) return null;
  const out: PinDef[] = [];
  for (const p of list as unknown[]) {
    if (p === null || typeof p !== 'object') return null;
    const { name, type } = p as Partial<Record<keyof Pin, unknown>>;
    if (!isText(name) || !isText(type)) return null;
    out.push({ name: name.toUpperCase(), type });
  }
  return out;
}

function checked(entry: unknown): Checked | null {
  if (entry === null || typeof entry !== 'object') return null;
  const e = entry as Record<string, unknown>;
  const inputs = pinsOf(e.inputs);
  const outputs = pinsOf(e.outputs);
  if (!isText(e.name) || inputs === null || outputs === null) return null;
  // A function has a result and no output; a function block outputs and no result.
  if (e.kind === 'function' && isText(e.result) && outputs.length === 0) {
    return { name: e.name, kind: 'function', inputs, outputs, result: e.result };
  }
  if (e.kind === 'function_block' && e.result === null) return { name: e.name, kind: 'function_block', inputs, outputs, result: null };
  return null;
}

/**
 * The blocks of the stored library's last check, as boxes, or why they are not known.
 * `listError`: why the list of the library's sources could not be read, when it could not (no
 * check of the library is asked then, so none is said to be under way).
 */
export function libraryBoxes(stored: StoredCheck | null, listError: string | null = null): LibraryBoxes {
  if (stored === null && listError !== null) return { boxes: null, why: notReadWords(listError) };
  if (stored === null) return { boxes: null, why: BEING_CHECKED };
  if (stored.answer === null) return { boxes: null, why: notCheckedWords(stored.error) };
  const answer = stored.answer;
  if (!answer.ok) return { boxes: null, why: doesNotCheckWords(Array.isArray(answer.diagnostics) ? answer.diagnostics.length : 0) };
  if (!Array.isArray(answer.blocks)) return { boxes: null, why: NOT_ANSWERED };
  const boxes: BoxDef[] = [];
  const names = new Set<string>();
  for (const entry of answer.blocks as unknown[]) {
    const b = checked(entry);
    if (b === null) return { boxes: null, why: OTHER_SHAPE };
    const upper = b.name.toUpperCase();
    if (names.has(upper) || findFbdBox(upper, []) !== null) return { boxes: null, why: NAMES_UNREAD };
    names.add(upper);
    boxes.push({
      name: b.name,
      kind: b.kind === 'function_block' ? 'functionBlock' : 'function',
      inputs: b.inputs,
      outputs: b.kind === 'function' ? [{ name: 'OUT', type: b.result }] : b.outputs,
      group: LIBRARY_GROUP,
    });
  }
  return { boxes, why: null };
}

/** The words of D4c, said while the library's blocks cannot be known (unknown-box.test.ts). */
export const editorDoesNotKnow = (type: string) =>
  `${type} is not a function or function block this editor knows: it knows this program's own and the standard ones. If the project's library declares it, Check compiles the program with it.`;

/**
 * What is said of a box naming a block the editor does not find. With the library's blocks
 * known: that neither the program nor the library declares it, and that it is not a standard
 * block this editor knows (the compiler declares standard blocks this editor does not hold,
 * CTU_DINT and TON_TIME among them, so the editor never says a name is not standard). Not known: the
 * words of D4c, and why the library's are not known. Not given (a caller that has no library):
 * the words of D4c alone.
 */
export function unknownBoxWords(type: string, library?: LibraryBoxes): string[] {
  if (library === undefined) return [editorDoesNotKnow(type)];
  if (library.boxes !== null) {
    return [`${type} is not a function or function block of this program or of the project's library, nor a standard one this editor knows.`];
  }
  const why = library.why.charAt(0).toLowerCase() + library.why.slice(1);
  return [editorDoesNotKnow(type), `The project's library's blocks are not known here: ${why}`];
}

const isPower = (formal: string) => {
  const f = formal.toUpperCase();
  return f === 'EN' || f === 'ENO';
};

/**
 * What a box's stored pins lack or have beyond its block's declaration now, per pin: a pin the
 * declaration does not have, and a declared input the box does not have. EN and ENO are never
 * compared. The box is not changed: what it holds is the document's until the engineer edits it.
 */
export function pinProblems(inputs: ReadonlyArray<{ formal: string }>, outputs: ReadonlyArray<{ formal: string }>, def: BoxDef): string[] {
  const has = (list: ReadonlyArray<{ name: string }>, formal: string) => list.some((p) => p.name.toUpperCase() === formal.toUpperCase());
  const out: string[] = [];
  for (const p of inputs) {
    if (!isPower(p.formal) && !has(def.inputs, p.formal)) out.push(`${p.formal} is not a pin of ${def.name} as it is declared now.`);
  }
  for (const p of outputs) {
    if (!isPower(p.formal) && !has(def.outputs, p.formal)) out.push(`${p.formal} is not a pin of ${def.name} as it is declared now.`);
  }
  for (const d of def.inputs) {
    if (!inputs.some((p) => p.formal.toUpperCase() === d.name.toUpperCase())) {
      out.push(`${def.name} declares the input ${d.name}, which this box does not have.`);
    }
  }
  return out;
}

/**
 * The type of pin `formal` as the box's block declares it now; `stored` (the type the model
 * took when the text was opened, or `BOOL` of EN and ENO) when the declaration does not have it.
 */
export function declaredType(def: BoxDef | null, which: 'inputs' | 'outputs', formal: string, stored: string): string {
  return def?.[which].find((p) => p.name.toUpperCase() === formal.toUpperCase())?.type ?? stored;
}

/**
 * Said of a box of a block this document declares when the project's library declares a block
 * of the same name too: the box has this document's pins, and the library's block is not offered.
 */
export const twinWords = (name: string) => `${name} is declared both by this document and by the project's library; this box has this document's pins.`;

/** The library's blocks named like one of the document's own POUs (`own` holds the document's). */
export function twinsOf(own: readonly BoxDef[], library: LibraryBoxes): BoxDef[] {
  const document = new Set(own.filter((d) => d.group !== LIBRARY_GROUP).map((d) => d.name.toUpperCase()));
  return (library.boxes ?? []).filter((b) => document.has(b.name.toUpperCase()));
}

/** Said in the library's group of the blocks it does not offer because the document declares the same name. */
export const twinsWords = (twins: readonly BoxDef[]) =>
  `Not offered: ${twins.map((d) => d.name).join(', ')}, which this document declares too.`;

/** Said of a box that keeps an instance while its block is declared a function now. */
export const instanceOfFunctionWords = (name: string, instance: string) =>
  `${name} is a function as it is declared now, which has no instance; this box still names ${instance}.`;

/** Said of a box's power pin (LD) whose declaration is not a BOOL now. */
export const powerPinWords = (formal: string, name: string, type: string) =>
  `${formal} carries the rung's power, a BOOL; ${name} declares it ${type} now.`;
