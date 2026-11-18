// A Ladder Diagram, drawn read-only as SVG from its TC6 document (ADR-013 §2.8, §2.15): the
// power rails, each rung numbered in the order it is evaluated, contacts, coils, blocks with
// their pins, variables, jumps, labels, returns and comments, where the document places them.
// The editor (LadderEditor.tsx, L3b-2b) draws each network with `PouView`.
//
// **Live state** is drawn only from what was watched, and only while the runtime runs the very
// version this text builds (the check's version equals the runtime's) in RUN. The compiler keeps
// the power of every element's output and of every junction of links in a variable assigned
// when the element is evaluated, and whether each rung ran (`power` of the check, §2.15):
// - an element (a contact, a coil, a BOOL variable) and a link: the power its output carried
//   when its rung was evaluated in the last scan; where several links join, the joined part
//   carries the junction's power;
// - a rung whose flag says it did not run in the last scan (a jump or a return skipped it) is
//   drawn as not executed, and none of its elements or links shows a power state: what they
//   keep is from a scan that ran it;
// - powered is drawn solid, not powered dashed (the meaning of TIA Portal's convention);
// - a value that cannot be read is UNKNOWN, never a state; an output that carries no power (a
//   value that is not a BOOL) is drawn without one;
// - under a contact or a coil, its variable's value; a forced variable is marked FORCED.

import type { ReactNode } from 'react';
import type { Diagnostic, PowerProbe, WatchedValue } from './bridge';
import { networks, type LdElement, type LdPou } from './ladder';
import { formatVariable } from './values';

/**
 * `on`/`off`: a power read; `unknown`: one should be read and cannot be; `none`: none is
 * claimed; `not-executed`: its rung did not run in the last scan.
 */
export type LiveState = 'on' | 'off' | 'unknown' | 'none' | 'not-executed';

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
  'not-executed': 'var(--c-text-faint)',
};
/** Powered solid, not powered dashed; UNKNOWN dotted, so it is never read as either. */
const DASH: Record<LiveState, string | undefined> = {
  on: undefined,
  off: '6 4',
  unknown: '2 3',
  none: undefined,
  'not-executed': undefined,
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

/** The power a probe names: ON, or its variable's value. */
function probeState(live: LadderLive, probe: PowerProbe): LiveState {
  if (probe.on === true) return 'on';
  const read = boolOf(live, probe.variable ?? null);
  return probe.negated ? not(read.state) : read.state;
}

/**
 * Whether the rung element `id` is in ran in the last scan: `on` it ran, `not-executed` it did
 * not, `unknown` when its flag cannot be read or the check names none; `none` for an element
 * of no rung (a rail, a label, a comment) or when nothing is drawn live.
 */
export function rungState(id: number | null, live: LadderLive | null): LiveState {
  if (live === null || id === null) return 'none';
  const flag = live.power.find((p) => p.network?.includes(id) === true);
  if (!flag) return 'unknown';
  const state = probeState(live, flag);
  return state === 'off' ? 'not-executed' : state;
}

/**
 * The power of a point of rung `rung` (the rung of element `rung`, `null` for none): of the
 * output `output` of element `from` (a block's formal parameter in upper case, `''` for the one
 * output of any other element), or of the junction at input `input` of element `from`.
 */
export function powerState(
  live: LadderLive | null,
  rung: number | null,
  from: number,
  point: { output: string } | { input: string }
): LiveState {
  if (live === null) return 'none';
  const ran = rungState(rung, live);
  if (ran === 'not-executed') return 'not-executed';
  const probe = live.power.find(
    (p) => p.local_id === from && ('output' in point ? p.output === point.output : p.input === point.input)
  );
  // A point no variable keeps carries no power: a value that is not a BOOL.
  if (!probe) return 'none';
  if (ran === 'unknown') return 'unknown';
  return probeState(live, probe);
}

/** The value under a contact or a coil: its variable's, with whether it is forced. */
export function variableState(e: LdElement, live: LadderLive | null): { state: LiveState; forced: boolean } {
  if (e.kind !== 'contact' && e.kind !== 'coil') return { state: 'none', forced: false };
  const read = boolOf(live, e.variable);
  return e.kind === 'contact' && e.negated && e.edge === null ? { ...read, state: not(read.state) } : read;
}

/** What an element is drawn with: the power of its output; for a block, only whether its rung ran. */
export function elementState(e: LdElement, rung: number | null, live: LadderLive | null): LiveState {
  if (e.kind === 'leftPowerRail' || e.kind === 'rightPowerRail' || rung === null) return 'none';
  return powerState(live, rung, e.localId, { output: '' });
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
  rung,
}: {
  e: LdElement;
  w: number;
  h: number;
  live: LadderLive | null;
  /** The element's own `localId` when it is in a rung; `null` for a rail, a label, a comment. */
  rung: number | null;
}) {
  const state = elementState(e, rung, live);
  const own = variableState(e, live);
  const forced = own.forced;
  const stroke = STROKE[state];
  const dash = DASH[state];
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
  const value = state === 'not-executed' ? null : valueText(e, live, own.state);
  return (
    <g
      data-testid="ld-element"
      data-local-id={e.localId}
      data-kind={e.kind}
      data-state={state}
      data-variable-state={live !== null && (e.kind === 'contact' || e.kind === 'coil') ? own.state : undefined}
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
        <text data-testid="ld-value" x={e.x + w / 2} y={e.y + h + 12} textAnchor="middle" style={{ ...TEXT, fill: STROKE[own.state === 'none' ? state : own.state] }}>
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

/** What is written under an element while live: a contact's or coil's variable, a variable's value. */
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
export function PouView({
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

  const order = networks(pou);
  const inRung = new Set(order.flat());
  const rungOf = (id: number): number | null => (inRung.has(id) ? id : null);

  const links: ReactNode[] = [];
  const junctions: ReactNode[] = [];
  for (const e of elements) {
    e.inputs.forEach((input, index) => {
      // Where the links into this input join: the right-most point where one turns to it.
      let joinX = Number.NEGATIVE_INFINITY;
      const ends: Point[] = [];
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
        joinX = Math.max(joinX, start.y === end.y ? start.x : mid);
        ends.push(end);
        // The output a connection without a formal parameter takes from a block: its first but ENO.
        const output =
          from.kind === 'block'
            ? (c.formal ?? from.outputs.find((o) => o.formal.toUpperCase() !== 'ENO')?.formal ?? '').toUpperCase()
            : '';
        // A link is of the rung of the element it reaches, or, to the right rail, of its source.
        const rung = rungOf(e.localId) ?? rungOf(c.from);
        const state = powerState(live, rung, c.from, { output });
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
            strokeDasharray={DASH[state]}
          />
        );
      }
      // Past the point where they join, the links carry the junction's power (§8.2.3).
      const end = ends[0];
      if (end !== undefined && ends.length > 1 && e.kind !== 'rightPowerRail') {
        const formal = (input.formal ?? '').toUpperCase();
        const state = powerState(live, rungOf(e.localId), e.localId, { input: formal });
        junctions.push(
          <path
            key={`junction>${e.localId}.${index}`}
            data-testid="ld-junction"
            data-to={e.localId}
            data-input={formal}
            data-state={state}
            d={`M${joinX} ${end.y}H${end.x}`}
            fill="none"
            stroke={STROKE[state]}
            strokeWidth={state === 'on' ? 3 : 1.5}
            strokeDasharray={DASH[state]}
          />
        );
      }
    });
  }

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
      {junctions}
      {elements.map((e) => (
        <ElementView key={e.localId} e={e} w={at(e).w} h={at(e).h} live={live} rung={rungOf(e.localId)} />
      ))}
      {order.map((ids, index) => {
        const first = byId.get(ids[0] ?? -1);
        if (!first) return null;
        const y = Math.min(...ids.map((id) => byId.get(id)?.y ?? first.y));
        const ran = rungState(first.localId, live);
        return (
          <text
            key={ids.join(',')}
            data-testid="ld-rung"
            data-rung={index + 1}
            data-ids={ids.join(',')}
            data-state={ran}
            x={left + 4}
            y={y + 14}
            style={{ ...TEXT, fill: ran === 'not-executed' ? STROKE[ran] : 'var(--c-text-muted)' }}
          >
            {index + 1}
            {ran === 'not-executed' && <title>Not executed in the last scan</title>}
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
