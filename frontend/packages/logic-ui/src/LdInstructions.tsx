// The Ladder Diagram's instructions: the catalogue, the viewer's favourites, and the
// "Instructions" task card that offers them, as TIA's task card does beside its editor.
//
// The task card is drawn by the workbench in its own pane, apart from the editor, so the two
// meet here: the open editor says what putting an instruction does (`place`) and which of the
// document's own blocks it offers; the card and the editor's favourites bar both put through
// it. What is dragged from the card is kept here too, since a drop on a network is the editor's.

import { useMemo, type DragEvent } from 'react';
import { create, type StoreApi, type UseBoundStore } from 'zustand';
import { Star, X } from 'lucide-react';
import { GROUPS, type BoxDef } from './ldcatalog';
import { STANDARD_BOXES, type Spec } from './ldedit';
import { LIBRARY_GROUP, libraryBoxes, NO_BLOCKS, twinsOf, twinsWords, type LibraryBoxes } from './libraryblocks';
import { useLibrary } from './librarystore';

/** What a Ladder Diagram instruction does: put an element, or open or close a branch, or name the network. */
export type LdSpec = Spec | 'branch-open' | 'branch-close' | 'label';

/** One instruction of the palette: what it puts, or the branch it opens or closes. */
export interface Instruction<S = LdSpec> {
  id: string;
  /** Its IEC 61131-3 semigraphic form, or its name. */
  symbol: string;
  name: string;
  spec: S;
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

/* eslint-disable @typescript-eslint/no-explicit-any -- a kit's instructions are its language's; the card only shows and passes them */
type AnyInstruction = Instruction<any>;
/* eslint-enable @typescript-eslint/no-explicit-any */

/**
 * The project library's blocks as the open project's last check of its stored library answered
 * them, or why they are not known: followed as the library is checked again, and dropped with
 * the project.
 */
export function useLibraryBoxes(): LibraryBoxes {
  const stored = useLibrary((state) => state.stored);
  // When the list of the library could not be read, no check of it is asked: that is said.
  const listError = useLibrary((state) => state.namesError);
  return useMemo(() => libraryBoxes(stored, listError), [stored, listError]);
}

/**
 * The boxes a document may call besides the standard ones: its own POUs, then the library's
 * blocks. A library block named like one of the document's own is left out: every lookup finds
 * the document's, so offering the library's would place the document's under its name.
 */
export const callable = (own: readonly BoxDef[], library: LibraryBoxes): BoxDef[] => {
  const twins = new Set(twinsOf(own, library));
  return [...own, ...(library.boxes ?? []).filter((b) => !twins.has(b))];
};

/**
 * The library's group: its blocks (`own`'s of group Library), or the one line that says why
 * there are none to offer. Never an empty list for a list that is not known. Its fourth element
 * names the blocks it leaves out because the document declares the same name.
 */
export function libraryGroup<I>(own: readonly BoxDef[], library: LibraryBoxes, as: (d: BoxDef) => I): [string, I[], string?, string?] {
  if (library.boxes === null) return [LIBRARY_GROUP, [], library.why];
  const blocks = own.filter((d) => d.group === LIBRARY_GROUP);
  const twins = twinsOf(own, library);
  const note = twins.length > 0 ? twinsWords(twins) : undefined;
  return blocks.length > 0 ? [LIBRARY_GROUP, blocks.map(as), undefined, note] : [LIBRARY_GROUP, [], twins.length > 0 ? undefined : NO_BLOCKS, note];
}

/** Every instruction there is, with the document's own blocks (and the library's, when `own` holds them). */
export function allInstructions(own: readonly BoxDef[]): Instruction[] {
  return [...BIT_LOGIC, EMPTY_BOX, ...STANDARD_BOXES.map(boxInstruction), ...own.map(boxInstruction), ...CONTROL];
}

/** The favourites a new viewer starts with, TIA's default set. */
const DEFAULT_FAVOURITES = ['contact-no', 'contact-nc', 'coil', 'box', 'branch-open', 'branch-close'];
const FAVOURITES_KEY = 'mythos.logic.ld.favourites';

function readFavourites(key: string, defaults: string[]): string[] {
  try {
    const raw = window.localStorage.getItem(key);
    const list = raw ? (JSON.parse(raw) as unknown) : null;
    return Array.isArray(list) && list.every((x) => typeof x === 'string') ? (list as string[]) : defaults;
  } catch {
    return defaults;
  }
}

function saveFavourites(key: string, list: string[]): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(list));
  } catch {
    // A viewer convenience: without storage the favourites last as long as the page.
  }
}

export interface PaletteState {
  /** What putting an instruction does in the open editor; `null` while none can take one. */
  place: ((instruction: AnyInstruction) => void) | null;
  /** The function blocks and functions the open document declares itself (the library's are read where they are shown). */
  own: BoxDef[];
  favourites: string[];
  toggleFavourite: (id: string) => void;
  /** Read the favourites again from the viewer's storage (a test starts from what it set). */
  reread: () => void;
}

export type PaletteStore = UseBoundStore<StoreApi<PaletteState>>;

/** A palette's state: what the open editor does with an instruction, and the viewer's favourites, kept under `key`. */
export function paletteStore(key: string, defaults: string[]): PaletteStore {
  return create<PaletteState>((set, get) => ({
    place: null,
    own: [],
    favourites: readFavourites(key, defaults),
    toggleFavourite: (id) => {
      const now = get().favourites;
      const next = now.includes(id) ? now.filter((x) => x !== id) : [...now, id];
      saveFavourites(key, next);
      set({ favourites: next });
    },
    reread: () => set({ favourites: readFavourites(key, defaults) }),
  }));
}

export const useLdPalette = paletteStore(FAVOURITES_KEY, DEFAULT_FAVOURITES);

/**
 * What a language's task card offers: its store, every instruction it has, its groups, and the
 * prefix of its test ids (`ld-palette-item`, `fbd-palette-item`, …).
 */
export interface Kit {
  store: PaletteStore;
  prefix: string;
  /** `own`: the document's own blocks, then the library's (group Library). */
  all: (own: readonly BoxDef[]) => AnyInstruction[];
  /**
   * A group's third element is the one line it shows in place of items it does not have; its
   * fourth, a line it shows beside them (the library's blocks the document declares too).
   */
  groups: (own: readonly BoxDef[], library: LibraryBoxes) => Array<[string, AnyInstruction[], string?, string?]>;
  /** The groups open at first. */
  open: string[];
  idle: string;
}

export const LD_KIT: Kit = {
  store: useLdPalette,
  prefix: 'ld',
  all: (own) => allInstructions(own),
  groups: (own, library) => {
    const document = own.filter((d) => d.group !== LIBRARY_GROUP);
    return [
      ['Bit logic', BIT_LOGIC],
      ...GROUPS.map((g): [string, Instruction[]] => [
        g,
        [...(g === 'Timers' ? [EMPTY_BOX] : []), ...STANDARD_BOXES.filter((d) => d.group === g).map(boxInstruction)],
      ]),
      libraryGroup(own, library, boxInstruction),
      ...(document.length > 0 ? [['This document', document.map(boxInstruction)] as [string, Instruction[]]] : []),
      ['Program control', CONTROL],
    ];
  },
  open: ['Bit logic', 'Timers'],
  idle: 'No Ladder Diagram can take an instruction now.',
};

/** The instruction being dragged, from the card or the favourites, until it is dropped. */
export const dragged: { current: AnyInstruction | null } = { current: null };

function onDragStart(instruction: AnyInstruction) {
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
export function InstructionButton({ x, where, kit = LD_KIT }: { x: AnyInstruction; where: 'palette' | 'favourites'; kit?: Kit }) {
  const place = kit.store((s) => s.place);
  const favourite = kit.store((s) => s.favourites.includes(x.id));
  const toggleFavourite = kit.store((s) => s.toggleFavourite);
  const p = kit.prefix;
  return (
    <span className="ld-instruction">
      <button
        type="button"
        className={where === 'favourites' ? 'btn' : 'btn btn--flat'}
        draggable
        disabled={place === null}
        onDragStart={onDragStart(x)}
        onClick={() => place?.(x)}
        data-testid={where === 'favourites' ? `${p}-favourite` : `${p}-palette-item`}
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
        data-testid={where === 'favourites' ? `${p}-favourite-remove` : `${p}-favourite-add`}
      >
        {where === 'favourites' ? <X size={11} aria-hidden /> : <Star size={11} aria-hidden fill={favourite ? 'currentColor' : 'none'} />}
      </button>
    </span>
  );
}

/** The editor's favourites bar: the viewer's chosen instructions, in their order. */
export function Favourites({ kit = LD_KIT }: { kit?: Kit }) {
  const favourites = kit.store((s) => s.favourites);
  const own = kit.store((s) => s.own);
  const library = useLibraryBoxes();
  const all = kit.all(callable(own, library));
  return (
    <>
      {favourites
        .map((id) => all.find((x) => x.id === id))
        .filter((x): x is AnyInstruction => x !== undefined)
        .map((x) => (
          <InstructionButton key={x.id} x={x} where="favourites" kit={kit} />
        ))}
    </>
  );
}

/** The "Instructions" task card: every instruction, by group, put by click or by drag. */
export function InstructionsCard({ kit = LD_KIT }: { kit?: Kit }) {
  const own = kit.store((s) => s.own);
  const place = kit.store((s) => s.place);
  const library = useLibraryBoxes();
  const p = kit.prefix;
  return (
    <div className="ld-palette" data-testid={`${p}-palette`} aria-label="Instructions">
      {place === null && (
        <p className="muted" data-testid={`${p}-palette-idle`}>
          {kit.idle}
        </p>
      )}
      {kit.groups(callable(own, library), library).map(([title, items, line, note]) => (
        <details key={title} open={kit.open.includes(title)} className="ld-palette__group" data-testid={`${p}-palette-group`} data-group={title}>
          <summary>{title}</summary>
          {line !== undefined && (
            <p className="muted" data-testid={`${p}-palette-library-why`}>
              {line}
            </p>
          )}
          {note !== undefined && (
            <p className="muted" data-testid={`${p}-palette-library-twins`}>
              {note}
            </p>
          )}
          {items.map((x) => (
            <InstructionButton key={x.id} x={x} where="palette" kit={kit} />
          ))}
        </details>
      ))}
    </div>
  );
}
