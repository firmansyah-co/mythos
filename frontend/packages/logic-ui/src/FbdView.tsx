// A Function Block Diagram, drawn read-only as SVG from its TC6 document (ADR-013 §2.8, §2.16):
// blocks with their type, instance and pins (a negated pin with its circle), input, output and
// in-out variables, connectors and continuations, labels, jumps, returns and comments, each where
// the document places it, every line between them, and each network numbered in the order it is
// evaluated. The FBD editor is the next slice (L3c-2); this view is everyone's until then.
//
// **Live state** is drawn only from what was watched, and only while the runtime runs the very
// version this text builds in RUN (as for LD). The compiler keeps the value of every output in a
// variable assigned when its element is evaluated, and whether each network ran (`values` of the
// check, §2.16):
// - a line carries the value its output had when its network was evaluated in the last scan: a
//   BOOL drawn on (solid) or off (dashed), as TIA Portal shows a signal, and its value written at
//   the output; a number, a TIME or a STRING written at the output as the variables table writes
//   it;
// - a function that did not run in the last scan (its EN, or the ENO of a function it reads, was
//   FALSE) computed nothing: its output is drawn as not computed, never with a value of an
//   earlier scan;
// - a network whose flag says it did not run is drawn as not executed, and none of its lines or
//   outputs shows a value: what they keep is from a scan that ran it;
// - a value that cannot be read is UNKNOWN, never a value; a line from a literal carries the
//   literal written on its element, and is drawn without a state;
// - under a variable, its value; a forced variable, or a forced output, is marked FORCED.

import type { ReactNode } from 'react';
import type { Diagnostic, ValueProbe, WatchedValue } from './bridge';
import { fbdNetworks, type FbdElement, type FbdPou } from './fbd';
import { formatVariable } from './values';

/**
 * `on`/`off`: a BOOL read; `value`: another value read; `unknown`: one should be read and cannot
 * be; `none`: none is claimed; `not-executed`: its network did not run in the last scan;
 * `not-computed`: the function did not run in the last scan.
 */
export type FbdState = 'on' | 'off' | 'value' | 'unknown' | 'none' | 'not-executed' | 'not-computed';

/**
 * What the view draws live: the watched values by variable in lower case, or `null` when they
 * cannot be read, and where the check says each value is kept; `null` as a whole when nothing
 * live is drawn.
 */
export interface FbdLive {
  values: Map<string, WatchedValue> | null;
  probes: ValueProbe[];
}

const STROKE: Record<FbdState, string> = {
  on: 'var(--c-good)',
  off: 'var(--c-text-muted)',
  value: 'var(--c-text)',
  unknown: 'var(--c-warn)',
  none: 'var(--c-text)',
  'not-executed': 'var(--c-text-faint)',
  'not-computed': 'var(--c-text-faint)',
};
/** Off dashed, UNKNOWN dotted so it is never read as either; not computed dashed and faint. */
const DASH: Record<FbdState, string | undefined> = {
  on: undefined,
  off: '6 4',
  value: undefined,
  unknown: '2 3',
  none: undefined,
  'not-executed': undefined,
  'not-computed': '2 6',
};
/** A variable's path as an element writes it: a name, members and subscripts. */
const PLACE = /^[A-Za-z_]\w*(\[[^\]]+\])?(\.[A-Za-z_]\w*(\[[^\]]+\])?)*$/;
const TEXT = { fill: 'var(--c-text)', fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-xs)' } as const;

interface Point {
  x: number;
  y: number;
}

/** What a watched variable reads: its value as text, or why there is none. */
interface Read {
  state: FbdState;
  text: string | null;
  forced: boolean;
}

function readVariable(live: FbdLive, path: string, negated = false): Read {
  if (live.values === null) return { state: 'unknown', text: 'UNKNOWN', forced: false };
  const found = live.values.get(path.toLowerCase());
  if (!found || found.error !== null || found.type === null) return { state: 'unknown', text: 'UNKNOWN', forced: false };
  const forced = found.forced === true;
  if (found.type === 'BOOL') {
    if (typeof found.value !== 'boolean') return { state: 'unknown', text: 'UNKNOWN', forced };
    const value = negated ? !found.value : found.value;
    return { state: value ? 'on' : 'off', text: value ? 'TRUE' : 'FALSE', forced };
  }
  const text = formatVariable(found.type, found.value);
  return text === null ? { state: 'unknown', text: 'UNKNOWN', forced } : { state: 'value', text, forced };
}

/**
 * Whether the network element `id` is in ran in the last scan: `on` it ran, `not-executed` it did
 * not, `unknown` when its flag cannot be read or the check names none; `none` when nothing is
 * drawn live or the element is in no network.
 */
export function networkState(id: number | null, live: FbdLive | null): FbdState {
  if (live === null || id === null) return 'none';
  const flag = live.probes.find((p) => p.network?.includes(id) === true);
  if (!flag || flag.variable === undefined) return 'unknown';
  const read = readVariable(live, flag.variable);
  if (read.state === 'unknown') return 'unknown';
  return read.state === 'on' ? 'on' : 'not-executed';
}

/**
 * The value of output `output` of element `from` (a block's formal parameter in upper case, `''`
 * for the one output of any other element), in the network of element `network`.
 */
export function outputValue(live: FbdLive | null, network: number | null, from: number, output: string): Read {
  const none: Read = { state: 'none', text: null, forced: false };
  if (live === null) return none;
  const ran = networkState(network, live);
  if (ran === 'not-executed') return { state: 'not-executed', text: null, forced: false };
  const probe = live.probes.find((p) => p.local_id === from && p.output === output);
  // A line no variable keeps carries a literal, written on its element.
  if (!probe) return none;
  if (ran === 'unknown') return { state: 'unknown', text: 'UNKNOWN', forced: false };
  if (probe.on === true) return { state: 'on', text: 'TRUE', forced: false };
  if (probe.variable === undefined) return { state: 'unknown', text: 'UNKNOWN', forced: false };
  if (probe.ran !== undefined) {
    const computed = readVariable(live, probe.ran);
    if (computed.state === 'unknown') return { state: 'unknown', text: 'UNKNOWN', forced: false };
    if (computed.state === 'off') return { state: 'not-computed', text: null, forced: false };
  }
  return readVariable(live, probe.variable, probe.negated === true);
}

/** The size of an element: the document's, or one that holds what is drawn in it. */
function sizeOf(e: FbdElement): { w: number; h: number } {
  const given = (w: number, h: number) => ({ w: e.width ?? w, h: e.height ?? h });
  switch (e.kind) {
    case 'block':
      return given(100, 20 + 20 * Math.max(1, e.inputs.length, e.outputs.length));
    case 'inVariable':
    case 'outVariable':
    case 'inOutVariable':
      return given(Math.max(60, 8 * (e.variable ?? '').length + 16), 20);
    case 'connector':
    case 'continuation':
      return given(Math.max(50, 8 * (e.label ?? '').length + 24), 20);
    case 'comment':
      return given(Math.max(100, 7 * (e.text ?? '').length), 20);
    default:
      return given(80, 20);
  }
}

/** The output a connection without a formal parameter takes from a block: its first but ENO. */
const outputOf = (from: FbdElement | undefined, formal: string | null): string =>
  from?.kind === 'block'
    ? (formal ?? from.outputs.find((o) => o.formal.toUpperCase() !== 'ENO')?.formal ?? '').toUpperCase()
    : '';

/** One element: its symbol, its text, and under a variable its value. */
function ElementView({
  e,
  w,
  h,
  live,
  network,
  values,
}: {
  e: FbdElement;
  w: number;
  h: number;
  live: FbdLive | null;
  network: number | null;
  /** The value each output carries, by its formal parameter in upper case. */
  values: Map<string, Read>;
}) {
  const ran = networkState(network, live);
  let symbol: ReactNode = null;
  let caption: string | null = null;
  const pinY = (i: number) => e.y + 30 + 20 * i;
  switch (e.kind) {
    case 'block':
      symbol = (
        <>
          <rect x={e.x} y={e.y} width={w} height={h} fill="var(--c-surface)" stroke="var(--c-text)" />
          <text x={e.x + w / 2} y={e.y + 14} textAnchor="middle" style={TEXT}>
            {e.typeName}
          </text>
          {e.inputs.map((pin, i) => (
            <g key={`in${i}`} data-testid="fbd-pin" data-pin={pin.formal ?? ''} data-negated={pin.negated ? 'true' : undefined}>
              <text x={e.x + 4} y={pinY(i) + 4} style={TEXT}>
                {pin.formal}
                {pin.expression !== null && pin.connections.length === 0 ? ` := ${pin.expression}` : ''}
              </text>
              {pin.negated && <circle cx={e.x - 4} cy={pinY(i)} r={4} fill="var(--c-surface)" stroke="var(--c-text)" />}
            </g>
          ))}
          {e.outputs.map((pin, i) => {
            const read = values.get(pin.formal.toUpperCase());
            return (
              <g key={`out${i}`} data-testid="fbd-pin" data-pin={pin.formal} data-negated={pin.negated ? 'true' : undefined}>
                <text x={e.x + w - 4} y={pinY(i) + 4} textAnchor="end" style={TEXT}>
                  {pin.formal}
                </text>
                {pin.negated && <circle cx={e.x + w + 4} cy={pinY(i)} r={4} fill="var(--c-surface)" stroke="var(--c-text)" />}
                {read && <OutputValue read={read} x={e.x + w + 10} y={pinY(i) - 4} from={e.localId} output={pin.formal.toUpperCase()} />}
              </g>
            );
          })}
        </>
      );
      caption = e.instanceName;
      break;
    case 'inVariable':
    case 'outVariable':
    case 'inOutVariable': {
      const read = values.get('');
      symbol = (
        <>
          <rect x={e.x} y={e.y} width={w} height={h} rx={8} fill="var(--c-surface)" stroke="var(--c-text)" />
          <text x={e.x + w / 2} y={e.y + h / 2 + 4} textAnchor="middle" style={TEXT}>
            {e.variable}
          </text>
          {e.negated && e.kind === 'inVariable' && <circle cx={e.x + w + 4} cy={e.y + h / 2} r={4} fill="var(--c-surface)" stroke="var(--c-text)" />}
          {e.negated && e.kind === 'outVariable' && <circle cx={e.x - 4} cy={e.y + h / 2} r={4} fill="var(--c-surface)" stroke="var(--c-text)" />}
          {read && e.kind !== 'outVariable' && <OutputValue read={read} x={e.x + w + 10} y={e.y + h / 2 - 4} from={e.localId} output="" />}
        </>
      );
      break;
    }
    case 'connector':
    case 'continuation':
      symbol = (
        <>
          <path
            d={`M${e.x} ${e.y}H${e.x + w - 10}L${e.x + w} ${e.y + h / 2}L${e.x + w - 10} ${e.y + h}H${e.x}${e.kind === 'continuation' ? `L${e.x + 10} ${e.y + h / 2}Z` : 'Z'}`}
            fill="var(--c-surface)"
            stroke="var(--c-text)"
          />
          <text x={e.x + w / 2} y={e.y + h / 2 + 4} textAnchor="middle" style={TEXT}>
            {e.label}
          </text>
          {e.kind === 'continuation' && values.get('') && (
            <OutputValue read={values.get('') as Read} x={e.x + w + 10} y={e.y + h / 2 - 4} from={e.localId} output="" />
          )}
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
  // Under a variable, its own value as watched (an element of an array among them).
  const own =
    live !== null && ran !== 'not-executed' && (e.kind === 'inVariable' || e.kind === 'outVariable' || e.kind === 'inOutVariable') && e.variable
      ? readVariable(live, e.variable)
      : null;
  // A literal, or an expression that is not a variable, has no value of its own to watch.
  const isVariable = own !== null && PLACE.test(e.variable ?? '') && !/^(TRUE|FALSE)$/i.test(e.variable ?? '');
  return (
    <g data-testid="fbd-element" data-local-id={e.localId} data-kind={e.kind} data-state={ran} data-forced={isVariable && own?.forced ? 'true' : undefined}>
      <title>{`${e.kind} ${e.localId}${e.variable ? `: ${e.variable}` : ''}`}</title>
      {symbol}
      {caption && (
        <text x={e.x + w / 2} y={e.y - 4} textAnchor="middle" style={TEXT}>
          {caption}
        </text>
      )}
      {isVariable && own !== null && (
        <text data-testid="fbd-variable-value" x={e.x + w / 2} y={e.y + h + 12} textAnchor="middle" style={{ ...TEXT, fill: STROKE[own.state] }}>
          {own.text}
        </text>
      )}
      {isVariable && own?.forced && (
        <text data-testid="variable-forced" x={e.x + w / 2} y={e.y + h + 24} textAnchor="middle" style={{ ...TEXT, fill: 'var(--c-warn)' }}>
          FORCED
        </text>
      )}
    </g>
  );
}

/** The value an output carries, written beside it. */
function OutputValue({ read, x, y, from, output }: { read: Read; x: number; y: number; from: number; output: string }) {
  if (read.state === 'none') return null;
  const text = read.state === 'not-executed' ? null : read.state === 'not-computed' ? 'not computed' : read.text;
  return (
    <text
      data-testid="fbd-value"
      data-from={from}
      data-output={output}
      data-state={read.state}
      data-forced={read.forced ? 'true' : undefined}
      x={x}
      y={y}
      style={{ ...TEXT, fill: STROKE[read.state] }}
    >
      {text}
      {read.forced ? ' FORCED' : ''}
    </text>
  );
}

/** One POU's body. */
export function FbdPouView({ pou, live, diagnostics }: { pou: FbdPou; live: FbdLive | null; diagnostics: Diagnostic[] }) {
  const elements = pou.elements;
  const size = new Map(elements.map((e) => [e.localId, sizeOf(e)]));
  const byId = new Map(elements.map((e) => [e.localId, e]));
  const at = (e: FbdElement) => size.get(e.localId) ?? { w: 0, h: 0 };
  const order = fbdNetworks(pou);
  const networkOf = new Map<number, number>();
  for (const ids of order) for (const id of ids) networkOf.set(id, ids[0] as number);
  // A connector or a continuation is in the network of the line it carries.
  const lineNetwork = (e: FbdElement): number | null => {
    const own = networkOf.get(e.localId);
    if (own !== undefined) return own;
    if (e.kind === 'connector') {
      const c = e.inputs[0]?.connections[0];
      return c ? (networkOf.get(c.from) ?? null) : null;
    }
    if (e.kind === 'continuation') {
      const connector = elements.find((c) => c.kind === 'connector' && (c.label ?? '').toUpperCase() === (e.label ?? '').toUpperCase());
      const c = connector?.inputs[0]?.connections[0];
      return c ? (networkOf.get(c.from) ?? null) : null;
    }
    return null;
  };

  const inPoint = (e: FbdElement, index: number): Point => {
    const pin = e.inputs[index];
    if (pin?.at) return { x: e.x + pin.at.x, y: e.y + pin.at.y };
    if (e.kind === 'block') return { x: e.x - (pin?.negated ? 8 : 0), y: e.y + 30 + 20 * index };
    return { x: e.x - (e.negated && e.kind === 'outVariable' ? 8 : 0), y: e.y + at(e).h / 2 };
  };
  const outPoint = (e: FbdElement, output: string): Point => {
    const { w, h } = at(e);
    if (e.kind === 'block') {
      const index = Math.max(
        0,
        e.outputs.findIndex((o) => o.formal.toUpperCase() === output)
      );
      const pin = e.outputs[index];
      if (pin?.at) return { x: e.x + pin.at.x, y: e.y + pin.at.y };
      return { x: e.x + w + (pin?.negated ? 8 : 0), y: e.y + 30 + 20 * index };
    }
    const pin = e.outputs[0];
    if (pin?.at) return { x: e.x + pin.at.x, y: e.y + pin.at.y };
    return { x: e.x + w + (e.negated && e.kind === 'inVariable' ? 8 : 0), y: e.y + h / 2 };
  };

  // What each output carries, by element and formal parameter.
  const valueAt = (from: FbdElement, output: string): Read => outputValue(live, lineNetwork(from), from.localId, output);

  const lines: ReactNode[] = [];
  for (const e of elements) {
    e.inputs.forEach((input, index) => {
      for (const c of input.connections) {
        const from = byId.get(c.from);
        if (!from) continue;
        const output = outputOf(from, c.formal);
        const start = outPoint(from, output);
        const end = inPoint(e, index);
        const mid = (start.x + end.x) / 2;
        const d = start.y === end.y ? `M${start.x} ${start.y}H${end.x}` : `M${start.x} ${start.y}H${mid}V${end.y}H${end.x}`;
        const read = valueAt(from, output);
        lines.push(
          <path
            key={`${c.from}.${output}>${e.localId}.${index}`}
            data-testid="fbd-line"
            data-from={c.from}
            data-output={output}
            data-to={e.localId}
            data-state={read.state}
            d={d}
            fill="none"
            stroke={STROKE[read.state]}
            strokeWidth={read.state === 'on' ? 3 : 1.5}
            strokeDasharray={DASH[read.state]}
          />
        );
      }
    });
  }

  const left = Math.min(0, ...elements.map((e) => e.x)) - 30;
  const right = Math.max(200, ...elements.map((e) => e.x + at(e).w)) + 80;
  const top = Math.min(0, ...elements.map((e) => e.y));
  const bottom = Math.max(40, ...elements.map((e) => e.y + at(e).h)) + 30;
  const marked = diagnostics.filter((d) => d.element?.pou === pou.name && d.element.local_id != null);

  return (
    <svg
      data-testid="fbd-diagram"
      data-pou={pou.name}
      role="img"
      aria-label={`Function Block Diagram of ${pou.name}`}
      viewBox={`${left} ${top - 30} ${right - left} ${bottom - top + 40}`}
      width={right - left}
      height={bottom - top + 40}
    >
      {lines}
      {elements.map((e) => {
        const values = new Map<string, Read>();
        for (const o of e.outputs) values.set(o.formal.toUpperCase(), valueAt(e, o.formal.toUpperCase()));
        return <ElementView key={e.localId} e={e} w={at(e).w} h={at(e).h} live={live} network={lineNetwork(e)} values={values} />;
      })}
      {order.map((ids, index) => {
        const first = byId.get(ids[0] ?? -1);
        if (!first) return null;
        const ran = networkState(first.localId, live);
        return (
          <text
            key={ids.join(',')}
            data-testid="fbd-network"
            data-network={index + 1}
            data-ids={ids.join(',')}
            data-state={ran}
            x={left + 4}
            y={first.y + 14}
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
            data-testid="fbd-diagnostic"
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
export function FbdView({
  pous,
  program,
  live,
  diagnostics,
}: {
  pous: FbdPou[];
  /** The PROGRAM's name: the POU whose variables are the runtime's. */
  program: string | null;
  live: FbdLive | null;
  diagnostics: Diagnostic[];
}) {
  if (pous.length === 0) {
    return (
      <p className="muted" data-testid="fbd-empty">
        The document holds no POU.
      </p>
    );
  }
  return (
    <div data-testid="fbd-view">
      {pous.map((pou) => {
        const isProgram = program !== null ? pou.name.toLowerCase() === program.toLowerCase() : pou.pouType === 'program';
        return (
          <section key={pou.name} aria-label={pou.name} data-testid="fbd-pou">
            <h3 className="logic-panel__title">
              {pou.name} <span className="muted">{pou.pouType}</span>
            </h3>
            {!pou.fbd && <p className="muted">Its body is not a Function Block Diagram and is not drawn here.</p>}
            {!isProgram && pou.fbd && (
              <p className="muted" data-testid="fbd-pou-not-live">
                A {pou.pouType === 'function' ? 'function' : 'function block'} has no state of its own to draw: each call
                or instance has its own.
              </p>
            )}
            {pou.fbd && <FbdPouView pou={pou} live={isProgram ? live : null} diagnostics={diagnostics} />}
            {pou.notDrawn.length > 0 && (
              <p className="form__error" data-testid="fbd-not-drawn">
                Not drawn: {pou.notDrawn.join('; ')}.
              </p>
            )}
          </section>
        );
      })}
    </div>
  );
}
