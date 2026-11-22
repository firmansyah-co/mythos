// The Sequential Function Chart's instructions (ADR-013 §2.8, L3d-3): what its Instructions task
// card and favourites bar offer, kept apart from LD's and FBD's (each language its own
// favourites, per viewer). The card itself is LD's (LdInstructions.tsx), given this kit.
//
// S7-GRAPH's sequence elements, with IEC 61131-3's names (§6.7, Table 61): a step and a
// transition, an alternative branch (selection), a simultaneous branch, a jump, the end of a
// sequence; an action of each qualifier of Table 59 (SD, SL and P0 as ADR-013 §2.17 says: the
// full delay, the full limit, a pulse only when the step is left); the initial step and a new
// sequence. The
// symbols are words and IEC's semigraphic forms, never Siemens' artwork.
//
// The keys are LD's positions given their SFC meaning, as FBD's are (ADR-013 §2.8, "Keys"): none
// is claimed to be TIA's.

import type { BoxDef } from './ldcatalog';
import { paletteStore, type Instruction, type Kit } from './LdInstructions';

/** What an SFC instruction does at the selection. */
export type SfcSpec = 'pair' | 'alt' | 'sim' | 'jump' | 'end' | 'initial' | 'chart' | { action: string };
export type SfcInstruction = Instruction<SfcSpec>;

export const SFC_SEQUENCE: SfcInstruction[] = [
  { id: 'pair', symbol: '[S]+T', name: 'Step and transition', spec: 'pair', key: 'Shift+F2' },
  { id: 'alt', symbol: '-+-', name: 'Alternative branch (selection)', spec: 'alt', key: 'Shift+F3' },
  { id: 'sim', symbol: '=+=', name: 'Simultaneous branch', spec: 'sim', key: 'Shift+F8' },
  { id: 'jump', symbol: '->S', name: 'Jump to a step', spec: 'jump', key: 'Shift+F9' },
  { id: 'end', symbol: '[S].', name: 'End of the sequence at a step, in place of a jump', spec: 'end' },
];

const QUALIFIER_NAMES: Array<[string, string]> = [
  ['N', 'non-stored'],
  ['S', 'set (stored)'],
  ['R', 'reset'],
  ['L', 'time limited'],
  ['D', 'time delayed'],
  ['P', 'pulse'],
  ['P1', 'pulse on entry'],
  ['DS', 'delayed and stored'],
  ['SD', 'stored and delayed: starts after the full delay, the step left or not'],
  ['SL', 'stored and time limited: runs the full limit, the step left or not'],
  ['P0', 'pulse when the step is left'],
];

export const SFC_ACTIONS: SfcInstruction[] = QUALIFIER_NAMES.map(([q, what]) => ({
  id: `action:${q}`,
  symbol: q,
  name: `Action ${q}, ${what}`,
  spec: { action: q },
  ...(q === 'N' ? { key: 'Shift+F7' } : {}),
}));

export const SFC_CHART: SfcInstruction[] = [
  { id: 'initial', symbol: '[[S]]', name: 'Make the step initial', spec: 'initial' },
  { id: 'chart', symbol: '+[[S]]', name: 'New sequence, with its initial step', spec: 'chart' },
];

export function allSfcInstructions(): SfcInstruction[] {
  return [...SFC_SEQUENCE, ...SFC_ACTIONS, ...SFC_CHART];
}

/** The favourites a new viewer starts with: the sequence's elements and an N action. */
const DEFAULT_FAVOURITES = ['pair', 'alt', 'sim', 'jump', 'action:N'];

export const useSfcPalette = paletteStore('mythos.logic.sfc.favourites', DEFAULT_FAVOURITES);

export const SFC_KIT: Kit = {
  store: useSfcPalette,
  prefix: 'sfc',
  all: (_own: readonly BoxDef[]) => allSfcInstructions(),
  groups: () => [
    ['Sequence', SFC_SEQUENCE],
    ['Actions', SFC_ACTIONS],
    ['Chart', SFC_CHART],
  ],
  open: ['Sequence', 'Actions'],
  idle: 'No Sequential Function Chart can take an instruction now.',
};
