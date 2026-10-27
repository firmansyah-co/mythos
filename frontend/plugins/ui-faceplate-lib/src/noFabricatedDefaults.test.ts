/**
 * SYS-REQ-096 static guard over the faceplate library.
 *
 * Why this exists, and why a behavioural test is not enough:
 * the faceplates gate every value on `quality`, so a parameter default of `operatingMode = 'AUTO'`
 * is invisible to a DOM test rendered with `quality="NOT_BOUND"` — the gate suppresses the value
 * whether or not the default is back. A behavioural test can only see the defect on the LIVE path.
 * This guard reads the component source directly, so reintroducing a default is red regardless of
 * how any particular render path behaves.
 *
 * The rule it enforces: a faceplate parameter may default to a presentation-neutral value
 * (`tier = 2`, `interlocks = []`, `className = ''`), but must NOT default to a process value,
 * device state, or stored device configuration. Those are the values that fabricate a plant.
 */

import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const COMPONENTS_DIR = path.resolve(__dirname, 'components');

function componentSources(): Array<{ file: string; source: string }> {
  return fs
    .readdirSync(COMPONENTS_DIR)
    .filter((name) => name.endsWith('.tsx'))
    .map((name) => ({ file: name, source: fs.readFileSync(path.join(COMPONENTS_DIR, name), 'utf8') }));
}

/**
 * Parameter defaults that would fabricate device state or stored configuration.
 *
 * Each entry is a parameter name and the kind of default value that must never appear. The check
 * is deliberately about the *named parameter in a destructuring default*, not any `=` in the file,
 * so ordinary code such as `const x = tier ?? 2` is not flagged.
 */
const FORBIDDEN_PARAM_DEFAULTS: Array<{ param: string; valuePattern: string; why: string }> = [
  {
    param: 'operatingMode',
    valuePattern: `'[A-Z]+'`,
    why: 'a control mode is device state; AUTO/MANUAL cannot be assumed',
  },
  {
    param: 'transmitterType',
    valuePattern: `'[A-Z]+'`,
    why: 'a measurement type is stored configuration; PRESSURE cannot be assumed',
  },
  { param: 'maxSpeed', valuePattern: '\\d+', why: 'an engineering range is stored configuration' },
  { param: 'running', valuePattern: '(true|false)', why: 'run state is device state' },
  { param: 'tripped', valuePattern: '(true|false)', why: 'trip state is device state' },
  { param: 'openLimit', valuePattern: '(true|false)', why: 'limit-switch state is device state' },
  { param: 'closedLimit', valuePattern: '(true|false)', why: 'limit-switch state is device state' },
  { param: 'transitFault', valuePattern: '(true|false)', why: 'fault state is device state' },
  { param: 'sensorFault', valuePattern: '(true|false)', why: 'sensor health is device state' },
  { param: 'actualSpeed', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a process value cannot be assumed' },
  { param: 'speedSetpoint', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a process value cannot be assumed' },
  { param: 'currentAmps', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a process value cannot be assumed' },
  { param: 'torquePercent', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a process value cannot be assumed' },
  { param: 'positionPercent', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a process value cannot be assumed' },
  { param: 'pv', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a process value cannot be assumed' },
  { param: 'sp', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a process value cannot be assumed' },
  { param: 'cv', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a process value cannot be assumed' },
  { param: 'minRange', valuePattern: '-?\\d+(\\.\\d+)?', why: 'an engineering range is stored configuration' },
  { param: 'maxRange', valuePattern: '-?\\d+(\\.\\d+)?', why: 'an engineering range is stored configuration' },
  { param: 'highLimit', valuePattern: '-?\\d+(\\.\\d+)?', why: 'an alarm limit is stored configuration' },
  { param: 'highHighLimit', valuePattern: '-?\\d+(\\.\\d+)?', why: 'an alarm limit is stored configuration' },
  { param: 'lowLimit', valuePattern: '-?\\d+(\\.\\d+)?', why: 'an alarm limit is stored configuration' },
  { param: 'lowLowLimit', valuePattern: '-?\\d+(\\.\\d+)?', why: 'an alarm limit is stored configuration' },
  { param: 'kp', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a tuning parameter is stored configuration' },
  { param: 'tn', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a tuning parameter is stored configuration' },
  { param: 'tv', valuePattern: '-?\\d+(\\.\\d+)?', why: 'a tuning parameter is stored configuration' },
];

/** Matches `<param> = <literal>` inside a destructuring pattern (a parameter default). */
function findParamDefault(source: string, param: string, valuePattern: string): string | null {
  const re = new RegExp(`\\b${param}\\s*=\\s*(${valuePattern})(?=[\\s,}])`);
  const match = source.match(re);
  return match ? match[0] : null;
}

describe('SYS-REQ-096: faceplates carry no fabricating parameter defaults', () => {
  const sources = componentSources();

  it('scans every faceplate component', () => {
    expect(sources.length).toBeGreaterThanOrEqual(4);
  });

  it.each(FORBIDDEN_PARAM_DEFAULTS)(
    'no $param default ($why)',
    ({ param, valuePattern, why }) => {
      const offenders = sources
        .map(({ file, source }) => ({ file, found: findParamDefault(source, param, valuePattern) }))
        .filter((entry) => entry.found !== null)
        .map((entry) => `${entry.file}: ${entry.found}`);
      expect(offenders, `${param} must not default to a device value — ${why}`).toEqual([]);
    }
  );

  it('no faceplate samples device state into local state at mount', () => {
    // The mirror-image defect of a fabricating default: a true state the surface suppresses.
    // `useState(operatingMode ?? null)` reads the prop once, so telemetry that arrives after
    // mount is never announced — and the canvas re-renders faceplates in place (keyed on the
    // component id), so there is no remount to rescue it. Device state is read from the prop on
    // every render; only operator intent may live in component state.
    for (const { file, source } of sources) {
      expect(source, `${file} must not snapshot operatingMode into state`).not.toMatch(
        /useState\s*<[^>]*>\s*\(\s*operatingMode|useState\s*\(\s*operatingMode/
      );
    }
  });

  it('never derives a slider range from the value being edited', () => {
    // The pre-fix defect: `spMax ?? (displayable && targetSp !== null ? Math.max(100, targetSp) : 100)`.
    // A range is a claim about what the device ACCEPTS, so an invented maximum is a fabricated
    // constraint — and one derived from the setpoint moves as the operator drags. Behavioural
    // tests cannot see this behind a disabled control, so the shape is guarded here.
    const RANGE_FROM_VALUE = /(SliderMax|SliderMin|RangeMax|RangeMin)\s*=[^;\n]*\b(targetSp|targetCv|inputSpeed|speedSetpoint|pv)\b|(?:min|max)=\{[^}]*\b(targetSp|targetCv|inputSpeed|speedSetpoint)\b/;
    for (const { file, source } of sources) {
      expect(source, `${file} sizes a slider range from the edited value`).not.toMatch(RANGE_FROM_VALUE);
    }
  });

  it('no faceplate paints a colour from an ungated device-state prop', () => {
    // `className={running === true ? 'text-emerald-300' : ...}` printed `----` in the RUNNING
    // colour on an unbound device. The text already went through the quality gate; the class
    // bypassed it. Colour must be driven by the gated value (`isRunning`, `isTripped`, ...).
    const RAW_STATE_COMPARISON = /className=\{[^}]*\b(running|tripped|openLimit|closedLimit|transitFault|sensorFault)\s*===/;
    for (const { file, source } of sources) {
      expect(source, `${file} paints from a raw device-state prop`).not.toMatch(RAW_STATE_COMPARISON);
    }
  });

  it('every faceplate takes quality as a required parameter with no default', () => {
    // Same discipline as the canvas: a faceplate that cannot prove its values are live must not
    // fall back to a permissive default. It is destructured with no `=` at all.
    for (const { file, source } of sources) {
      const destructured = /\n\s*quality,\s*\n/.test(source);
      expect(destructured, `${file} must destructure 'quality,' with no default`).toBe(true);
      // A parameter default (`quality = ...`), as opposed to a JSX attribute (`quality={quality}`).
      const anyDefault = /\bquality\s*=\s*(?!\{)/.test(source);
      expect(anyDefault, `${file} must not give quality any default value`).toBe(false);
    }
  });
});
