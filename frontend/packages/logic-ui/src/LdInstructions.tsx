// The Ladder Diagram's instructions: the catalogue, the viewer's favourites, and the
// "Instructions" task card that offers them, as TIA's task card does beside its editor.
//
// The task card is drawn by the workbench in its own pane, apart from the editor, so the two
// meet here: the open editor says what putting an instruction does (`place`) and which of the
// document's own blocks it offers; the card and the editor's favourites bar both put through
// it. What is dragged from the card is kept here too, since a drop on a network is the editor's.

import type { DragEvent } from 'react';
import { create } from 'zustand';
import { Star, X } from 'lucide-react';
import { GROUPS, type BoxDef } from './ldcatalog';
import { STANDARD_BOXES, type Spec } from './ldedit';

/** One instruction of the palette: what it puts, or the branch it opens or closes. */
export interface Instruction {
  id: string;
  /** Its IEC 61131-3 semigraphic form, or its name. */
  symbol: string;
  name: string;
  spec: Spec | 'branch-open' | 'branch-close' | 'label';
  key?: string;
}

export const BIT_LOGIC: Instruction[] = [
  { id: 'contact-no', symbol: '-| |-', name: 'Normally open contact', spec: { kind: 'contact' }, key: 'Shift+F2' },
  { id: 'contact-nc', symbol: '-|/|-', name: 'Normally closed contact', spec: { kind: 'contact', negated: true }, key: 'Shift+F3' },
  { id: 'contact-p', symbol: '-|P|-', name: 'Positive edge contact', spec: { kind: 'contact', edge: 'rising' } },
  { id: 'contact-n', symbol: '-|N|-', name: 'Negative edge contact', spec: { kind: 'contact', edge: 'falling' } },
  { id: 'coil', symbol: '-( )-', name: 'Coil', spec: { kind: 'coil' }, key: 'Shift+F7' },
  { id: 'coil-negated', symbol: '-(/)-', name: 'Negated coil', spec: { kind: 'coil', negated: true } },
  { id: 'coil-set', symbol: '-(S)-', name: 'Set coil', spec: { kind: 'coil', storage: 'set' } },
  { id: 'coil-reset', symbol: '-(R)-', name: 'Reset coil', spec: { kind: 'coil', storage: 'reset' } },
  { id: 'coil-p', symbol: '-(P)-', name: 'Positive edge coil', spec: { kind: 'coil', edge: 'rising' } },
  { id: 'coil-n', symbol: '-(N)-', name: 'Negative edge coil', spec: { kind: 'coil', edge: 'falling' } },
  { id: 'branch-open', symbol: '-+-', name: 'Open branch', spec: 'branch-open', key: 'Shift+F8' },
  { id: 'branch-close', symbol: '-+=', name: 'Close branch', spec: 'branch-close', key: 'Shift+F9' },
];

export const CONTROL: Instruction[] = [
  { id: 'jump', symbol: '->>', name: 'Jump to a label', spec: { kind: 'jump' } },
  { id: 'label', symbol: 'L:', name: 'Label of the network', spec: 'label' },
  { id: 'return', symbol: '<RET>', name: 'Return', spec: { kind: 'return' } },
];

export const EMPTY_BOX: Instruction = {
  id: 'box',
  symbol: '[ ?? ]',
  name: 'Empty box: any function or function block',
  spec: { kind: 'box', type: '' },
  key: 'Shift+F5',
};

export const boxInstruction = (def: BoxDef): Instruction => ({
  id: `box:${def.name}`,
  symbol: def.name,
  name: `${def.name} (${def.kind === 'functionBlock' ? 'function block' : 'function'})`,
  spec: { kind: 'box', type: def.name },
});

/** Every instruction there is, with the document's own blocks. */
export function allInstructions(own: readonly BoxDef[]): Instruction[] {
  return [...BIT_LOGIC, EMPTY_BOX, ...STANDARD_BOXES.map(boxInstruction), ...own.map(boxInstruction), ...CONTROL];
}

/** The favourites a new viewer starts with, TIA's default set. */
const DEFAULT_FAVOURITES = ['contact-no', 'contact-nc', 'coil', 'box', 'branch-open', 'branch-close'];
const FAVOURITES_KEY = 'mythos.logic.ld.favourites';

function readFavourites(): string[] {
  try {
    const raw = window.localStorage.getItem(FAVOURITES_KEY);
    const list = raw ? (JSON.parse(raw) as unknown) : null;
    return Array.isArray(list) && list.every((x) => typeof x === 'string') ? (list as string[]) : DEFAULT_FAVOURITES;
  } catch {
    return DEFAULT_FAVOURITES;
  }
}

function saveFavourites(list: string[]): void {
  try {
    window.localStorage.setItem(FAVOURITES_KEY, JSON.stringify(list));
  } catch {
    // A viewer convenience: without storage the favourites last as long as the page.
  }
}

interface PaletteState {
  /** What putting an instruction does in the open editor; `null` while none can take one. */
  place: ((instruction: Instruction) => void) | null;
  /** The function blocks and functions the open document declares itself. */
  own: BoxDef[];
  favourites: string[];
  toggleFavourite: (id: string) => void;
  /** Read the favourites again from the viewer's storage (a test starts from what it set). */
  reread: () => void;
}

export const useLdPalette = create<PaletteState>((set, get) => ({
  place: null,
  own: [],
  favourites: readFavourites(),
  toggleFavourite: (id) => {
    const now = get().favourites;
    const next = now.includes(id) ? now.filter((x) => x !== id) : [...now, id];
    saveFavourites(next);
    set({ favourites: next });
  },
  reread: () => set({ favourites: readFavourites() }),
}));

/** The instruction being dragged, from the card or the favourites, until it is dropped. */
export const dragged: { current: Instruction | null } = { current: null };

function onDragStart(instruction: Instruction) {
  return (e: DragEvent) => {
    dragged.current = instruction;
    try {
      e.dataTransfer?.setData('text/plain', instruction.id);
    } catch {
      // The data is for other targets only; the editor takes what is dragged from `dragged`.
    }
  };
}

/** One instruction: a button that puts it, and the star that makes it a favourite or not. */
export function InstructionButton({ x, where }: { x: Instruction; where: 'palette' | 'favourites' }) {
  const place = useLdPalette((s) => s.place);
  const favourite = useLdPalette((s) => s.favourites.includes(x.id));
  const toggleFavourite = useLdPalette((s) => s.toggleFavourite);
  return (
    <span className="ld-instruction">
      <button
        type="button"
        className={where === 'favourites' ? 'btn' : 'btn btn--flat'}
        draggable
        disabled={place === null}
        onDragStart={onDragStart(x)}
        onClick={() => place?.(x)}
        data-testid={where === 'favourites' ? 'ld-favourite' : 'ld-palette-item'}
        data-item={x.id}
        title={`${x.name}${x.key ? ` (${x.key})` : ''}`}
        aria-label={x.name}
      >
        <span className="mono">{x.symbol}</span>
        {where === 'palette' && <span className="ld-instruction__name">{x.name}</span>}
      </button>
      <button
        type="button"
        className="icon-btn"
        onClick={() => toggleFavourite(x.id)}
        aria-label={favourite ? `Remove ${x.name} from the favourites` : `Add ${x.name} to the favourites`}
        title={favourite ? 'Remove from the favourites' : 'Add to the favourites'}
        data-testid={where === 'favourites' ? 'ld-favourite-remove' : 'ld-favourite-add'}
      >
        {where === 'favourites' ? <X size={11} aria-hidden /> : <Star size={11} aria-hidden fill={favourite ? 'currentColor' : 'none'} />}
      </button>
    </span>
  );
}

/** The editor's favourites bar: the viewer's chosen instructions, in their order. */
export function Favourites() {
  const favourites = useLdPalette((s) => s.favourites);
  const own = useLdPalette((s) => s.own);
  const all = allInstructions(own);
  return (
    <>
      {favourites
        .map((id) => all.find((x) => x.id === id))
        .filter((x): x is Instruction => x !== undefined)
        .map((x) => (
          <InstructionButton key={x.id} x={x} where="favourites" />
        ))}
    </>
  );
}

/** The "Instructions" task card: every instruction, by group, put by click or by drag. */
export function InstructionsCard() {
  const own = useLdPalette((s) => s.own);
  const place = useLdPalette((s) => s.place);
  const groups: Array<[string, Instruction[]]> = [
    ['Bit logic', BIT_LOGIC],
    ...GROUPS.map((g): [string, Instruction[]] => [
      g,
      [...(g === 'Timers' ? [EMPTY_BOX] : []), ...STANDARD_BOXES.filter((d) => d.group === g).map(boxInstruction)],
    ]),
    ...(own.length > 0 ? [['This document', own.map(boxInstruction)] as [string, Instruction[]]] : []),
    ['Program control', CONTROL],
  ];
  return (
    <div className="ld-palette" data-testid="ld-palette" aria-label="Instructions">
      {place === null && (
        <p className="muted" data-testid="ld-palette-idle">
          No Ladder Diagram can take an instruction now.
        </p>
      )}
      {groups.map(([title, items]) => (
        <details key={title} open={title === 'Bit logic' || title === 'Timers'} className="ld-palette__group">
          <summary>{title}</summary>
          {items.map((x) => (
            <InstructionButton key={x.id} x={x} where="palette" />
          ))}
        </details>
      ))}
    </div>
  );
}
