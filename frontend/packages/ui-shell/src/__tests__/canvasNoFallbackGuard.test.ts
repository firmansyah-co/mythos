/**
 * SYS-REQ-096 static guard: the HMI canvas renderer must not carry fabricated process-value
 * literals.
 *
 * The DOM test in `honestCanvas.test.tsx` proves the rendered output is quiescent. It cannot, on
 * its own, prove a call-site fallback was not reintroduced: a faceplate with `NOT_BOUND` quality
 * suppresses a value even if the renderer invents one before passing it down. This test reads the
 * renderer source and fails on the literals themselves, so a reintroduced `?? 1450` is red.
 */

import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const RENDERER = path.resolve(__dirname, '../components/workbench/DockviewWorkspace.tsx');

/** Process-value literals the pre-fix renderer substituted (see the safety finding). */
const FABRICATED_LITERALS = [
  '?? 1450',
  '?? 1500',
  '?? 18.5',
  '?? 78',
  '?? 82.5',
  '?? 80.0',
  '?? 45.2',
  '?? 6.84',
  '?? 16',
  '?? 12',
  '?? 14',
  '?? true',
  '?? false',
  '?? 100',
];

describe('SYS-REQ-096: canvas renderer carries no fabricated process-value fallbacks', () => {
  const source = fs.readFileSync(RENDERER, 'utf8');

  it('has no hardcoded process-value fallback literal', () => {
    // Boundary-anchored so `?? 14` does not also match a legitimate `?? 1400`-class token.
    const found = FABRICATED_LITERALS.filter((literal) =>
      new RegExp(`${literal.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\d.])`).test(source)
    );
    expect(found, `fabricated fallbacks reintroduced: ${found.join(', ')}`).toEqual([]);
  });

  it('resolves component data through resolveComponentData', () => {
    expect(source).toContain('resolveComponentData');
  });

  it('passes a declared quality to every faceplate it renders', () => {
    for (const faceplate of ['MotorFaceplate', 'ValveFaceplate', 'PidControllerFaceplate', 'AnalogTransmitterFaceplate']) {
      const usage = source.split(`<${faceplate}`)[1];
      expect(usage, `${faceplate} is rendered without a quality prop`).toBeDefined();
      const block = usage.slice(0, usage.indexOf('/>'));
      expect(block, `${faceplate} is rendered without a quality prop`).toContain('quality={quality}');
    }
  });
});