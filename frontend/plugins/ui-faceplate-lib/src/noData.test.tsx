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