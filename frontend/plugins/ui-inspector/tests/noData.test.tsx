/**
 * SYS-REQ-096 — the Inspector Drawer must not fabricate device data.
 *
 * Negative tests. Each fails against the pre-fix drawer, which spread `DEFAULT_CONTROL`,
 * `DEFAULT_INTERLOCKS`, `DEFAULT_MAINTENANCE` and a `Math.random()` trend walk into every render.
 *
 * The drawer is read by a human under time pressure, so these also assert that the quiescent state
 * is *explained*, not merely blank: a grid of dashes without a reason is its own failure.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { InspectorDrawer } from '../src/InspectorDrawer';
import { InspectorTabType } from '../src/types';
import type { InterlockCondition, MiniTrendPoint, DeviceMaintenanceInfo, ManualControlState } from '../src/types';

function normalizeHtml(html: string): string {
  return html.replace(/<!-- -->/g, '');
}

function render(props: Record<string, unknown>, tab: InspectorTabType = 'MANUAL_CONTROL'): string {
  return normalizeHtml(
    renderToString(
      React.createElement(InspectorDrawer, {
        isOpen: true,
        onClose: () => {},
        initialTab: tab,
        ...props,
      } as any)
    )
  );
}

/** Text content of the element carrying the given data-testid, tags stripped. */
function textOf(html: string, testid: string): string | null {
  const match = html.match(new RegExp(`data-testid="${testid}"[^>]*>([\\s\\S]*?)</(?:span|div|h2)>`));
  if (!match) return null;
  return match[1].replace(/<[^>]*>/g, '').trim();
}

/** Every literal the pre-fix drawer fabricated. None may appear on an unbound render. */
const FABRICATED_LITERALS = [
  '48.7',
  '50.0',
  '62.4',
  '4328.5',
  '1420',
  '00:1B:1B:4F:92:0A',
  'S-VPA2026-99482',
  'V2.9.2-RT',
  '2026-08-15',
  '2027-02-15',
  'Emergency Stop Chain',
  'Motor Thermal Overload',
  'Suction Line Pressure Permissive',
  'Discharge Isolation Valve Limit Switch',
  'Seal Flush Lubrication Flow',
];

const REAL_INTERLOCKS: InterlockCondition[] = [
  {
    id: 'int-1',
    name: 'Guard Closed',
    expression: '%I0.0 == TRUE',
    isPermissive: true,
    status: 'SATISFIED',
    channelAddress: '%I0.0',
  },
];

const REAL_TREND: MiniTrendPoint[] = [
  { timestamp: 1_700_000_000_000, pv: 41.2, sp: 42.0 },
  { timestamp: 1_700_000_001_000, pv: 41.6, sp: 42.0 },
  { timestamp: 1_700_000_002_000, pv: 41.9, sp: 42.0 },
];

const REAL_MAINTENANCE: DeviceMaintenanceInfo = {
  operatingHours: 120.5,
  totalStarts: 7,
  dcpMacAddress: 'AA:BB:CC:DD:EE:FF',
  serialNumber: 'SN-REAL-1',
  firmwareVersion: 'FW-1.0.0',
};

const REAL_CONTROL: ManualControlState = {
  isAutoMode: false,
  setpoint: 42.0,
  processValue: 41.2,
  minSetpoint: 0,
  maxSetpoint: 100,
  engineeringUnit: 'bar',
  outputPowerPct: 33.0,
};


/**
 * Prop-update path.
 *
 * `renderToString` renders each tree once, so it cannot see the defect this block covers: the
 * drawer used to snapshot `controlState` into state at mount (`useState(controlState ?? {})`)
 * and never re-read it. It is latent only while the sole call site passes `quality="NOT_BOUND"`;
 * the moment telemetry populates the props, a device that starts reporting after mount would
 * show its mount-time reading under GOOD — a stale number presented as a measurement.
 *
 * These render a mounted instance and re-render the SAME instance, which is what the canvas does
 * (DockviewWorkspace keys the drawer on the component id, so a data update re-renders in place).
 */
describe('SYS-REQ-096: the drawer re-reads device state supplied after mount', () => {
  let container: HTMLDivElement;
  let root: Root;

  const draw = (props: Record<string, unknown>) => {
    flushSync(() => {
      root.render(
        React.createElement(InspectorDrawer, {
          isOpen: true,
          onClose: () => {},
          initialTab: 'MANUAL_CONTROL',
          ...props,
        } as any)
      );
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

  it('renders a process value that arrives after mount, not the one from mount', () => {
    const first = draw({ quality: 'GOOD', controlState: { processValue: 12.5 } });
    expect(textOf(first, 'inspector-process-value')).toBe('12.5');

    const second = draw({ quality: 'GOOD', controlState: { processValue: 77.5 } });
    expect(textOf(second, 'inspector-process-value')).toBe('77.5');
    expect(second).not.toContain('>12.5<');
  });

  it('updates the output reading and the setpoint readout after mount', () => {
    draw({ quality: 'GOOD', controlState: { ...REAL_CONTROL, processValue: 12.5 } });

    const second = draw({
      quality: 'GOOD',
      controlState: { ...REAL_CONTROL, processValue: 77.5, outputPowerPct: 88.0, setpoint: 90.0 },
    });
    expect(textOf(second, 'inspector-output-power')).toBe('88.0');
    expect(textOf(second, 'inspector-setpoint')).toBe('90.0 bar');
    expect(second).not.toContain('>12.5<');
    expect(second).not.toContain('>33.0<');
  });

  it('announces a control mode that is supplied only on a later render', () => {
    const first = draw({ quality: 'GOOD', controlState: { processValue: 41.2 } });
    expect(textOf(first, 'inspector-control-mode')).toBe('----');

    const second = draw({ quality: 'GOOD', controlState: { processValue: 41.2, isAutoMode: true } });
    expect(textOf(second, 'inspector-control-mode')).toBe('CASCADE / AUTO');

    const third = draw({ quality: 'GOOD', controlState: { processValue: 41.2, isAutoMode: false } });
    expect(textOf(third, 'inspector-control-mode')).toBe('HAND / MANUAL');
  });

  it('follows the device from NOT_BOUND to GOOD when telemetry starts', () => {
    const unbound = draw({ quality: 'NOT_BOUND' });
    expect(textOf(unbound, 'inspector-process-value')).toBe('----');

    const live = draw({ quality: 'GOOD', controlState: { processValue: 61.4 } });
    expect(textOf(live, 'inspector-process-value')).toBe('61.4');
    expect(live).not.toContain('data-testid="inspector-no-data-banner"');
  });

  it('park a stale reading when the device stops reporting', () => {
    draw({ quality: 'GOOD', controlState: { processValue: 61.4 } });

    const dropped = draw({ quality: 'BAD_QUALITY', controlState: { processValue: 61.4 } });
    expect(textOf(dropped, 'inspector-process-value')).toBe('----');
    expect(dropped).not.toContain('>61.4<');
  });

  it('does not invent a setpoint range the device did not report', () => {
    const noRange = draw({ quality: 'GOOD', controlState: { processValue: 41.2, setpoint: 42.0 } });
    expect(noRange).toMatch(/data-testid="inspector-setpoint-slider"[^>]*\sdisabled=""/);
    expect(noRange).toContain('data-range-known="false"');
    expect(noRange).not.toContain('>0.0 bar<');
    expect(noRange).not.toContain('>100.0 bar<');

    const withRange = draw({
      quality: 'GOOD',
      controlState: { processValue: 41.2, setpoint: 42.0, minSetpoint: 10, maxSetpoint: 90, engineeringUnit: 'bar' },
    });
    expect(withRange).toContain('data-range-known="true"');
    expect(withRange).not.toMatch(/data-testid="inspector-setpoint-slider"[^>]*\sdisabled=""/);
    expect(withRange).toContain('>10 bar<');
    expect(withRange).toContain('>90 bar<');
  });

  it('read the setpoint range the device reports, not a fixed 0-100', () => {
    const html = draw({
      quality: 'GOOD',
      controlState: { setpoint: 55, minSetpoint: 10, maxSetpoint: 90, engineeringUnit: 'bar' },
    });
    expect(html).toMatch(/data-testid="inspector-setpoint-slider"[^>]*min="10"/);
    expect(html).toMatch(/data-testid="inspector-setpoint-slider"[^>]*max="90"/);
  });
});

describe('SYS-REQ-096: the drawer fabricates nothing', () => {
  describe('unbound (NOT_BOUND)', () => {
    it('renders no process value, setpoint or output reading', () => {
      const html = render({ quality: 'NOT_BOUND' }, 'MANUAL_CONTROL');
      for (const literal of FABRICATED_LITERALS) {
        expect(html, `fabricated literal ${literal} rendered`).not.toContain(literal);
      }
    });

    it('shows the quiescent banner explaining why', () => {
      const html = render({ quality: 'NOT_BOUND' });
      expect(html).toContain('data-testid="inspector-no-data-banner"');
      expect(html).toContain('NOT BOUND');
      expect(html).toContain('not bound to a tag');
    });

    it('renders every readout as ---- rather than a number', () => {
      const html = render({ quality: 'NOT_BOUND' });
      expect(textOf(html, 'inspector-process-value')).toBe('----');
      expect(textOf(html, 'inspector-output-power')).toBe('----');
      expect(textOf(html, 'inspector-setpoint')).toBe('----');
      expect(textOf(html, 'inspector-control-mode')).toBe('----');
    });

    it('disables commands against a device it cannot observe', () => {
      const html = render({ quality: 'NOT_BOUND' });
      expect(html).toMatch(/data-testid="inspector-mode-toggle"[^>]*disabled/);
      expect(html).toMatch(/data-testid="inspector-jog-fwd"[^>]*disabled/);
      expect(html).toMatch(/data-testid="inspector-jog-rev"[^>]*disabled/);
    });

    it('does not invent a device identity', () => {
      const html = render({ quality: 'NOT_BOUND' });
      expect(textOf(html, 'inspector-device-name')).toBe('----');
      expect(textOf(html, 'inspector-device-id')).toBe('----');
      expect(textOf(html, 'inspector-device-type')).toBe('----');
      expect(html).not.toContain('DEV-PUMP-101');
      expect(html).not.toContain('Main Feed Pump 101');
      expect(html).not.toContain('Sinamics G120');
    });

    it('reports interlock states as unavailable, never as satisfied', () => {
      const html = render({ quality: 'NOT_BOUND' }, 'INTERLOCK_MATRIX');
      expect(html).toContain('data-testid="inspector-interlocks-empty"');
      expect(html).not.toContain('Permissive Satisfied');
      expect(html).not.toContain('Interlock Tripped');
      expect(html).not.toContain('data-testid="inspector-interlock-row"');
      for (const name of FABRICATED_LITERALS.slice(10)) {
        expect(html).not.toContain(name);
      }
    });

    it('renders an explicit empty trend state instead of a random walk', () => {
      const html = render({ quality: 'NOT_BOUND' }, 'MINI_TREND');
      expect(html).toContain('data-testid="inspector-trend-empty"');
      expect(html).not.toContain('<polyline');
      expect(textOf(html, 'inspector-setpoint')).toBeNull(); // KPI text lives in its own node
      expect(html).not.toContain('100 ms (10 Hz)');
    });

    it('renders an explicit empty maintenance record', () => {
      const html = render({ quality: 'NOT_BOUND' }, 'MAINTENANCE');
      expect(html).toContain('data-testid="inspector-maintenance-empty"');
      expect(textOf(html, 'inspector-serial')).toBeNull();
      expect(html).not.toContain('S-VPA2026-99482');
      expect(html).not.toContain('00:1B:1B:4F:92:0A');
      expect(html).not.toContain('V2.9.2-RT');
    });
  });

  describe('bound but not reporting (BAD_QUALITY)', () => {
    it('suppresses values and says the device is not reporting', () => {
      const html = render({ quality: 'BAD_QUALITY', controlState: REAL_CONTROL }, 'MANUAL_CONTROL');
      expect(html).toContain('BAD COMM FAILURE');
      expect(html).toContain('bound but not reporting');
      expect(textOf(html, 'inspector-process-value')).toBe('----');
      expect(html).not.toContain('41.2');
    });
  });

  describe('live (GOOD) still renders real data', () => {
    it('renders the values actually supplied', () => {
      const html = render({ quality: 'GOOD', controlState: REAL_CONTROL }, 'MANUAL_CONTROL');
      expect(textOf(html, 'inspector-process-value')).toBe('41.2');
      expect(textOf(html, 'inspector-output-power')).toBe('33.0');
      expect(html).toContain('bar');
      expect(html).not.toContain('data-testid="inspector-no-data-banner"');
      expect(html).not.toContain('NOT BOUND');
    });

    it('renders only the interlocks it was given', () => {
      const html = render({ quality: 'GOOD', interlocks: REAL_INTERLOCKS }, 'INTERLOCK_MATRIX');
      expect(html).toContain('Guard Closed');
      expect(html).toContain('Permissive Satisfied');
      expect(html).not.toContain('Emergency Stop Chain');
      expect(html).not.toContain('Motor Thermal Overload');
    });

    it('renders no interlock rows when a live device has no interlocks bound', () => {
      const html = render({ quality: 'GOOD', interlocks: [] }, 'INTERLOCK_MATRIX');
      expect(html).toContain('data-testid="inspector-interlocks-empty"');
      expect(html).toContain('No interlock telemetry');
      expect(html).not.toContain('data-testid="inspector-interlock-row"');
    });

    it('plots only the samples it was given', () => {
      const html = render({ quality: 'GOOD', controlState: REAL_CONTROL, trendHistory: REAL_TREND }, 'MINI_TREND');
      expect(html).toContain('<polyline');
      expect(html).toContain('3 samples');
      expect(html).not.toContain('data-testid="inspector-trend-empty"');
    });

    it('renders the maintenance record it was given, and --- for fields it was not', () => {
      const html = render({ quality: 'GOOD', maintenanceInfo: REAL_MAINTENANCE }, 'MAINTENANCE');
      expect(textOf(html, 'inspector-serial')).toBe('SN-REAL-1');
      expect(textOf(html, 'inspector-firmware')).toBe('FW-1.0.0');
      expect(textOf(html, 'inspector-dcp-mac')).toBe('AA:BB:CC:DD:EE:FF');
      expect(textOf(html, 'inspector-last-calibration')).toBe('----');
      expect(html).not.toContain('S-VPA2026-99482');
    });

    it('announces the real control mode instead of a default', () => {
      const html = render({ quality: 'GOOD', controlState: REAL_CONTROL });
      expect(textOf(html, 'inspector-control-mode')).toBe('HAND / MANUAL');
    });

    it('does not assert a control mode when the live device did not report one', () => {
      const html = render({ quality: 'GOOD', controlState: { processValue: 41.2 } });
      expect(textOf(html, 'inspector-control-mode')).toBe('----');
      expect(html).not.toContain('CASCADE / AUTO');
      expect(html).not.toContain('HAND / MANUAL');
    });
  });

  describe('undeclared provenance is not treated as live', () => {
    it('renders the no-data state when no quality prop is given', () => {
      // A JavaScript or plugin caller can omit the required prop: fail closed.
      const html = render({ controlState: REAL_CONTROL });
      expect(html).toContain('NOT BOUND');
      expect(textOf(html, 'inspector-process-value')).toBe('----');
      expect(html).not.toContain('41.2');
    });
  });

  describe('live path: no state supplied', () => {
    // The quality gate suppresses everything when quality is not GOOD, so it cannot show whether a
    // fabricated default is back. These render live and omit the data entirely — the only shape in
    // which a default becomes visible. Verified: with the pre-fix `{...DEFAULT_CONTROL, ...controlState}`
    // spread reintroduced, these are the assertions that go red while the NOT_BOUND cases stay green.
    it('renders ---- for every reading when a live device supplies no control state', () => {
      const html = render({ quality: 'GOOD' });
      expect(textOf(html, 'inspector-process-value')).toBe('----');
      expect(textOf(html, 'inspector-output-power')).toBe('----');
      expect(textOf(html, 'inspector-setpoint')).toBe('----');
      for (const literal of ['48.7', '50.0', '62.4']) {
        expect(html, `fabricated literal ${literal} rendered on the live path`).not.toContain(literal);
      }
    });

    it('renders no readings for a live device that supplies only some control state', () => {
      const html = render({ quality: 'GOOD', controlState: { processValue: 41.2 } });
      expect(textOf(html, 'inspector-process-value')).toBe('41.2');
      // The fields the caller did not supply stay empty rather than being filled from a default.
      expect(textOf(html, 'inspector-output-power')).toBe('----');
      expect(textOf(html, 'inspector-setpoint')).toBe('----');
      expect(html).not.toContain('62.4');
      expect(html).not.toContain('50.0');
    });

    it('shows no identity strings for a live device the caller did not identify', () => {
      const html = render({ quality: 'GOOD' });
      expect(textOf(html, 'inspector-device-name')).toBe('----');
      expect(textOf(html, 'inspector-device-id')).toBe('----');
      expect(html).not.toContain('DEV-PUMP-101');
      expect(html).not.toContain('Sinamics G120');
    });
  });

  describe('interlock states are never assumed', () => {
    it('renders UNKNOWN as unknown, not as satisfied', () => {
      const html = render(
        {
          quality: 'GOOD',
          interlocks: [
            { id: 'x', name: 'Unreadable Permissive', expression: '%I9.9', isPermissive: true, status: 'UNKNOWN' },
          ] satisfies InterlockCondition[],
        },
        'INTERLOCK_MATRIX'
      );
      expect(html).toContain('Unreadable Permissive');
      expect(html).toContain('State Unknown');
      expect(html).not.toContain('Permissive Satisfied');
    });
  });
});
