/**
 * SYS-REQ-096 static guard: no surface may substitute a fabricated process value.
 *
 * The DOM test in `honestCanvas.test.tsx` proves the rendered output is quiescent. It cannot, on
 * its own, prove a call-site fallback was not reintroduced: a faceplate with `NOT_BOUND` quality
 * suppresses the value even if the caller invents one before passing it down. So this test reads
 * source.
 *
 * WHY IT WAS REWRITTEN — it used to enumerate the 14 literals the pre-fix renderer substituted
 * (`?? 1450`, `?? 18.5`, …). That is a membership test against values we have already shipped: a
 * NEW literal introduced at a call site walks straight through it, and so does the whole class
 * behind it. The reviewer's example was exactly this — `pv={values.pv ?? 82.6}` — where the
 * blacklist does not know `82.6` and the DOM test cannot see it, because the `NOT_BOUND` gate
 * suppresses the value before it renders. Both layers green, defect present.
 *
 * This now matches the SHAPE: a numeric literal as the right operand of `??` or `||` feeding a
 * process value. It fails on a value it has never seen, and the mutation below proves that.
 *
 * WHERE THE GUARD SITS, and why it is not component-local (the reviewer's addition on this row):
 * a guard on a faceplate's own parameter defaults does not protect a literal in the PARENT at the
 * call site, and every value here arrives as a prop from the renderer. So this scans the whole
 * production tree rather than one component, and separately asserts that the renderer resolves
 * component data through `resolveComponentData` and declares a quality for each faceplate it
 * renders. Component-local, call-site, and tree-wide are three different reach; the class needs
 * the last one.
 *
 * SCOPE: production source only. Tests legitimately use numbers, so scanning them would produce
 * noise that gets the guard relaxed.
 */

import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = process.cwd();
const SOURCE_ROOTS = ['packages', 'plugins', 'src'];

/**
 * Props that carry a MEASURED process value into a surface.
 *
 * Deliberately excludes the generic `value` and `values`: those name an editable control's own
 * value or a resolver result, not a reading. Including them made `value={targetCv ?? 0}` on a
 * slider and `parseInt(e.target.value, 10) || 0` in a change handler look like fabricated
 * measurements, which they are not.
 */
const PROCESS_VALUE_PROPS = [
  'pv',
  'sp',
  'mv',
  'actualSpeed',
  'speedSetpoint',
  'currentAmps',
  'torquePercent',
  'outputPowerPct',
  'processValue',
  'setpoint',
  'level',
  'pressure',
  'temperature',
  'flow',
  'positionPercent',
];

function* walk(dir: string): Generator<string> {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'dist') continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === '__tests__' || entry.name === '__mocks__' || entry.name === 'tests') continue;
      yield* walk(full);
    } else if (/\.tsx?$/.test(entry.name) && !/\.(test|spec)\.tsx?$/.test(entry.name)) {
      yield full;
    }
  }
}

function productionFiles(): string[] {
  const files: string[] = [];
  for (const root of SOURCE_ROOTS) {
    const abs = path.join(ROOT, root);
    if (fs.existsSync(abs)) files.push(...walk(abs));
  }
  return files;
}

/**
 * A numeric literal as the fallback operand — the fabricated measurement. Boundary-anchored so
 * `?? 14` does not match `?? 1400`.
 *
 * There is deliberately NO value exemption. An earlier version exempted `0`, `1`, `2` and `-1` as
 * "structural", which waved through `values.pv ?? 0` while flagging `values.pv ?? 0.0` — the same
 * reading, spelled differently. A fabricated zero is not structural on a process value: 0 degC,
 * 0 bar, 0 %, 0 flow are ordinary measurements, and an operator cannot tell an invented zero from a
 * measured one. That was the defect class surviving in the one shape the guard still permitted.
 *
 * Whether a line is a measurement is decided by the PROP, not by the value: the guard requires an
 * actual process-value prop on the line, rather than treating any `values.<anything>` as one. That
 * is what makes `values.count ?? 0` ordinary code and `values.pv ?? 0` a fabrication.
 */
const NUMERIC_FALLBACK = /(?:\?\?|\|\|)\s*(-?\d+(?:\.\d+)?)(?![\d.])/g;

function numericFallbackOffenders(source: string): string[] {
  const offenders: string[] = [];
  // The prop must be passed as a JSX attribute (`pv={…}`), which is the call site that hands a
  // value to a surface. A bare `processValue ?? 0` inside a width calculation, or a control's
  // `value={…}` bound to local state, is a different thing and is not what this guard is for.
  const processProp = new RegExp(`\\b(?:${PROCESS_VALUE_PROPS.join('|')})\\s*[=:]`);
  NUMERIC_FALLBACK.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = NUMERIC_FALLBACK.exec(source)) !== null) {
    const lineStart = source.lastIndexOf('\n', m.index) + 1;
    const lineEnd = source.indexOf('\n', m.index);
    const line = source.slice(lineStart, lineEnd === -1 ? source.length : lineEnd).trim();
    if (processProp.test(line)) offenders.push(line.slice(0, 160));
  }
  return offenders;
}

describe('SYS-REQ-096: no surface substitutes a fabricated process value', () => {
  const RENDERER = path.resolve(ROOT, 'packages/ui-shell/src/components/workbench/DockviewWorkspace.tsx');
  const files = productionFiles();

  it('finds production source to guard (the sweep is not silently empty)', () => {
    expect(files.length).toBeGreaterThan(20);
  });

  it('has no numeric fallback feeding a process value, anywhere in production source', () => {
    const offenders: string[] = [];
    for (const file of files) {
      for (const hit of numericFallbackOffenders(fs.readFileSync(file, 'utf8'))) {
        offenders.push(`${path.relative(ROOT, file)}: ${hit}`);
      }
    }
    expect(offenders, `fabricated process-value fallback:\n${offenders.join('\n')}`).toEqual([]);
  });

  it('calibrates which lines count as a measurement, including the zero case first missed', () => {
    // The boundary is asserted rather than described, so a change to it fails loudly. The first
    // version exempted 0, 1, 2 and -1 as "structural", which waved through `values.pv ?? 0` while
    // flagging `values.pv ?? 0.0` — the same reading, spelled differently. A fabricated zero on a
    // process value is a fabricated reading: 0 degC, 0 bar, 0 %, 0 flow are ordinary measurements.
    const fabricated = [
      'pv={values.pv ?? 82.6}',
      'pv={values.pv ?? 0}',
      'pv={values.pv ?? 0.0}',
      'pv={values.pv ?? -1}',
      'temperature={values.temperature ?? 1}',
      'level={values.level ?? 2}',
      'outputPowerPct={values.outputPowerPct || 0}',
    ];
    const ordinary = [
      'const tally = values.count ?? 0;',                       // not a measurement prop
      'value={targetCv ?? 0}',                                  // a control's own value
      'onChange={(e) => setNumber(parseInt(e.target.value, 10) || 0)}',
      'const [inputSpeed, setInputSpeed] = useState(speedSetpoint ?? 0);',
      'style={{ width: `${live ? ctrl.processValue ?? 0 : 0}%` }}',
    ];
    for (const line of fabricated) {
      expect(numericFallbackOffenders(line), `should be flagged: ${line}`).toHaveLength(1);
    }
    for (const line of ordinary) {
      expect(numericFallbackOffenders(line), `should NOT be flagged: ${line}`).toEqual([]);
    }
  });

  it('resolves component data through resolveComponentData rather than at the call site', () => {
    const source = fs.readFileSync(RENDERER, 'utf8');
    expect(source).toContain('resolveComponentData');
    // The values handed to a faceplate must come from the resolver, never be invented inline.
    for (const prop of ['actualSpeed', 'speedSetpoint', 'pv']) {
      const usage = new RegExp(`${prop}=\\{([^}]*)\\}`).exec(source);
      if (usage) expect(usage[1], `${prop} is not read from resolved values`).toContain('values.');
    }
  });

  it('passes a declared quality to every faceplate it renders', () => {
    const source = fs.readFileSync(RENDERER, 'utf8');
    for (const faceplate of [
      'MotorFaceplate',
      'ValveFaceplate',
      'PidControllerFaceplate',
      'AnalogTransmitterFaceplate',
    ]) {
      const usage = source.split(`<${faceplate}`)[1];
      expect(usage, `${faceplate} is rendered without a quality prop`).toBeDefined();
      const block = usage.slice(0, usage.indexOf('/>'));
      expect(block, `${faceplate} is rendered without a quality prop`).toContain('quality={quality}');
    }
  });
});
