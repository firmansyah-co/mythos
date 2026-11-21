// A Sequential Function Chart, drawn read-only as SVG from its TC6 document (ADR-013 §2.8,
// §2.17), with IEC 61131-3's graphical forms (§6.7): each step a box with its name (an initial
// step double-framed), each transition a bar across its line with its condition written beside
// it (and its priority where the document gives one), a selection divergence or convergence a
// single line, a simultaneous one a double line, a jump an arrow with the step it goes to, each
// action block beside its step with a row per association (qualifier and duration, the action
// or variable it names or its body), comments; every link between them. Since L3d-3 it is what a
// viewer and an operator get, and the chart editor (SfcEditor.tsx) draws the chart with it.
//
// **Where**: at the document's positions, two and a half times their units, since the steps of a
// chart written by hand are 40 apart (the corpus, the module's template) and a step's name and
// its time need more room than that; each element on an axis 55 right of its x, the middle of a
// step's box, so a step and the transition under it at the same x are on one line.
//
// **Live state** is drawn only from what was watched, and only while the runtime runs the very
// version this text builds in RUN (as for LD and FBD). The compiler keeps every step's flag X and
// time T, every transition's condition with whether it was enabled, and every action's Q
// (`chart` of the check, §2.17), and the view draws, as TIA Portal's sequence monitoring shows
// them:
// - an active step marked active (its frame solid and thick, filled), with its time written in
//   it as the variables table writes a TIME; a step that is not active plain, with no time;
// - a transition's condition TRUE or FALSE only where the transition was enabled in the last scan
//   (its condition was evaluated then); otherwise *not evaluated*, since what its variable holds is
//   an earlier scan's, never shown as this one's;
// - each action's Q in its row;
// - a value that cannot be read is UNKNOWN, never a state: a step whose X cannot be read is drawn
//   UNKNOWN, not inactive; a transition whose enable flag cannot be read is UNKNOWN, not its
//   condition; a forced value is marked FORCED where it is drawn.

import type { ReactNode } from 'react';
import type { ChartProbe, Diagnostic, WatchedValue } from './bridge';
import { actionText, conditionText, type SfcAction, type SfcElement, type SfcPou } from './sfc';
import { formatVariable } from './values';

/** What the view draws live: the watched values by variable in lower case (`null`: they cannot be read), and where each state is kept. */
export interface SfcLive {
  values: Map<string, WatchedValue> | null;
  probes: ChartProbe[];
}

/** A step: `active`, `inactive`, `unknown` (its flag cannot be read), `none` (nothing is drawn live). */
export type StepState = 'active' | 'inactive' | 'unknown' | 'none';
/** A transition's condition in the last scan: `true`, `false`, `not-evaluated` (not enabled), `unknown`, `none`. */
export type ConditionState = 'true' | 'false' | 'not-evaluated' | 'unknown' | 'none';
/** An action's Q: `on`, `off`, `unknown`, `none`. */
export type ActionState = 'on' | 'off' | 'unknown' | 'none';

/** The drawing's scale of the document's units, and the geometry of each symbol in drawn units. */
export const SCALE = 2.5;
const STEP_W = 110;
const STEP_H = 40;
const AXIS = STEP_W / 2;
const TRANSITION_H = 20;
const BAR_W = 30;
const ROW_H = 20;
const DOUBLE = 4;
const CHAR = 7;
/** How much of a condition or an action's body is written in the drawing; all of it is in its title. */
const SHOWN = 40;

const TEXT = { fill: 'var(--c-text)', fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-xs)' } as const;

/** A BOOL read: its value, or UNKNOWN; and whether it is forced. */
interface Bool {
  state: 'on' | 'off' | 'unknown';
  forced: boolean;
}

function readBool(values: Map<string, WatchedValue>, path: string, negated = false): Bool {
  const found = values.get(path.toLowerCase());
  if (!found || found.error !== null || found.type !== 'BOOL' || typeof found.value !== 'boolean') {
    return { state: 'unknown', forced: found?.forced === true };
  }
  const value = negated ? !found.value : found.value;
  return { state: value ? 'on' : 'off', forced: found.forced === true };
}

const probeOf = (live: SfcLive, id: number | null, kind: 'X' | 'T' | 'transition' | 'Q') =>
  id === null
    ? undefined
    : live.probes.find(
        (p) =>
          p.local_id === id &&
          (kind === 'transition' ? p.transition === true : kind === 'Q' ? p.action === 'Q' : p.step === kind)
      );

/** A step's state and, while it is active, its time as text (`UNKNOWN` when it cannot be read). */
export function stepState(live: SfcLive | null, id: number): { state: StepState; time: string | null; forced: boolean } {
  if (live === null) return { state: 'none', time: null, forced: false };
  const x = probeOf(live, id, 'X');
  if (live.values === null || x === undefined) return { state: 'unknown', time: null, forced: false };
  const flag = readBool(live.values, x.variable, x.negated);
  if (flag.state === 'unknown') return { state: 'unknown', time: null, forced: flag.forced };
  if (flag.state === 'off') return { state: 'inactive', time: null, forced: flag.forced };
  const t = probeOf(live, id, 'T');
  const found = t === undefined ? undefined : live.values.get(t.variable.toLowerCase());
  const time =
    found && found.error === null && found.type === 'TIME' ? formatVariable('TIME', found.value) : null;
  return { state: 'active', time: time ?? 'UNKNOWN', forced: flag.forced || found?.forced === true };
}

/**
 * A transition's condition as the last scan evaluated it: TRUE or FALSE only where the transition
 * was enabled (`ran`); not evaluated where it was not, since its variable then holds an earlier
 * scan's value; UNKNOWN where either cannot be read, or the check does not say where it is kept.
 */
export function conditionState(live: SfcLive | null, id: number): { state: ConditionState; forced: boolean } {
  if (live === null) return { state: 'none', forced: false };
  const probe = probeOf(live, id, 'transition');
  if (live.values === null || probe === undefined || probe.ran === undefined) return { state: 'unknown', forced: false };
  const ran = readBool(live.values, probe.ran);
  if (ran.state === 'unknown') return { state: 'unknown', forced: ran.forced };
  if (ran.state === 'off') return { state: 'not-evaluated', forced: ran.forced };
  const held = readBool(live.values, probe.variable, probe.negated);
  const forced = ran.forced || held.forced;
  if (held.state === 'unknown') return { state: 'unknown', forced };
  return { state: held.state === 'on' ? 'true' : 'false', forced };
}

/** An action's Q, by the association's own `localId`. */
export function actionState(live: SfcLive | null, id: number | null): { state: ActionState; forced: boolean } {
  if (live === null) return { state: 'none', forced: false };
  const probe = probeOf(live, id, 'Q');
  if (live.values === null || probe === undefined) return { state: 'unknown', forced: false };
  return readBool(live.values, probe.variable, probe.negated);
}

const shorten = (text: string) => {
  const one = text.replace(/\s+/g, ' ').trim();
  return one.length > SHOWN ? `${one.slice(0, SHOWN - 1)}…` : one;
};
const textWidth = (text: string) => CHAR * text.length;
const qualifierText = (a: SfcAction) => (a.duration ? `${a.qualifier} ${a.duration}` : a.qualifier);

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Where each element is drawn: its box, and the axis its links meet. */
export interface Placed {
  e: SfcElement;
  box: Box;
  axis: number;
  /** An action block's columns. */
  columns?: { qualifier: number; action: number; value: number };
  /**
   * A transition's priority, where it counts: only among the transitions of a selection
   * divergence (Table 61 feature 2b), which is where the compiler reads it (`sfc.rs`).
   */
  priority?: number | null;
}

const isBranch = (e: SfcElement) =>
  e.kind === 'selectionDivergence' ||
  e.kind === 'selectionConvergence' ||
  e.kind === 'simultaneousDivergence' ||
  e.kind === 'simultaneousConvergence';
const isDouble = (e: SfcElement) => e.kind === 'simultaneousDivergence' || e.kind === 'simultaneousConvergence';

/** Every element of a POU placed for drawing; `live` decides whether an action block has its Q column. */
export function place(pou: SfcPou, live: boolean): Map<number, Placed> {
  const out = new Map<number, Placed>();
  for (const e of pou.elements) {
    const x = e.x * SCALE;
    const y = e.y * SCALE;
    const axis = x + AXIS;
    switch (e.kind) {
      case 'step': {
        const w = e.width !== null ? e.width * SCALE : Math.max(STEP_W, textWidth(e.name ?? '') + 24);
        const h = Math.max(STEP_H, e.height !== null ? e.height * SCALE : 0);
        out.set(e.localId, { e, axis, box: { x: axis - w / 2, y, w, h } });
        break;
      }
      case 'transition':
        out.set(e.localId, {
          e,
          axis,
          priority:
            e.priority !== null && pou.elements.find((other) => other.localId === e.from[0])?.kind === 'selectionDivergence'
              ? e.priority
              : null,
          box: { x: axis - BAR_W / 2, y, w: BAR_W + 8 + textWidth(shorten(conditionText(e.condition))), h: TRANSITION_H },
        });
        break;
      case 'jumpStep':
        out.set(e.localId, { e, axis, box: { x: axis - 8, y, w: 20 + textWidth(e.target ?? ''), h: TRANSITION_H } });
        break;
      case 'actionBlock': {
        const qualifier = Math.max(30, ...e.actions.map((a) => textWidth(qualifierText(a)) + 10));
        const action = Math.max(60, ...e.actions.map((a) => textWidth(shorten(actionText(a))) + 10));
        // Room for the longest state; a FORCED mark, rare, is written on past the block.
        const value = live ? textWidth('UNKNOWN') + 10 : 0;
        out.set(e.localId, {
          e,
          axis: x,
          box: { x, y, w: qualifier + action + value, h: ROW_H * Math.max(1, e.actions.length) },
          columns: { qualifier, action, value },
        });
        break;
      }
      case 'comment':
        out.set(e.localId, {
          e,
          axis: x,
          box: {
            x,
            y,
            w: e.width !== null ? e.width * SCALE : Math.max(100, textWidth(e.text ?? '') + 8),
            h: e.height !== null ? e.height * SCALE : 20,
          },
        });
        break;
      default:
        // A divergence or convergence: its line spans its own axis and every element it links.
        out.set(e.localId, { e, axis, box: { x: axis, y, w: 0, h: isDouble(e) ? DOUBLE : 0 } });
        break;
    }
  }
  // The span of each divergence and convergence, once every element is placed.
  for (const p of out.values()) {
    if (!isBranch(p.e)) continue;
    const linked = [
      ...p.e.from,
      ...pou.elements.filter((other) => other.from.includes(p.e.localId)).map((other) => other.localId),
    ];
    const axes = [p.axis, ...linked.flatMap((id) => (out.has(id) ? [(out.get(id) as Placed).axis] : []))];
    const left = Math.min(...axes);
    p.box = { ...p.box, x: left, w: Math.max(...axes) - left };
  }
  return out;
}

const bottomOf = (p: Placed) => (p.e.kind === 'transition' ? p.box.y + TRANSITION_H : p.box.y + p.box.h);

/** The path of the link from `from` to `to`, down from one to the other. */
function linkPath(from: Placed, to: Placed): string {
  const start = isBranch(from.e) ? { x: to.axis, y: from.box.y + from.box.h } : { x: from.axis, y: bottomOf(from) };
  const end = isBranch(to.e) ? { x: from.axis, y: to.box.y } : { x: to.axis, y: to.box.y };
  if (start.x === end.x) return `M${start.x} ${start.y}V${end.y}`;
  const mid = (start.y + end.y) / 2;
  return `M${start.x} ${start.y}V${mid}H${end.x}V${end.y}`;
}

/** The link from a step to its action block: from the side of the step the block is on. */
function actionLinkPath(step: Placed, block: Placed): string {
  const right = block.box.x + block.box.w / 2 >= step.axis;
  const sx = right ? step.box.x + step.box.w : step.box.x;
  const sy = step.box.y + STEP_H / 2;
  const ex = right ? block.box.x : block.box.x + block.box.w;
  const ey = block.box.y + ROW_H / 2;
  if (sy === ey) return `M${sx} ${sy}H${ex}`;
  const mid = (sx + ex) / 2;
  return `M${sx} ${sy}H${mid}V${ey}H${ex}`;
}

const STEP_STROKE: Record<StepState, string> = {
  active: 'var(--c-good)',
  inactive: 'var(--c-text-muted)',
  unknown: 'var(--c-warn)',
  none: 'var(--c-text)',
};
const CONDITION_STROKE: Record<ConditionState, string> = {
  true: 'var(--c-good)',
  false: 'var(--c-text-muted)',
  'not-evaluated': 'var(--c-text-faint)',
  unknown: 'var(--c-warn)',
  none: 'var(--c-text)',
};
/** FALSE dashed, UNKNOWN dotted so it is never read as either; not evaluated faint and sparse. */
const CONDITION_DASH: Record<ConditionState, string | undefined> = {
  true: undefined,
  false: '6 4',
  'not-evaluated': '2 6',
  unknown: '2 3',
  none: undefined,
};
const CONDITION_WORDS: Record<ConditionState, string> = {
  true: 'TRUE',
  false: 'FALSE',
  'not-evaluated': 'not evaluated',
  unknown: 'UNKNOWN',
  none: '',
};
const ACTION_WORDS: Record<ActionState, string> = { on: 'TRUE', off: 'FALSE', unknown: 'UNKNOWN', none: '' };

function Forced() {
  return (
    <tspan data-testid="variable-forced" style={{ fill: 'var(--c-warn)' }}>
      {' FORCED'}
    </tspan>
  );
}

function StepView({ p, live }: { p: Placed; live: SfcLive | null }) {
  const { e, box } = p;
  const read = stepState(live, e.localId);
  const stroke = STEP_STROKE[read.state];
  const status = read.state === 'active' ? `T ${read.time}` : read.state === 'unknown' ? 'UNKNOWN' : '';
  return (
    <g
      data-testid="sfc-element"
      data-kind="step"
      data-local-id={e.localId}
      data-name={e.name ?? ''}
      data-initial={e.initial ? 'true' : undefined}
      data-state={read.state}
      data-forced={read.forced ? 'true' : undefined}
    >
      <title>{`${e.initial ? 'initial step' : 'step'} ${e.name ?? ''} (element ${e.localId})`}</title>
      <rect
        x={box.x}
        y={box.y}
        width={box.w}
        height={box.h}
        fill={read.state === 'active' ? 'var(--c-good-soft)' : 'var(--c-surface)'}
        stroke={stroke}
        strokeWidth={read.state === 'active' ? 3 : 1.5}
        strokeDasharray={read.state === 'unknown' ? '2 3' : undefined}
      />
      {e.initial && (
        <rect data-testid="sfc-initial-frame" x={box.x + 4} y={box.y + 4} width={box.w - 8} height={box.h - 8} fill="none" stroke={stroke} />
      )}
      <text x={p.axis} y={box.y + 17} textAnchor="middle" style={TEXT}>
        {e.name}
      </text>
      {(status !== '' || read.forced) && (
        <text
          data-testid="sfc-step-status"
          x={p.axis}
          y={box.y + 31}
          textAnchor="middle"
          style={{ ...TEXT, fill: read.state === 'unknown' ? 'var(--c-warn)' : 'var(--c-text)' }}
        >
          {read.state === 'active' && <tspan data-testid="sfc-step-time">{status}</tspan>}
          {read.state === 'unknown' && status}
          {read.forced && <Forced />}
        </text>
      )}
    </g>
  );
}

function TransitionView({ p, live }: { p: Placed; live: SfcLive | null }) {
  const { e, box, axis } = p;
  const read = conditionState(live, e.localId);
  const condition = conditionText(e.condition);
  const bar = box.y + TRANSITION_H / 2;
  return (
    <g data-testid="sfc-element" data-kind="transition" data-local-id={e.localId} data-state={read.state} data-forced={read.forced ? 'true' : undefined}>
      <title>{`transition ${e.localId}: ${condition}${
        e.priority !== null && p.priority === null ? ` (priority ${e.priority} not used: the transition follows no selection divergence)` : ''
      }`}</title>
      <line
        data-testid="sfc-transition-bar"
        x1={axis - BAR_W / 2}
        x2={axis + BAR_W / 2}
        y1={bar}
        y2={bar}
        stroke={CONDITION_STROKE[read.state]}
        strokeWidth={read.state === 'true' ? 5 : 3}
        strokeDasharray={CONDITION_DASH[read.state]}
      />
      {p.priority !== null && p.priority !== undefined && (
        <text data-testid="sfc-priority" x={axis - BAR_W / 2 - 6} y={bar + 4} textAnchor="end" style={{ ...TEXT, fill: 'var(--c-text-muted)' }}>
          {e.priority}
        </text>
      )}
      <text data-testid="sfc-condition" x={axis + BAR_W / 2 + 8} y={bar + 4} style={TEXT}>
        {shorten(condition)}
      </text>
      {read.state !== 'none' && (
        <text
          data-testid="sfc-condition-value"
          data-state={read.state}
          x={axis + BAR_W / 2 + 8}
          y={bar + 17}
          style={{ ...TEXT, fill: CONDITION_STROKE[read.state] }}
        >
          {CONDITION_WORDS[read.state]}
          {read.forced && <Forced />}
        </text>
      )}
    </g>
  );
}

function ActionBlockView({ p, live }: { p: Placed; live: SfcLive | null }) {
  const { e, box } = p;
  const columns = p.columns ?? { qualifier: 30, action: 60, value: 0 };
  return (
    <g data-testid="sfc-element" data-kind="actionBlock" data-local-id={e.localId}>
      <title>{`action block ${e.localId}`}</title>
      {e.actions.map((a, i) => {
        const y = box.y + i * ROW_H;
        const read = actionState(live, a.localId);
        const what = actionText(a);
        return (
          <g
            key={a.localId ?? `row${i}`}
            data-testid="sfc-action"
            data-local-id={a.localId ?? undefined}
            data-qualifier={a.qualifier}
            data-state={read.state}
            data-forced={read.forced ? 'true' : undefined}
          >
            <title>{`${qualifierText(a)} ${what}`}</title>
            <rect x={box.x} y={y} width={columns.qualifier} height={ROW_H} fill="var(--c-surface)" stroke="var(--c-text)" />
            <rect
              x={box.x + columns.qualifier}
              y={y}
              width={columns.action}
              height={ROW_H}
              fill={read.state === 'on' ? 'var(--c-good-soft)' : 'var(--c-surface)'}
              stroke="var(--c-text)"
            />
            <text data-testid="sfc-qualifier" x={box.x + columns.qualifier / 2} y={y + 14} textAnchor="middle" style={TEXT}>
              {qualifierText(a)}
            </text>
            <text data-testid="sfc-action-text" x={box.x + columns.qualifier + 5} y={y + 14} style={TEXT}>
              {shorten(what)}
            </text>
            {read.state !== 'none' && (
              <>
                <rect
                  x={box.x + columns.qualifier + columns.action}
                  y={y}
                  width={columns.value}
                  height={ROW_H}
                  fill="var(--c-surface)"
                  stroke="var(--c-text)"
                />
                <text
                  data-testid="sfc-action-value"
                  data-state={read.state}
                  x={box.x + columns.qualifier + columns.action + 5}
                  y={y + 14}
                  style={{ ...TEXT, fill: STEP_STROKE[read.state === 'on' ? 'active' : read.state === 'off' ? 'inactive' : 'unknown'] }}
                >
                  {ACTION_WORDS[read.state]}
                  {read.forced && <Forced />}
                </text>
              </>
            )}
          </g>
        );
      })}
    </g>
  );
}

function ElementView({ p, live }: { p: Placed; live: SfcLive | null }) {
  const { e, box, axis } = p;
  switch (e.kind) {
    case 'step':
      return <StepView p={p} live={live} />;
    case 'transition':
      return <TransitionView p={p} live={live} />;
    case 'actionBlock':
      return <ActionBlockView p={p} live={live} />;
    case 'jumpStep':
      return (
        <g data-testid="sfc-element" data-kind="jumpStep" data-local-id={e.localId} data-target={e.target ?? ''}>
          <title>{`jump to step ${e.target ?? ''}`}</title>
          <path d={`M${axis - 7} ${box.y}H${axis + 7}L${axis} ${box.y + 12}Z`} fill="var(--c-text)" stroke="var(--c-text)" />
          <text x={axis + 12} y={box.y + 11} style={TEXT}>
            {e.target}
          </text>
        </g>
      );
    case 'comment':
      return (
        <g data-testid="sfc-element" data-kind="comment" data-local-id={e.localId}>
          <rect x={box.x} y={box.y} width={box.w} height={box.h} fill="none" stroke="var(--c-text-faint)" strokeDasharray="2 2" />
          <text x={box.x + 4} y={box.y + 14} style={{ ...TEXT, fill: 'var(--c-text-muted)' }}>
            {e.text}
          </text>
        </g>
      );
    default:
      return (
        <g data-testid="sfc-element" data-kind={e.kind} data-local-id={e.localId}>
          <title>{`${e.kind} ${e.localId}`}</title>
          <line x1={box.x} x2={box.x + box.w} y1={box.y} y2={box.y} stroke="var(--c-text)" strokeWidth={1.5} />
          {isDouble(e) && <line x1={box.x} x2={box.x + box.w} y1={box.y + DOUBLE} y2={box.y + DOUBLE} stroke="var(--c-text)" strokeWidth={1.5} />}
        </g>
      );
  }
}

/** Where a diagnostic is drawn: each box it marks, by the `localId` of what is there. */
function marksOf(pou: SfcPou, placed: Map<number, Placed>, d: Diagnostic): Array<{ id: number | null; box: Box }> {
  const at = d.element;
  if (!at || at.pou !== pou.name) return [];
  const rows: Array<{ id: number | null; a: SfcAction; box: Box }> = [];
  for (const p of placed.values()) {
    p.e.actions.forEach((a, i) => rows.push({ id: a.localId, a, box: { x: p.box.x, y: p.box.y + i * ROW_H, w: p.box.w, h: ROW_H } }));
  }
  // The chart's element that holds what the message is about, by its localId: an element of the
  // chart, or the association of a block.
  const holder = (id: number) => {
    const found = placed.get(id);
    if (found) return { id, box: found.box };
    const row = rows.find((r) => r.id === id);
    return row ? { id, box: row.box } : null;
  };
  // In a body of LD or FBD (L3d-1b), `local_id` is the body's own element, never the chart's of
  // that id: the mark goes on what holds the body. Written in the block or the transition:
  if (at.inline !== null && at.inline !== undefined) {
    const found = holder(at.inline);
    return found ? [found] : [];
  }
  // An action or a transition of the POU: at every row or transition that names it.
  const same = (a: string | null | undefined, b: string | null | undefined) => (a ?? '').toLowerCase() === (b ?? '').toLowerCase() && a !== '';
  if (at.action) return rows.filter((r) => same(r.a.reference, at.action)).map((r) => ({ id: r.id, box: r.box }));
  if (at.transition) {
    return [...placed.values()]
      .filter((p) => p.e.condition?.kind === 'reference' && same(p.e.condition.name, at.transition))
      .map((p) => ({ id: p.e.localId, box: p.box }));
  }
  if (at.local_id !== null && at.local_id !== undefined) {
    const id = at.local_id;
    const found = holder(id);
    if (found) return [found];
    // An element of a network drawn in the chart: at the transition it reaches directly.
    const reached = [...placed.values()].filter((p) => p.e.condition?.kind === 'wired' && p.e.condition.from.includes(id));
    if (reached.length > 0) return reached.map((p) => ({ id: p.e.localId, box: p.box }));
    // Where the module says the element is, when this text has no element of that id.
    return [{ id, box: { x: (at.x ?? 0) * SCALE, y: (at.y ?? 0) * SCALE, w: 30, h: 20 } }];
  }
  return [];
}

/** One POU's chart. */
export function SfcPouView({ pou, live, diagnostics }: { pou: SfcPou; live: SfcLive | null; diagnostics: Diagnostic[] }) {
  const placed = place(pou, live !== null);
  const links: ReactNode[] = [];
  for (const p of placed.values()) {
    if (p.e.kind === 'comment') continue;
    for (const id of p.e.from) {
      const from = placed.get(id);
      if (!from) continue;
      const d = p.e.kind === 'actionBlock' ? actionLinkPath(from, p) : linkPath(from, p);
      links.push(
        <path
          key={`${id}>${p.e.localId}`}
          data-testid="sfc-link"
          data-from={id}
          data-to={p.e.localId}
          d={d}
          fill="none"
          stroke="var(--c-text)"
          strokeWidth={1.5}
        />
      );
    }
  }
  const marks = diagnostics.flatMap((d) => marksOf(pou, placed, d).map((m) => ({ ...m, message: d.message })));
  // A mark where the module says an element is lies within the drawing too.
  const boxes = [...[...placed.values()].map((p) => p.box), ...marks.map((m) => m.box)];
  // Room on the right of a transition for its condition's live state, under it.
  const left = Math.min(0, ...boxes.map((b) => b.x)) - 30;
  const right = Math.max(200, ...boxes.map((b) => b.x + b.w)) + 40;
  const top = Math.min(0, ...boxes.map((b) => b.y)) - 20;
  const bottom = Math.max(40, ...boxes.map((b) => b.y + b.h)) + 40;
  return (
    <svg
      data-testid="sfc-diagram"
      data-pou={pou.name}
      role="img"
      aria-label={`Sequential Function Chart of ${pou.name}`}
      viewBox={`${left} ${top} ${right - left} ${bottom - top}`}
      width={right - left}
      height={bottom - top}
    >
      {links}
      {[...placed.values()].map((p) => (
        <ElementView key={p.e.localId} p={p} live={live} />
      ))}
      {marks.map((m, index) => (
        <rect
          key={`${m.id}:${index}`}
          data-testid="sfc-diagnostic"
          data-local-id={m.id ?? undefined}
          x={m.box.x - 4}
          y={m.box.y - 4}
          width={m.box.w + 8}
          height={m.box.h + 8}
          fill="none"
          stroke="var(--c-bad)"
          strokeWidth={2}
        >
          <title>{m.message}</title>
        </rect>
      ))}
    </svg>
  );
}

/** The POU's own actions and transitions, by name with their text, and what the compiler says of each. */
export function NamedBodies({ pou, diagnostics }: { pou: SfcPou; diagnostics: Diagnostic[] }) {
  if (pou.actions.length === 0 && pou.transitions.length === 0) return null;
  const about = (kind: 'action' | 'transition', name: string) =>
    diagnostics.filter((d) => d.element?.pou === pou.name && (d.element?.[kind] ?? '').toLowerCase() === name.toLowerCase());
  const rows = [
    ...pou.actions.map((a) => ({ kind: 'action' as const, ...a })),
    ...pou.transitions.map((t) => ({ kind: 'transition' as const, ...t })),
  ];
  return (
    <dl className="sfc-bodies" data-testid="sfc-bodies">
      {rows.map((row) => (
        <div key={`${row.kind}:${row.name}`} data-testid={`sfc-pou-${row.kind}`} data-name={row.name}>
          <dt>
            {row.kind === 'action' ? 'Action' : 'Transition'} <span className="mono">{row.name}</span>
          </dt>
          <dd>
            <pre className="mono">{row.body.st ?? `Its body is in ${row.body.language ?? 'no language'} and is not shown here.`}</pre>
            {about(row.kind, row.name).map((d, i) => (
              <p key={i} className="form__error" data-testid="sfc-named-diagnostic">
                {d.element?.text?.line ? `Line ${d.element.text.line}: ` : ''}
                {d.message}
              </p>
            ))}
          </dd>
        </div>
      ))}
    </dl>
  );
}

/** Every POU of a document, the PROGRAM's live. */
export function SfcView({
  pous,
  program,
  live,
  diagnostics,
}: {
  pous: SfcPou[];
  /** The PROGRAM's name: the POU whose variables are the runtime's. */
  program: string | null;
  live: SfcLive | null;
  diagnostics: Diagnostic[];
}) {
  if (pous.length === 0) {
    return (
      <p className="muted" data-testid="sfc-empty">
        The document holds no POU.
      </p>
    );
  }
  return (
    <div data-testid="sfc-view">
      {pous.map((pou) => {
        const isProgram = program !== null ? pou.name.toLowerCase() === program.toLowerCase() : pou.pouType === 'program';
        return (
          <section key={pou.name} aria-label={pou.name} data-testid="sfc-pou">
            <h3 className="logic-panel__title">
              {pou.name} <span className="muted">{pou.pouType}</span>
            </h3>
            {!pou.sfc && (
              <p className="muted" data-testid="sfc-pou-other">
                Its body is {pou.language ? `in ${pou.language}` : 'missing'} and is not drawn here.
              </p>
            )}
            {!isProgram && pou.sfc && (
              <p className="muted" data-testid="sfc-pou-not-live">
                A function block has no state of its own to draw: each instance has its own.
              </p>
            )}
            {pou.sfc && <SfcPouView pou={pou} live={isProgram ? live : null} diagnostics={diagnostics} />}
            {pou.notDrawn.length > 0 && (
              <p className="form__error" data-testid="sfc-not-drawn">
                Not drawn: {pou.notDrawn.join('; ')}.
              </p>
            )}
            <NamedBodies pou={pou} diagnostics={diagnostics} />
          </section>
        );
      })}
    </div>
  );
}
