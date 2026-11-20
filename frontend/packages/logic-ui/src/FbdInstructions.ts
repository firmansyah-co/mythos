// The Function Block Diagram's instructions (ADR-013 §2.8, L3c-2): what its Instructions task
// card and favourites bar offer, kept apart from the Ladder Diagram's (each language its own
// favourites, per viewer). The card itself is LD's (LdInstructions.tsx), given this kit.
//
// The keys are LD's positions given their FBD meaning (ADR-013 §2.8, "Keys"): Shift+F2 AND (LD's
// normally open contact: a series is an AND), Shift+F3 OR (LD's normally closed contact, whose
// key is beside it; a parallel is an OR), Shift+F7 assignment (LD's coil), Shift+F5 empty box,
// Shift+F8 one more assignment from the selected box's output (LD's open branch), Shift+F9 one
// more input of an AND, OR or XOR (TIA's "insert input"). TIA's own FBD keys could not be read
// from a public source, so none is claimed to be TIA's.

import { FBD_BOXES, type FbdSpec } from './fbdedit';
import { GROUPS, type BoxDef } from './ldcatalog';
import { boxInstruction, paletteStore, type Instruction, type Kit } from './LdInstructions';

/** What an FBD instruction does: put an element, branch an output, add an input, or name the network. */
export type FbdInstructionSpec = FbdSpec | 'fbd-branch' | 'insert-input' | 'label';
export type FbdInstruction = Instruction<FbdInstructionSpec>;

const box = (type: string, symbol: string, name: string, key?: string): FbdInstruction => ({
  id: `box:${type}`,
  symbol,
  name,
  spec: { kind: 'box', type },
  ...(key ? { key } : {}),
});

export const FBD_BIT_LOGIC: FbdInstruction[] = [
  box('AND', '&', 'AND box', 'Shift+F2'),
  box('OR', '>=1', 'OR box', 'Shift+F3'),
  box('XOR', 'XOR', 'XOR box'),
  box('NOT', 'NOT', 'NOT box'),
  { id: 'assign', symbol: '=', name: 'Assignment', spec: { kind: 'assign' }, key: 'Shift+F7' },
  { id: 'assign-negated', symbol: '/=', name: 'Negated assignment', spec: { kind: 'assign', negated: true } },
  { id: 'branch', symbol: '-+=', name: 'One more assignment from the output', spec: 'fbd-branch', key: 'Shift+F8' },
  { id: 'insert-input', symbol: '+IN', name: 'Insert an input', spec: 'insert-input', key: 'Shift+F9' },
];

export const FBD_EMPTY_BOX: FbdInstruction = {
  id: 'box',
  symbol: '[ ?? ]',
  name: 'Empty box: any function or function block',
  spec: { kind: 'box', type: '' },
  key: 'Shift+F5',
};

export const FBD_CONTROL: FbdInstruction[] = [
  { id: 'jump', symbol: '->>', name: 'Jump to a label', spec: { kind: 'jump' } },
  { id: 'label', symbol: 'L:', name: 'Label of the network', spec: 'label' },
  { id: 'return', symbol: '<RET>', name: 'Return', spec: { kind: 'return' } },
];

const asFbd = (x: Instruction): FbdInstruction => x as unknown as FbdInstruction;
/** Every box of the library and the document, except the Boolean operators (in Bit logic). */
const libraryBoxes = (group: string) => FBD_BOXES.filter((d) => d.group === group && !['AND', 'OR', 'XOR', 'NOT'].includes(d.name)).map((d) => asFbd(boxInstruction(d)));

export function allFbdInstructions(own: readonly BoxDef[]): FbdInstruction[] {
  return [...FBD_BIT_LOGIC, FBD_EMPTY_BOX, ...GROUPS.flatMap(libraryBoxes), ...own.map((d) => asFbd(boxInstruction(d))), ...FBD_CONTROL];
}

/** The favourites a new viewer starts with: the bit logic, the assignment and the empty box, as TIA's FBD bar holds them. */
const DEFAULT_FAVOURITES = ['box:AND', 'box:OR', 'assign', 'box', 'branch', 'insert-input'];

export const useFbdPalette = paletteStore('mythos.logic.fbd.favourites', DEFAULT_FAVOURITES);

export const FBD_KIT: Kit = {
  store: useFbdPalette,
  prefix: 'fbd',
  all: (own) => allFbdInstructions(own),
  groups: (own) => [
    ['Bit logic', FBD_BIT_LOGIC],
    ...GROUPS.map((g): [string, FbdInstruction[]] => [g, [...(g === 'Timers' ? [FBD_EMPTY_BOX] : []), ...libraryBoxes(g)]]),
    ...(own.length > 0 ? [['This document', own.map((d) => asFbd(boxInstruction(d)))] as [string, FbdInstruction[]]] : []),
    ['Program control', FBD_CONTROL],
  ],
  open: ['Bit logic', 'Timers'],
  idle: 'No Function Block Diagram can take an instruction now.',
};
