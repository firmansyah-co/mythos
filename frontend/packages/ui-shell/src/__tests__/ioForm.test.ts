import { describe, expect, it } from 'vitest';
import type { IoConfig } from '../api/client';
import { fromForm, newConnection, toForm, type ConnectionForm } from '../data/ioForm';
import { modbusConnection } from './harness';

/** Every leaf value of an object, to assert on what would be sent. */
function leaves(value: unknown): unknown[] {
  if (Array.isArray(value)) return value.flatMap(leaves);
  if (value && typeof value === 'object') return Object.values(value).flatMap(leaves);
  return [value];
}

function validConnection(): ConnectionForm {
  const connection = newConnection();
  return {
    ...connection,
    id: 'plc1',
    pollIntervalMs: '500',
    modbus: {
      ...connection.modbus!,
      host: '192.0.2.10',
      mappings: [
        { ...connection.modbus!.mappings[0]!, uri: 'external.Plant.PT101.pressure', address: '100' },
      ],
    },
  };
}

describe('reading and writing the stored configuration', () => {
  it('saves back exactly what was loaded when nothing is edited', () => {
    const stored: IoConfig = {
      connections: [
        { ...modbusConnection('plc1', ['external.A.B.c', 'external.A.B.d'], 1502), cycle_timeout_ms: 3000 },
        modbusConnection('plc2', ['external.A.B.e']),
      ],
    };
    const result = fromForm(toForm(stored));
    expect(result).toEqual({ ok: true, config: stored });
  });

  it('keeps a driver configuration it has no form for exactly as stored', () => {
    const raw = { transport: 'rtu', serial_port: '/dev/ttyUSB0', baud_rate: 19200, mappings: [] };
    const stored: IoConfig = {
      connections: [
        { id: 'serial', driver: 'modbus', poll_interval_ms: 1000, config: raw },
        { id: 'other', driver: 'opcua', poll_interval_ms: 1000, config: { endpoint: 'opc.tcp://x' } },
      ],
    };
    const form = toForm(stored);
    expect(form.map((c) => c.modbus)).toEqual([null, null]);
    expect(fromForm(form)).toEqual({ ok: true, config: stored });
  });

  it('maps input registers, data types, word order and scale in both directions', () => {
    const stored: IoConfig = {
      connections: [
        {
          id: 'plc1',
          driver: 'modbus',
          poll_interval_ms: 250,
          config: {
            transport: 'tcp',
            host: 'h',
            port: 502,
            slave_id: 7,
            timeout_ms: 800,
            mappings: [
              { uri: 'external.A.B.c', address: 3, function_code: 4, data_type: 'f32', word_order: 'low_first', scale: 0.1 },
            ],
          },
        },
      ],
    };
    const [form] = toForm(stored);
    expect(form?.modbus?.mappings[0]).toMatchObject({
      register: 'input',
      dataType: 'f32',
      wordOrder: 'low_first',
      scale: '0.1',
      address: '3',
    });
    expect(fromForm(toForm(stored))).toEqual({ ok: true, config: stored });
  });
});

describe('a new connection', () => {
  it('cannot be saved until its required fields are filled, and says which', () => {
    const result = fromForm([newConnection()]);
    expect(result.ok).toBe(false);
    const fields = result.ok ? [] : result.errors.map((e) => e.field).sort();
    expect(fields).toEqual(['address', 'host', 'id', 'pollIntervalMs', 'uri']);
  });

  it('is valid once they are', () => {
    const result = fromForm([validConnection()]);
    expect(result.ok).toBe(true);
  });
});

describe('numbers', () => {
  const NOT_NUMBERS = ['', '   ', 'abc', '12abc', '1.5', '1e3', '0x10', '--1', '١٢'];

  it.each(NOT_NUMBERS)('refuses %o as a poll interval instead of turning it into a number', (text) => {
    const result = fromForm([{ ...validConnection(), pollIntervalMs: text }]);
    expect(result.ok).toBe(false);
    expect(result.ok ? [] : result.errors.map((e) => e.field)).toEqual(['pollIntervalMs']);
  });

  it.each(NOT_NUMBERS)('refuses %o as a register address', (text) => {
    const base = validConnection();
    const result = fromForm([
      { ...base, modbus: { ...base.modbus!, mappings: [{ ...base.modbus!.mappings[0]!, address: text }] } },
    ]);
    expect(result.ok ? [] : result.errors.map((e) => e.field)).toEqual(['address']);
  });

  it.each([
    ['port', { port: '0' }],
    ['port', { port: '65536' }],
    ['slaveId', { slaveId: '0' }],
    ['slaveId', { slaveId: '248' }],
    ['timeoutMs', { timeoutMs: '9' }],
    ['timeoutMs', { timeoutMs: '10001' }],
  ])('refuses an out-of-range %s', (field, change) => {
    const base = validConnection();
    const result = fromForm([{ ...base, modbus: { ...base.modbus!, ...change } }]);
    expect(result.ok ? [] : result.errors.map((e) => e.field)).toEqual([field]);
  });

  it.each(['', '0', 'abc', 'Infinity', 'NaN'])('refuses %o as a scale', (text) => {
    const base = validConnection();
    const result = fromForm([
      { ...base, modbus: { ...base.modbus!, mappings: [{ ...base.modbus!.mappings[0]!, scale: text }] } },
    ]);
    expect(result.ok ? [] : result.errors.map((e) => e.field)).toEqual(['scale']);
  });

  it('an empty cycle deadline is left out, and a typed one is kept', () => {
    const without = fromForm([validConnection()]);
    expect(without.ok && 'cycle_timeout_ms' in without.config.connections[0]!).toBe(false);

    const withOne = fromForm([{ ...validConnection(), cycleTimeoutMs: '4000' }]);
    expect(withOne.ok && withOne.config.connections[0]!.cycle_timeout_ms).toBe(4000);
  });

  it('never sends a placeholder: a configuration that converts has no NaN and no null in it', () => {
    const result = fromForm([validConnection(), { ...validConnection(), id: 'plc2', cycleTimeoutMs: '4000' }]);
    expect(result.ok).toBe(true);
    const values = result.ok ? leaves(result.config) : [];
    expect(values.length).toBeGreaterThan(10);
    for (const value of values) {
      expect(value).not.toBeNull();
      expect(Number.isNaN(value)).toBe(false);
    }
  });
});
