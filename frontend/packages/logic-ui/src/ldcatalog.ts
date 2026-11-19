// What a Ladder Diagram box can call, with its pins (ADR-013 §2.15, L3b-2b): the standard
// function blocks of the library written in ST (§2.14), the standard functions the front-end
// knows (§5, "2.1 ST front-end"), and the function and function block POUs of the document
// itself. A program compiles from its own source alone (§2.6), so these are all a box can name.
//
// Which pin carries the rung's power (IEC 61131-3 ed. 3 §8.2.6): a function block whose first
// input is a BOOL takes the power there (TON's IN, CTU's CU, SR's S1), and gives it from its
// first output when that is a BOOL (Q, QU, Q1); any other box takes it at EN and gives it from
// ENO, except a function whose result is a BOOL (GT, EQ, …), which gives it as OUT, so a
// comparison passes power when it holds. Every other pin takes an operand.

export interface PinDef {
  name: string;
  /** An elementary type, a generic one (`ANY_NUM`, …), or `ANY`. */
  type: string;
}

export interface BoxDef {
  name: string;
  kind: 'functionBlock' | 'function';
  inputs: PinDef[];
  outputs: PinDef[];
  /** The palette group it is listed in. */
  group: string;
}

const pin = (name: string, type: string): PinDef => ({ name, type });
const fb = (name: string, inputs: PinDef[], outputs: PinDef[], group: string): BoxDef => ({
  name,
  kind: 'functionBlock',
  inputs,
  outputs,
  group,
});
const fn = (name: string, inputs: PinDef[], out: string, group: string): BoxDef => ({
  name,
  kind: 'function',
  inputs,
  outputs: [pin('OUT', out)],
  group,
});

const timer = (name: string) => fb(name, [pin('IN', 'BOOL'), pin('PT', 'TIME')], [pin('Q', 'BOOL'), pin('ET', 'TIME')], 'Timers');
const compare = (name: string) => fn(name, [pin('IN1', 'ANY_ELEMENTARY'), pin('IN2', 'ANY_ELEMENTARY')], 'BOOL', 'Compare');
const math2 = (name: string) => fn(name, [pin('IN1', 'ANY_NUM'), pin('IN2', 'ANY_NUM')], 'ANY_NUM', 'Math');
const math1 = (name: string) => fn(name, [pin('IN', 'ANY_REAL')], 'ANY_REAL', 'Math');

/** The standard library's function blocks and functions, as their declarations give them. */
export const STANDARD: readonly BoxDef[] = [
  timer('TON'),
  timer('TOF'),
  timer('TP'),
  fb('CTU', [pin('CU', 'BOOL'), pin('R', 'BOOL'), pin('PV', 'INT')], [pin('Q', 'BOOL'), pin('CV', 'INT')], 'Counters'),
  fb('CTD', [pin('CD', 'BOOL'), pin('LD', 'BOOL'), pin('PV', 'INT')], [pin('Q', 'BOOL'), pin('CV', 'INT')], 'Counters'),
  fb(
    'CTUD',
    [pin('CU', 'BOOL'), pin('CD', 'BOOL'), pin('R', 'BOOL'), pin('LD', 'BOOL'), pin('PV', 'INT')],
    [pin('QU', 'BOOL'), pin('QD', 'BOOL'), pin('CV', 'INT')],
    'Counters'
  ),
  fb('R_TRIG', [pin('CLK', 'BOOL')], [pin('Q', 'BOOL')], 'Edges and bistables'),
  fb('F_TRIG', [pin('CLK', 'BOOL')], [pin('Q', 'BOOL')], 'Edges and bistables'),
  fb('SR', [pin('S1', 'BOOL'), pin('R', 'BOOL')], [pin('Q1', 'BOOL')], 'Edges and bistables'),
  fb('RS', [pin('S', 'BOOL'), pin('R1', 'BOOL')], [pin('Q1', 'BOOL')], 'Edges and bistables'),
  compare('GT'),
  compare('GE'),
  compare('EQ'),
  compare('NE'),
  compare('LE'),
  compare('LT'),
  math2('ADD'),
  math2('SUB'),
  math2('MUL'),
  math2('DIV'),
  fn('MOVE', [pin('IN', 'ANY')], 'ANY', 'Move'),
  fn('SEL', [pin('G', 'BOOL'), pin('IN0', 'ANY'), pin('IN1', 'ANY')], 'ANY', 'Move'),
  fn('MAX', [pin('IN1', 'ANY_ELEMENTARY'), pin('IN2', 'ANY_ELEMENTARY')], 'ANY_ELEMENTARY', 'Math'),
  fn('MIN', [pin('IN1', 'ANY_ELEMENTARY'), pin('IN2', 'ANY_ELEMENTARY')], 'ANY_ELEMENTARY', 'Math'),
  fn('LIMIT', [pin('MN', 'ANY_ELEMENTARY'), pin('IN', 'ANY_ELEMENTARY'), pin('MX', 'ANY_ELEMENTARY')], 'ANY_ELEMENTARY', 'Math'),
  fn('ABS', [pin('IN', 'ANY_NUM')], 'ANY_NUM', 'Math'),
  math1('SQRT'),
  math1('LN'),
  math1('LOG'),
  math1('EXP'),
  math1('SIN'),
  math1('COS'),
  math1('TAN'),
  math1('ASIN'),
  math1('ACOS'),
  math1('ATAN'),
  fn('ATAN2', [pin('Y', 'ANY_REAL'), pin('X', 'ANY_REAL')], 'ANY_REAL', 'Math'),
  fn('EXPT', [pin('IN1', 'ANY_REAL'), pin('IN2', 'ANY_NUM')], 'ANY_REAL', 'Math'),
  fn('LEN', [pin('IN', 'STRING')], 'INT', 'Strings'),
  fn('LEFT', [pin('IN', 'STRING'), pin('L', 'ANY_INT')], 'STRING', 'Strings'),
  fn('RIGHT', [pin('IN', 'STRING'), pin('L', 'ANY_INT')], 'STRING', 'Strings'),
  fn('MID', [pin('IN', 'STRING'), pin('L', 'ANY_INT'), pin('P', 'ANY_INT')], 'STRING', 'Strings'),
  fn('CONCAT', [pin('IN1', 'STRING'), pin('IN2', 'STRING')], 'STRING', 'Strings'),
  fn('FIND', [pin('IN1', 'STRING'), pin('IN2', 'STRING')], 'INT', 'Strings'),
];

const ELEMENTARY = ['BOOL', 'SINT', 'INT', 'DINT', 'LINT', 'REAL', 'LREAL', 'TIME', 'STRING'];

/** A conversion `X_TO_Y` between elementary types, or `TRUNC_<int>`: a function of IN. */
function conversion(upper: string): BoxDef | null {
  const to = /^([A-Z]+)_TO_([A-Z]+)$/.exec(upper);
  if (to && ELEMENTARY.includes(to[1] as string) && ELEMENTARY.includes(to[2] as string)) {
    return fn(upper, [pin('IN', to[1] as string)], to[2] as string, 'Convert');
  }
  const trunc = /^TRUNC_(SINT|INT|DINT|LINT)$/.exec(upper);
  if (trunc) return fn(upper, [pin('IN', 'ANY_REAL')], trunc[1] as string, 'Convert');
  return null;
}

/** A box type by name: the document's own POUs first, then the standard library. */
export function findBox(name: string, own: readonly BoxDef[]): BoxDef | null {
  const upper = name.trim().toUpperCase();
  if (upper === '') return null;
  return own.find((d) => d.name.toUpperCase() === upper) ?? STANDARD.find((d) => d.name === upper) ?? conversion(upper);
}

/** The pins of a box as it is drawn and written: EN and ENO where they carry the power. */
export interface BoxPins {
  inputs: PinDef[];
  outputs: PinDef[];
  powerIn: string;
  powerOut: string;
}

export function pinsOf(def: BoxDef): BoxPins {
  const firstIn = def.inputs[0];
  const firstOut = def.outputs[0];
  const inAtFirst = def.kind === 'functionBlock' && firstIn?.type === 'BOOL';
  const outAtFirst = firstOut?.type === 'BOOL' && (def.kind === 'functionBlock' || firstOut.name === 'OUT');
  return {
    inputs: inAtFirst ? def.inputs : [pin('EN', 'BOOL'), ...def.inputs],
    // ENO first when it carries the power, so it is level with EN and the rung runs straight.
    outputs: outAtFirst ? def.outputs : [pin('ENO', 'BOOL'), ...def.outputs],
    powerIn: inAtFirst ? (firstIn?.name ?? 'EN') : 'EN',
    powerOut: outAtFirst ? (firstOut?.name ?? 'ENO') : 'ENO',
  };
}

/**
 * The family of a type, for the editor's own check of an operand: a value of one family never
 * fits a pin of another. `null` when the type says nothing (`ANY`, `ANY_ELEMENTARY`) or is not
 * elementary; finer rules (widening, ranges) are the compiler's.
 */
export function family(type: string): 'BOOL' | 'TIME' | 'STRING' | 'NUM' | null {
  const t = type.toUpperCase().replace(/\s+/g, '');
  if (t === 'BOOL') return 'BOOL';
  if (t === 'TIME') return 'TIME';
  if (t.startsWith('STRING')) return 'STRING';
  if (['SINT', 'INT', 'DINT', 'LINT', 'REAL', 'LREAL', 'ANY_NUM', 'ANY_INT', 'ANY_REAL'].includes(t)) return 'NUM';
  return null;
}

/** The palette's groups, in order. */
export const GROUPS = ['Timers', 'Counters', 'Edges and bistables', 'Compare', 'Math', 'Move', 'Strings'];
