// The Function Block Diagram editor (ADR-013 §2.8, §2.16, L3c-2), an engineer's; everyone else
// gets the read-only view. It follows the Ladder Diagram editor (LadderEditor.tsx), which follows
// the TIA Portal's workflow by the owner's decision (2026-11-18): networks with a number, a title,
// a label and a comment; the Instructions task card and a favourites bar; symbolic operands with
// completion, declared where they are typed; online monitoring. Standard FBD, as TIA's FBD
// editor: each network one tree laid out by the editor, not placed freely (a CFC mode is later).
//
// The document is the model (fbdmodel.ts) written back into the TC6 text after every edit, and
// each network is drawn by the view (`FbdPouView`) from that text, with the live values the view
// draws: what is on screen is what is saved and what compiles. The program is unsaved until
// Save, as Structured Text is.
//
// Keys, while the editor has the focus (ADR-013 §2.8 says why these):
//   Shift+F2 AND · Shift+F3 OR · Shift+F7 assignment · Shift+F5 empty box
//   Shift+F8 one more assignment from the selected box · Shift+F9 one more input (AND, OR, XOR)
//   Ctrl+R a network after the selected one · Alt+↑/↓ move the network
//   ←/→ the previous or next element of the network · ↑/↓ the previous or next network
//   Enter the first field of the selection · Delete it
//   Ctrl+Z undo · Ctrl+Y or Ctrl+Shift+Z redo · Ctrl+S save

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { ArrowDown, ArrowUp, Plus, Redo, Trash2, Undo } from 'lucide-react';
import type { Diagnostic } from './bridge';
import { readFbd, type FbdElement, type FbdPou } from './fbd';
import { EXTENSIBLE, findFbdBox } from './fbdcatalog';
import {
  addInput,
  addNetwork,
  addSink,
  boxesOf,
  check,
  concrete,
  declarationFor,
  declareVariable,
  findBoxIn,
  FBD_BOXES,
  insert,
  moveNetwork,
  operands,
  orAt,
  pinOfOperand,
  remove,
  setBoxType,
  setHeader,
  setInstance,
  setLabel,
  setNegated,
  setPinOperand,
  setSinkText,
  sinksOf,
  type Edited,
  type Problems,
  type Sel,
} from './fbdedit';
import { allFbdInstructions, useFbdPalette, FBD_KIT, type FbdInstruction } from './FbdInstructions';
import { open, varsOfPou, write, type Box, type DocModel, type Drawn, type Network, type Root, type Sink, type Source } from './fbdmodel';
import { FbdPouView, type FbdLive } from './FbdView';
import type { BoxDef } from './ldcatalog';
import { ownBoxes } from './ldmodel';
import type { Ids, Section, VarInfo } from './ldmodel';
import { dragged, Favourites } from './LdInstructions';
import { DeclareOffer, instanceName, OperandField } from './LadderEditor';

interface Session {
  source: Source;
  ids: Ids;
  history: DocModel[];
  at: number;
  /** The text the model was last written to: what the program holds now. */
  text: string;
  drawn: Drawn[][];
}

const idsOf = (source: Source): Ids => ({ next: source.firstFree, used: new Set() });

function start(text: string, before: Session | null): Session | string {
  const opened = open(text);
  if (!opened.ok) return opened.error;
  const ids = idsOf(opened.source);
  // An id given out in this editor is not given out again, even after a discard.
  if (before) ids.next = Math.max(ids.next, before.ids.next);
  return { source: opened.source, ids, history: [opened.model], at: 0, text, drawn: write(opened.source, opened.model, idsOf(opened.source)).drawn };
}

/** The view's size of an element, where the document gives none. */
function sizeOf(e: FbdElement): { w: number; h: number } {
  if (e.kind === 'block') return { w: e.width ?? 100, h: e.height ?? 20 + 20 * Math.max(1, e.inputs.length, e.outputs.length) };
  return { w: e.width ?? 80, h: e.height ?? 20 };
}

/** One network of the written document, as a POU of its own for the view, moved to the top. */
function networkPou(pou: FbdPou, drawn: Drawn): { view: FbdPou; dy: number } {
  const ids = new Set(drawn.ids);
  const own = pou.elements.filter((e) => ids.has(e.localId) && e.kind !== 'comment' && e.kind !== 'label');
  const top = own.length > 0 ? Math.min(...own.map((e) => e.y)) : 0;
  const dy = own.length > 0 ? 20 - top : 0;
  return { view: { ...pou, elements: own.map((e) => ({ ...e, y: e.y + dy })), notDrawn: [] }, dy };
}

/** The key of the field that takes the focus: a box's type or instance, an input, an assignment. */
type FieldKey = string;
const typeKey = (id: number) => `type:${id}`;
const instanceKey = (id: number) => `instance:${id}`;
const pinKey = (box: number, formal: string) => `pin:${box}:${formal}`;
const sinkKey = (id: number) => `sink:${id}`;

/** The field a new element, or the selection on Enter, takes the focus at. */
function firstField(root: Root | null, sel: Sel, own: readonly BoxDef[]): FieldKey | null {
  if (sel.kind === 'pin') return pinKey(sel.box, sel.formal);
  if (sel.kind !== 'element') return null;
  const box = findBoxIn(root, sel.id);
  if (box) {
    if (box.typeName === '') return typeKey(box.id);
    if (findFbdBox(box.typeName, own)?.kind === 'functionBlock') return instanceKey(box.id);
    // The first input that reads nothing, else the first with an operand.
    const free = box.inputs.find((p) => p.src === null) ?? box.inputs.find((p) => p.src?.t === 'operand');
    return free ? pinKey(box.id, free.formal) : typeKey(box.id);
  }
  return sinkKey(sel.id);
}

/** The elements of a network in the order ← and → go through them: what a box reads before it, its assignments after. */
function order(root: Root | null): number[] {
  if (!root) return [];
  if (root.t === 'sink') return [root.sink.id];
  const out: number[] = [];
  const visit = (b: Box) => {
    for (const p of b.inputs) if (p.src?.t === 'box') visit(p.src.box);
    out.push(b.id);
    for (const o of b.outputs) out.push(...o.to.map((s) => s.id));
  };
  visit(root.box);
  return out;
}

export function FbdEditor({
  text,
  onText,
  onSave,
  live,
  program,
  diagnostics,
  outputTypes,
  fallback,
}: {
  text: string;
  onText: (text: string) => void;
  onSave: () => void;
  /** What is drawn live on the PROGRAM; `null` when nothing is. */
  live: FbdLive | null;
  program: string | null;
  diagnostics: Diagnostic[];
  /**
   * The type of each output of the PROGRAM's boxes as the compiler gives it (`<localId>:<FORMAL>`),
   * from the check of the text as it is now; empty when there is none.
   */
  outputTypes: Map<string, string>;
  fallback?: ReactNode;
}) {
  const [session, setSession] = useState<Session | null>(null);
  const [unreadable, setUnreadable] = useState<string | null>(null);
  const [pouIndex, setPouIndex] = useState(0);
  const [sel, setSel] = useState<Sel | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [focusReq, setFocusReq] = useState<{ key: FieldKey; token: number } | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const sessionRef = useRef<Session | null>(null);
  sessionRef.current = session;

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
    setPouIndex((at) => {
      if (next.source.pous[at]?.fbd) return at;
      const prog = next.source.pous.findIndex((p) => p.fbd !== null && p.pouType === 'program');
      return Math.max(0, prog >= 0 ? prog : next.source.pous.findIndex((p) => p.fbd !== null));
    });
  }, [text]);

  const model = session ? (session.history[session.at] as DocModel) : null;
  const fbdPous = useMemo(() => (session ? session.source.pous.map((p, i) => ({ i, p })).filter(({ p }) => p.fbd !== null) : []), [session]);
  const pou = model?.pous[pouIndex] ?? null;
  const pouSource = session?.source.pous[pouIndex] ?? null;
  const own = useMemo(
    () => (session ? ownBoxes(session.source).filter((d) => d.name.toLowerCase() !== (pouSource?.name ?? '').toLowerCase()) : []),
    [session, pouSource]
  );
  const vars: VarInfo[] = useMemo(() => (session && model ? varsOfPou(session.source, model, pouIndex) : []), [session, model, pouIndex]);
  const suggestions = useMemo(() => operands(vars, own), [vars, own]);
  const problems = useMemo(() => (pou ? check(pou, vars, own) : null), [pou, vars, own]);
  const read = useMemo(() => (session ? readFbd(session.text) : null), [session]);
  const drawnPou = read?.ok ? (read.pous.find((p) => p.name === pouSource?.name) ?? null) : null;

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
      if (focus && edited.sel) {
        const s = edited.sel;
        const rootNow = edited.model.pous[s.pou]?.networks[s.net]?.root ?? null;
        const ownNow = ownBoxes(now.source).filter((d) => d.name.toLowerCase() !== (now.source.pous[s.pou]?.name ?? '').toLowerCase());
        const key = firstField(rootNow, s, ownNow);
        if (key) setFocusReq({ key, token: Date.now() });
      }
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
    (instruction: FbdInstruction, where: Sel | null) => {
      if (!sessionRef.current) return;
      const s = where ?? sel;
      const spec = instruction.spec;
      if (spec === 'label') {
        if (!s) {
          setMessage('Select the network the label is for.');
          return;
        }
        document.querySelector<HTMLInputElement>(`[data-testid="fbd-network-label"][data-network="${s.net + 1}"]`)?.focus();
        return;
      }
      if (spec === 'insert-input') {
        commit((m) => addInput(m, s), true);
        return;
      }
      if (spec === 'fbd-branch') {
        commit((m, ids) => insert(m, ids, s, { kind: 'assign' }, own), true);
        return;
      }
      const def = spec.kind === 'box' ? findFbdBox(spec.type, own) : null;
      commit((m, ids) => {
        const edited = insert(m, ids, s, spec, own);
        if (!edited.ok || def?.kind !== 'functionBlock' || edited.sel?.kind !== 'element') return edited;
        // A function block gets an instance, as TIA asks for an instance DB: a name no
        // variable has is proposed in its field, and Enter there declares it.
        const named = setInstance(edited.model, ids, edited.sel.pou, edited.sel.net, edited.sel.id, instanceName(def.name, vars));
        return named.ok ? { ...named, sel: edited.sel } : named;
      }, true);
    },
    [sel, own, vars, commit]
  );

  const all = useMemo(() => allFbdInstructions(own), [own]);
  const placeRef = useRef(place);
  placeRef.current = place;
  const editable = session !== null;
  useEffect(() => {
    if (!editable) return undefined;
    useFbdPalette.setState({ place: (x) => placeRef.current(x as FbdInstruction, null) });
    return () => useFbdPalette.setState({ place: null });
  }, [editable]);
  useEffect(() => {
    useFbdPalette.setState({ own });
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
        return run(() => {
          if (!sel) return;
          const k = firstField(pou.networks[sel.net]?.root ?? null, sel, own);
          if (k) setFocusReq({ key: k, token: Date.now() });
        });
      case 'Alt+ArrowUp':
      case 'Alt+ArrowDown':
        return run(() => sel && commit((m) => moveNetwork(m, pouIndex, sel.net, key === 'Alt+ArrowUp' ? -1 : 1)));
      case 'ArrowUp':
      case 'ArrowDown': {
        const to = Math.max(0, Math.min(pou.networks.length - 1, (sel ? sel.net : -1) + (key === 'ArrowUp' ? -1 : 1)));
        return run(() => pou.networks[to] && setSel({ kind: 'network', pou: pouIndex, net: to }));
      }
      case 'ArrowLeft':
      case 'ArrowRight': {
        const net = sel?.net ?? 0;
        const list = order(pou.networks[net]?.root ?? null);
        const by = key === 'ArrowLeft' ? -1 : 1;
        return run(() => {
          const now = sel?.kind === 'element' ? list.indexOf(sel.id) : sel?.kind === 'pin' ? list.indexOf(sel.box) : by > 0 ? -1 : list.length;
          const next = list[now + by];
          if (next !== undefined) setSel({ kind: 'element', pou: pouIndex, net, id: next });
        });
      }
      default:
        return undefined;
    }
  };

  if (unreadable !== null) {
    return (
      <>
        <div className="notice notice--bad" role="alert" data-testid="fbd-editor-unreadable">
          <p>The diagram cannot be edited: {unreadable}.</p>
        </div>
        {fallback}
      </>
    );
  }
  if (!session || !model || !pou || !pouSource) return <p className="muted">Reading the diagram…</p>;
  if (!pouSource.fbd) {
    return (
      <>
        <p className="muted" data-testid="fbd-editor-no-body">
          The document has no Function Block Diagram body to edit.
        </p>
        {fallback}
      </>
    );
  }

  const isProgram = program !== null ? pou.name.toLowerCase() === program.toLowerCase() : pou.pouType === 'program';
  const drawnOf = session.drawn[pouIndex] ?? [];

  return (
    <div className="ld-editor fbd-editor" data-testid="fbd-editor" tabIndex={0} ref={root} onKeyDown={onKey} aria-label="Function Block Diagram editor">
      <div className="toolbar ld-editor__bar" role="toolbar" aria-label="Favourites" data-testid="fbd-favourites">
        <Favourites kit={FBD_KIT} />
        <span className="toolbar__sep" aria-hidden />
        <button type="button" className="btn" data-testid="fbd-undo" disabled={session.at === 0} onClick={() => travel(-1)} title="Undo (Ctrl+Z)">
          <Undo size={13} aria-hidden /> Undo
        </button>
        <button type="button" className="btn" data-testid="fbd-redo" disabled={session.at >= session.history.length - 1} onClick={() => travel(1)} title="Redo (Ctrl+Y)">
          <Redo size={13} aria-hidden /> Redo
        </button>
        <button
          type="button"
          className="btn"
          data-testid="fbd-add-network"
          onClick={() => commit((m) => addNetwork(m, pouIndex, sel ? sel.net + 1 : pou.networks.length))}
          title="Add a network after the selected one (Ctrl+R)"
        >
          <Plus size={13} aria-hidden /> Network
        </button>
      </div>
      {message && (
        <p className="form__error" role="alert" data-testid="fbd-edit-error">
          {message}
        </p>
      )}
      <div className="ld-editor__body">
        <div className="ld-editor__networks" data-testid="fbd-networks">
          {fbdPous.length > 1 && (
            <div className="inspector__tabs" role="tablist" aria-label="POUs of the document">
              {fbdPous.map(({ i, p }) => (
                <button
                  key={p.name}
                  type="button"
                  role="tab"
                  aria-selected={i === pouIndex}
                  className="inspector__tab"
                  data-testid="fbd-pou-tab"
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
            <p className="muted" data-testid="fbd-pou-not-live">
              A {pou.pouType === 'function' ? 'function' : 'function block'} has no state of its own to draw: each call or instance has its own.
            </p>
          )}
          {pou.networks.length === 0 && (
            <p className="muted" data-testid="fbd-no-networks">
              No network yet. Add one (Ctrl+R).
            </p>
          )}
          {pou.networks.map((net, n) => {
            const drawn = drawnOf.find((d) => d.key === net.key) ?? { key: net.key, ids: [], top: 0, bottom: 0 };
            const mine = sel?.pou === pouIndex && sel.net === n ? sel : null;
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
                sel={mine}
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
                  if (x) place(x as FbdInstruction, s);
                }}
                onHeader={(what, value) =>
                  commit((m, ids) => (what === 'label' ? setLabel(m, ids, pouIndex, n, value) : setHeader(m, ids, pouIndex, n, what, value)))
                }
                onMove={(by) => commit((m) => moveNetwork(m, pouIndex, n, by))}
                onDelete={() => commit((m) => remove(m, { kind: 'network', pou: pouIndex, net: n }))}
                onInsertAbove={() => commit((m) => addNetwork(m, pouIndex, n))}
              >
                {mine && mine.kind !== 'network' && net.root && (
                  <Fields
                    root={net.root}
                    sel={mine}
                    focus={focusReq}
                    onFocused={() => setFocusReq(null)}
                    suggestions={suggestions}
                    vars={vars}
                    own={own}
                    problems={problems}
                    compiler={diagnostics.filter((d) => d.element?.pou === pou.name)}
                    outputTypes={isProgram ? outputTypes : new Map()}
                    onEdit={(f, focus) => commit(f, focus)}
                    onSelect={(s) => setSel(s)}
                    onDeclare={(name, type, section) => commit((m) => declareVariable(m, pouIndex, { name, type, section }, pouSource.vars))}
                    onLeave={backToDiagram}
                  />
                )}
              </NetworkView>
            );
          })}
          <button type="button" className="btn btn--flat" onClick={() => commit((m) => addNetwork(m, pouIndex, pou.networks.length))} data-testid="fbd-add-network-end">
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
  net: Network;
  drawn: Drawn;
  pouIndex: number;
  drawnPou: FbdPou | null;
  live: FbdLive | null;
  diagnostics: Diagnostic[];
  sel: Sel | null;
  problems: Problems | null;
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
  const view = drawnPou && drawn.ids.length > 0 ? networkPou(drawnPou, drawn) : null;
  useLayoutEffect(() => {
    const found = holder.current?.querySelector<SVGSVGElement>('svg[data-testid="fbd-diagram"]') ?? null;
    const now = found ? `${found.getAttribute('viewBox')}|${found.getAttribute('width')}|${found.getAttribute('height')}` : '';
    if (found !== svg) setSvg(found);
    if (now !== box) setBox(now);
  });
  const number = n + 1;
  const tree = net.root;
  const at = new Map<number, { x: number; y: number; w: number; h: number }>();
  for (const e of view?.view.elements ?? []) at.set(e.localId, { x: e.x, y: e.y, ...sizeOf(e) });
  /** What a click on a drawn element selects: a box or an assignment itself, an operand the input it feeds. */
  const selectionOf = (id: number): Sel | null => {
    if (!tree) return null;
    if (findBoxIn(tree, id) || sinksOf(tree).some((s) => s.id === id)) return { kind: 'element', pou: pouIndex, net: n, id };
    const pin = pinOfOperand(tree, id);
    return pin ? { kind: 'pin', pou: pouIndex, net: n, ...pin } : null;
  };
  const selectAt = (target: Element) => {
    const g = target.closest('[data-testid="fbd-element"]');
    const id = g ? Number(g.getAttribute('data-local-id')) : NaN;
    const s = Number.isFinite(id) ? selectionOf(id) : null;
    onSelect(s ?? { kind: 'network', pou: pouIndex, net: n });
  };
  // Where each input can be selected: at a box's pins, and at an assignment that is the whole tree.
  const targets: Array<{ box: number; formal: string; x: number; y: number }> = [];
  if (tree?.t === 'box') {
    for (const b of boxesOf(tree)) {
      const p = at.get(b.id);
      if (p) b.inputs.forEach((pin, i) => targets.push({ box: b.id, formal: pin.formal, x: p.x - 6, y: p.y + 30 + 20 * i }));
    }
  } else if (tree?.t === 'sink') {
    const p = at.get(tree.sink.id);
    if (p) targets.push({ box: tree.sink.id, formal: '', x: p.x - 6, y: p.y + p.h / 2 });
  }
  const selected = sel?.kind === 'element' ? [sel.id] : [];
  const marked = [...(problems?.elements.keys() ?? [])].filter((id) => drawn.ids.includes(id));
  return (
    <section
      className={`ld-network${sel ? ' is-selected' : ''}`}
      data-testid="fbd-network-edit"
      data-network={number}
      data-kept={net.why !== null ? 'true' : undefined}
      aria-label={`Network ${number}${net.title ? `: ${net.title}` : ''}`}
    >
      <header className="ld-network__header">
        <button type="button" className="btn btn--flat ld-network__number" onClick={() => onSelect({ kind: 'network', pou: pouIndex, net: n })} data-testid="fbd-network-select">
          Network {number}
        </button>
        <input
          className="ld-network__title"
          aria-label={`Title of network ${number}`}
          data-testid="fbd-network-title"
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
            data-testid="fbd-network-label"
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
        data-testid="fbd-network-comment"
        placeholder="Comment"
        rows={Math.max(1, comment.split('\n').length)}
        value={comment}
        onChange={(e) => setComment(e.target.value)}
        onBlur={() => comment !== net.comment && onHeader('comment', comment)}
      />
      {net.why !== null && (
        <p className="muted" data-testid="fbd-network-kept">
          Kept as the document has it: {net.why}. It can be moved, deleted, titled, labelled and commented; its elements are written back as they are.
        </p>
      )}
      <div
        className="ld-network__diagram"
        ref={holder}
        data-testid="fbd-network-diagram"
        onClick={(e) => selectAt(e.target as Element)}
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          const g = (e.target as Element).closest('[data-testid="fbd-element"]');
          const id = g ? Number(g.getAttribute('data-local-id')) : NaN;
          onDrop((Number.isFinite(id) ? selectionOf(id) : null) ?? { kind: 'network', pou: pouIndex, net: n });
        }}
      >
        {view ? (
          <FbdPouView pou={view.view} live={live} diagnostics={diagnostics.map((d) => (d.element ? { ...d, element: { ...d.element, y: (d.element.y ?? 0) + view.dy } } : d))} />
        ) : (
          <p className="muted ld-network__empty" data-testid="fbd-network-empty">
            Empty: put a box or an assignment here.
          </p>
        )}
        {svg && view && (
          <svg
            className="ld-overlay"
            data-testid="fbd-overlay"
            viewBox={svg.getAttribute('viewBox') ?? undefined}
            width={svg.getAttribute('width') ?? undefined}
            height={svg.getAttribute('height') ?? undefined}
            aria-hidden
          >
            {selected.map((id) => {
              const p = at.get(id);
              return p ? <rect key={`sel${id}`} data-testid="fbd-selected" data-local-id={id} x={p.x - 6} y={p.y - 6} width={p.w + 12} height={p.h + 12} fill="none" stroke="var(--c-focus)" strokeWidth={2} /> : null;
            })}
            {marked.map((id) => {
              const p = at.get(id);
              return p ? (
                <rect key={`p${id}`} data-testid="fbd-problem" data-local-id={id} x={p.x - 3} y={p.y - 3} width={p.w + 6} height={p.h + 6} fill="none" stroke="var(--c-warn)" strokeDasharray="3 2">
                  <title>{(problems?.elements.get(id) ?? []).join(' ')}</title>
                </rect>
              ) : null;
            })}
            {targets.map((t) => {
              const chosen = sel?.kind === 'pin' && sel.box === t.box && sel.formal === t.formal;
              return (
                <rect
                  key={`t${t.box}:${t.formal}`}
                  className="ld-gap"
                  data-testid="fbd-pin-target"
                  data-box={t.box}
                  data-formal={t.formal}
                  data-selected={chosen ? 'true' : undefined}
                  x={t.x - 4}
                  y={t.y - 4}
                  width={8}
                  height={8}
                  fill={chosen ? 'var(--c-focus)' : 'var(--c-surface)'}
                  stroke="var(--c-text-muted)"
                  onClick={(e) => {
                    e.stopPropagation();
                    onSelect({ kind: 'pin', pou: pouIndex, net: n, box: t.box, formal: t.formal });
                  }}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    onDrop({ kind: 'pin', pou: pouIndex, net: n, box: t.box, formal: t.formal });
                  }}
                >
                  <title>{t.formal === '' ? 'Its input' : `Input ${t.formal}`}</title>
                </rect>
              );
            })}
          </svg>
        )}
      </div>
      {(problems?.networks.get(n) ?? []).map((p) => (
        <p key={p} className="muted" data-testid="fbd-network-problem">
          {p}
        </p>
      ))}
      {children}
    </section>
  );
}

/** The fields of what is selected: a box with its inputs and outputs, or an assignment, jump or return. */
function Fields({
  root,
  sel,
  focus,
  onFocused,
  suggestions,
  vars,
  own,
  problems,
  compiler,
  outputTypes,
  onEdit,
  onSelect,
  onDeclare,
  onLeave,
}: {
  root: Root;
  sel: Exclude<Sel, { kind: 'network' }>;
  focus: { key: FieldKey; token: number } | null;
  onFocused: () => void;
  suggestions: VarInfo[];
  vars: VarInfo[];
  own: BoxDef[];
  problems: Problems | null;
  compiler: Diagnostic[];
  outputTypes: Map<string, string>;
  onEdit: (f: (m: DocModel, ids: Ids) => Edited, focus?: boolean) => void;
  onSelect: (s: Sel) => void;
  onDeclare: (name: string, type: string, section: Section) => void;
  onLeave: () => void;
}) {
  const { pou, net } = sel;
  const token = (key: FieldKey) => (focus?.key === key ? focus.token : 0);
  const undeclared = (text: string) =>
    /^[A-Za-z_]\w*$/.test(text.trim()) && !vars.some((v) => v.name.toLowerCase() === text.trim().toLowerCase()) ? text.trim() : null;
  const offer = (text: string, where: Parameters<typeof declarationFor>[0]) => {
    const name = undeclared(text);
    if (!name) return null;
    return <DeclareOffer name={name} proposed={declarationFor(where)} fixedType={where.instance !== undefined} onDeclare={(type, section) => onDeclare(name, type, section)} />;
  };
  const declareNow = (text: string, where: Parameters<typeof declarationFor>[0]) => {
    const name = undeclared(text);
    const proposed = declarationFor(where);
    if (name && proposed.type !== '') onDeclare(name, proposed.type, proposed.section);
  };
  const said = (id: number) => [
    ...(problems?.elements.get(id) ?? []),
    ...compiler.filter((d) => d.element?.local_id === id).map((d) => `The compiler: ${d.message}`),
  ];
  const told = (id: number) =>
    said(id).map((p) => (
      <p key={p} className="form__error" data-testid="fbd-element-problem" data-local-id={id}>
        {p}
      </p>
    ));

  /** An assignment's variable, a jump's label, or a return. */
  const sinkFields = (s: Sink, outType: string | null) => (
    <span key={s.id} className="ld-pin" data-testid="fbd-sink" data-local-id={s.id} data-kind={s.kind}>
      {s.kind === 'return' ? (
        <span className="mono">RETURN</span>
      ) : (
        <>
          <span className="mono">{s.kind === 'assign' ? (s.negated ? '/=' : '=') : '>>'}</span>
          <OperandField
            label={s.kind === 'assign' ? `Variable assignment ${s.id} writes` : `Label jump ${s.id} goes to`}
            testId="fbd-sink-operand"
            value={s.text}
            suggestions={s.kind === 'assign' ? suggestions : []}
            focusToken={token(sinkKey(s.id))}
            onFocused={onFocused}
            onCommit={(text) => onEdit((m, ids) => setSinkText(m, ids, pou, net, s.id, text))}
            onDeclare={
              s.kind === 'assign'
                ? (text) => {
                    onEdit((m, ids) => setSinkText(m, ids, pou, net, s.id, text));
                    declareNow(text, { assign: outType });
                  }
                : undefined
            }
            onLeave={onLeave}
          />
        </>
      )}
      {s.kind === 'assign' && (
        <label className="muted">
          <input
            type="checkbox"
            data-testid="fbd-sink-negated"
            checked={s.negated}
            onChange={(e) => onEdit((m, ids) => setNegated(m, ids, pou, net, { sink: s.id }, e.target.checked))}
          />{' '}
          negated
        </label>
      )}
      <button type="button" className="icon-btn" aria-label={`Delete ${s.kind} ${s.id}`} title="Delete" onClick={() => onEdit((m) => remove(m, { kind: 'element', pou, net, id: s.id }))}>
        <Trash2 size={12} aria-hidden />
      </button>
      {s.kind === 'assign' && offer(s.text, { assign: outType })}
      {told(s.id)}
    </span>
  );

  // An assignment, jump or return that is the network's whole tree.
  if (root.t === 'sink') {
    const s = root.sink;
    const src = root.src;
    return (
      <div className="ld-fields" data-testid="fbd-fields" data-local-id={s.id}>
        <span className="ld-pin" data-testid="fbd-pin" data-formal="">
          <span className="mono">reads</span>
          <OperandField
            label={`Operand ${s.kind} ${s.id} reads`}
            testId="fbd-pin-operand"
            value={src?.text ?? ''}
            suggestions={suggestions}
            focusToken={token(pinKey(s.id, ''))}
            onFocused={onFocused}
            onCommit={(text) => onEdit((m, ids) => setPinOperand(m, ids, { kind: 'pin', pou, net, box: s.id, formal: '' }, text))}
            onLeave={onLeave}
          />
          {src && offer(src.text, { pinType: s.kind === 'assign' ? '' : 'BOOL' })}
        </span>
        {sinkFields(s, null)}
      </div>
    );
  }
  // A box: the selected one, or the one an input or an assignment of it belongs to.
  const boxId = sel.kind === 'pin' ? sel.box : (findBoxIn(root, sel.id)?.id ?? boxesOf(root).find((b) => b.outputs.some((o) => o.to.some((t) => t.id === sel.id)))?.id);
  const box = boxId !== undefined ? findBoxIn(root, boxId) : null;
  if (!box) return null;
  const def = findFbdBox(box.typeName, own);
  const boxes = [...own, ...FBD_BOXES].map((d) => ({ name: d.name, type: d.kind === 'functionBlock' ? 'function block' : 'function', section: '' }));
  const pinSel = (formal: string): Extract<Sel, { kind: 'pin' }> => ({ kind: 'pin', pou, net, box: box.id, formal });
  return (
    <div className="ld-fields" data-testid="fbd-fields" data-local-id={box.id}>
      <OperandField
        label={`Type of box ${box.id}`}
        testId="fbd-box-type"
        value={box.typeName}
        suggestions={boxes}
        focusToken={token(typeKey(box.id))}
        onFocused={onFocused}
        onCommit={(text) => {
          const d = findFbdBox(text, own);
          onEdit((m, ids) => {
            const typed = setBoxType(m, ids, pou, net, box.id, text, own);
            if (!typed.ok || d?.kind !== 'functionBlock' || box.instance !== '') return typed;
            return setInstance(typed.model, ids, pou, net, box.id, instanceName(d.name, vars));
          });
        }}
        onLeave={onLeave}
      />
      {def?.kind === 'functionBlock' && (
        <OperandField
          label={`Instance of box ${box.id}`}
          testId="fbd-box-instance"
          value={box.instance}
          suggestions={suggestions.filter((v) => v.type.toUpperCase() === def.name.toUpperCase())}
          focusToken={token(instanceKey(box.id))}
          onFocused={onFocused}
          declareOnEnter
          onCommit={(text) => onEdit((m, ids) => setInstance(m, ids, pou, net, box.id, text))}
          onDeclare={(text) => {
            onEdit((m, ids) => setInstance(m, ids, pou, net, box.id, text));
            declareNow(text, { instance: def.name });
          }}
          onLeave={onLeave}
        />
      )}
      {def?.kind === 'functionBlock' && box.instance !== '' && offer(box.instance, { instance: def.name })}
      {box.inputs.map((p) => {
        const chosen = sel.kind === 'pin' && sel.formal === p.formal;
        return (
          <span key={`in${p.formal}`} className={`ld-pin${chosen ? ' is-selected' : ''}`} data-testid="fbd-pin" data-formal={p.formal}>
            <button type="button" className="btn btn--flat mono" onClick={() => onSelect(pinSel(p.formal))} title={`Select ${p.formal}: a box put now goes into it`}>
              {p.formal}
            </button>
            {p.src?.t === 'box' ? (
              <>
                <span className="muted" data-testid="fbd-pin-from">
                  from block {p.src.box.id} ({p.src.box.typeName || '??'}) {p.src.output}
                </span>
                <button type="button" className="btn btn--flat" data-testid="fbd-or-here" title="Join this input with a second signal in an OR box" onClick={() => onEdit((m, ids) => orAt(m, ids, pinSel(p.formal), own), true)}>
                  Join with OR
                </button>
              </>
            ) : (
              <OperandField
                label={`${p.formal} of box ${box.id}`}
                testId="fbd-pin-operand"
                value={p.src?.text ?? ''}
                suggestions={suggestions}
                focusToken={token(pinKey(box.id, p.formal))}
                onFocused={onFocused}
                onCommit={(text) => onEdit((m, ids) => setPinOperand(m, ids, pinSel(p.formal), text))}
                onDeclare={(text) => {
                  onEdit((m, ids) => setPinOperand(m, ids, pinSel(p.formal), text));
                  declareNow(text, { pinType: p.type });
                }}
                onLeave={onLeave}
              />
            )}
            <label className="muted">
              <input
                type="checkbox"
                data-testid="fbd-pin-negated"
                checked={p.negated}
                onChange={(e) => onEdit((m, ids) => setNegated(m, ids, pou, net, { box: box.id, input: p.formal }, e.target.checked))}
              />{' '}
              negated
            </label>
            {p.src?.t === 'operand' && offer(p.src.text, { pinType: p.type })}
          </span>
        );
      })}
      {EXTENSIBLE.has(box.typeName.toUpperCase()) && (
        <button type="button" className="btn btn--flat" data-testid="fbd-add-input" onClick={() => onEdit((m) => addInput(m, { kind: 'element', pou, net, id: box.id }), true)}>
          <Plus size={12} aria-hidden /> Input
        </button>
      )}
      {box.outputs.map((o) => {
        const compiled = outputTypes.get(`${box.id}:${o.formal.toUpperCase()}`) ?? null;
        return (
          <div key={`out${o.formal}`} className="ld-pin" data-testid="fbd-output" data-formal={o.formal}>
            <span className="mono">{o.formal}</span>
            {/* The type is the compiler's (ADR-013 §2.16), never the editor's guess. */}
            <span className="muted" data-testid="fbd-output-type">
              {compiled ?? 'type UNKNOWN until the program compiles'}
            </span>
            <label className="muted">
              <input
                type="checkbox"
                data-testid="fbd-output-negated"
                checked={o.negated}
                onChange={(e) => onEdit((m, ids) => setNegated(m, ids, pou, net, { box: box.id, output: o.formal }, e.target.checked))}
              />{' '}
              negated
            </label>
            {o.to.map((s) => sinkFields(s, compiled ?? (concrete(o.type) ? o.type : null)))}
            <button
              type="button"
              className="btn btn--flat"
              data-testid="fbd-add-assignment"
              title={`Assign ${o.formal} to a variable`}
              onClick={() => onEdit((m, ids) => addSink(m, ids, pou, net, box.id, o.formal), true)}
            >
              <Plus size={12} aria-hidden /> =
            </button>
          </div>
        );
      })}
      {told(box.id)}
    </div>
  );
}
