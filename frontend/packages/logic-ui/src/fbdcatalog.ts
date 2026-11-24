// What a Function Block Diagram box is known to call, with its pins (ADR-013 §2.8, §2.16,
// L3c-2): the Ladder Diagram's catalogue (ldcatalog.ts: the standard library's function blocks,
// the standard functions, a conversion `X_TO_Y`, the document's own POUs) and the Boolean
// operators and MOD, which IEC 61131-3 gives a graphical form (Tables 29 and 31) and the FBD
// front-end takes as blocks: AND, OR and XOR of IN1, IN2, … (two or more), NOT of IN, MOD of IN1
// and IN2. The functions and function blocks of the project's library (ADR-014 §2.8), which
// every program is compiled with, are not held here: the editors add them after the document's
// own, as the logic module answers them (libraryblocks.ts).
//
// Unlike a Ladder Diagram's box, an FBD box carries no rung: its pins are its declaration's, and
// EN and ENO are written only where the document already has them (a box read from another tool),
// so a box that always runs is drawn without them, as TIA's FBD draws AND and OR.

import { findBox, STANDARD, type BoxDef, type PinDef } from './ldcatalog';

const pin = (name: string, type: string): PinDef => ({ name, type });
const logic = (name: string, inputs: PinDef[], group = 'Bit logic'): BoxDef => ({
  name,
  kind: 'function',
  inputs,
  outputs: [pin('OUT', 'BOOL')],
  group,
});

/** The Boolean operators as boxes, and MOD. */
export const OPERATORS: readonly BoxDef[] = [
  logic('AND', [pin('IN1', 'BOOL'), pin('IN2', 'BOOL')]),
  logic('OR', [pin('IN1', 'BOOL'), pin('IN2', 'BOOL')]),
  logic('XOR', [pin('IN1', 'BOOL'), pin('IN2', 'BOOL')]),
  logic('NOT', [pin('IN', 'BOOL')]),
  { name: 'MOD', kind: 'function', inputs: [pin('IN1', 'ANY_INT'), pin('IN2', 'ANY_INT')], outputs: [pin('OUT', 'ANY_INT')], group: 'Math' },
];

/** Every standard box an FBD offers, the operators first. */
export const FBD_STANDARD: readonly BoxDef[] = [...OPERATORS, ...STANDARD];

/** The boxes that take more inputs than their declaration shows (AND, OR, XOR: IN3, IN4, …). */
export const EXTENSIBLE = new Set(['AND', 'OR', 'XOR']);

/** A box type by name: the document's own POUs first, then the operators, then the rest. */
export function findFbdBox(name: string, own: readonly BoxDef[]): BoxDef | null {
  const upper = name.trim().toUpperCase();
  if (upper === '') return null;
  return own.find((d) => d.name.toUpperCase() === upper) ?? OPERATORS.find((d) => d.name === upper) ?? findBox(name, own);
}

/**
 * The output a line from a box takes when it names none, and the one a nested box gives its
 * parent: its first output that is not ENO (OUT for a function, Q for a timer).
 */
export function mainOutput(outputs: ReadonlyArray<{ formal: string }>): string {
  return outputs.find((o) => o.formal.toUpperCase() !== 'ENO')?.formal ?? outputs[0]?.formal ?? '';
}
