// The Sequential Function Chart editor (ADR-013 §2.8, §2.17, L3d-3), an engineer's; everyone else
// gets the read-only view. It follows the FBD editor (FbdEditor.tsx), which follows the TIA
// Portal's workflow by the owner's decision (2026-11-18); for a chart that is S7-GRAPH's sequence
// editor: a step and a transition put as a pair, alternative and simultaneous branches opened and
// closed at once, jumps, the end of a sequence; a transition's condition and a step's actions
// (qualifier, duration, action) edited where they are selected; the initial step marked; online
// monitoring as the view draws it. IEC 61131-3's symbols (§6.7), never Siemens' artwork.
//
// The document is the model (sfcmodel.ts) written back into the TC6 text after every edit, and
// the chart is drawn by the view (`SfcPouView`) from that text, with the live state the view
// draws: what is on screen is what is saved and what compiles. The program is unsaved until Save,
// as Structured Text is.
//
// Keys, while the editor has the focus (ADR-013 §2.8 says why these):
//   Shift+F2 step and transition · Shift+F3 alternative branch · Shift+F8 simultaneous branch
//   Shift+F9 jump · Shift+F7 an N action of the selected step
//   ↑/↓ the previous or next element of the sequence · ←/→ the branch beside
//   Ctrl+R a new sequence · Enter the first field of the selection · Delete it
//   Ctrl+Z undo · Ctrl+Y or Ctrl+Shift+Z redo · Ctrl+S save

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { ArrowLeft, ArrowRight, Plus, Redo, Trash2, Undo } from 'lucide-react';
import type { Diagnostic } from './bridge';
import { DeclareOffer, OperandField } from './LadderEditor';
import { dragged, Favourites } from './LdInstructions';
import type { Ids, Section, VarInfo } from './ldmodel';
import { readSfc, type SfcPou } from './sfc';
import {
  addAction,
  addChart,
  addJump,
  altBranch,
  check,
  declareVariable,
  endSequence,
  insertPair,
  locate,
  moveBranch,
  remove,
  removeBranch,
  renameStep,
  setAction,
  setCondition,
  setConditionNegated,
  setInitial,
  setJumpTarget,
  simBranch,
  stepOfBlock,
  type Edited,
  type Problems,
  type Sel,
} from './sfcedit';
import { allSfcInstructions, SFC_KIT, useSfcPalette, type SfcInstruction } from './SfcInstructions';
import { itemsOf, open, QUALIFIERS, shapeOf, stepsOf, TIMED, varsOfPou, write, type Assoc, type DocModel, type Item, type PouModel, type Source, type Step, type Trans } from './sfcmodel';
import { NamedBodies, place, SfcPouView, type SfcLive } from './SfcView';

interface Session {
  source: Source;
  ids: Ids;
  history: DocModel[];
  at: number;
  /** The text the model was last written to: what the program holds now. */
  text: string;
  /** Each POU's shape as the editor opened it, for what an online change will do. */
  shapes: string[];
}

const idsOf = (source: Source): Ids => ({ next: source.firstFree, used: new Set() });

function start(text: string, before: Session | null): Session | string {
  const opened = open(text);
  if (!opened.ok) return opened.error;
  const ids = idsOf(opened.source);
  // An id given out in this editor is not given out again, even after a discard.
  if (before) ids.next = Math.max(ids.next, before.ids.next);
  return { source: opened.source, ids, history: [opened.model], at: 0, text, shapes: opened.model.pous.map(shapeOf) };
}

/** The key of the field that takes the focus. */
type FieldKey = string;
const nameKey = (id: number) => `name:${id}`;
const condKey = (id: number) => `cond:${id}`;
const rowKey = (id: number) => `row:${id}`;
const jumpKey = (id: number) => `jump:${id}`;

/** The field the selection takes the focus at on Enter, or a new element at once. */
function firstField(pou: PouModel, sel: Sel | null): FieldKey | null {
  if (sel?.kind !== 'el') return null;
  if (sel.row !== undefined) return rowKey(sel.row);
  const item = locate(pou, sel.id)?.item;
  if (item?.t === 'step') return nameKey(item.step.id);
  if (item?.t === 'trans') return condKey(item.trans.id);
  if (item?.t === 'jump') return jumpKey(item.id);
  return null;
}

/** The element ids of a chart in the order ↑ and ↓ go through them: as drawn, each branch in turn. */
const orderOf = (pou: PouModel): number[] =>
  pou.charts.flatMap((c) => itemsOf(c.items)).map((i) => (i.t === 'step' ? i.step.id : i.t === 'trans' ? i.trans.id : i.t === 'jump' ? i.id : i.div));

export function SfcEditor({
  text,
  onText,
  onSave,
  live,
  program,
  diagnostics,
  fallback,
}: {
  text: string;
  onText: (text: string) => void;
  onSave: () => void;
  /** What is drawn live on the PROGRAM; `null` when nothing is. */
  live: SfcLive | null;
  program: string | null;
  diagnostics: Diagnostic[];
  /** What is shown under the reason when the document cannot be edited: the read-only view. */
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
      if (next.source.pous[at]?.sfc) return at;
      const prog = next.source.pous.findIndex((p) => p.sfc !== null && p.pouType === 'program');
      return Math.max(0, prog >= 0 ? prog : next.source.pous.findIndex((p) => p.sfc !== null));
    });
  }, [text]);

  const model = session ? (session.history[session.at] as DocModel) : null;
  const sfcPous = useMemo(() => (session ? session.source.pous.map((p, i) => ({ i, p })).filter(({ p }) => p.sfc !== null) : []), [session]);
  const pou = model?.pous[pouIndex] ?? null;
  const vars: VarInfo[] = useMemo(() => (session && model ? varsOfPou(session.source, model, pouIndex) : []), [session, model, pouIndex]);
  const problems = useMemo(() => (pou && pou.why === null ? check(pou, vars) : null), [pou, vars]);
  const read = useMemo(() => (session ? readSfc(session.text) : null), [session]);
  const drawnPou = read?.ok ? (read.pous.find((p) => p.name === pou?.name) ?? null) : null;

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
      if (edited.model === now.history[now.at]) {
        if (edited.sel) setSel(edited.sel);
        return;
      }
      const history = [...now.history.slice(0, now.at + 1), edited.model];
      const written = write(now.source, edited.model, now.ids);
      const next: Session = { ...now, history, at: history.length - 1, text: written };
      sessionRef.current = next;
      setSession(next);
      if (edited.sel) setSel(edited.sel);
      if (focus && edited.sel) {
        const key = firstField(edited.model.pous[edited.sel.pou] as PouModel, edited.sel);
        if (key) setFocusReq({ key, token: Date.now() });
      }
      onText(written);
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
      const next: Session = { ...now, at, text: written };
      sessionRef.current = next;
      setSession(next);
      setSel(null);
      setMessage(null);
      onText(written);
    },
    [onText]
  );

  const backToChart = useCallback(() => root.current?.focus(), []);

  const place_ = useCallback(
    (instruction: SfcInstruction, where: Sel | null) => {
      const now = sessionRef.current;
      if (!now) return;
      const s = where ?? sel ?? { kind: 'chart' as const, pou: pouIndex, chart: 0 };
      const spec = instruction.spec;
      if (typeof spec === 'object') return commit((m, ids) => addAction(m, ids, pouIndex, s, spec.action), true);
      switch (spec) {
        case 'pair':
          return commit((m, ids) => insertPair(m, ids, pouIndex, s, vars), true);
        case 'alt':
          return commit((m, ids) => altBranch(m, ids, pouIndex, s, vars), true);
        case 'sim':
          return commit((m, ids) => simBranch(m, ids, pouIndex, s, vars), true);
        case 'jump':
          return commit((m, ids) => addJump(m, ids, pouIndex, s), true);
        case 'end':
          return commit((m, ids) => endSequence(m, ids, pouIndex, s, vars), true);
        case 'chart':
          return commit((m, ids) => addChart(m, ids, pouIndex, vars), true);
        case 'initial':
          if (s.kind !== 'el') return setMessage('Select the step that is to be initial.');
          return commit((m) => setInitial(m, pouIndex, s.row !== undefined ? (stepOfBlock(m.pous[pouIndex] as PouModel, s.row)?.id ?? s.id) : s.id));
        default:
          return undefined;
      }
    },
    [sel, pouIndex, vars, commit]
  );

  const all = useMemo(() => allSfcInstructions(), []);
  const placeRef = useRef(place_);
  placeRef.current = place_;
  const editable = session !== null && pou !== null && pou.sfc && pou.why === null;
  useEffect(() => {
    if (!editable) return undefined;
    useSfcPalette.setState({ place: (x) => placeRef.current(x as SfcInstruction, null) });
    return () => useSfcPalette.setState({ place: null });
  }, [editable]);

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
    if (fav && editable) return run(() => place_(fav, null));
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
        return run(() => editable && commit((m, ids) => addChart(m, ids, pouIndex, vars), true));
      case 'Delete':
        return run(() => commit((m) => remove(m, pouIndex, sel)));
      case 'Enter':
        return run(() => {
          const k = firstField(pou, sel);
          if (k) setFocusReq({ key: k, token: Date.now() });
        });
      case 'ArrowUp':
      case 'ArrowDown': {
        const list = orderOf(pou);
        const now = sel?.kind === 'el' ? list.indexOf(sel.row !== undefined ? (stepOfBlock(pou, sel.row)?.id ?? -1) : sel.id) : -1;
        const to = list[Math.max(0, Math.min(list.length - 1, now + (key === 'ArrowUp' ? -1 : 1)))];
        return run(() => to !== undefined && setSel({ kind: 'el', pou: pouIndex, id: to }));
      }
      case 'ArrowLeft':
      case 'ArrowRight': {
        if (sel?.kind !== 'el') return undefined;
        const loc = locate(pou, sel.id);
        const owner = loc?.owner;
        const to = owner?.branches[(loc?.branch ?? 0) + (key === 'ArrowLeft' ? -1 : 1)]?.[0];
        return run(() => to && setSel({ kind: 'el', pou: pouIndex, id: firstId(to) }));
      }
      default:
        return undefined;
    }
  };

  if (unreadable !== null) {
    return (
      <>
        <div className="notice notice--bad" role="alert" data-testid="sfc-editor-unreadable">
          <p>The chart cannot be edited: {unreadable}.</p>
        </div>
        {fallback}
      </>
    );
  }
  if (!session || !model || !pou) return <p className="muted">Reading the chart…</p>;
  if (!pou.sfc) {
    return (
      <>
        <p className="muted" data-testid="sfc-editor-no-body">
          The document has no Sequential Function Chart body to edit.
        </p>
        {fallback}
      </>
    );
  }

  const isProgram = program !== null ? pou.name.toLowerCase() === program.toLowerCase() : pou.pouType === 'program';
  const shapeChanged = session.shapes[pouIndex] !== undefined && session.shapes[pouIndex] !== shapeOf(pou);
  const mine = diagnostics.filter((d) => d.element?.pou === pou.name);

  return (
    <div className="ld-editor sfc-editor" data-testid="sfc-editor" tabIndex={0} ref={root} onKeyDown={onKey} aria-label="Sequential Function Chart editor">
      <div className="toolbar ld-editor__bar" role="toolbar" aria-label="Favourites" data-testid="sfc-favourites">
        <Favourites kit={SFC_KIT} />
        <span className="toolbar__sep" aria-hidden />
        <button type="button" className="btn" data-testid="sfc-undo" disabled={session.at === 0} onClick={() => travel(-1)} title="Undo (Ctrl+Z)">
          <Undo size={13} aria-hidden /> Undo
        </button>
        <button type="button" className="btn" data-testid="sfc-redo" disabled={session.at >= session.history.length - 1} onClick={() => travel(1)} title="Redo (Ctrl+Y)">
          <Redo size={13} aria-hidden /> Redo
        </button>
      </div>
      {message && (
        <p className="form__error" role="alert" data-testid="sfc-edit-error">
          {message}
        </p>
      )}
      {sfcPous.length > 1 && (
        <div className="inspector__tabs" role="tablist" aria-label="POUs of the document">
          {sfcPous.map(({ i, p }) => (
            <button
              key={p.name}
              type="button"
              role="tab"
              aria-selected={i === pouIndex}
              className="inspector__tab"
              data-testid="sfc-pou-tab"
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
        <p className="muted" data-testid="sfc-pou-not-live">
          A function block has no state of its own to draw: each instance has its own.
        </p>
      )}
      {pou.why !== null ? (
        <>
          <p className="muted" data-testid="sfc-kept">
            Kept as the document has it: {pou.why}. The editor writes this chart back as it is and puts nothing into it.
          </p>
          {drawnPou && <SfcPouView pou={drawnPou} live={isProgram ? live : null} diagnostics={mine} />}
        </>
      ) : (
        <>
          {shapeChanged && (
            <p className="muted" data-testid="sfc-shape-changed">
              Its steps or transitions differ from the chart as this editor opened it: at the next Load the chart starts again from its initial step, every action idle (a
              change of an action, a condition or a position alone carries on where the chart was).
            </p>
          )}
          {pou.charts.length > 1 && (
            <div className="toolbar" data-testid="sfc-charts">
              {pou.charts.map((c, n) => (
                <button
                  key={c.key}
                  type="button"
                  className="btn btn--flat"
                  data-testid="sfc-chart-select"
                  aria-pressed={sel?.kind === 'chart' && sel.chart === n}
                  onClick={() => {
                    setSel({ kind: 'chart', pou: pouIndex, chart: n });
                    root.current?.focus();
                  }}
                >
                  Sequence {n + 1}
                </button>
              ))}
            </div>
          )}
          {drawnPou && (
            <ChartView
              pou={pou}
              drawn={drawnPou}
              pouIndex={pouIndex}
              live={isProgram ? live : null}
              diagnostics={mine}
              sel={sel?.pou === pouIndex ? sel : null}
              problems={problems}
              onSelect={(s) => {
                setSel(s);
                setMessage(null);
                root.current?.focus();
              }}
              onDrop={(s) => {
                const x = dragged.current;
                dragged.current = null;
                if (x) place_(x as SfcInstruction, s);
              }}
            />
          )}
          {sel?.pou === pouIndex && sel.kind === 'el' && (
            <Fields
              pou={pou}
              pouIndex={pouIndex}
              sel={sel}
              vars={vars}
              problems={problems}
              compiler={mine}
              focus={focusReq}
              onFocused={() => setFocusReq(null)}
              onEdit={(f, focus) => commit(f, focus)}
              onSelect={(s) => setSel(s)}
              onDeclare={(name, type, section) => commit((m) => declareVariable(m, pouIndex, { name, type, section }, session.source.pous[pouIndex]?.vars ?? []))}
              onLeave={backToChart}
            />
          )}
          {drawnPou && drawnPou.notDrawn.length > 0 && (
            <p className="form__error" data-testid="sfc-not-drawn">
              Not drawn: {drawnPou.notDrawn.join('; ')}.
            </p>
          )}
          {drawnPou && <NamedBodies pou={drawnPou} diagnostics={mine} />}
          {sel?.kind !== 'el' && (
            <p className="muted" data-testid="sfc-hint">
              Select a step or a transition: a new step and transition, a branch or a jump go after it, and its fields are shown here.
            </p>
          )}
        </>
      )}
    </div>
  );
}

const firstId = (item: Item): number => (item.t === 'step' ? item.step.id : item.t === 'trans' ? item.trans.id : item.t === 'jump' ? item.id : item.div);

/** The chart drawn by the view, with the selection and the editor's own marks over it. */
function ChartView({
  pou,
  drawn,
  pouIndex,
  live,
  diagnostics,
  sel,
  problems,
  onSelect,
  onDrop,
}: {
  pou: PouModel;
  drawn: SfcPou;
  pouIndex: number;
  live: SfcLive | null;
  diagnostics: Diagnostic[];
  sel: Sel | null;
  problems: Problems | null;
  onSelect: (s: Sel) => void;
  onDrop: (s: Sel) => void;
}) {
  const holder = useRef<HTMLDivElement>(null);
  const [svg, setSvg] = useState<SVGSVGElement | null>(null);
  const [box, setBox] = useState('');
  useLayoutEffect(() => {
    const found = holder.current?.querySelector<SVGSVGElement>('svg[data-testid="sfc-diagram"]') ?? null;
    const now = found ? `${found.getAttribute('viewBox')}|${found.getAttribute('width')}|${found.getAttribute('height')}` : '';
    if (found !== svg) setSvg(found);
    if (now !== box) setBox(now);
  });
  const placed = place(drawn, live !== null);
  /** The box of what `id` names: an element, or a row of an action block. */
  const boxOf = (id: number): { x: number; y: number; w: number; h: number } | null => {
    const p = placed.get(id);
    if (p) return p.box;
    for (const b of placed.values()) {
      const i = b.e.actions.findIndex((a) => a.localId === id);
      if (i >= 0) return { x: b.box.x, y: b.box.y + i * 20, w: b.box.w, h: 20 };
    }
    return null;
  };
  /** What a click on the drawing selects: an element, a row of an action block (with its step), or the chart. */
  const selectionAt = (target: Element): Sel => {
    const row = target.closest('[data-testid="sfc-action"]');
    const rowId = row ? Number(row.getAttribute('data-local-id')) : NaN;
    if (Number.isFinite(rowId)) {
      const step = stepOfBlock(pou, rowId);
      if (step) return { kind: 'el', pou: pouIndex, id: step.id, row: rowId };
    }
    const g = target.closest('[data-testid="sfc-element"]');
    const id = g ? Number(g.getAttribute('data-local-id')) : NaN;
    if (Number.isFinite(id)) {
      if (locate(pou, id)) return { kind: 'el', pou: pouIndex, id };
      const step = stepOfBlock(pou, id);
      if (step) return { kind: 'el', pou: pouIndex, id: step.id };
    }
    return { kind: 'chart', pou: pouIndex, chart: 0 };
  };
  const selected: number[] = [];
  if (sel?.kind === 'el') {
    const item = locate(pou, sel.id)?.item;
    // A divergence is selected with its convergence.
    if (item && (item.t === 'sel' || item.t === 'sim')) selected.push(item.div, ...(item.conv === null ? [] : [item.conv]));
    else selected.push(sel.id);
    if (sel.row !== undefined) selected.push(sel.row);
  }
  const marked = [...(problems?.elements.keys() ?? [])];
  return (
    <div
      className="ld-network__diagram sfc-editor__chart"
      ref={holder}
      data-testid="sfc-editor-chart"
      onClick={(e) => onSelect(selectionAt(e.target as Element))}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        onDrop(selectionAt(e.target as Element));
      }}
    >
      <SfcPouView pou={drawn} live={live} diagnostics={diagnostics} />
      {svg && (
        <svg
          className="ld-overlay"
          data-testid="sfc-overlay"
          viewBox={svg.getAttribute('viewBox') ?? undefined}
          width={svg.getAttribute('width') ?? undefined}
          height={svg.getAttribute('height') ?? undefined}
          aria-hidden
        >
          {selected.map((id) => {
            const p = boxOf(id);
            return p ? (
              <rect key={`sel${id}`} data-testid="sfc-selected" data-local-id={id} x={p.x - 6} y={p.y - 6} width={p.w + 12} height={p.h + 12} fill="none" stroke="var(--c-focus)" strokeWidth={2} />
            ) : null;
          })}
          {marked.map((id) => {
            const p = boxOf(id);
            return p ? (
              <rect key={`p${id}`} data-testid="sfc-problem" data-local-id={id} x={p.x - 3} y={p.y - 3} width={p.w + 6} height={p.h + 6} fill="none" stroke="var(--c-warn)" strokeDasharray="3 2">
                <title>{(problems?.elements.get(id) ?? []).join(' ')}</title>
              </rect>
            ) : null;
          })}
        </svg>
      )}
    </div>
  );
}

/** A text committed on Enter or when the field is left; Escape goes back to the chart without it. */
function TextArea({ label, testId, value, focusToken, onFocused, onCommit, onLeave }: {
  label: string;
  testId: string;
  value: string;
  focusToken: number;
  onFocused: () => void;
  onCommit: (text: string) => void;
  onLeave: () => void;
}) {
  const [draft, setDraft] = useState(value);
  const area = useRef<HTMLTextAreaElement>(null);
  // What the field holds now: an Enter right after a keystroke takes it, never an older render's.
  const latest = useRef(value);
  useEffect(() => {
    setDraft(value);
    latest.current = value;
  }, [value]);
  useEffect(() => {
    if (focusToken > 0) {
      area.current?.focus();
      area.current?.select();
      onFocused();
    }
  }, [focusToken]);
  return (
    <textarea
      ref={area}
      className="mono sfc-text"
      aria-label={label}
      data-testid={testId}
      rows={Math.max(1, draft.split('\n').length)}
      value={draft}
      onChange={(e) => {
        latest.current = e.target.value;
        setDraft(e.target.value);
      }}
      onBlur={() => latest.current !== value && onCommit(latest.current)}
      onKeyDown={(e) => {
        e.stopPropagation();
        // Enter takes the text; Shift+Enter is a new line of it.
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          const text = latest.current;
          // Taken once: the blur that leaving the field causes gives nothing again.
          latest.current = value === text ? value : text;
          if (text !== value) onCommit(text);
          onLeave();
        }
        if (e.key === 'Escape') {
          e.preventDefault();
          latest.current = value;
          setDraft(value);
          onLeave();
        }
      }}
    />
  );
}

/** The fields of the selection: a step with its actions, a transition's condition, a jump's target, a divergence. */
function Fields({
  pou,
  pouIndex,
  sel,
  vars,
  problems,
  compiler,
  focus,
  onFocused,
  onEdit,
  onSelect,
  onDeclare,
  onLeave,
}: {
  pou: PouModel;
  pouIndex: number;
  sel: Extract<Sel, { kind: 'el' }>;
  vars: VarInfo[];
  problems: Problems | null;
  compiler: Diagnostic[];
  focus: { key: FieldKey; token: number } | null;
  onFocused: () => void;
  onEdit: (f: (m: DocModel, ids: Ids) => Edited, focus?: boolean) => void;
  onSelect: (s: Sel) => void;
  onDeclare: (name: string, type: string, section: Section) => void;
  onLeave: () => void;
}) {
  const loc = locate(pou, sel.id);
  if (!loc) return null;
  const pi = pouIndex;
  const token = (key: FieldKey) => (focus?.key === key ? focus.token : 0);
  const said = (id: number) => [
    ...(problems?.elements.get(id) ?? []),
    ...compiler.filter((d) => d.element?.local_id === id || d.element?.inline === id).map((d) => `The compiler: ${d.message}`),
  ];
  const told = (id: number) =>
    said(id).map((p) => (
      <p key={p} className="form__error" data-testid="sfc-element-problem" data-local-id={id}>
        {p}
      </p>
    ));
  const item = loc.item;
  const branchTools = loc.owner ? (
    <span className="sfc-fields__branch" data-testid="sfc-branch-tools">
      <span className="muted">
        Branch {loc.branch + 1} of {loc.owner.branches.length} ({loc.owner.t === 'sel' ? 'alternative: tested left to right' : 'simultaneous'})
      </span>
      <button type="button" className="icon-btn" aria-label="Move the branch left" title="Move the branch left" disabled={loc.branch === 0} onClick={() => onEdit((m) => moveBranch(m, pi, sel, -1))}>
        <ArrowLeft size={12} aria-hidden />
      </button>
      <button
        type="button"
        className="icon-btn"
        aria-label="Move the branch right"
        title="Move the branch right"
        disabled={loc.branch === loc.owner.branches.length - 1}
        onClick={() => onEdit((m) => moveBranch(m, pi, sel, 1))}
      >
        <ArrowRight size={12} aria-hidden />
      </button>
      <button type="button" className="btn btn--flat" data-testid="sfc-delete-branch" onClick={() => onEdit((m) => removeBranch(m, pi, sel))}>
        <Trash2 size={12} aria-hidden /> Delete the branch
      </button>
    </span>
  ) : null;
  const deleteButton = (what: string) => (
    <button type="button" className="btn btn--flat" data-testid="sfc-delete" onClick={() => onEdit((m) => remove(m, pi, { kind: 'el', pou: pi, id: sel.id }))} title="Delete (Delete)">
      <Trash2 size={12} aria-hidden /> Delete {what}
    </button>
  );
  if (item.t === 'step') {
    return (
      <div className="ld-fields sfc-fields" data-testid="sfc-fields" data-kind="step" data-local-id={item.step.id}>
        <StepFields
          step={item.step}
          pou={pou}
          pi={pi}
          sel={sel}
          vars={vars}
          token={token}
          onFocused={onFocused}
          onEdit={onEdit}
          onSelect={onSelect}
          onDeclare={onDeclare}
          onLeave={onLeave}
          told={told}
        />
        {deleteButton('the step')}
        {branchTools}
        {told(item.step.id)}
      </div>
    );
  }
  if (item.t === 'trans') {
    return (
      <div className="ld-fields sfc-fields" data-testid="sfc-fields" data-kind="transition" data-local-id={item.trans.id}>
        <TransFields trans={item.trans} pou={pou} pi={pi} token={token} onFocused={onFocused} onEdit={onEdit} onLeave={onLeave} />
        {deleteButton('the transition')}
        {branchTools}
        {told(item.trans.id)}
      </div>
    );
  }
  if (item.t === 'jump') {
    const steps = pou.charts.flatMap((c) => stepsOf(c.items)).map((s) => s.name);
    return (
      <div className="ld-fields sfc-fields" data-testid="sfc-fields" data-kind="jump" data-local-id={item.id}>
        <label className="ld-pin">
          <span>Jump to step</span>
          <JumpTarget id={item.id} value={item.target} steps={steps} focusToken={token(jumpKey(item.id))} onFocused={onFocused} onChange={(to) => onEdit((m) => setJumpTarget(m, pi, item.id, to))} />
        </label>
        {deleteButton('the jump')}
        {branchTools}
        {told(item.id)}
      </div>
    );
  }
  return (
    <div className="ld-fields sfc-fields" data-testid="sfc-fields" data-kind={item.t === 'sel' ? 'selectionDivergence' : 'simultaneousDivergence'} data-local-id={item.div}>
      <span>
        {item.t === 'sel' ? 'Alternative branches (selection)' : 'Simultaneous branches'}: {item.branches.length}
      </span>
      <button
        type="button"
        className="btn btn--flat"
        data-testid="sfc-add-branch"
        onClick={() => onEdit((m, ids) => (item.t === 'sel' ? altBranch(m, ids, pi, sel, vars) : simBranch(m, ids, pi, sel, vars)), true)}
      >
        <Plus size={12} aria-hidden /> Branch
      </button>
      <button type="button" className="btn btn--flat" data-testid="sfc-delete" onClick={() => onEdit((m) => remove(m, pi, sel))} title="Keeps the first branch that meets the convergence; deletes the others">
        <Trash2 size={12} aria-hidden /> Delete the other branches
      </button>
      {branchTools}
      {told(item.div)}
    </div>
  );
}

function JumpTarget({ id, value, steps, focusToken, onFocused, onChange }: { id: number; value: string; steps: string[]; focusToken: number; onFocused: () => void; onChange: (to: string) => void }) {
  const ref = useRef<HTMLSelectElement>(null);
  useEffect(() => {
    if (focusToken > 0) {
      ref.current?.focus();
      onFocused();
    }
  }, [focusToken]);
  const known = steps.some((s) => s.toLowerCase() === value.toLowerCase());
  return (
    <select ref={ref} aria-label={`Step jump ${id} goes to`} data-testid="sfc-jump-target" value={value} onChange={(e) => onChange(e.target.value)}>
      {!known && <option value={value}>{value || '(none)'}</option>}
      {steps.map((s) => (
        <option key={s} value={s}>
          {s}
        </option>
      ))}
    </select>
  );
}

function TransFields({
  trans,
  pou,
  pi,
  token,
  onFocused,
  onEdit,
  onLeave,
}: {
  trans: Trans;
  pou: PouModel;
  pi: number;
  token: (key: FieldKey) => number;
  onFocused: () => void;
  onEdit: (f: (m: DocModel, ids: Ids) => Edited, focus?: boolean) => void;
  onLeave: () => void;
}) {
  const id = trans.id;
  return (
    <>
      <span className="ld-pin">
        <span>Condition</span>
        {(trans.kind === 'st' || trans.kind === 'reference' || trans.kind === 'none') && pou.transitions.length > 0 && (
          <select
            aria-label={`What the condition of transition ${id} is`}
            data-testid="sfc-condition-kind"
            value={trans.kind === 'reference' ? 'reference' : 'st'}
            onChange={(e) => onEdit((m) => setCondition(m, pi, id, e.target.value as 'st' | 'reference', e.target.value === 'reference' ? (pou.transitions[0] ?? '') : 'FALSE'))}
          >
            <option value="st">ST expression</option>
            <option value="reference">Transition of the POU</option>
          </select>
        )}
      </span>
      {trans.kind === 'reference' ? (
        <select aria-label={`Transition of the POU transition ${id} takes`} data-testid="sfc-condition-reference" value={trans.text} onChange={(e) => onEdit((m) => setCondition(m, pi, id, 'reference', e.target.value))}>
          {!pou.transitions.some((t) => t.toLowerCase() === trans.text.toLowerCase()) && <option value={trans.text}>{trans.text}</option>}
          {pou.transitions.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
      ) : trans.kind === 'kept' ? (
        <span className="ld-pin">
          <span className="muted" data-testid="sfc-condition-kept">
            In {trans.text}, kept as the document has it (this editor does not draw or change a body in {trans.text}).
          </span>
          <button type="button" className="btn btn--flat" data-testid="sfc-condition-to-st" onClick={() => onEdit((m) => setCondition(m, pi, id, 'st', 'FALSE'), true)}>
            Replace with an ST condition
          </button>
        </span>
      ) : trans.kind === 'wired' ? (
        <span className="muted" data-testid="sfc-condition-kept">
          A network of LD or FBD drawn in the chart gives this condition; it is kept as the document has it and moves with its transition.
        </span>
      ) : (
        <TextArea
          label={`Condition of transition ${id}`}
          testId="sfc-condition"
          value={trans.text}
          focusToken={token(condKey(id))}
          onFocused={onFocused}
          onCommit={(text) => onEdit((m) => setCondition(m, pi, id, 'st', text))}
          onLeave={onLeave}
        />
      )}
      <label className="muted">
        <input type="checkbox" data-testid="sfc-condition-negated" checked={trans.negated} onChange={(e) => onEdit((m) => setConditionNegated(m, pi, id, e.target.checked))} /> negated
      </label>
    </>
  );
}

function StepFields({
  step,
  pou,
  pi,
  sel,
  vars,
  token,
  onFocused,
  onEdit,
  onSelect,
  onDeclare,
  onLeave,
  told,
}: {
  step: Step;
  pou: PouModel;
  pi: number;
  sel: Extract<Sel, { kind: 'el' }>;
  vars: VarInfo[];
  token: (key: FieldKey) => number;
  onFocused: () => void;
  onEdit: (f: (m: DocModel, ids: Ids) => Edited, focus?: boolean) => void;
  onSelect: (s: Sel) => void;
  onDeclare: (name: string, type: string, section: Section) => void;
  onLeave: () => void;
  told: (id: number) => ReactNode[];
}) {
  // An association names an action of the POU or a BOOL variable (Table 56 feature 1).
  const targets: VarInfo[] = [
    ...pou.actions.map((a) => ({ name: a, type: 'action', section: '' })),
    ...vars.filter((v) => v.type.toUpperCase() === 'BOOL'),
  ];
  const undeclared = (a: Assoc) =>
    a.kind === 'reference' &&
    /^[A-Za-z_]\w*$/.test(a.text) &&
    !vars.some((v) => v.name.toLowerCase() === a.text.toLowerCase()) &&
    !pou.actions.some((n) => n.toLowerCase() === a.text.toLowerCase());
  return (
    <>
      <span className="ld-pin">
        <span>Step</span>
        <OperandField
          label={`Name of step ${step.id}`}
          testId="sfc-step-name"
          value={step.name}
          suggestions={[]}
          focusToken={token(nameKey(step.id))}
          onFocused={onFocused}
          onCommit={(text) => onEdit((m) => renameStep(m, pi, step.id, text, vars))}
          onLeave={onLeave}
        />
      </span>
      <label className="muted">
        <input type="checkbox" data-testid="sfc-step-initial" checked={step.initial} disabled={step.initial} onChange={() => onEdit((m) => setInitial(m, pi, step.id))} title="A chart has one initial step: make another step initial to move it" />{' '}
        initial step
      </label>
      <table className="sfc-actions" data-testid="sfc-action-table" aria-label={`Actions of step ${step.name}`}>
        <thead>
          <tr>
            <th scope="col">Qualifier</th>
            <th scope="col">Duration</th>
            <th scope="col">Action</th>
            <th scope="col" aria-label="Delete" />
          </tr>
        </thead>
        <tbody>
          {step.actions.map((a) => (
            <tr
              key={a.id}
              data-testid="sfc-action-row"
              data-local-id={a.id}
              className={sel.row === a.id ? 'is-selected' : undefined}
              onClick={() => sel.row !== a.id && onSelect({ kind: 'el', pou: pi, id: step.id, row: a.id })}
            >
              <td>
                <select aria-label={`Qualifier of action ${a.id}`} data-testid="sfc-action-qualifier" value={a.qualifier} onChange={(e) => onEdit((m) => setAction(m, pi, a.id, { qualifier: e.target.value }))}>
                  {!(QUALIFIERS as readonly string[]).includes(a.qualifier) && <option value={a.qualifier}>{a.qualifier} (refused)</option>}
                  {QUALIFIERS.map((q) => (
                    <option key={q} value={q}>
                      {q}
                    </option>
                  ))}
                </select>
              </td>
              <td>
                {TIMED.has(a.qualifier) || a.duration !== '' ? (
                  <OperandField
                    label={`Duration of action ${a.id}`}
                    testId="sfc-action-duration"
                    value={a.duration}
                    suggestions={[]}
                    focusToken={0}
                    onCommit={(text) => onEdit((m) => setAction(m, pi, a.id, { duration: text }))}
                    onLeave={onLeave}
                  />
                ) : (
                  <span className="muted">none</span>
                )}
              </td>
              <td>
                {a.kind === 'kept' ? (
                  <span className="muted" data-testid="sfc-action-kept">
                    In {a.text}, kept as the document has it.
                  </span>
                ) : (
                  <span className="ld-pin">
                    <select
                      aria-label={`What action ${a.id} runs`}
                      data-testid="sfc-action-kind"
                      value={a.kind}
                      onChange={(e) => onEdit((m) => setAction(m, pi, a.id, { kind: e.target.value as 'reference' | 'st' }), true)}
                    >
                      <option value="reference">Action or BOOL variable</option>
                      <option value="st">ST statements</option>
                    </select>
                    {a.kind === 'reference' ? (
                      <OperandField
                        label={`What action ${a.id} names`}
                        testId="sfc-action-name"
                        value={a.text}
                        suggestions={targets}
                        focusToken={token(rowKey(a.id))}
                        onFocused={onFocused}
                        onCommit={(text) => onEdit((m) => setAction(m, pi, a.id, { text }))}
                        onDeclare={(text) => {
                          onEdit((m) => setAction(m, pi, a.id, { text }));
                          if (/^[A-Za-z_]\w*$/.test(text) && !vars.some((v) => v.name.toLowerCase() === text.toLowerCase())) onDeclare(text, 'BOOL', 'outputVars');
                        }}
                        onLeave={onLeave}
                      />
                    ) : (
                      <TextArea
                        label={`Statements of action ${a.id}`}
                        testId="sfc-action-st"
                        value={a.text}
                        focusToken={token(rowKey(a.id))}
                        onFocused={onFocused}
                        onCommit={(text) => onEdit((m) => setAction(m, pi, a.id, { text }))}
                        onLeave={onLeave}
                      />
                    )}
                  </span>
                )}
                {undeclared(a) && <DeclareOffer name={a.text} proposed={{ type: 'BOOL', section: 'outputVars' }} fixedType onDeclare={(type, section) => onDeclare(a.text, type, section)} />}
                {told(a.id)}
              </td>
              <td>
                <button type="button" className="icon-btn" aria-label={`Delete action ${a.id}`} title="Delete the action" onClick={() => onEdit((m) => remove(m, pi, { kind: 'el', pou: pi, id: step.id, row: a.id }))}>
                  <Trash2 size={12} aria-hidden />
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <button type="button" className="btn btn--flat" data-testid="sfc-add-action" onClick={() => onEdit((m, ids) => addAction(m, ids, pi, { kind: 'el', pou: pi, id: step.id }), true)}>
        <Plus size={12} aria-hidden /> Action
      </button>
    </>
  );
}
