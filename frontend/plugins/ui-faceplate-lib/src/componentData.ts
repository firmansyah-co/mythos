/**
 * Canvas component → faceplate data resolution.
 *
 * A screen document stores what the engineer authored: component type, tag, hardware address
 * and design configuration. It does not, by itself, contain a live process value. This module
 * is the single place where a canvas component is turned into the values a faceplate may
 * display, so no renderer invents a fallback of its own.
 *
 * Resolution:
 *   1. An explicit `quality` / `dataQuality` on the component or its props wins.
 *   2. No device binding → `NOT_BOUND`.
 *   3. Bound with a good live payload → `GOOD`.
 *   4. Bound without a usable live payload → `BAD_QUALITY`.
 *
 * Process values are read only from the live payload, and only when the quality is `GOOD`.
 * Values stored in the document are design configuration, never a reading — an unbound screen
 * therefore shows no numbers at all.
 */

import type { ProcessDataQuality } from './types';

/** Values that are live process measurements or device state. Never read from design props. */
export const PROCESS_KEYS = [
  'running',
  'tripped',
  'actualSpeed',
  'speedSetpoint',
  'currentAmps',
  'torquePercent',
  'openLimit',
  'closedLimit',
  'transitFault',
  'positionPercent',
  'positionSetpoint',
  'pv',
  'sp',
  'cv',
  'sensorFault',
  'interlocks',
  'operatingMode',
] as const;

/** Design configuration that is legitimately stored with the screen and always displayable. */
export const CONFIG_KEYS = [
  'tag',
  'label',
  'namespace',
  'unit',
  'transmitterType',
  'minRange',
  'maxRange',
  'highLimit',
  'highHighLimit',
  'lowLimit',
  'lowLowLimit',
  'maxSpeed',
  'spMin',
  'spMax',
  'kp',
  'tn',
  'tv',
  'deadband',
  'tier',
  'udtArchetype',
  'hardwareAddress',
] as const;

export type ProcessValue = number | boolean | string | null;
export type DesignConfig = Record<string, unknown>;

export interface CanvasComponentLike {
  id?: string;
  type?: string;
  quality?: unknown;
  dataQuality?: unknown;
  bindings?: unknown;
  binding?: unknown;
  live?: unknown;
  telemetry?: unknown;
  props?: unknown;
  [key: string]: unknown;
}

export interface ResolvedComponentData {
  /** Provenance of the values below; pass straight through to the faceplate. */
  quality: ProcessDataQuality;
  /** Live process values. Empty unless the quality is `GOOD`. */
  values: Record<string, ProcessValue>;
  /** Stored design configuration (ranges, limits, units, tuning). */
  config: DesignConfig;
}

const QUALITY_VALUES: ProcessDataQuality[] = ['GOOD', 'BAD_QUALITY', 'NOT_BOUND'];

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function asQuality(value: unknown): ProcessDataQuality | null {
  return typeof value === 'string' && (QUALITY_VALUES as string[]).includes(value)
    ? (value as ProcessDataQuality)
    : null;
}

function readProps(component: CanvasComponentLike | null | undefined): Record<string, unknown> {
  return isPlainObject(component?.props) ? component.props : {};
}

/** The live telemetry payload for this component, if the canvas supplied one. */
export function readLivePayload(
  component: CanvasComponentLike | null | undefined
): Record<string, unknown> | null {
  const props = readProps(component);
  for (const candidate of [component?.live, component?.telemetry, props.live, props.telemetry]) {
    if (isPlainObject(candidate)) return candidate;
  }
  return null;
}

/** A binding is any explicit declaration that a faceplate property reads a TagBus tag. */
export function hasDeviceBinding(component: CanvasComponentLike | null | undefined): boolean {
  const props = readProps(component);
  const bindings = component?.bindings ?? component?.binding ?? props.bindings ?? props.binding;
  if (Array.isArray(bindings)) return bindings.length > 0;
  return bindings !== null && bindings !== undefined && bindings !== false && bindings !== '';
}

function pick(keys: readonly string[], source: Record<string, unknown>): Record<string, unknown> {
  const picked: Record<string, unknown> = {};
  for (const key of keys) {
    if (key in source) picked[key] = source[key];
  }
  return picked;
}

function toProcessValues(source: Record<string, unknown>): Record<string, ProcessValue> {
  const values: Record<string, ProcessValue> = {};
  const picked = pick(PROCESS_KEYS, source);
  for (const [key, value] of Object.entries(picked)) {
    if (value === null || typeof value === 'number' || typeof value === 'boolean' || typeof value === 'string') {
      values[key] = value as ProcessValue;
    }
  }
  return values;
}

export function resolveComponentQuality(component: CanvasComponentLike | null | undefined): ProcessDataQuality {
  const props = readProps(component);
  const declared = asQuality(component?.dataQuality) ?? asQuality(component?.quality) ?? asQuality(props.dataQuality);
  if (declared) return declared;

  if (!hasDeviceBinding(component)) return 'NOT_BOUND';

  const live = readLivePayload(component);
  if (!live) return 'BAD_QUALITY';
  const liveQuality = asQuality(live.quality) ?? asQuality(live.dataQuality);
  if (liveQuality === 'BAD_QUALITY' || liveQuality === 'NOT_BOUND') return 'BAD_QUALITY';
  if (liveQuality === 'GOOD') return 'GOOD';
  // No explicit quality on the payload: it is usable only if it actually carries a value.
  return Object.keys(toProcessValues(live)).length > 0 ? 'GOOD' : 'BAD_QUALITY';
}

/**
 * Resolve a canvas component into the data a faceplate may render.
 *
 * Only a `GOOD` component yields process values, and those come exclusively from the live
 * payload — stored design values are never promoted to readings.
 */
export function resolveComponentData(component: CanvasComponentLike | null | undefined): ResolvedComponentData {
  const quality = resolveComponentQuality(component);
  const props = readProps(component);
  const live = readLivePayload(component);

  return {
    quality,
    values: quality === 'GOOD' && live ? toProcessValues(live) : {},
    config: pick(CONFIG_KEYS, props),
  };
}
