/**
 * Process-data honesty helpers.
 *
 * SYS-REQ-096 (Zero Silent Fallback): the supervisor must never generate synthetic process
 * values. An operator display that shows a plausible number when the device is not reporting
 * is the worst form of that violation, because it looks exactly like a healthy plant.
 *
 * Every process value a faceplate renders goes through these helpers, which turn "no value"
 * into a deliberate ANSI/ISA-101 quiescent state (`----` plus an explicit quality indication)
 * instead of a number.
 */

import type { ProcessDataQuality } from './types';

/** ANSI/ISA-101 quiescent no-data text. Not a measurement, and not mistakable for one. */
export const NO_DATA_TEXT = '----';

/** Quality indicator for a component with no tag binding (a configuration state). */
export const NOT_BOUND_TEXT = 'NOT BOUND';

/** Quality indicator for a bound component whose values are not usable (an abnormal state). */
export const BAD_QUALITY_TEXT = 'BAD COMM FAILURE';

/**
 * Quality assumed when a caller declares nothing.
 *
 * This is deliberately the safe direction. A faceplate with undeclared provenance cannot prove
 * its values are live, so it renders the quiescent no-data state. `quality` is also a required
 * TypeScript prop, so an ordinary call site that omits it fails to compile; this constant is the
 * fail-closed runtime backstop for `any`-typed, JavaScript or plugin callers.
 */
export const DEFAULT_QUALITY: ProcessDataQuality = 'NOT_BOUND';

export function normalizeQuality(quality?: ProcessDataQuality | null): ProcessDataQuality {
  return quality ?? DEFAULT_QUALITY;
}

/** True only for live, good-quality telemetry. Every other state suppresses process values. */
export function isLiveQuality(quality?: ProcessDataQuality | null): boolean {
  return normalizeQuality(quality) === 'GOOD';
}

/** True when the faceplate may render a process value at all. */
export function hasDisplayableValue(quality?: ProcessDataQuality | null): boolean {
  return isLiveQuality(quality);
}

/**
 * Label for the explicit data-quality annunciation, or `null` when there is nothing to announce.
 * `NOT_BOUND` is a configuration state and stays neutral; `BAD_QUALITY` is abnormal and is the
 * only one that earns a saturated (amber) tone.
 */
export function dataQualityLabel(quality?: ProcessDataQuality | null): string | null {
  switch (normalizeQuality(quality)) {
    case 'NOT_BOUND':
      return NOT_BOUND_TEXT;
    case 'BAD_QUALITY':
      return BAD_QUALITY_TEXT;
    case 'GOOD':
    default:
      return null;
  }
}

/**
 * Format a numeric process value. Returns `----` when the data is not live or the value is
 * missing, so a faceplate can never print `undefined`, `null` or `NaN` as if it were a reading.
 */
export function formatProcessValue(
  value: number | null | undefined,
  quality: ProcessDataQuality | null | undefined,
  digits = 0
): string {
  if (!hasDisplayableValue(quality) || value === null || value === undefined || !Number.isFinite(value)) {
    return NO_DATA_TEXT;
  }
  return value.toFixed(digits);
}

/** Format a boolean process state. Returns `----` when the data is not live. */
export function formatProcessState(
  value: boolean | null | undefined,
  quality: ProcessDataQuality | null | undefined,
  trueText: string,
  falseText: string
): string {
  if (!hasDisplayableValue(quality) || value === null || value === undefined) {
    return NO_DATA_TEXT;
  }
  return value ? trueText : falseText;
}

/** Percentage (0-100) for an analog bar, or 0 when there is nothing to draw. */
export function processBarPercent(
  value: number | null | undefined,
  quality: ProcessDataQuality | null | undefined,
  max = 100,
  min = 0
): number {
  if (!hasDisplayableValue(quality) || value === null || value === undefined || !Number.isFinite(value)) {
    return 0;
  }
  if (max === min) return 0;
  const clamped = Math.min(max, Math.max(min, value));
  return ((clamped - min) / (max - min)) * 100;
}

/** A stored configuration value (range, limit, unit), or `----` when the design stores none. */
export function configText(value: number | string | null | undefined): string {
  return value === null || value === undefined || value === '' ? NO_DATA_TEXT : String(value);
}
