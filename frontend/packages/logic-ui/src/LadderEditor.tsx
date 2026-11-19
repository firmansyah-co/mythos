// The Ladder Diagram editor (ADR-013 §2.8, §2.15, L3b-2b), an engineer's; everyone else gets
// the read-only view. The owner decided (2026-11-18) that the IEC 61131-3 editors follow the
// TIA Portal's workflow and conventions as closely as they can, with artwork of our own:
// networks, each with a title and a comment; an instruction palette and a favourites bar,
// used by click or by drag; symbolic operands typed with completion from the program's
// variables, and declared from where they are typed; online monitoring with power-flow
// colours. Its visual design is the owner's to lead: it uses the workbench's classes and
// tokens, and decides nothing past clarity.
//
// The document is the model (ldmodel.ts) written back into the TC6 text after every edit, and
// what is drawn is that text as the viewer reads it (`PouView`, network by network): what is on
// screen is what is saved. The program is unsaved until Save, as Structured Text is.
//
// Keys, while the editor has the focus (TIA's for the favourites):
//   Shift+F2 normally open contact · Shift+F3 normally closed contact · Shift+F7 coil
//   Shift+F5 empty box · Shift+F8 open a branch · Shift+F9 close it
//   Ctrl+R a network after the selected one · Alt+↑/↓ move the network
//   ←/→ the previous or next element · Shift+←/→ widen the selection along its series
//   ↑/↓ the previous or next network · Enter the operand of the selection · Delete it
//   Ctrl+Z undo · Ctrl+Y or Ctrl+Shift+Z redo · Ctrl+S save
// In an operand field: ↑/↓ choose a suggestion · Enter take it · Ctrl+Enter take it and
// declare it as proposed · Escape back to the diagram.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { ArrowDown, ArrowUp, Plus, Redo, Trash2, Undo } from 'lucide-react';
import type { Diagnostic } from './bridge';
import { findBox, type BoxDef } from './ldcatalog';
import {
  addNetwork,
  branch,
  check,
  declarationFor,
  declareVariable,
  elementsOf,
  findEl,
  insert,
  locate,
  moveNetwork,
  operands,
  remove,
  seriesAt,
  setBoxType,
  setHeader,
  setInstance,
  setLabel,
  setOperand,
  setPin,
  setVariant,
  STANDARD_BOXES,
  type Edited,
  type Sel,
} from './ldedit';
import { idsFor, open, ownBoxes, write, type DocModel, type Drawn, type El, type Ids, type Item, type Section, type Source, type VarInfo } from './ldmodel';
import { readLadder, type LdElement, type LdPou } from './ladder';
import { allInstructions, dragged, Favourites, useLdPalette, type Instruction } from './LdInstructions';
import { PouView, type LadderLive } from './LadderView';

// ---------------------------------------------------------------------------------------------
// The session: the document as opened, the models it went through, the ids given out
// ---------------------------------------------------------------------------------------------

interface Session {
  source: Source;
  ids: Ids;
  history: DocModel[];
  at: number;
  /** The text the model was last written to: what the program holds now. */
  text: string;
  drawn: Drawn[][];
}

function start(text: string, before: Session | null): Session | string {
  const opened = open(text);
  if (!opened.ok) return opened.error;
  const ids = idsFor(opened.source);
  // An id given out in this editor is not given out again, even after a discard.
  if (before) ids.next = Math.max(ids.next, before.ids.next);
  return { source: opened.source, ids, history: [opened.model], at: 0, text, drawn: write(opened.source, opened.model, idsFor(opened.source)).drawn };
}

// ---------------------------------------------------------------------------------------------
// Drawing a network with the viewer
// ---------------------------------------------------------------------------------------------

/** The viewer's size of an element, where the document gives none. */
function boxOf(e: LdElement): { x: number; y: number; w: number; h: number } {
  const n = Math.max(1, e.inputs.length, e.outputs.length);
  const w = e.width ?? (e.kind === 'contact' || e.kind === 'coil' ? 30 : e.kind === 'block' ? 100 : 80);
  const h = e.height ?? (e.kind === 'block' ? 20 + 20 * n : 20);
  return { x: e.x, y: e.y, w, h };
}

/** One network of the written document, as a POU of its own for the viewer, moved to the top. */
function networkPou(pou: LdPou, drawn: Drawn): { view: LdPou; dy: number } {
  const ids = new Set(drawn.ids);
  const own = pou.elements.filter((e) => ids.has(e.localId));
  const top = own.length > 0 ? Math.min(...own.map((e) => e.y)) : 0;
  const bottom = own.length > 0 ? Math.max(...own.map((e) => boxOf(e).y + boxOf(e).h)) : 30;
  const dy = own.length > 0 ? 40 - top : 0;
  const moved = own.map((e) => ({ ...e, y: e.y + dy }));
  // The rails of the body, drawn the height of this network: TC6 has one pair for all.
  const rails = pou.elements
    .filter((e) => e.kind === 'leftPowerRail' || e.kind === 'rightPowerRail')
    .map((e) => ({ ...e, y: 10, height: bottom + dy + 30 - 10 }));
  return { view: { ...pou, elements: [...rails, ...moved], notDrawn: [] }, dy };
}

interface Gap {
  path: number[];
  index: number;
  x: number;
  y: number;
}

/** The points of a rung where an instruction can go, from where its elements are drawn. */
function gapsOf(rung: Item[], at: Map<number, { x: number; y: number; w: number; h: number; line: number }>, railRight: number): Gap[] {
  const out: Gap[] = [];
  const left = (item: Item): number =>
    item.t === 'el' ? (at.get(item.el.id)?.x ?? 0) : Math.min(...item.branches.flatMap((b) => b.map(left)), Infinity);
  const right = (item: Item): number =>
    item.t === 'el' ? ((at.get(item.el.id)?.x ?? 0) + (at.get(item.el.id)?.w ?? 0)) : Math.max(...item.branches.flatMap((b) => b.map(right)), -Infinity);
  const lineOf = (item: Item): number => (item.t === 'el' ? (at.get(item.el.id)?.line ?? 0) : lineOf((item.branches[0] ?? [])[0] as Item));
  const bottom = (item: Item): number =>
    item.t === 'el' ? ((at.get(item.el.id)?.y ?? 0) + (at.get(item.el.id)?.h ?? 0)) : Math.max(...item.branches.flatMap((b) => b.map(bottom)), -Infinity);
  const walk = (series: Item[], path: number[], from: number, line: number) => {
    if (series.length === 0) {
      out.push({ path, index: 0, x: from + 20, y: line });
      return;
    }
    const y = lineOf(series[0] as Item);
    series.forEach((item, i) => {
      const before = i === 0 ? from : right(series[i - 1] as Item);
      out.push({ path, index: i, x: (before + left(item)) / 2, y });
      if (item.t === 'par') {
        let below = y;
        item.branches.forEach((b, k) => {
          if (b.length === 0) out.push({ path: [...path, i, k], index: 0, x: left(item) + 10, y: below + 30 });
          else {
            walk(b, [...path, i, k], left(item) - 10, lineOf(b[0] as Item));
            below = Math.max(below, ...b.map(bottom));
          }
        });
      }
    });
    out.push({ path, index: series.length, x: right(series[series.length - 1] as Item) + 15, y });
  };
  walk(rung, [], railRight, 50);
  return out.filter((g) => Number.isFinite(g.x) && Number.isFinite(g.y));
}

const sameGap = (sel: Sel | null, g: Gap) => sel?.kind === 'gap' && sel.index === g.index && sel.path.join() === g.path.join();

/** The overlay over a network's drawing: the selection, the problems found here, the insertion points. */
function Overlay({
  svg,
  children,
}: {
  svg: SVGSVGElement | null;
  children: ReactNode;
}) {
  if (!svg) return null;
  return (
    <svg
      className="ld-overlay"
      data-testid="ld-overlay"
      viewBox={svg.getAttribute('viewBox') ?? undefined}
      width={svg.getAttribute('width') ?? undefined}
      height={svg.getAttribute('height') ?? undefined}
      aria-hidden
    >
      {children}
    </svg>
  );
}

// ---------------------------------------------------------------------------------------------
// An operand field with completion
// ---------------------------------------------------------------------------------------------

function OperandField({
  label,
  value,
  testId,
  suggestions,
  focusToken,
  onFocused,
  onCommit,
  onDeclare,
  declareOnEnter = false,
  onLeave,
}: {
  label: string;
  value: string;
  testId: string;
  suggestions: VarInfo[];
  /** Above 0: the field takes the focus, once for each new value. */
  focusToken: number;
  onFocused?: () => void;
  onCommit: (text: string) => void;
  /** Take the text and declare it as proposed (Ctrl+Enter). */
  onDeclare?: (text: string) => void;
  /** Enter declares as well (an instance's field: TIA asks for the instance there). */
  declareOnEnter?: boolean;
  onLeave: () => void;
}) {
  const [draft, setDraft] = useState(value);
  const [open, setOpen] = useState(false);
  const [pick, setPick] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  // What the field holds now, and what it last gave as its operand: a blur after Enter or
  // Escape gives nothing again, and never an older text.
  const latest = useRef(value);
  const sent = useRef(value);
  useEffect(() => {
    setDraft(value);
    latest.current = value;
    sent.current = value;
  }, [value]);
  useEffect(() => {
    if (focusToken > 0) {
      input.current?.focus();
      input.current?.select();
      onFocused?.();
    }
    // Only a new request moves the focus.
  }, [focusToken]);
  const listOf = (text: string) => {
    const query = text.trim().toLowerCase();
    return suggestions.filter((v) => query === '' || v.name.toLowerCase().startsWith(query)).slice(0, 12);
  };
  const shown = listOf(draft);
  // A suggestion is taken with Enter only once the arrows went into the list: Enter takes what
  // was typed otherwise.
  const navigated = useRef(false);
  const commit = (text: string) => {
    setOpen(false);
    latest.current = text;
    if (text === sent.current) return;
    sent.current = text;
    onCommit(text);
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation();
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      setOpen(true);
      const first = !navigated.current;
      navigated.current = true;
      setPick((p) => (first ? 0 : Math.max(0, Math.min(shown.length - 1, p + (e.key === 'ArrowDown' ? 1 : -1)))));
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const list = listOf(latest.current);
      const chosen = navigated.current && list[pick] ? list[pick].name : latest.current;
      navigated.current = false;
      setDraft(chosen);
      if ((e.ctrlKey || e.metaKey || declareOnEnter) && onDeclare) {
        setOpen(false);
        latest.current = chosen;
        sent.current = chosen;
        onDeclare(chosen);
      } else commit(chosen);
      onLeave();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      latest.current = sent.current;
      setDraft(sent.current);
      setOpen(false);
      onLeave();
    }
  };
  return (
    <span className="ld-operand">
      <input
        ref={input}
        className="mono"
        aria-label={label}
        data-testid={testId}
        value={draft}
        role="combobox"
        aria-expanded={open && shown.length > 0}
        aria-autocomplete="list"
        onChange={(e) => {
          latest.current = e.target.value;
          navigated.current = false;
          setDraft(e.target.value);
          setOpen(true);
          setPick(0);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => commit(latest.current)}
        onKeyDown={onKey}
      />
      {open && shown.length > 0 && (
        <ul className="ld-suggestions" role="listbox" data-testid="ld-suggestions" aria-label={`Variables for ${label}`}>
          {shown.map((v, i) => (
            <li
              key={v.name}
              role="option"
              aria-selected={i === pick}
              className={i === pick ? 'is-picked' : undefined}
              onMouseDown={(e) => {
                e.preventDefault();
                setDraft(v.name);
                commit(v.name);
              }}
            >
              <span className="mono">{v.name}</span> <span className="muted">{v.type}</span>
            </li>
          ))}
        </ul>
      )}
    </span>
  );
}

const SECTIONS: Array<[Section, string]> = [
  ['inputVars', 'Input (VAR_INPUT)'],
  ['outputVars', 'Output (VAR_OUTPUT)'],
  ['localVars', 'Local (VAR)'],
];
const TYPES = ['BOOL', 'INT', 'DINT', 'LINT', 'SINT', 'REAL', 'LREAL', 'TIME', 'STRING'];

/** "x is not declared": declare it here, in a section, with a type. */
function DeclareOffer({
  name,
  proposed,
  fixedType,
  onDeclare,
}: {
  name: string;
  proposed: { type: string; section: Section };
  fixedType: boolean;
  onDeclare: (type: string, section: Section) => void;
}) {
  const [type, setType] = useState(proposed.type);
  const [section, setSection] = useState<Section>(proposed.section);
  useEffect(() => {
    setType(proposed.type);
    setSection(proposed.section);
  }, [name, proposed.type, proposed.section]);
  return (
    <span className="ld-declare" data-testid="ld-declare-offer">
      <span>
        <span className="mono">{name}</span> is not declared. Declare it as
      </span>
      {fixedType ? (
        <span className="mono">{type}</span>
      ) : (
        <select aria-label={`Type of ${name}`} value={type} onChange={(e) => setType(e.target.value)}>
          <option value="">choose a type</option>
          {TYPES.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
      )}
      <select aria-label={`Section of ${name}`} value={section} onChange={(e) => setSection(e.target.value as Section)}>
        {SECTIONS.map(([s, text]) => (
          <option key={s} value={s}>
            {text}
          </option>
        ))}
      </select>
      <button type="button" className="btn" data-testid="ld-declare" disabled={type === ''} onClick={() => onDeclare(type, section)}>
        Declare
      </button>
    </span>
  );
}

// ---------------------------------------------------------------------------------------------
// The editor
// ---------------------------------------------------------------------------------------------

const variantOf = (el: El): string =>
  el.kind === 'contact'
    ? el.negated
      ? 'nc'
      : el.edge === 'rising'
        ? 'p'
        : el.edge === 'falling'
          ? 'n'
          : 'no'
    : el.storage === 'set'
      ? 's'
      : el.storage === 'reset'
        ? 'r'
        : el.negated
          ? 'neg'
          : el.edge === 'rising'
            ? 'p'
            : el.edge === 'falling'
              ? 'n'
              : 'plain';

const VARIANTS: Record<string, Array<[string, string, Pick<El, 'negated' | 'edge' | 'storage'>]>> = {
  contact: [
    ['no', 'Normally open -| |-', { negated: false, edge: null, storage: null }],
    ['nc', 'Normally closed -|/|-', { negated: true, edge: null, storage: null }],
    ['p', 'Positive edge -|P|-', { negated: false, edge: 'rising', storage: null }],
    ['n', 'Negative edge -|N|-', { negated: false, edge: 'falling', storage: null }],
  ],
  coil: [
    ['plain', 'Coil -( )-', { negated: false, edge: null, storage: null }],
    ['neg', 'Negated -(/)-', { negated: true, edge: null, storage: null }],
    ['s', 'Set -(S)-', { negated: false, edge: null, storage: 'set' }],
    ['r', 'Reset -(R)-', { negated: false, edge: null, storage: 'reset' }],
    ['p', 'Positive edge -(P)-', { negated: false, edge: 'rising', storage: null }],
    ['n', 'Negative edge -(N)-', { negated: false, edge: 'falling', storage: null }],
  ],
};

/** A name for a new instance of `type` that no variable has: its name in lower case and a number. */
function instanceName(type: string, vars: readonly VarInfo[]): string {
  const base = type.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/_+$/, '');
  for (let n = 1; ; n += 1) {
    const name = `${base}${n}`;
    if (!vars.some((v) => v.name.toLowerCase() === name)) return name;
  }
}

export function LadderEditor({
  text,
  onText,
  onSave,
  live,
  program,
  diagnostics,
  fallback,
}: {
  /** What is shown under the reason when the document cannot be edited: the read-only view. */
  fallback?: ReactNode;
  /** The program's text now. */
  text: string;
  /** The text after an edit. */
  onText: (text: string) => void;
  onSave: () => void;
  /** What is drawn live on the PROGRAM; `null` when nothing is. */
  live: LadderLive | null;
  /** The PROGRAM's name, when a check says it. */
  program: string | null;
  diagnostics: Diagnostic[];
}) {
  const [session, setSession] = useState<Session | null>(null);
  const [unreadable, setUnreadable] = useState<string | null>(null);
  const [pouIndex, setPouIndex] = useState(0);
  const [sel, setSel] = useState<Sel | null>(null);
  const [mark, setMark] = useState<{ pou: number; net: number; id: number } | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  /** The element whose first field takes the focus: one just put, or the selection on Enter. */
  const [focusReq, setFocusReq] = useState<{ id: number; token: number } | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const sessionRef = useRef<Session | null>(null);
  sessionRef.current = session;

  // The text is opened again whenever it is not the one this editor wrote: saved elsewhere,
  // read again, or discarded.
  useEffect(() => {
    const now = sessionRef.current;
    if (now !== null && now.text === text) return;
    const next = start(text, now);
    if (typeof next === 'string') {
      setUnreadable(next);
      setSession(null);
      return;
    }
    setUnreadable(null);
    setSession(next);
    setSel(null);
    setMark(null);
    // The POU shown stays while the document has it as an LD body; else the PROGRAM's.
    setPouIndex((at) => {
      if (next.source.pous[at]?.ld) return at;
      const program = next.source.pous.findIndex((p) => p.ld !== null && p.pouType === 'program');
      return Math.max(0, program >= 0 ? program : next.source.pous.findIndex((p) => p.ld !== null));
    });
  }, [text]);

  const model = session ? (session.history[session.at] as DocModel) : null;
  const ldPous = useMemo(() => (session ? session.source.pous.map((p, i) => ({ i, p })).filter(({ p }) => p.ld !== null) : []), [session]);
  const pou = model?.pous[pouIndex] ?? null;
  const pouSource = session?.source.pous[pouIndex] ?? null;
  const own = useMemo(
    () => (session ? ownBoxes(session.source).filter((d) => d.name.toLowerCase() !== (pouSource?.name ?? '').toLowerCase()) : []),
    [session, pouSource]
  );
  const vars: VarInfo[] = useMemo(
    () => [...(pouSource?.vars ?? []), ...(pou?.declared ?? []).map((d) => ({ name: d.name, type: d.type, section: d.section }))],
    [pouSource, pou]
  );
  const suggestions = useMemo(() => operands(vars, own), [vars, own]);
  const problems = useMemo(() => (pou ? check(pou, vars, own) : null), [pou, vars, own]);
  const ladder = useMemo(() => (session ? readLadder(session.text) : null), [session]);
  const drawnPou = ladder?.ok ? (ladder.pous.find((p) => p.name === pouSource?.name) ?? null) : null;

  /**
   * Make an edit of the model as it is now: each edit takes the one before it, so two made in
   * one event (an operand and its declaration) both stay.
   */
  const commit = useCallback(
    (make: (m: DocModel, ids: Ids) => Edited, focus = false) => {
      const now = sessionRef.current;
      if (!now) return;
      const edited = make(now.history[now.at] as DocModel, now.ids);
      if (!edited.ok) {
        setMessage(edited.error);
        return;
      }
      setMessage(null);
      const history = [...now.history.slice(0, now.at + 1), edited.model];
      const written = write(now.source, edited.model, now.ids);
      const next: Session = { ...now, history, at: history.length - 1, text: written.text, drawn: written.drawn };
      sessionRef.current = next;
      setSession(next);
      if (edited.sel) setSel(edited.sel);
      const chosen = edited.sel;
      if (focus && chosen?.kind === 'element') setFocusReq({ id: chosen.id, token: Date.now() });
      onText(written.text);
    },
    [onText]
  );

  const travel = useCallback(
    (by: -1 | 1) => {
      const now = sessionRef.current;
      if (!now) return;
      const at = now.at + by;
      if (at < 0 || at >= now.history.length) return;
      const m = now.history[at] as DocModel;
      const written = write(now.source, m, now.ids);
      const next: Session = { ...now, at, text: written.text, drawn: written.drawn };
      sessionRef.current = next;
      setSession(next);
      setSel(null);
      setMessage(null);
      onText(written.text);
    },
    [onText]
  );

  const backToDiagram = useCallback(() => root.current?.focus(), []);

  const place = useCallback(
    (instruction: Instruction, where: Sel | null) => {
      if (!sessionRef.current) return;
      const s = where ?? sel;
      if (instruction.spec === 'branch-open') {
        if (s?.kind !== 'element') {
          setMessage('Select the element a branch opens at.');
          return;
        }
        setMark({ pou: s.pou, net: s.net, id: s.id });
        setMessage(null);
        return;
      }
      if (instruction.spec === 'branch-close') {
        const opened = mark && s?.kind === 'element' && mark.pou === s.pou && mark.net === s.net ? mark.id : null;
        commit((m) => branch(m, s, opened));
        setMark(null);
        return;
      }
      if (instruction.spec === 'label') {
        if (!s) {
          setMessage('Select the network the label is for.');
          return;
        }
        document.querySelector<HTMLInputElement>(`[data-testid="ld-network-label"][data-network="${s.net + 1}"]`)?.focus();
        return;
      }
      const spec = instruction.spec;
      const def = spec.kind === 'box' ? findBox(spec.type, own) : null;
      commit((m, ids) => {
        const edited = insert(m, ids, s, spec, own);
        if (!edited.ok || def?.kind !== 'functionBlock' || edited.sel?.kind !== 'element') return edited;
        // A function block gets an instance, as TIA asks for an instance DB: a name no
        // variable has is proposed in its field, and Enter there declares it.
        const named = setInstance(edited.model, ids, edited.sel.pou, edited.sel.id, instanceName(def.name, vars));
        return named.ok ? { ...named, sel: edited.sel } : named;
      }, true);
    },
    [sel, mark, own, vars, commit]
  );

  const all: Instruction[] = useMemo(() => allInstructions(own), [own]);

  // The Instructions task card and the favourites bar put through this editor while it is open
  // and can take an instruction: at the selection, as a click on the diagram would.
  const placeRef = useRef(place);
  placeRef.current = place;
  const editable = session !== null;
  useEffect(() => {
    if (!editable) return undefined;
    useLdPalette.setState({ place: (x) => placeRef.current(x, null) });
    return () => useLdPalette.setState({ place: null });
  }, [editable]);
  useEffect(() => {
    useLdPalette.setState({ own });
  }, [own]);

  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!session || !model || !pou) return;
    const target = e.target as HTMLElement;
    if (target !== root.current && target.closest('input, textarea, select')) return;
    const key = `${e.ctrlKey || e.metaKey ? 'Ctrl+' : ''}${e.altKey ? 'Alt+' : ''}${e.shiftKey ? 'Shift+' : ''}${e.key}`;
    const fav = all.find((x) => x.key === key);
    const run = (f: () => void) => {
      e.preventDefault();
      f();
    };
    if (fav) return run(() => place(fav, null));
    const net = sel?.net ?? 0;
    switch (key) {
      case 'Ctrl+z':
      case 'Ctrl+Z':
        return run(() => travel(-1));
      case 'Ctrl+y':
      case 'Ctrl+Y':
      case 'Ctrl+Shift+z':
      case 'Ctrl+Shift+Z':
        return run(() => travel(1));
      case 'Ctrl+s':
      case 'Ctrl+S':
        return run(onSave);
      case 'Ctrl+r':
      case 'Ctrl+R':
        return run(() => commit((m) => addNetwork(m, pouIndex, sel ? sel.net + 1 : pou.networks.length)));
      case 'Delete':
        return run(() => commit((m) => remove(m, sel)));
      case 'Enter':
        return run(() => sel?.kind === 'element' && setFocusReq({ id: sel.id, token: Date.now() }));
      case 'Alt+ArrowUp':
      case 'Alt+ArrowDown':
        return run(() => sel && commit((m) => moveNetwork(m, pouIndex, sel.net, key === 'Alt+ArrowUp' ? -1 : 1)));
      case 'ArrowUp':
      case 'ArrowDown': {
        const to = Math.max(0, Math.min(pou.networks.length - 1, (sel ? sel.net : -1) + (key === 'ArrowUp' ? -1 : 1)));
        return run(() => pou.networks[to] && setSel({ kind: 'network', pou: pouIndex, net: to }));
      }
      case 'ArrowLeft':
      case 'ArrowRight':
      case 'Shift+ArrowLeft':
      case 'Shift+ArrowRight': {
        const rung = pou.networks[net]?.rung;
        if (!rung) return;
        const by = key.endsWith('Left') ? -1 : 1;
        return run(() => {
          if (key.startsWith('Shift') && sel?.kind === 'element') {
            const at = locate(rung, sel.to ?? sel.id);
            const series = at ? seriesAt(rung, at.path) : null;
            const next = at && series ? series[at.index + by] : undefined;
            if (next?.t === 'el') setSel({ ...sel, to: next.el.id });
            return;
          }
          const order = elementsOf(rung);
          const now = sel?.kind === 'element' ? order.findIndex((x) => x.id === (sel.to ?? sel.id)) : by > 0 ? -1 : order.length;
          const next = order[now + by];
          if (next) setSel({ kind: 'element', pou: pouIndex, net, id: next.id, to: null });
        });
      }
      default:
        return undefined;
    }
  };

  if (unreadable !== null) {
    return (
      <>
        <div className="notice notice--bad" role="alert" data-testid="ld-editor-unreadable">
          <p>The diagram cannot be edited: {unreadable}.</p>
        </div>
        {fallback}
      </>
    );
  }
  if (!session || !model || !pou || !pouSource) return <p className="muted">Reading the diagram…</p>;

  const isProgram = program !== null ? pou.name.toLowerCase() === program.toLowerCase() : pou.pouType === 'program';
  const drawnOf = session.drawn[pouIndex] ?? [];

  return (
    <div className="ld-editor" data-testid="ld-editor" tabIndex={0} ref={root} onKeyDown={onKey} aria-label="Ladder Diagram editor">
      <div className="toolbar ld-editor__bar" role="toolbar" aria-label="Favourites" data-testid="ld-favourites">
        <Favourites />
        <span className="toolbar__sep" aria-hidden />
        <button type="button" className="btn" data-testid="ld-undo" disabled={session.at === 0} onClick={() => travel(-1)} title="Undo (Ctrl+Z)">
          <Undo size={13} aria-hidden /> Undo
        </button>
        <button type="button" className="btn" data-testid="ld-redo" disabled={session.at >= session.history.length - 1} onClick={() => travel(1)} title="Redo (Ctrl+Y)">
          <Redo size={13} aria-hidden /> Redo
        </button>
        <button
          type="button"
          className="btn"
          data-testid="ld-add-network"
          onClick={() => commit((m) => addNetwork(m, pouIndex, sel ? sel.net + 1 : pou.networks.length))}
          title="Add a network after the selected one (Ctrl+R)"
        >
          <Plus size={13} aria-hidden /> Network
        </button>
      </div>
      {message && (
        <p className="form__error" role="alert" data-testid="ld-edit-error">
          {message}
        </p>
      )}
      {mark && (
        <p className="muted" data-testid="ld-branch-open">
          A branch is open at element {mark.id}: select where it closes, in the same series, and close it (Shift+F9).
        </p>
      )}
      <div className="ld-editor__body">
        <div className="ld-editor__networks" data-testid="ld-networks">
          {ldPous.length > 1 && (
            <div className="inspector__tabs" role="tablist" aria-label="POUs of the document">
              {ldPous.map(({ i, p }) => (
                <button
                  key={p.name}
                  type="button"
                  role="tab"
                  aria-selected={i === pouIndex}
                  className="inspector__tab"
                  data-testid="ld-pou-tab"
                  onClick={() => {
                    setPouIndex(i);
                    setSel(null);
                  }}
                >
                  {p.name} <span className="muted">{p.pouType}</span>
                </button>
              ))}
            </div>
          )}
          {!isProgram && (
            <p className="muted" data-testid="ld-pou-not-live">
              A {pou.pouType === 'function' ? 'function' : 'function block'} has no state of its own to draw: each call or instance has its own.
            </p>
          )}
          {pou.networks.length === 0 && (
            <p className="muted" data-testid="ld-no-networks">
              No network yet. Add one (Ctrl+R).
            </p>
          )}
          {pou.networks.map((net, n) => {
            const drawn = drawnOf.find((d) => d.key === net.key) ?? { key: net.key, ids: [], top: 0, bottom: 0 };
            return (
              <NetworkView
                key={net.key}
                n={n}
                net={net}
                drawn={drawn}
                pouIndex={pouIndex}
                drawnPou={drawnPou}
                live={isProgram ? live : null}
                diagnostics={diagnostics.filter((d) => d.element?.pou === pou.name && d.element.local_id != null && drawn.ids.includes(d.element.local_id))}
                sel={sel?.pou === pouIndex && sel.net === n ? sel : null}
                problems={problems}
                count={pou.networks.length}
                onSelect={(s) => {
                  setSel(s);
                  setMessage(null);
                  root.current?.focus();
                }}
                onDrop={(s) => {
                  const x = dragged.current;
                  dragged.current = null;
                  if (x) place(x, s);
                }}
                onHeader={(what, value) =>
                  commit((m, ids) => (what === 'label' ? setLabel(m, ids, pouIndex, n, value) : setHeader(m, ids, pouIndex, n, what, value)))
                }
                onMove={(by) => commit((m) => moveNetwork(m, pouIndex, n, by))}
                onDelete={() => commit((m) => remove(m, { kind: 'network', pou: pouIndex, net: n }))}
                onInsertAbove={() => commit((m) => addNetwork(m, pouIndex, n))}
              >
                {sel?.pou === pouIndex && sel.net === n && sel.kind === 'element' && (
                  <ElementFields
                    key={`${sel.id}`}
                    el={findEl(model, pouIndex, sel.id)?.el ?? null}
                    focus={focusReq?.id === sel.id ? focusReq.token : 0}
                    onFocused={() => setFocusReq(null)}
                    suggestions={suggestions}
                    vars={vars}
                    own={own}
                    problems={problems?.elements.get(sel.id) ?? []}
                    compiler={diagnostics.filter((d) => d.element?.pou === pou.name && d.element.local_id === sel.id).map((d) => d.message)}
                    onEdit={(f) => commit(f)}
                    onDeclare={(name, type, section) => commit((m) => declareVariable(m, pouIndex, { name, type, section }, pouSource.vars))}
                    pouIndex={pouIndex}
                    onLeave={backToDiagram}
                  />
                )}
              </NetworkView>
            );
          })}
          <button type="button" className="btn btn--flat" onClick={() => commit((m) => addNetwork(m, pouIndex, pou.networks.length))} data-testid="ld-add-network-end">
            <Plus size={13} aria-hidden /> Add network
          </button>
        </div>
      </div>
    </div>
  );
}

/** One network: its number, title, label and comment, its drawing, and the fields of its selection. */
function NetworkView({
  n,
  net,
  drawn,
  pouIndex,
  drawnPou,
  live,
  diagnostics,
  sel,
  problems,
  count,
  onSelect,
  onDrop,
  onHeader,
  onMove,
  onDelete,
  onInsertAbove,
  children,
}: {
  n: number;
  net: DocModel['pous'][number]['networks'][number];
  drawn: Drawn;
  pouIndex: number;
  drawnPou: LdPou | null;
  live: LadderLive | null;
  diagnostics: Diagnostic[];
  sel: Sel | null;
  problems: ReturnType<typeof check> | null;
  count: number;
  onSelect: (s: Sel) => void;
  onDrop: (s: Sel) => void;
  onHeader: (what: 'title' | 'comment' | 'label', value: string) => void;
  onMove: (by: -1 | 1) => void;
  onDelete: () => void;
  onInsertAbove: () => void;
  children: ReactNode;
}) {
  const [title, setTitle] = useState(net.title);
  const [comment, setComment] = useState(net.comment);
  const [label, setLabelDraft] = useState(net.label);
  useEffect(() => setTitle(net.title), [net.title]);
  useEffect(() => setComment(net.comment), [net.comment]);
  useEffect(() => setLabelDraft(net.label), [net.label]);
  const holder = useRef<HTMLDivElement>(null);
  const [svg, setSvg] = useState<SVGSVGElement | null>(null);
  const [box, setBox] = useState('');
  const view = drawnPou ? networkPou(drawnPou, drawn) : null;
  useLayoutEffect(() => {
    const found = holder.current?.querySelector<SVGSVGElement>('svg[data-testid="ld-diagram"]') ?? null;
    const now = found ? `${found.getAttribute('viewBox')}|${found.getAttribute('width')}|${found.getAttribute('height')}` : '';
    if (found !== svg) setSvg(found);
    if (now !== box) setBox(now);
  });
  const number = n + 1;
  const selectAt = (target: Element, shift: boolean) => {
    const g = target.closest('[data-testid="ld-element"]');
    const id = g ? Number(g.getAttribute('data-local-id')) : NaN;
    const rung = net.rung;
    if (rung && Number.isFinite(id)) {
      const owner = elementsOf(rung).find((e) => e.id === id || e.outputs.some((p) => p.varId === id) || e.inputs.some((p) => p.varId === id));
      if (owner) {
        onSelect(
          shift && sel?.kind === 'element' ? { ...sel, to: owner.id } : { kind: 'element', pou: pouIndex, net: n, id: owner.id, to: null }
        );
        return;
      }
    }
    const link = target.closest('[data-testid="ld-link"]');
    const to = link ? Number(link.getAttribute('data-to')) : NaN;
    const at = rung && Number.isFinite(to) ? locate(rung, to) : null;
    if (at) {
      onSelect({ kind: 'gap', pou: pouIndex, net: n, path: at.path, index: at.index });
      return;
    }
    onSelect({ kind: 'network', pou: pouIndex, net: n });
  };
  // Where each element of the rung is drawn, in the drawing's coordinates.
  const places = new Map<number, { x: number; y: number; w: number; h: number; line: number }>();
  for (const e of view?.view.elements ?? []) {
    const b = boxOf(e);
    const pin = e.kind === 'block' ? 30 : b.h / 2;
    places.set(e.localId, { ...b, line: b.y + (e.inputs[0]?.at?.y ?? pin) });
  }
  if (net.rung) {
    for (const el of elementsOf(net.rung)) {
      const p = places.get(el.id);
      if (p && el.kind === 'block') p.line = p.y + 30 + 20 * Math.max(0, el.inputs.findIndex((i) => i.formal === el.powerIn));
    }
  }
  const leftRail = view?.view.elements.find((e) => e.kind === 'leftPowerRail');
  const railRight = leftRail ? leftRail.x + (leftRail.width ?? 3) : 3;
  const gaps = net.rung ? gapsOf(net.rung, places, railRight) : [];
  const selected = sel?.kind === 'element' && net.rung ? (() => {
    const at = locate(net.rung, sel.id);
    const end = locate(net.rung, sel.to ?? sel.id);
    const series = at ? seriesAt(net.rung, at.path) : null;
    if (!at || !end || !series || at.path.join() !== end.path.join()) return [sel.id];
    const [a, b] = [Math.min(at.index, end.index), Math.max(at.index, end.index)];
    return series.slice(a, b + 1).flatMap((item) => (item.t === 'el' ? [item.el.id] : item.branches.flatMap(elementsOf).map((e) => e.id)));
  })() : [];
  const ownProblems = net.rung ? elementsOf(net.rung).filter((e) => (problems?.elements.get(e.id) ?? []).length > 0) : [];
  return (
    <section
      className={`ld-network${sel ? ' is-selected' : ''}`}
      data-testid="ld-network"
      data-network={number}
      data-kept={net.rung === null ? 'true' : undefined}
      aria-label={`Network ${number}${net.title ? `: ${net.title}` : ''}`}
    >
      <header className="ld-network__header">
        <button type="button" className="btn btn--flat ld-network__number" onClick={() => onSelect({ kind: 'network', pou: pouIndex, net: n })} data-testid="ld-network-select">
          Network {number}
        </button>
        <input
          className="ld-network__title"
          aria-label={`Title of network ${number}`}
          data-testid="ld-network-title"
          placeholder="Title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={() => title !== net.title && onHeader('title', title)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
          }}
        />
        <label className="ld-network__label">
          <span className="muted">Label</span>
          <input
            className="mono"
            aria-label={`Label of network ${number}`}
            data-testid="ld-network-label"
            data-network={number}
            value={label}
            size={8}
            onChange={(e) => setLabelDraft(e.target.value)}
            onBlur={() => label.trim() !== net.label && onHeader('label', label)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
            }}
          />
        </label>
        <span className="ld-network__actions">
          <button type="button" className="icon-btn" aria-label={`Insert a network above network ${number}`} title="Insert a network above" onClick={onInsertAbove}>
            <Plus size={13} aria-hidden />
          </button>
          <button type="button" className="icon-btn" aria-label={`Move network ${number} up`} title="Move up (Alt+↑)" disabled={n === 0} onClick={() => onMove(-1)}>
            <ArrowUp size={13} aria-hidden />
          </button>
          <button type="button" className="icon-btn" aria-label={`Move network ${number} down`} title="Move down (Alt+↓)" disabled={n === count - 1} onClick={() => onMove(1)}>
            <ArrowDown size={13} aria-hidden />
          </button>
          <button type="button" className="icon-btn" aria-label={`Delete network ${number}`} title="Delete the network" onClick={onDelete}>
            <Trash2 size={13} aria-hidden />
          </button>
        </span>
      </header>
      <textarea
        className="ld-network__comment"
        aria-label={`Comment of network ${number}`}
        data-testid="ld-network-comment"
        placeholder="Comment"
        rows={Math.max(1, comment.split('\n').length)}
        value={comment}
        onChange={(e) => setComment(e.target.value)}
        onBlur={() => comment !== net.comment && onHeader('comment', comment)}
      />
      {net.rung === null && (
        <p className="muted" data-testid="ld-network-kept">
          Kept as the document has it: {net.why}. It can be moved, deleted, titled and commented; its elements are written back as they are.
        </p>
      )}
      <div
        className="ld-network__diagram"
        ref={holder}
        onClick={(e) => selectAt(e.target as Element, e.shiftKey)}
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          const g = (e.target as Element).closest('[data-testid="ld-element"]');
          const id = g ? Number(g.getAttribute('data-local-id')) : NaN;
          const owner = net.rung && Number.isFinite(id) ? elementsOf(net.rung).find((x) => x.id === id) : undefined;
          onDrop(owner ? { kind: 'element', pou: pouIndex, net: n, id: owner.id, to: null } : { kind: 'network', pou: pouIndex, net: n });
        }}
      >
        {view && <PouView pou={view.view} live={live} diagnostics={diagnostics.map((d) => (d.element ? { ...d, element: { ...d.element, y: (d.element.y ?? 0) + view.dy } } : d))} />}
        <Overlay svg={svg}>
          {selected.map((id) => {
            const p = places.get(id);
            return p ? <rect key={`sel${id}`} data-testid="ld-selected" data-local-id={id} x={p.x - 6} y={p.y - 6} width={p.w + 12} height={p.h + 12} fill="none" stroke="var(--c-focus)" strokeWidth={2} /> : null;
          })}
          {ownProblems.map((e) => {
            const p = places.get(e.id);
            return p ? (
              <rect key={`p${e.id}`} data-testid="ld-problem" data-local-id={e.id} x={p.x - 3} y={p.y - 3} width={p.w + 6} height={p.h + 6} fill="none" stroke="var(--c-warn)" strokeDasharray="3 2">
                <title>{(problems?.elements.get(e.id) ?? []).join(' ')}</title>
              </rect>
            ) : null;
          })}
          {gaps.map((g) => (
            <rect
              key={`g${g.path.join('.')}:${g.index}`}
              className="ld-gap"
              data-testid="ld-gap"
              data-path={g.path.join('.')}
              data-index={g.index}
              data-selected={sameGap(sel, g) ? 'true' : undefined}
              x={g.x - 5}
              y={g.y - 5}
              width={10}
              height={10}
              fill={sameGap(sel, g) ? 'var(--c-focus)' : 'var(--c-surface)'}
              stroke="var(--c-text-muted)"
              onClick={(e) => {
                e.stopPropagation();
                onSelect({ kind: 'gap', pou: pouIndex, net: n, path: g.path, index: g.index });
              }}
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault();
                e.stopPropagation();
                onDrop({ kind: 'gap', pou: pouIndex, net: n, path: g.path, index: g.index });
              }}
            >
              <title>Insert here</title>
            </rect>
          ))}
        </Overlay>
      </div>
      {(problems?.networks.get(n) ?? []).map((p) => (
        <p key={p} className="muted" data-testid="ld-network-problem">
          {p}
        </p>
      ))}
      {children}
    </section>
  );
}

/** The fields of the selected element: its operand, its kind, a box's type, instance and pins. */
function ElementFields({
  el,
  focus,
  onFocused,
  suggestions,
  vars,
  own,
  problems,
  compiler,
  onEdit,
  onDeclare,
  pouIndex,
  onLeave,
}: {
  el: El | null;
  focus: number;
  onFocused: () => void;
  suggestions: VarInfo[];
  vars: VarInfo[];
  own: BoxDef[];
  problems: string[];
  compiler: string[];
  onEdit: (f: (m: DocModel, ids: Ids) => Edited) => void;
  onDeclare: (name: string, type: string, section: Section) => void;
  pouIndex: number;
  onLeave: () => void;
}) {
  if (!el) return null;
  const bools = suggestions.filter((v) => v.type.toUpperCase() === 'BOOL');
  const undeclared = (text: string) =>
    /^[A-Za-z_]\w*$/.test(text.trim()) && !vars.some((v) => v.name.toLowerCase() === text.trim().toLowerCase()) ? text.trim() : null;
  const offer = (text: string, where: Parameters<typeof declarationFor>[1]) => {
    const name = undeclared(text);
    if (!name) return null;
    const proposed = declarationFor(el, where);
    return <DeclareOffer name={name} proposed={proposed} fixedType={where.instance === true} onDeclare={(type, section) => onDeclare(name, type, section)} />;
  };
  const declareNow = (text: string, where: Parameters<typeof declarationFor>[1]) => {
    const name = undeclared(text);
    const proposed = declarationFor(el, where);
    if (name && proposed.type !== '') onDeclare(name, proposed.type, proposed.section);
  };
  let fields: ReactNode = null;
  if (el.kind === 'contact' || el.kind === 'coil') {
    fields = (
      <>
        <OperandField
          label={`Operand of ${el.kind} ${el.id}`}
          testId="ld-operand"
          value={el.operand}
          suggestions={bools.length > 0 ? bools : suggestions}
          focusToken={focus}
          onFocused={onFocused}
          onCommit={(text) => onEdit((m, ids) => setOperand(m, ids, pouIndex, el.id, text))}
          onDeclare={(text) => {
            onEdit((m, ids) => setOperand(m, ids, pouIndex, el.id, text));
            declareNow(text, {});
          }}
          onLeave={onLeave}
        />
        <select
          aria-label={`Kind of ${el.kind} ${el.id}`}
          data-testid="ld-variant"
          value={variantOf(el)}
          onChange={(e) => {
            const v = VARIANTS[el.kind]?.find(([k]) => k === e.target.value);
            if (v) onEdit((m, ids) => setVariant(m, ids, pouIndex, el.id, v[2]));
          }}
        >
          {(VARIANTS[el.kind] ?? []).map(([k, text]) => (
            <option key={k} value={k}>
              {text}
            </option>
          ))}
        </select>
        {offer(el.operand, {})}
      </>
    );
  } else if (el.kind === 'jump') {
    fields = (
      <OperandField
        label={`Label jump ${el.id} goes to`}
        testId="ld-operand"
        value={el.operand}
        suggestions={[]}
        focusToken={focus}
        onFocused={onFocused}
        onCommit={(text) => onEdit((m, ids) => setOperand(m, ids, pouIndex, el.id, text))}
        onLeave={onLeave}
      />
    );
  } else if (el.kind === 'block') {
    const def = findBox(el.typeName, own);
    const boxes = [...own, ...STANDARD_BOXES].map((d) => ({ name: d.name, type: d.kind === 'functionBlock' ? 'function block' : 'function', section: '' }));
    const needsType = el.typeName === '';
    fields = (
      <>
        <OperandField
          label={`Type of box ${el.id}`}
          testId="ld-box-type"
          value={el.typeName}
          suggestions={boxes}
          focusToken={needsType ? focus : 0}
          onFocused={onFocused}
          onCommit={(text) => {
            const d = findBox(text, own);
            onEdit((m, ids) => {
              const typed = setBoxType(m, ids, pouIndex, el.id, text, own);
              if (!typed.ok || d?.kind !== 'functionBlock' || el.instance !== '') return typed;
              return setInstance(typed.model, ids, pouIndex, el.id, instanceName(d.name, vars));
            });
          }}
          onLeave={onLeave}
        />
        {def?.kind === 'functionBlock' && (
          <OperandField
            label={`Instance of box ${el.id}`}
            testId="ld-box-instance"
            value={el.instance}
            suggestions={suggestions.filter((v) => v.type.toUpperCase() === def.name.toUpperCase())}
            focusToken={needsType ? 0 : focus}
            onFocused={onFocused}
            declareOnEnter
            onCommit={(text) => onEdit((m, ids) => setInstance(m, ids, pouIndex, el.id, text))}
            onDeclare={(text) => {
              onEdit((m, ids) => setInstance(m, ids, pouIndex, el.id, text));
              declareNow(text, { instance: true });
            }}
            onLeave={onLeave}
          />
        )}
        {def?.kind === 'functionBlock' && el.instance !== '' && offer(el.instance, { instance: true })}
        {el.inputs
          .filter((p) => p.formal !== el.powerIn)
          .map((p) => (
            <span key={`in${p.formal}`} className="ld-pin" data-testid="ld-pin" data-formal={p.formal}>
              <span className="mono">{p.formal}</span>
              <OperandField
                label={`${p.formal} of box ${el.id}`}
                testId="ld-pin-operand"
                value={p.operand}
                suggestions={suggestions}
                focusToken={0}
                onCommit={(text) => onEdit((m, ids) => setPin(m, ids, pouIndex, el.id, 'inputs', p.formal, text))}
                onLeave={onLeave}
              />
              {offer(p.operand, { pin: p.formal, which: 'inputs' })}
            </span>
          ))}
        {el.outputs
          .filter((p) => p.formal !== el.powerOut)
          .map((p) => (
            <span key={`out${p.formal}`} className="ld-pin" data-testid="ld-pin" data-formal={p.formal}>
              <span className="mono">{p.formal}</span>
              <OperandField
                label={`${p.formal} of box ${el.id}`}
                testId="ld-pin-operand"
                value={p.operand}
                suggestions={suggestions}
                focusToken={0}
                onCommit={(text) => onEdit((m, ids) => setPin(m, ids, pouIndex, el.id, 'outputs', p.formal, text))}
                onLeave={onLeave}
              />
              {offer(p.operand, { pin: p.formal, which: 'outputs' })}
            </span>
          ))}
      </>
    );
  } else {
    fields = <span className="muted">A return leaves the POU when its rung has power.</span>;
  }
  return (
    <div className="ld-fields" data-testid="ld-fields" data-local-id={el.id}>
      {fields}
      {[...problems, ...compiler.map((c) => `The compiler: ${c}`)].map((p) => (
        <p key={p} className="form__error" data-testid="ld-element-problem">
          {p}
        </p>
      ))}
    </div>
  );
}
