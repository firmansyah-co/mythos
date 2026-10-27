/**
 * Inspector Drawer no-data policy.
 *
 * One place that decides what the drawer is allowed to show, built on the shared
 * `@mythos/ui-faceplate-lib` quiescent-state contract (`ProcessDataQuality`, `NO_DATA_TEXT`).
 * Reusing that contract rather than forking a second one keeps the canvas and the drawer from
 * drifting into two different definitions of "no data".
 *
 * SYS-REQ-096 (Zero Silent Fallback): a value is displayed only when the drawer's quality is
 * `GOOD`. Everything else renders `----`, and the drawer explains why in words, because a human
 * reads this under time pressure and a grid of dashes alone does not say what is missing.
 */

import {
  NO_DATA_TEXT,
  dataQualityLabel,
  formatProcessValue,
  isLiveQuality,
} from '@mythos/ui-faceplate-lib';
import type { ProcessDataQuality } from '@mythos/ui-faceplate-lib';

/** Fail-closed runtime backstop: an undeclared quality is treated as unbound. */
export const DEFAULT_INSPECTOR_QUALITY: ProcessDataQuality = 'NOT_BOUND';

export function resolveInspectorQuality(quality?: ProcessDataQuality | null): ProcessDataQuality {
  return quality ?? DEFAULT_INSPECTOR_QUALITY;
}

export function inspectorIsLive(quality?: ProcessDataQuality | null): boolean {
  return isLiveQuality(resolveInspectorQuality(quality));
}

/** Short qualifier shown next to the drawer title, or `null` for live data. */
export function inspectorQualityLabel(quality?: ProcessDataQuality | null): string | null {
  return dataQualityLabel(resolveInspectorQuality(quality));
}

/**
 * Plain-language explanation for the quiescent banner. This is deliberately a sentence rather
 * than a code: the operator needs to know nothing is being measured, not to decode a token.
 */
export function inspectorNoDataReason(quality?: ProcessDataQuality | null): string | null {
  switch (resolveInspectorQuality(quality)) {
    case 'NOT_BOUND':
      return 'No device data — this device is not bound to a tag, so nothing is being measured.';
    case 'BAD_QUALITY':
      return 'No device data — the device is bound but not reporting. Values are suppressed rather than shown stale.';
    case 'GOOD':
    default:
      return null;
  }
}

/** A numeric process value, or `----` when the drawer has no live data for it. */
export function inspectorValue(
  value: number | null | undefined,
  quality?: ProcessDataQuality | null,
  digits = 1
): string {
  return formatProcessValue(value, resolveInspectorQuality(quality), digits);
}

/** A textual device attribute (serial, firmware, MAC), or `----` when absent. */
export function inspectorText(
  value: string | null | undefined,
  quality?: ProcessDataQuality | null
): string {
  if (!inspectorIsLive(quality) || value === null || value === undefined || value === '') {
    return NO_DATA_TEXT;
  }
  return value;
}

/** True when there is at least one field worth showing. Empty records render an empty state. */
export function hasAnyField(record: Record<string, unknown> | null | undefined): boolean {
  if (!record) return false;
  return Object.values(record).some((v) => v !== null && v !== undefined && v !== '');
}

export { NO_DATA_TEXT };
