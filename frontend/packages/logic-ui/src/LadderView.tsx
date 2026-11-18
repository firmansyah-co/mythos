// A Ladder Diagram, drawn read-only as SVG from its TC6 document (ADR-013 §2.8, §2.15): the
// power rails, each rung numbered in the order it is evaluated, contacts, coils, blocks with
// their pins, variables, jumps, labels, returns and comments, where the document places them.
// Editing it is the next slice (L3b-2).
//
// **Live state** is drawn only from what was watched, and only while the runtime runs the very
// version this text builds (the check's version equals the runtime's) in RUN:
// - a normally open or negated contact: its variable's value, negated when it is negated
//   (a transition-sensing contact conducts for one scan only, which its variable does not say,
//   so it is not coloured);
// - a coil: its variable's value;
// - a link: the power its source element's output carried, where the logic module says which
//   variable holds it once a scan is over (`power` of the check); any other link has no state
//   anything holds, and is drawn without one.
// A value that cannot be read is UNKNOWN, never a state; a forced variable is marked FORCED.

import type { ReactNode } from 'react';
import type { Diagnostic, PowerProbe, WatchedValue } from './bridge';
import { networks, type LdElement, type LdPou } from './ladder';
import { formatVariable } from './values';

/** `on`/`off`: a state read; `unknown`: one should be read and cannot be; `none`: none is claimed. */
export type LiveState = 'on' | 'off' | 'unknown' | 'none';

/**
 * What the view draws live: the watched values by variable in lower case, or `null` when
 * they cannot be read; `null` as a whole when nothing live is drawn.
 */
export interface LadderLive {
  values: Map<string, WatchedValue> | null;
  power: PowerProbe[];
}

const STROKE: Record<LiveState, string> = {
  on: 'var(--c-good)',
  off: 'var(--c-text-muted)',
  unknown: 'var(--c-warn)',
  none: 'var(--c-text)',
};
const TEXT = { fill: 'var(--c-text)', fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-xs)' } as const;

interface Point {
  x: number;
  y: number;
}

/** A BOOL variable's state as watched, with whether it is forced. */
function boolOf(live: LadderLive | null, path: string | null): { state: LiveState; forced: boolean } {
  if (live === null || path === null || path === '') return { state: 'none', forced: false };
  if (live.values === null) return { state: 'unknown', forced: false };
  const found = live.values.get(path.toLowerCase());
  if (!found || found.error !== null || found.type !== 'BOOL' || typeof found.value !== 'boolean') {
    return { state: 'unknown', forced: false };
  }
  return { state: found.value ? 'on' : 'off', forced: found.forced === true };
}

const not = (state: LiveState): LiveState => (state === 'on' ? 'off' : state === 'off' ? 'on' : state);

/** The state of an element's symbol: a contact's or a coil's; `none` for the others. */
export function elementState(e: LdElement, live: LadderLive | null): { state: LiveState; forced: boolean } {
  if (e.kind === 'coil') return boolOf(live, e.variable);
  if (e.kind === 'contact') {
    if (e.edge !== null) return { state: 'none', forced: boolOf(live, e.variable).forced };
    const read = boolOf(live, e.variable);
    return { ...read, state: e.negated ? not(read.state) : read.state };
  }
  return { state: 'none', forced: false };
}

/**
 * The power a link carries: that of output `output` (a block's formal parameter in upper case,
 * `''` for any other element's one output) of element `from`, where a variable holds it.
 */
export function linkState(from: number, output: string, live: LadderLive | null): LiveState {
  if (live === null) return 'none';
  const probe = live.power.find((p) => p.local_id === from && p.output === output);
  if (!probe) return 'none';
  if (probe.on === true) return 'on';
  const read = boolOf(live, probe.variable ?? null);
  return probe.negated ? not(read.state) : read.state;
}

/** The size of an element: the document's, or one that holds what is drawn in it. */
function sizeOf(e: LdElement, body: { top: number; bottom: number }): { w: number; h: number } {
  const given = (w: number, h: number) => ({ w: e.width ?? w, h: e.height ?? h });
  switch (e.kind) {
    case 'leftPowerRail':
    case 'rightPowerRail':
      return { w: e.width ?? 3, h: e.height ?? Math.max(20, body.bottom - e.y) };
    case 'contact':
    case 'coil':
      return given(30, 20);
    case 'block':
      return given(100, 20 + 20 * Math.max(1, e.inputs.length, e.outputs.length));
    case 'inVariable':
    case 'outVariable':
    case 'inOutVariable':
      return given(Math.max(60, 8 * (e.variable ?? '').length + 16), 20);
    case 'comment':
      return given(Math.max(100, 7 * (e.text ?? '').length), 20);
    default:
      return given(80, 20);
  }
}

function Contact({ e, w, h, stroke }: { e: LdElement; w: number; h: number; stroke: string }) {
  const [a, b, m] = [e.x + w / 3, e.x + (2 * w) / 3, e.y + h / 2];
  const mark = e.edge === 'rising' ? 'P' : e.edge === 'falling' ? 'N' : null;
  return (
    <>
      <path d={`M${e.x} ${m}H${a}M${a} ${e.y}V${e.y + h}M${b} ${e.y}V${e.y + h}M${b} ${m}H${e.x + w}`} fill="none" stroke={stroke} strokeWidth={2} />
      {e.negated && <path d={`M${a + 2} ${e.y + h - 2}L${b - 2} ${e.y + 2}`} stroke={stroke} strokeWidth={1.5} />}
      {mark && (
        <text x={e.x + w / 2} y={m + 4} textAnchor="middle" style={TEXT}>
          {mark}
        </text>
      )}
    </>
  );
}

function CoilSymbol({ e, w, h, stroke }: { e: LdElement; w: number; h: number; stroke: string }) {
  const [a, b, m, r] = [e.x + w / 3, e.x + (2 * w) / 3, e.y + h / 2, h / 2];
  const mark =
    e.storage === 'set' ? 'S' : e.storage === 'reset' ? 'R' : e.edge === 'rising' ? 'P' : e.edge === 'falling' ? 'N' : null;
  return (
    <>
      <path
        d={`M${e.x} ${m}H${a}M${a + r / 2} ${e.y}A${r} ${r} 0 0 0 ${a + r / 2} ${e.y + h}M${b - r / 2} ${e.y}A${r} ${r} 0 0 1 ${b - r / 2} ${e.y + h}M${b} ${m}H${e.x + w}`}
        fill="none"
        stroke={stroke}
        strokeWidth={2}
      />
      {e.negated && <path d={`M${a + 3} ${e.y + h - 3}L${b - 3} ${e.y + 3}`} stroke={stroke} strokeWidth={1.5} />}
      {mark && (
        <text x={e.x + w / 2} y={m + 4} textAnchor="middle" style={TEXT}>
          {mark}
        </text>
      )}
    </>
  );
}

/** One element: its symbol, its text, its live state and value. */
function ElementView({
  e,
  w,
  h,
  live,
}: {
  e: LdElement;
  w: number;
  h: number;
  live: LadderLive | null;
}) {
  const { state, forced } = elementState(e, live);
  const stroke = STROKE[state];
  const dash = state === 'unknown' ? '4 3' : undefined;
  let symbol: ReactNode = null;
  let caption: string | null = null;
  switch (e.kind) {
    case 'leftPowerRail':
    case 'rightPowerRail':
      symbol = <rect x={e.x} y={e.y} width={w} height={h} fill="var(--c-text)" />;
      break;
    case 'contact':
      symbol = <Contact e={e} w={w} h={h} stroke={stroke} />;
      caption = e.variable;
      break;
    case 'coil':
      symbol = <CoilSymbol e={e} w={w} h={h} stroke={stroke} />;
      caption = e.variable;
      break;
    case 'block':
      symbol = (
        <>
          <rect x={e.x} y={e.y} width={w} height={h} fill="var(--c-surface)" stroke="var(--c-text)" />
          <text x={e.x + w / 2} y={e.y + 14} textAnchor="middle" style={TEXT}>
            {e.typeName}
          </text>
          {e.inputs.map((pin, i) => (
            <text key={`in${i}`} x={e.x + 4} y={e.y + 34 + 20 * i} style={TEXT}>
              {pin.negated ? `¬${pin.formal ?? ''}` : pin.formal}
              {pin.expression !== null && pin.connections.length === 0 ? ` := ${pin.expression}` : ''}
            </text>
          ))}
          {e.outputs.map((pin, i) => (
            <text key={`out${i}`} x={e.x + w - 4} y={e.y + 34 + 20 * i} textAnchor="end" style={TEXT}>
              {pin.negated ? `¬${pin.formal}` : pin.formal}
            </text>
          ))}
        </>
      );
      caption = e.instanceName;
      break;
    case 'inVariable':
    case 'outVariable':
    case 'inOutVariable':
      symbol = (
        <>
          <rect x={e.x} y={e.y} width={w} height={h} rx={8} fill="var(--c-surface)" stroke="var(--c-text)" />
          <text x={e.x + w / 2} y={e.y + h / 2 + 4} textAnchor="middle" style={TEXT}>
            {e.negated ? `NOT ${e.variable ?? ''}` : e.variable}
          </text>
        </>
      );
      break;
    case 'label':
      symbol = (
        <text x={e.x} y={e.y + h / 2 + 4} style={TEXT}>
          {e.label}:
        </text>
      );
      break;
    case 'jump':
      symbol = (
        <text x={e.x} y={e.y + h / 2 + 4} style={TEXT}>
          {`>> ${e.label ?? ''}`}
        </text>
      );
      break;
    case 'return':
      symbol = (
        <text x={e.x} y={e.y + h / 2 + 4} style={TEXT}>
          {'<RETURN>'}
        </text>
      );
      break;
    case 'comment':
      symbol = (
        <>
          <rect x={e.x} y={e.y} width={w} height={h} fill="none" stroke="var(--c-text-faint)" strokeDasharray="2 2" />
          <text x={e.x + 4} y={e.y + 14} style={{ ...TEXT, fill: 'var(--c-text-muted)' }}>
            {e.text}
          </text>
        </>
      );
      break;
  }
  const value = valueText(e, live, state);
  return (
    <g
      data-testid="ld-element"
      data-local-id={e.localId}
      data-kind={e.kind}
      data-state={state}
      data-forced={forced ? 'true' : undefined}
      strokeDasharray={dash}
    >
      <title>{`${e.kind} ${e.localId}${e.variable ? `: ${e.variable}` : ''}`}</title>
      {symbol}
      {caption && (
        <text x={e.x + w / 2} y={e.y - 4} textAnchor="middle" style={TEXT}>
          {caption}
        </text>
      )}
      {value !== null && (
        <text data-testid="ld-value" x={e.x + w / 2} y={e.y + h + 12} textAnchor="middle" style={{ ...TEXT, fill: STROKE[state] }}>
          {value}
        </text>
      )}
      {forced && (
        <text data-testid="variable-forced" x={e.x + w / 2} y={e.y + h + 24} textAnchor="middle" style={{ ...TEXT, fill: 'var(--c-warn)' }}>
          FORCED
        </text>
      )}
    </g>
  );
}

/** What is written under an element while live: a contact's or coil's state, a variable's value. */
function valueText(e: LdElement, live: LadderLive | null, state: LiveState): string | null {
  if (live === null) return null;
  if (e.kind === 'contact' || e.kind === 'coil') {
    if (state === 'unknown') return 'UNKNOWN';
    if (e.kind === 'contact' && e.edge !== null) return null;
    return state === 'on' ? 'ON' : state === 'off' ? 'OFF' : null;
  }
  if (e.kind === 'inVariable' || e.kind === 'outVariable' || e.kind === 'inOutVariable') {
    if (live.values === null) return 'UNKNOWN';
    const found = e.variable ? live.values.get(e.variable.toLowerCase()) : undefined;
    if (!found || found.error !== null || found.type === null) return null;
    return formatVariable(found.type, found.value) ?? 'UNKNOWN';
  }
  return null;
}

/** One POU's body. */
function PouView({
  pou,
  live,
  diagnostics,
}: {
  pou: LdPou;
  live: LadderLive | null;
  diagnostics: Diagnostic[];
}) {
  const elements = pou.elements;
  const placed = elements.filter((e) => e.kind !== 'leftPowerRail' && e.kind !== 'rightPowerRail');
  const top = Math.min(0, ...elements.map((e) => e.y));
  const bottom = Math.max(40, ...placed.map((e) => e.y + sizeOf(e, { top: 0, bottom: 0 }).h)) + 20;
  const size = new Map(elements.map((e) => [e.localId, sizeOf(e, { top, bottom })]));
  const byId = new Map(elements.map((e) => [e.localId, e]));
  const at = (e: LdElement) => size.get(e.localId) ?? { w: 0, h: 0 };

  const inPoint = (e: LdElement, index: number, other: Point): Point => {
    const h = at(e).h;
    const pin = e.inputs[index];
    if (pin?.at) return { x: e.x + pin.at.x, y: e.y + pin.at.y };
    if (e.kind === 'rightPowerRail') return { x: e.x, y: other.y };
    if (e.kind === 'block') return { x: e.x, y: e.y + 30 + 20 * index };
    return { x: e.x, y: e.y + h / 2 };
  };
  const outPoint = (e: LdElement, formal: string | null): Point => {
    const { w, h } = at(e);
    if (e.kind === 'block') {
      const index = Math.max(
        0,
        formal === null
          ? e.outputs.findIndex((o) => o.formal.toUpperCase() !== 'ENO')
          : e.outputs.findIndex((o) => o.formal.toUpperCase() === formal.toUpperCase())
      );
      const pin = e.outputs[index];
      if (pin?.at) return { x: e.x + pin.at.x, y: e.y + pin.at.y };
      return { x: e.x + w, y: e.y + 30 + 20 * index };
    }
    const pin = e.outputs[0];
    if (pin?.at) return { x: e.x + pin.at.x, y: e.y + pin.at.y };
    return { x: e.x + w, y: e.y + h / 2 };
  };

  const links: ReactNode[] = [];
  for (const e of elements) {
    e.inputs.forEach((input, index) => {
      for (const c of input.connections) {
        const from = byId.get(c.from);
        if (!from) continue;
        // A link from a rail is drawn level with the pin it reaches; one to a rail level with its source.
        const end0 = inPoint(e, index, { x: 0, y: from.y + at(from).h / 2 });
        const start = from.kind === 'leftPowerRail' ? { x: from.x + at(from).w, y: end0.y } : outPoint(from, c.formal);
        const end = e.kind === 'rightPowerRail' ? { x: e.x, y: start.y } : end0;
        const mid = (start.x + end.x) / 2;
        const d =
          start.y === end.y
            ? `M${start.x} ${start.y}H${end.x}`
            : `M${start.x} ${start.y}H${mid}V${end.y}H${end.x}`;
        // The output a connection without a formal parameter takes from a block: its first but ENO.
        const output =
          from.kind === 'block'
            ? (c.formal ?? from.outputs.find((o) => o.formal.toUpperCase() !== 'ENO')?.formal ?? '').toUpperCase()
            : '';
        const state = linkState(c.from, output, live);
        links.push(
          <path
            key={`${c.from}.${output}>${e.localId}.${index}`}
            data-testid="ld-link"
            data-from={c.from}
            data-output={output}
            data-to={e.localId}
            data-state={state}
            d={d}
            fill="none"
            stroke={STROKE[state]}
            strokeWidth={state === 'on' ? 3 : 1.5}
            strokeDasharray={state === 'unknown' ? '4 3' : undefined}
          />
        );
      }
    });
  }

  const order = networks(pou);
  const left = Math.min(0, ...elements.map((e) => e.x)) - 30;
  const right = Math.max(200, ...elements.map((e) => e.x + at(e).w)) + 20;
  const marked = diagnostics.filter((d) => d.element?.pou === pou.name && d.element.local_id != null);

  return (
    <svg
      data-testid="ld-diagram"
      data-pou={pou.name}
      role="img"
      aria-label={`Ladder Diagram of ${pou.name}`}
      viewBox={`${left} ${top - 30} ${right - left} ${bottom - top + 40}`}
      width={right - left}
      height={bottom - top + 40}
    >
      {links}
      {elements.map((e) => (
        <ElementView key={e.localId} e={e} w={at(e).w} h={at(e).h} live={live} />
      ))}
      {order.map((ids, index) => {
        const first = byId.get(ids[0] ?? -1);
        if (!first) return null;
        const y = Math.min(...ids.map((id) => byId.get(id)?.y ?? first.y));
        return (
          <text
            key={ids.join(',')}
            data-testid="ld-rung"
            data-rung={index + 1}
            data-ids={ids.join(',')}
            x={left + 4}
            y={y + 14}
            style={{ ...TEXT, fill: 'var(--c-text-muted)' }}
          >
            {index + 1}
          </text>
        );
      })}
      {marked.map((d, index) => {
        const e = byId.get(d.element?.local_id ?? -1);
        // Where the module says the element is, when this text has no element of that id.
        const x = e?.x ?? d.element?.x ?? 0;
        const y = e?.y ?? d.element?.y ?? 0;
        const { w, h } = e ? at(e) : { w: 30, h: 20 };
        return (
          <rect
            key={`${d.element?.local_id}:${index}`}
            data-testid="ld-diagnostic"
            data-local-id={d.element?.local_id ?? undefined}
            x={x - 4}
            y={y - 4}
            width={w + 8}
            height={h + 8}
            fill="none"
            stroke="var(--c-bad)"
            strokeWidth={2}
          >
            <title>{d.message}</title>
          </rect>
        );
      })}
    </svg>
  );
}

/** Every POU of a document, the PROGRAM's live. */
export function LadderView({
  pous,
  program,
  live,
  diagnostics,
}: {
  pous: LdPou[];
  /** The PROGRAM's name: the POU whose variables are the runtime's. */
  program: string | null;
  live: LadderLive | null;
  diagnostics: Diagnostic[];
}) {
  if (pous.length === 0) {
    return (
      <p className="muted" data-testid="ld-empty">
        The document holds no POU.
      </p>
    );
  }
  return (
    <div data-testid="ld-view">
      {pous.map((pou) => {
        const isProgram = program !== null ? pou.name.toLowerCase() === program.toLowerCase() : pou.pouType === 'program';
        return (
          <section key={pou.name} aria-label={pou.name} data-testid="ld-pou">
            <h3 className="logic-panel__title">
              {pou.name} <span className="muted">{pou.pouType}</span>
            </h3>
            {!pou.ld && <p className="muted">Its body is not a Ladder Diagram and is not drawn.</p>}
            {!isProgram && pou.ld && (
              <p className="muted" data-testid="ld-pou-not-live">
                A {pou.pouType === 'function' ? 'function' : 'function block'} has no state of its own to draw: each call
                or instance has its own.
              </p>
            )}
            {pou.ld && <PouView pou={pou} live={isProgram ? live : null} diagnostics={diagnostics} />}
            {pou.notDrawn.length > 0 && (
              <p className="form__error" data-testid="ld-not-drawn">
                Not drawn: {pou.notDrawn.join('; ')}.
              </p>
            )}
          </section>
        );
      })}
    </div>
  );
}
