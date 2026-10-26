/**
 * SYS-REQ-096 — resolution of a canvas component into displayable process data.
 *
 * The distinguishing rule: stored design props are configuration, never a reading. Process
 * values exist only when the component is bound and a live payload is present.
 */

import { describe, it, expect } from 'vitest';
import {
  resolveComponentData,
  resolveComponentQuality,
  hasDeviceBinding,
  PROCESS_KEYS,
  CONFIG_KEYS,
} from './componentData';

const BINDING = [{ property: 'actualSpeed', tag: 'external.plant.MTR-1.rSpeedFeedback' }];

describe('resolveComponentQuality', () => {
  it('reports NOT_BOUND for a bare component', () => {
    expect(resolveComponentQuality({ id: 'mtr_1', type: 'MOTOR' })).toBe('NOT_BOUND');
  });

  it('reports NOT_BOUND for a component that only carries design props', () => {
    expect(
      resolveComponentQuality({ id: 'mtr_1', type: 'MOTOR', props: { actualSpeed: 1450, running: true } })
    ).toBe('NOT_BOUND');
  });

  it('reports BAD_QUALITY for a bound component with no telemetry payload', () => {
    expect(resolveComponentQuality({ bindings: BINDING })).toBe('BAD_QUALITY');
  });

  it('reports BAD_QUALITY when the telemetry payload declares bad quality', () => {
    expect(resolveComponentQuality({ bindings: BINDING, live: { quality: 'BAD_QUALITY', actualSpeed: 0 } })).toBe(
      'BAD_QUALITY'
    );
  });

  it('reports GOOD for a bound component with a usable payload', () => {
    expect(resolveComponentQuality({ bindings: BINDING, live: { actualSpeed: 1450 } })).toBe('GOOD');
  });

  it('honours an explicit quality declaration over inference', () => {
    expect(resolveComponentQuality({ quality: 'NOT_BOUND', bindings: BINDING, live: { actualSpeed: 1 } })).toBe(
      'NOT_BOUND'
    );
    expect(resolveComponentQuality({ dataQuality: 'BAD_QUALITY' })).toBe('BAD_QUALITY');
  });

  it('ignores an unrecognised quality value rather than trusting it', () => {
    expect(resolveComponentQuality({ quality: 'PROBABLY_FINE' })).toBe('NOT_BOUND');
  });
});

describe('hasDeviceBinding', () => {
  it('treats an empty binding list as unbound', () => {
    expect(hasDeviceBinding({ bindings: [] })).toBe(false);
  });

  it('accepts a non-empty binding list or a binding object', () => {
    expect(hasDeviceBinding({ bindings: BINDING })).toBe(true);
    expect(hasDeviceBinding({ binding: { tag: 'external.x.y' } })).toBe(true);
  });
});

describe('resolveComponentData', () => {
  it('returns no process values and only design config for an unbound component', () => {
    const { quality, values, config } = resolveComponentData({
      id: 'pt_1',
      type: 'TRANSMITTER',
      props: { pv: 6.84, unit: 'bar', minRange: 0, maxRange: 16, tier: 2 },
    });
    expect(quality).toBe('NOT_BOUND');
    expect(values).toEqual({});
    expect(config).toEqual({ unit: 'bar', minRange: 0, maxRange: 16, tier: 2 });
  });

  it('never promotes stored design props to readings, even when bound', () => {
    const { quality, values } = resolveComponentData({
      bindings: BINDING,
      live: { actualSpeed: 1200 },
      props: { actualSpeed: 9999 },
    });
    expect(quality).toBe('GOOD');
    expect(values).toEqual({ actualSpeed: 1200 });
  });

  it('exposes only process keys from the live payload, not arbitrary payload fields', () => {
    const { values } = resolveComponentData({
      bindings: BINDING,
      live: { actualSpeed: 1200, timestamp: 12345, internalNote: 'x' },
    });
    expect(values).toEqual({ actualSpeed: 1200 });
    expect(PROCESS_KEYS).toContain('actualSpeed');
    expect(PROCESS_KEYS).not.toContain('timestamp');
  });

  it('keeps configuration keys out of the process values', () => {
    for (const key of CONFIG_KEYS) {
      expect(PROCESS_KEYS as readonly string[]).not.toContain(key);
    }
  });

  it('suppresses values entirely for BAD_QUALITY', () => {
    const { quality, values } = resolveComponentData({ bindings: BINDING, props: { actualSpeed: 1450 } });
    expect(quality).toBe('BAD_QUALITY');
    expect(values).toEqual({});
  });

  it('handles a null component without throwing', () => {
    expect(resolveComponentData(null)).toEqual({ quality: 'NOT_BOUND', values: {}, config: {} });
  });
});