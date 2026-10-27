/**
 * SYS-REQ-096 — the mirror-image failure: a true state the surface suppresses.
 *
 * `useState(operatingMode ?? null)` sampled the prop once at mount with no re-sync, so a control
 * mode that arrived with telemetry after mount was never announced. The canvas keys a faceplate on
 * its component id (DockviewWorkspace), so a later data update re-renders the same instance and
 * there is no remount to rescue it. A badge that never appears is as wrong as a value that is
 * invented: absence reads as "nothing to report".
 *
 * These need a mounted instance re-rendered in place — a `renderToString` tree is rendered once
 * and cannot see a prop update at all.
 *
 * Latent today because the only call sites pass quality="NOT_BOUND". It becomes live the moment a
 * telemetry transport populates these props.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { MotorFaceplate } from './components/MotorFaceplate';
import { ValveFaceplate } from './components/ValveFaceplate';
import { PidControllerFaceplate } from './components/PidControllerFaceplate';

function normalizeHtml(html: string): string {
  return html.replace(/<!-- -->/g, '');
}

/** The class attribute of the element carrying `data-testid="<id>"`. */
function classOf(html: string, testid: string): string {
  const start = html.indexOf(`data-testid="${testid}"`);
  expect(start, `missing ${testid}`).toBeGreaterThan(-1);
  const end = html.indexOf('>', start);
  const match = html.slice(start, end).match(/class="([^"]*)"/);
  expect(match, `no class on ${testid}`).not.toBeNull();
  return match![1];
}

const MOTOR = {
  tag: 'MTR-700',
  label: 'Mode fixture',
  running: true,
  tripped: false,
  actualSpeed: 1450,
  speedSetpoint: 1500,
  tier: 2 as const,
};

const VALVE = {
  tag: 'XV-701',
  label: 'Mode fixture',
  openLimit: true,
  closedLimit: false,
  transitFault: false,
  tier: 2 as const,
};

const PID = {
  tag: 'TIC-700',
  label: 'Mode fixture',
  pv: 82.5,
  sp: 80,
  cv: 45.2,
  unit: '°C',
  tier: 2 as const,
};

describe('a faceplate announces a control mode that arrives after mount', () => {
  let container: HTMLDivElement;
  let root: Root;

  const draw = (element: React.ReactElement) => {
    flushSync(() => {
      root.render(element);
    });
    return normalizeHtml(container.innerHTML);
  };

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    flushSync(() => {
      root.unmount();
    });
    container.remove();
  });

  it('Motor: NOT_BOUND at mount, then live AUTO on the same instance', () => {
    const first = draw(React.createElement(MotorFaceplate, { ...MOTOR, quality: 'NOT_BOUND' }));
    expect(first).not.toContain('data-testid="motor-mode"');

    const second = draw(
      React.createElement(MotorFaceplate, { ...MOTOR, quality: 'GOOD', operatingMode: 'AUTO' })
    );
    expect(second).toContain('data-testid="motor-mode"');
    expect(classOf(second, 'motor-mode')).toContain('text-sky-300');
  });

  it('Motor: a mode change after mount moves the badge, and a dropped mode removes it', () => {
    draw(React.createElement(MotorFaceplate, { ...MOTOR, quality: 'GOOD', operatingMode: 'AUTO' }));

    const manual = draw(
      React.createElement(MotorFaceplate, { ...MOTOR, quality: 'GOOD', operatingMode: 'MANUAL' })
    );
    expect(manual).toMatch(/data-testid="motor-mode"[^>]*>MANUAL</);
    expect(manual).not.toMatch(/data-testid="motor-mode"[^>]*>AUTO</);

    const gone = draw(React.createElement(MotorFaceplate, { ...MOTOR, quality: 'GOOD' }));
    expect(gone).not.toContain('data-testid="motor-mode"');
  });

  it('Motor: a live device that reports no mode still announces none', () => {
    const html = draw(React.createElement(MotorFaceplate, { ...MOTOR, quality: 'GOOD' }));
    expect(html).not.toContain('data-testid="motor-mode"');
    expect(html).not.toContain('>AUTO<');
    expect(html).not.toContain('>MANUAL<');
  });

  it('Valve: a mode supplied only on the second render is announced', () => {
    const first = draw(React.createElement(ValveFaceplate, { ...VALVE, quality: 'NOT_BOUND' }));
    expect(first).not.toContain('data-testid="valve-mode"');

    const second = draw(
      React.createElement(ValveFaceplate, { ...VALVE, quality: 'GOOD', operatingMode: 'AUTO' })
    );
    expect(second).toContain('data-testid="valve-mode"');
    expect(second).toMatch(/data-testid="valve-mode"[^>]*>AUTO</);
  });

  it('PID: a mode supplied only on the second render is announced', () => {
    const first = draw(React.createElement(PidControllerFaceplate, { ...PID, quality: 'NOT_BOUND' }));
    expect(first).not.toContain('data-testid="pid-mode-badge"');

    const second = draw(
      React.createElement(PidControllerFaceplate, { ...PID, quality: 'GOOD', operatingMode: 'MANUAL' })
    );
    expect(second).toContain('data-testid="pid-mode-badge"');
    expect(second).toMatch(/data-testid="pid-mode-badge"[^>]*>MANUAL</);
  });

  it('the mode selector follows the device, so AUTO gating is not released optimistically', () => {
    // The motor's speed control is enabled only in MANUAL. Claiming the mode locally on click
    // would hand the operator a live command path on a controller that is still in AUTO.
    const slider = () => container.querySelector('input[type="range"]') as HTMLInputElement;

    draw(
      React.createElement(MotorFaceplate, {
        ...MOTOR,
        maxSpeed: 1500,
        quality: 'GOOD',
        operatingMode: 'AUTO',
        tier: 3,
      })
    );
    // Asserted on the DOM property: a client-rendered `disabled` is set as a property and does
    // not always survive innerHTML serialisation.
    expect(slider().disabled).toBe(true);

    draw(
      React.createElement(MotorFaceplate, {
        ...MOTOR,
        maxSpeed: 1500,
        quality: 'GOOD',
        operatingMode: 'MANUAL',
        tier: 3,
      })
    );
    expect(slider().disabled).toBe(false);
  });
});
