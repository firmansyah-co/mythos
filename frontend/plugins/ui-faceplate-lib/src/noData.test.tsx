/**
 * SYS-REQ-096 — Zero Silent Fallback: no-data states in the faceplate library.
 *
 * These are negative tests. Each asserts that a faceplate which cannot vouch for its process
 * values renders an explicit quiescent state instead of a number. They fail against the pre-fix
 * code (which substituted plausible literals) and against any permissive quality default.
 */

import { describe, it, expect } from 'vitest';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { MotorFaceplate } from './components/MotorFaceplate';
import { ValveFaceplate } from './components/ValveFaceplate';
import { AnalogTransmitterFaceplate } from './components/AnalogTransmitterFaceplate';
import { PidControllerFaceplate } from './components/PidControllerFaceplate';
import { DEFAULT_QUALITY, NO_DATA_TEXT } from './processData';

function normalizeHtml(html: string): string {
  return html.replace(/<!-- -->/g, '');
}

/** Text content of the element carrying `data-testid="<id>"`, tags stripped. */
function textOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`data-testid="${testid}"[^>]*>([\\s\\S]*?)</span>`));
  if (!match) return null;
  return match[1].replace(/<[^>]*>/g, '').trim();
}

/** Values indistinguishable from a live, healthy plant — exactly what must never be invented. */
const MOTOR_VALUES = {
  running: true,
  tripped: false,
  actualSpeed: 1450,
  speedSetpoint: 1500,
  currentAmps: 18.5,
  torquePercent: 78,
};
const VALVE_VALUES = { openLimit: true, closedLimit: false, transitFault: false, positionPercent: 100 };
const PID_VALUES = { pv: 82.5, sp: 80.0, cv: 45.2, unit: '°C', kp: 1.5, tn: 20, tv: 3 };
const TRANSMITTER_VALUES = { pv: 6.84, unit: 'bar', minRange: 0, maxRange: 16, highLimit: 12, highHighLimit: 14 };

function expectQuiescent(testids: string[], html: string) {
  for (const testid of testids) {
    expect(textOf(html, testid), `${testid} must be quiescent`).toBe(NO_DATA_TEXT);
  }
}

describe('SYS-REQ-096: faceplates never fabricate process values', () => {
  describe('NOT_BOUND (no device binding)', () => {
    it('Motor shows no speed, current or torque and declares NOT BOUND', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'MTR-900',
          label: 'Unbound motor',
          quality: 'NOT_BOUND',
          ...MOTOR_VALUES,
          tier: 2,
        })
      ));
      expect(html).toContain('data-quality="NOT_BOUND"');
      expect(html).toContain('NOT BOUND');
      expectQuiescent(['motor-speed', 'motor-setpoint', 'motor-current', 'motor-torque', 'motor-run-state', 'motor-trip-state'], html);
      // The START/STOP controls must not be usable against a device that is not connected.
      expect(html).toMatch(/data-testid="btn-motor-start"[^>]*disabled/);
      expect(html).toMatch(/data-testid="btn-motor-stop"[^>]*disabled/);
    });

    it('Valve shows no position or open state and declares NOT BOUND', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(ValveFaceplate, {
          tag: 'XV-900',
          label: 'Unbound valve',
          quality: 'NOT_BOUND',
          ...VALVE_VALUES,
          tier: 2,
        })
      ));
      expect(html).toContain('NOT BOUND');
      expect(textOf(html, 'valve-position')).toBe(NO_DATA_TEXT);
      expect(textOf(html, 'valve-status')).toBe(NO_DATA_TEXT);
      expect(html).not.toContain('FULLY OPEN');
      expect(html).toMatch(/data-testid="btn-valve-open"[^>]*disabled/);
      expect(html).toMatch(/data-testid="btn-valve-close"[^>]*disabled/);
    });

    it('PID controller shows no PV, SP or CV and declares NOT BOUND', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(PidControllerFaceplate, {
          tag: 'TIC-900',
          label: 'Unbound loop',
          quality: 'NOT_BOUND',
          ...PID_VALUES,
          tier: 2,
        })
      ));
      expect(html).toContain('NOT BOUND');
      expectQuiescent(['pid-pv', 'pid-sp', 'pid-cv'], html);
    });

    it('Transmitter shows no PV and declares NOT BOUND', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(AnalogTransmitterFaceplate, {
          tag: 'PT-900',
          label: 'Unbound transmitter',
          quality: 'NOT_BOUND',
          ...TRANSMITTER_VALUES,
          tier: 2,
        })
      ));
      expect(html).toContain('NOT BOUND');
      expect(textOf(html, 'transmitter-pv')).toBe(NO_DATA_TEXT);
    });
  });

  describe('BAD_QUALITY (bound but not reporting)', () => {
    it('Motor suppresses values and annunciates BAD COMM FAILURE', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'MTR-901',
          label: 'Silent motor',
          quality: 'BAD_QUALITY',
          ...MOTOR_VALUES,
          tier: 2,
        })
      ));
      expect(html).toContain('BAD COMM FAILURE');
      expect(html).toContain('data-quality="BAD_QUALITY"');
      expectQuiescent(['motor-speed', 'motor-current', 'motor-torque', 'motor-run-state', 'motor-trip-state'], html);
      expect(html).not.toContain('RUNNING');
    });
  });

  describe('undeclared provenance is not treated as live', () => {
    it('defaults to the fail-closed quality', () => {
      expect(DEFAULT_QUALITY).toBe('NOT_BOUND');
    });

    it('Motor rendered with values but no quality prop shows ---- and NOT BOUND', () => {
      // A JavaScript or plugin caller can omit the prop despite the required type: the runtime
      // backstop must fail closed, not present the values as live.
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'MTR-902',
          label: 'Undeclared motor',
          ...MOTOR_VALUES,
          tier: 2,
        } as any)
      ));
      expect(html).toContain('NOT BOUND');
      expectQuiescent(['motor-speed', 'motor-setpoint', 'motor-current', 'motor-torque'], html);
    });

    it('Valve rendered with values but no quality prop shows ---- and NOT BOUND', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(ValveFaceplate, {
          tag: 'XV-902',
          label: 'Undeclared valve',
          ...VALVE_VALUES,
          tier: 2,
        } as any)
      ));
      expect(html).toContain('NOT BOUND');
      expect(html).not.toContain('FULLY OPEN');
      expect(textOf(html, 'valve-position')).toBe(NO_DATA_TEXT);
    });
  });

  describe('booleans: an absent value is unknown, never the reassuring state', () => {
    it('Motor with no tripped value does not render as healthy', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'MTR-903',
          label: 'Motor without trip feedback',
          quality: 'GOOD',
          actualSpeed: 1450,
          speedSetpoint: 1500,
          tier: 2,
        } as any)
      ));
      expect(textOf(html, 'motor-trip-state')).toBe(NO_DATA_TEXT);
      expect(textOf(html, 'motor-run-state')).toBe(NO_DATA_TEXT);
      expect(html).not.toContain('NO TRIP');
      expect(html).not.toContain('RUNNING');
    });

    it('Valve with no limit-switch feedback does not render as open or in motion', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(ValveFaceplate, {
          tag: 'XV-903',
          label: 'Valve without limits',
          quality: 'GOOD',
          positionPercent: 100,
          tier: 2,
        } as any)
      ));
      expect(html).not.toContain('FULLY OPEN');
      expect(html).not.toContain('IN MOTION');
      expect(html).not.toContain('FULLY CLOSED');
      expect(textOf(html, 'valve-status')).toBe(NO_DATA_TEXT);
    });

    it('Transmitter with no PV does not render a reading', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(AnalogTransmitterFaceplate, {
          tag: 'PT-903',
          label: 'Transmitter without reading',
          quality: 'GOOD',
          unit: 'bar',
          tier: 2,
        } as any)
      ));
      expect(textOf(html, 'transmitter-pv')).toBe(NO_DATA_TEXT);
    });
  });

  describe('device state and configuration are not invented either', () => {
    it('unbound Motor asserts no control mode', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'MTR-905',
          label: 'Unbound motor',
          quality: 'NOT_BOUND',
          tier: 2,
        })
      ));
      expect(html).not.toContain('AUTO');
      expect(html).not.toContain('MANUAL');
      expect(html).not.toContain('CASCADE');
      expect(html).not.toContain('data-testid="motor-mode"');
    });

    it('unbound Valve asserts no control mode', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(ValveFaceplate, {
          tag: 'XV-905',
          label: 'Unbound valve',
          quality: 'NOT_BOUND',
          tier: 2,
        })
      ));
      expect(html).not.toContain('AUTO');
      expect(html).not.toContain('MANUAL');
      expect(html).not.toContain('data-testid="valve-mode"');
    });

    it('unbound PID controller asserts no control mode', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(PidControllerFaceplate, {
          tag: 'TIC-905',
          label: 'Unbound loop',
          quality: 'NOT_BOUND',
          tier: 2,
        })
      ));
      expect(html).not.toContain('AUTO');
      expect(html).not.toContain('MANUAL');
      expect(html).not.toContain('CASCADE');
      expect(html).not.toContain('data-testid="pid-mode-badge"');
    });

    it('Motor with an absent speed range does not print an invented span', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'MTR-906',
          label: 'Motor without a stored range',
          quality: 'NOT_BOUND',
          tier: 3,
        })
      ));
      expect(html).not.toContain('1500');
      expect(html).not.toContain('Speed Target (0 - 1500');
    });

    it('Transmitter with no stored measurement type prints no type chip', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(AnalogTransmitterFaceplate, {
          tag: 'PT-906',
          label: 'Typeless transmitter',
          quality: 'NOT_BOUND',
          tier: 2,
        })
      ));
      expect(html).not.toContain('PRESSURE');
      expect(html).not.toContain('TEMPERATURE');
      expect(html).not.toContain('data-testid="transmitter-type"');
    });

    it('live telemetry still announces a real control mode', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'MTR-907',
          label: 'Live motor in manual',
          quality: 'GOOD',
          operatingMode: 'MANUAL',
          ...MOTOR_VALUES,
          tier: 2,
        })
      ));
      expect(textOf(html, 'motor-mode')).toBe('MANUAL');
    });
  });

  describe('live path: values present but state/config absent', () => {
    // The quality gate suppresses everything when quality is not GOOD, so it cannot show whether a
    // parameter default fabricates a value. These render with quality='GOOD' and omit the state and
    // configuration props entirely — the only shape in which a default becomes visible.

    it('Motor with GOOD quality and no operatingMode does not assert AUTO', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'MTR-910',
          label: 'Live motor with no reported mode',
          quality: 'GOOD',
          running: true,
          tripped: false,
          actualSpeed: 1450,
          speedSetpoint: 1500,
          tier: 2,
        })
      ));
      expect(html).not.toContain('AUTO');
      expect(html).not.toContain('MANUAL');
      expect(html).not.toContain('CASCADE');
      expect(html).not.toContain('data-testid="motor-mode"');
    });

    it('Valve with GOOD quality and no operatingMode does not assert AUTO', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(ValveFaceplate, {
          tag: 'XV-910',
          label: 'Live valve with no reported mode',
          quality: 'GOOD',
          openLimit: true,
          closedLimit: false,
          transitFault: false,
          positionPercent: 100,
          tier: 2,
        })
      ));
      expect(html).not.toContain('AUTO');
      expect(html).not.toContain('MANUAL');
      expect(html).not.toContain('data-testid="valve-mode"');
    });

    it('PID with GOOD quality and no operatingMode does not assert AUTO', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(PidControllerFaceplate, {
          tag: 'TIC-910',
          label: 'Live loop with no reported mode',
          quality: 'GOOD',
          pv: 82.5,
          sp: 80.0,
          cv: 45.2,
          tier: 2,
        })
      ));
      expect(html).not.toContain('AUTO');
      expect(html).not.toContain('MANUAL');
      expect(html).not.toContain('CASCADE');
      expect(html).not.toContain('data-testid="pid-mode-badge"');
    });

    it('Transmitter with GOOD quality and no transmitterType shows no type chip', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(AnalogTransmitterFaceplate, {
          tag: 'PT-910',
          label: 'Live transmitter with no stored type',
          quality: 'GOOD',
          pv: 6.84,
          unit: 'bar',
          tier: 2,
        })
      ));
      expect(html).not.toContain('PRESSURE');
      expect(html).not.toContain('TEMPERATURE');
      expect(html).not.toContain('data-testid="transmitter-type"');
    });

    it('Motor with GOOD quality and no maxSpeed prints no invented speed span', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'MTR-911',
          label: 'Live motor with no stored range',
          quality: 'GOOD',
          running: true,
          tripped: false,
          actualSpeed: 1450,
          speedSetpoint: 1500,
          tier: 3,
        })
      ));
      expect(html).not.toContain('1500 RPM)');
      expect(html).not.toContain('Speed Target (0 - 1500');
    });
  });

  describe('live telemetry still renders normally', () => {
    it('Motor with GOOD quality renders its real values', () => {
      const html = normalizeHtml(renderToString(
        React.createElement(MotorFaceplate, {
          tag: 'MTR-904',
          label: 'Live motor',
          quality: 'GOOD',
          ...MOTOR_VALUES,
          tier: 2,
        })
      ));
      expect(textOf(html, 'motor-speed')).toBe('1450');
      expect(textOf(html, 'motor-current')).toBe('18.5 A');
      expect(textOf(html, 'motor-trip-state')).toBe('NO TRIP');
      expect(html).not.toContain('NOT BOUND');
      expect(html).not.toContain('faceplate-no-data');
    });
  });
});
/**
 * SYS-REQ-096 — a slider's RANGE is a claim about what the device accepts.
 *
 * `PidControllerFaceplate` computed its setpoint span as
 * `spMax ?? (displayable && targetSp !== null ? Math.max(100, targetSp) : 100)`, inventing a
 * maximum whenever the device reported none — and deriving it from the setpoint being edited, so
 * the accepted range moved as the operator dragged. An invented maximum is a fabricated *constraint*
 * rather than a fabricated reading: it can show the operator a value the device will reject, or stop
 * them short of one it would take.
 *
 * These assert on the RANGE. A test that the current value renders correctly passes before and
 * after this change and proves nothing here.
 */
describe('SYS-REQ-096: a setpoint slider never invents the range it accepts', () => {
  const PID = { tag: 'TIC-800', label: 'Range fixture', pv: 55, cv: 45.2, unit: 'degC', tier: 2 as const };

  /** The opening tag of the setpoint slider, attributes included. */
  function sliderMarkup(props: Record<string, unknown>): string {
    const html = normalizeHtml(
      renderToString(React.createElement(PidControllerFaceplate, { ...PID, ...props, quality: 'GOOD' }))
    );
    const start = html.indexOf('data-testid="pid-sp-slider"');
    expect(start, 'setpoint slider missing').toBeGreaterThan(-1);
    // The className sits before the attributes, so walk back to the tag's opening bracket.
    const open = html.lastIndexOf('<input', start);
    return html.slice(open, html.indexOf('>', start));
  }

  it('presents no fabricated maximum when the device reports no range', () => {
    const tag = sliderMarkup({ sp: 42.5 });
    expect(tag).toContain('data-range-known="false"');
    // `disabled=""`, not the substring `disabled`: the className carries `disabled:opacity-30`.
    expect(tag).toContain('disabled=""');
    // The pre-fix code produced max="100" here.
    expect(tag).not.toContain('max="100"');
  });

  it('does not size the range from the setpoint being edited', () => {
    // The sharper half of the defect: with the old expression a large setpoint produced a large
    // invented maximum (Math.max(100, 250) === 250), so the accepted range followed the drag.
    const tag = sliderMarkup({ sp: 250 });
    expect(tag).toContain('data-range-known="false"');
    expect(tag).not.toContain('max="250"');
    expect(tag).not.toContain('max="100"');
  });

  it('requires BOTH ends before it will offer a span', () => {
    expect(sliderMarkup({ sp: 42.5, spMax: 90 })).toContain('data-range-known="false"');
    expect(sliderMarkup({ sp: 42.5, spMin: 10 })).toContain('data-range-known="false"');
    expect(sliderMarkup({ sp: 42.5, spMin: 10, spMax: 90 })).toContain('data-range-known="true"');
  });

  it('uses the range the device reported, and only then makes the control usable', () => {
    const tag = sliderMarkup({ sp: 42.5, spMin: 10, spMax: 90 });
    expect(tag).toContain('min="10"');
    expect(tag).toContain('max="90"');
    expect(tag).not.toContain('disabled=""');
  });

  it('states the range it will accept, and says so in words when there is none', () => {
    const known = normalizeHtml(
      renderToString(
        React.createElement(PidControllerFaceplate, { ...PID, quality: 'GOOD', sp: 42.5, spMin: 10, spMax: 90 })
      )
    );
    expect(known).toContain('Target Setpoint (10 - 90)');

    const unknown = normalizeHtml(
      renderToString(React.createElement(PidControllerFaceplate, { ...PID, quality: 'GOOD', sp: 42.5 }))
    );
    expect(unknown).toContain('Target Setpoint (----)');
  });

  it('leaves a degenerate span unusable rather than inviting a write that means nothing', () => {
    const tag = sliderMarkup({ sp: 42.5, spMin: 42.5, spMax: 42.5 });
    expect(tag).toContain('data-range-known="false"');
    // `disabled=""`, not the substring `disabled`: the className carries `disabled:opacity-30`.
    expect(tag).toContain('disabled=""');
  });
});
