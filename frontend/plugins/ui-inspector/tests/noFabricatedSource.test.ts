/**
 * SYS-REQ-096 static guard over the Inspector Drawer.
 *
 * Why a behavioural test is not sufficient, learned the hard way on the canvas: the drawer gates
 * every value on `quality`, so a fabricated literal or a parameter default can sit in the source
 * while a behavioural test rendered with `quality="NOT_BOUND"` stays green — the gate hides it.
 * This guard reads the source and fails on the literals and default-spread patterns themselves.
 */

import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const SRC = path.resolve(__dirname, '../src');

function read(file: string): string {
  return fs.readFileSync(path.join(SRC, file), 'utf8');
}

/** Every value the pre-fix drawer fabricated. None may exist in the source. */
const FORBIDDEN_SOURCE_LITERALS = [
  // DEFAULT_CONTROL
  '48.7',
  '50.0',
  '62.4',
  // DEFAULT_MAINTENANCE — device identity is the worst of these
  '4328.5',
  '1420',
  '00:1B:1B:4F:92:0A',
  'S-VPA2026-99482',
  'V2.9.2-RT',
  '2026-08-15',
  '2027-02-15',
  // DEFAULT_INTERLOCKS
  'Emergency Stop Chain',
  'Motor Thermal Overload',
  'Suction Line Pressure Permissive',
  'Discharge Isolation Valve Limit Switch',
  'Seal Flush Lubrication Flow',
  // fabricated device identity defaults
  'DEV-PUMP-101',
  'Main Feed Pump 101',
  'Sinamics G120',
  // synthetic trend + invented telemetry rate
  'generateSampleTrend',
  'Math.random',
  '100 ms (10 Hz)',
];

describe('SYS-REQ-096: Inspector Drawer source carries no fabricated device data', () => {
  const drawer = read('InspectorDrawer.tsx');
  const deviceData = read('deviceData.ts');
  const types = read('types.ts');
  const allSources = [drawer, deviceData, types].join('\n');

  it.each(FORBIDDEN_SOURCE_LITERALS)('does not contain %s', (literal) => {
    expect(allSources, `fabricated literal "${literal}" reintroduced`).not.toContain(literal);
  });

  it('defines no DEFAULT_* fabrication constants', () => {
    const offenders = [...allSources.matchAll(/\bDEFAULT_(?!INSPECTOR_QUALITY)\w+/g)].map((m) => m[0]);
    expect(offenders, `fabricated default constants reintroduced: ${offenders.join(', ')}`).toEqual([]);
  });

  it('spreads no default object into component state', () => {
    // The pre-fix defect was `{ ...DEFAULT_CONTROL, ...controlState }`: a caller supplying nothing
    // silently received a full fabricated device.
    const spreads = [...drawer.matchAll(/\{\s*\.\.\.(?!prev)\w+/g)].map((m) => m[0]);
    expect(spreads.filter((s) => /DEFAULT/i.test(s))).toEqual([]);
  });

  it('gates rendered values on quality rather than defaulting them', () => {
    expect(drawer).toContain('inspectorIsLive');
    expect(drawer).toContain('inspectorValue');
    expect(drawer).toContain('inspectorText');
  });

  it('never snapshots device state into local state', () => {
    // The prop-update defect: `useState(controlState ?? {})` pins the drawer to its mount-time
    // values, so a reading supplied after mount is never displayed. Only operator intent (a
    // setpoint being dragged, the jog latches) may live in state; measured values come from the
    // prop on every render. A behavioural test cannot see this while the call site passes
    // quality="NOT_BOUND", so the shape itself is guarded here.
    expect(drawer).not.toMatch(/useState<[^>]*>\s*\(\s*controlState/);
    expect(drawer).not.toMatch(/useState\(\s*controlState/);
  });

  it('does not invent a setpoint range when the device reports none', () => {
    // One level down from the same class: the readout said `----` while the slider still handed
    // the operator a usable 0-100 command span.
    expect(drawer).not.toMatch(/minSetpoint\s*\?\?\s*\d/);
    expect(drawer).not.toMatch(/maxSetpoint\s*\?\?\s*\d/);
  });

  it('never gives the quality prop a permissive default', () => {
    expect(drawer).toContain('DEFAULT_INSPECTOR_QUALITY');
    expect(deviceData).toMatch(/DEFAULT_INSPECTOR_QUALITY:\s*ProcessDataQuality\s*=\s*'NOT_BOUND'/);
  });

  it('reuses the shared faceplate no-data contract instead of forking one', () => {
    // One definition of "no data" across surfaces; a second contract would drift.
    expect(deviceData).toContain("@mythos/ui-faceplate-lib");
    expect(deviceData).toMatch(/from '@mythos\/ui-faceplate-lib'/);
  });

  it('treats an unreadable interlock as UNKNOWN, not SATISFIED', () => {
    expect(types).toContain("'UNKNOWN'");
    expect(allSources).toContain('State Unknown');
  });
});
