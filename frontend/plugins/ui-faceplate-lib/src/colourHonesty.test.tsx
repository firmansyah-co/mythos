/**
 * SYS-REQ-096 — a colour shown for a state the surface never measured.
 *
 * `motor-run-state` printed `----` (honest) in the RUNNING colour, because the class read the raw
 * `running` prop while every sibling read the gated value. An operator scanning colour sees green
 * where nothing was measured. The same shape sits one level down in the interlock rows, where a
 * caller-supplied `isSatisfied: true` was painted in the reassuring colour on a quiescent device.
 *
 * Asserted on the CLASS, not the text: the text already went through the quality gate, so a text
 * assertion passes against the defect and proves nothing.
 *
 * Latent today because the only call sites pass quality="NOT_BOUND". It becomes live the moment a
 * telemetry transport populates these props.
 */

import { describe, it, expect } from 'vitest';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { MotorFaceplate } from './components/MotorFaceplate';
import { ValveFaceplate } from './components/ValveFaceplate';
import { AnalogTransmitterFaceplate } from './components/AnalogTransmitterFaceplate';
import { PidControllerFaceplate } from './components/PidControllerFaceplate';

function normalizeHtml(html: string): string {
  return html.replace(/<!-- -->/g, '');
}

/** The opening tag carrying `data-testid="<id>"`, attributes included. */
function markupOf(html: string, testid: string): string {
  const start = html.indexOf(`data-testid="${testid}"`);
  expect(start, `missing ${testid}`).toBeGreaterThan(-1);
  const end = html.indexOf('>', start);
  return html.slice(start, end);
}

/** The class attribute of the element carrying `data-testid="<id>"`. */
function classOf(html: string, testid: string): string {
  const tag = markupOf(html, testid);
  const match = tag.match(/class="([^"]*)"/);
  expect(match, `no class on ${testid}`).not.toBeNull();
  return match![1];
}

/** The saturated run colour. Reserved for a positive, live observation. */
const RUNNING_COLOUR = 'text-emerald-300';

const MOTOR = {
  tag: 'MTR-700',
  label: 'Run-state fixture',
  running: true,
  tripped: false,
  actualSpeed: 1450,
  speedSetpoint: 1500,
  tier: 2 as const,
};

describe('faceplate colour follows its provenance gate', () => {

  describe('run-state colour is gated on live telemetry', () => {
    it('does not paint an unbound motor with the running colour', () => {
      const html = normalizeHtml(
        renderToString(React.createElement(MotorFaceplate, { ...MOTOR, quality: 'NOT_BOUND' }))
      );
      expect(classOf(html, 'motor-run-state')).not.toContain(RUNNING_COLOUR);
      expect(classOf(html, 'motor-run-state')).toContain('text-slate-300');
    });

    it('does not paint a bound but silent motor with the running colour', () => {
      const html = normalizeHtml(
        renderToString(React.createElement(MotorFaceplate, { ...MOTOR, quality: 'BAD_QUALITY' }))
      );
      expect(classOf(html, 'motor-run-state')).not.toContain(RUNNING_COLOUR);
    });

    it('still paints a live running motor, and a live stopped one is neutral', () => {
      const running = normalizeHtml(
        renderToString(React.createElement(MotorFaceplate, { ...MOTOR, quality: 'GOOD' }))
      );
      expect(classOf(running, 'motor-run-state')).toContain(RUNNING_COLOUR);

      const stopped = normalizeHtml(
        renderToString(
          React.createElement(MotorFaceplate, { ...MOTOR, quality: 'GOOD', running: false })
        )
      );
      expect(classOf(stopped, 'motor-run-state')).not.toContain(RUNNING_COLOUR);
    });
  });

  describe('interlock rows are device state, not configuration', () => {
    const SATISFIED = [{ id: 'guard', name: 'Guard Closed', isSatisfied: true }];

    it('does not show satisfied permissives for a device it cannot observe', () => {
      const motor = normalizeHtml(
        renderToString(
          React.createElement(MotorFaceplate, {
            ...MOTOR,
            quality: 'NOT_BOUND',
            tier: 3,
            interlocks: SATISFIED,
          })
        )
      );
      expect(motor).not.toContain('[✓] OK');
      expect(motor).not.toContain('text-emerald-400');
      expect(motor).toContain('device is not reporting');

      const valve = normalizeHtml(
        renderToString(
          React.createElement(ValveFaceplate, {
            tag: 'XV-700',
            label: 'Interlock fixture',
            openLimit: true,
            closedLimit: false,
            transitFault: false,
            quality: 'NOT_BOUND',
            tier: 3,
            interlocks: SATISFIED,
          })
        )
      );
      expect(valve).not.toContain('[✓] OK');
      expect(valve).not.toContain('text-emerald-400');
    });

    it('shows the permissives a live device actually reports', () => {
      const html = normalizeHtml(
        renderToString(
          React.createElement(MotorFaceplate, {
            ...MOTOR,
            quality: 'GOOD',
            tier: 3,
            interlocks: SATISFIED,
          })
        )
      );
      expect(html).toContain('[✓] OK');
      expect(html).toContain('Guard Closed');
    });
  });


  describe('the sweep: no other faceplate paints a state it did not measure', () => {
    it('renders every faceplate quiescent when the caller passes reassuring raw props', () => {
      // Each faceplate is given props that all say "healthy" alongside NOT_BOUND. Nothing
      // saturated may reach a state element: the alarm annunciation is itself gated, so it
      // cannot appear either.
      const render = (element: React.ReactElement) => normalizeHtml(renderToString(element));

      const motor = render(
        React.createElement(MotorFaceplate, { ...MOTOR, quality: 'NOT_BOUND', tier: 2 })
      );
      const valve = render(
        React.createElement(ValveFaceplate, {
          tag: 'XV-702',
          label: 'Quiescent valve',
          openLimit: true,
          closedLimit: false,
          transitFault: false,
          positionPercent: 100,
          quality: 'NOT_BOUND',
          tier: 2,
        })
      );
      const pid = render(
        React.createElement(PidControllerFaceplate, {
          tag: 'TIC-701',
          label: 'Quiescent loop',
          pv: 82.5,
          sp: 80,
          cv: 45.2,
          unit: '°C',
          quality: 'NOT_BOUND',
          tier: 2,
        })
      );
      const transmitter = render(
        React.createElement(AnalogTransmitterFaceplate, {
          tag: 'PT-700',
          label: 'Quiescent transmitter',
          pv: 6.84,
          unit: 'bar',
          minRange: 0,
          maxRange: 16,
          highLimit: 12,
          sensorFault: false,
          quality: 'NOT_BOUND',
          tier: 2,
        })
      );

      for (const [name, html] of Object.entries({ motor, valve, pid, transmitter })) {
        // State-dependent colours on the state elements. A static design colour on a label
        // (the PID setpoint is always sky) is not a state claim and is not asserted here.
        for (const testid of ['motor-run-state', 'motor-trip-state', 'valve-status']) {
          if (html.includes(`data-testid="${testid}"`)) {
            const cls = classOf(html, testid);
            expect(cls, `${name} ${testid}`).not.toContain(RUNNING_COLOUR);
            expect(cls, `${name} ${testid}`).not.toContain('text-red-300');
            expect(cls, `${name} ${testid}`).not.toContain('text-amber-400');
          }
        }
        // No annunciation for a state nobody measured.
        expect(html, `${name} alarm badge`).not.toContain('isa18-alarm-badge');
        expect(html, `${name} alarm text`).not.toContain('ALARM:');
        expect(html, `${name} interlock colour`).not.toContain('text-emerald-400');
        expect(html, `${name} interlock row`).not.toContain('[✓] OK');
        expect(html, `${name} mode badge`).not.toContain('data-testid="pid-mode-badge"');
        expect(html, `${name} mode badge`).not.toContain('data-testid="pid-mode"');
      }
    });
  });
});
