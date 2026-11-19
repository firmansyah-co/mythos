import { describe, expect, it } from 'vitest';
import type { IoConfig } from '../api/client';
import {
  connectionTags,
  connectionTarget,
  fromForm,
  newConnection,
  toForm,
  type ConnectionForm,
} from '../data/ioForm';
import { newRow, type ObjectForm, type RowsForm } from '../data/schemaForm';
import { MODBUS_SCHEMA, SCHEMAS, modbusConnection } from './harness';

/** Every leaf value of an object, to assert on what would be sent. */
function leaves(value: unknown): unknown[] {
  if (Array.isArray(value)) return value.flatMap(leaves);
  if (value && typeof value === 'object') return Object.values(value).flatMap(leaves);
  return [value];
}

const rowsOf = (config: ObjectForm | null): RowsForm => config!.fields.mappings as RowsForm;

/** Change driver fields of a connection, and fields of its first tag row. */
function withFields(
  connection: ConnectionForm,
  fields: Record<string, string>,
  row: Record<string, string> = {}
): ConnectionForm {
  const config = connection.config!;
  const rows = rowsOf(config);
  const [first, ...rest] = rows.rows;
  return {
    ...connection,
    config: {
      ...config,
      fields: {
        ...config.fields,
        ...fields,
        mappings: {
          kind: 'rows',
          rows: first ? [{ ...first, value: { ...first.value, fields: { ...first.value.fields, ...row } } }, ...rest] : rest,
        },
      },
    },
  };
}

function validConnection(): ConnectionForm {
  return withFields(
    { ...newConnection('modbus', SCHEMAS), id: 'plc1', pollIntervalMs: '500' },
    { host: '192.0.2.10' },
    { uri: 'external.Plant.PT101.pressure', address: '100' }
  );
}

const fieldsOf = (result: ReturnType<typeof fromForm>) =>
  result.ok ? [] : result.errors.map((e) => e.field.replace(/\.r\d+\./, '.<row>.')).sort();

describe('reading and writing the stored configuration', () => {
  it('saves back exactly what was loaded when nothing is edited', () => {
    const stored: IoConfig = {
      connections: [
        { ...modbusConnection('plc1', ['external.A.B.c', 'external.A.B.d'], 1502), cycle_timeout_ms: 3000 },
        modbusConnection('plc2', ['external.A.B.e']),
      ],
    };
    expect(fromForm(toForm(stored, SCHEMAS), SCHEMAS)).toEqual({ ok: true, config: stored });
  });

  it('keeps keys the schema does not describe, and a driver it has no schema for, exactly as stored', () => {
    const stored: IoConfig = {
      connections: [
        {
          id: 'plc1',
          driver: 'modbus',
          poll_interval_ms: 1000,
          config: {
            transport: 'tcp',
            host: 'h',
            port: 502,
            slave_id: 1,
            timeout_ms: 1000,
            serial_port: '/dev/ttyUSB0',
            mappings: [
              { uri: 'external.A.B.c', address: 1, function_code: 3, data_type: 'u16', word_order: 'high_first', scale: 1, note: 'kept' },
            ],
          },
        },
        { id: 'other', driver: 'opcua', poll_interval_ms: 1000, config: { endpoint: 'opc.tcp://x' } },
      ],
    };
    const form = toForm(stored, SCHEMAS);
    expect(form[1]!.config).toBeNull();
    expect(fromForm(form, SCHEMAS)).toEqual({ ok: true, config: stored });
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
    const [form] = toForm(stored, SCHEMAS);
    expect(rowsOf(form!.config).rows[0]!.value.fields).toMatchObject({
      function_code: '4',
      data_type: 'f32',
      word_order: 'low_first',
      scale: '0.1',
      address: '3',
    });
    expect(fromForm(toForm(stored, SCHEMAS), SCHEMAS)).toEqual({ ok: true, config: stored });
  });

  it('a field left out of a stored configuration shows the default the module applies', () => {
    const stored: IoConfig = {
      connections: [
        {
          id: 'plc1',
          driver: 'modbus',
          poll_interval_ms: 250,
          config: { transport: 'tcp', host: 'h', mappings: [{ uri: 'external.A.B.c', address: 0, function_code: 3 }] },
        },
      ],
    };
    const [form] = toForm(stored, SCHEMAS);
    expect(form!.config!.fields).toMatchObject({ port: '502', slave_id: '1', timeout_ms: '1000' });
  });
});

describe('a new connection', () => {
  it('starts with the defaults the schema declares and one empty tag row', () => {
    const connection = newConnection('modbus', SCHEMAS);
    expect(connection.config!.fields).toMatchObject({
      transport: 'tcp',
      host: '',
      port: '502',
      slave_id: '1',
      timeout_ms: '1000',
    });
    const rows = rowsOf(connection.config).rows;
    expect(rows).toHaveLength(1);
    expect(rows[0]!.value.fields).toMatchObject({ uri: '', address: '', function_code: '3', data_type: 'u16' });
  });

  it('cannot be saved until its required fields are filled, and says which', () => {
    expect(fieldsOf(fromForm([newConnection('modbus', SCHEMAS)], SCHEMAS))).toEqual([
      'config.host',
      'config.mappings.<row>.address',
      'config.mappings.<row>.uri',
      'id',
      'pollIntervalMs',
    ]);
  });

  it('is valid once they are', () => {
    expect(fromForm([validConnection()], SCHEMAS).ok).toBe(true);
  });

  it('a driver with no schema starts with an empty configuration kept as it is', () => {
    const connection = newConnection('opcua', SCHEMAS);
    expect(connection.config).toBeNull();
    expect(connection.raw).toEqual({});
  });
});

describe('numbers', () => {
  const NOT_NUMBERS = ['', '   ', 'abc', '12abc', '1.5', '1e3', '0x10', '--1', '١٢'];

  it.each(NOT_NUMBERS)('refuses %o as a poll interval instead of turning it into a number', (text) => {
    expect(fieldsOf(fromForm([{ ...validConnection(), pollIntervalMs: text }], SCHEMAS))).toEqual([
      'pollIntervalMs',
    ]);
  });

  it.each(NOT_NUMBERS)('refuses %o as a register address', (text) => {
    expect(fieldsOf(fromForm([withFields(validConnection(), {}, { address: text })], SCHEMAS))).toEqual([
      'config.mappings.<row>.address',
    ]);
  });

  it.each([
    ['port', '0'],
    ['port', '65536'],
    ['slave_id', '0'],
    ['slave_id', '248'],
    ['timeout_ms', '9'],
    ['timeout_ms', '10001'],
  ])('refuses an out-of-range %s (%s)', (field, text) => {
    expect(fieldsOf(fromForm([withFields(validConnection(), { [field]: text })], SCHEMAS))).toEqual([
      `config.${field}`,
    ]);
  });

  it.each(['abc', 'Infinity', 'NaN', '1,5'])('refuses %o as a scale', (text) => {
    expect(fieldsOf(fromForm([withFields(validConnection(), {}, { scale: text })], SCHEMAS))).toEqual([
      'config.mappings.<row>.scale',
    ]);
  });

  it('an optional field left empty is left out, so the module applies its own default', () => {
    const result = fromForm([withFields(validConnection(), { timeout_ms: '' }, { scale: '' })], SCHEMAS);
    expect(result.ok).toBe(true);
    const config = result.ok ? (result.config.connections[0]!.config as Record<string, unknown>) : {};
    expect('timeout_ms' in config).toBe(false);
    expect('scale' in (config.mappings as Record<string, unknown>[])[0]!).toBe(false);
  });

  it('an empty cycle deadline is left out, and a typed one is kept', () => {
    const without = fromForm([validConnection()], SCHEMAS);
    expect(without.ok && 'cycle_timeout_ms' in without.config.connections[0]!).toBe(false);
    const withOne = fromForm([{ ...validConnection(), cycleTimeoutMs: '4000' }], SCHEMAS);
    expect(withOne.ok && withOne.config.connections[0]!.cycle_timeout_ms).toBe(4000);
  });

  it('never sends a placeholder: a configuration that converts has no NaN and no null in it', () => {
    const result = fromForm([validConnection(), { ...validConnection(), id: 'plc2', cycleTimeoutMs: '4000' }], SCHEMAS);
    expect(result.ok).toBe(true);
    const values = result.ok ? leaves(result.config) : [];
    expect(values.length).toBeGreaterThan(10);
    for (const value of values) {
      expect(value).not.toBeNull();
      expect(Number.isNaN(value)).toBe(false);
    }
  });

  it('a choice is saved with its type: a register code as a number, a data type as text', () => {
    const result = fromForm([validConnection()], SCHEMAS);
    const mapping = result.ok
      ? ((result.config.connections[0]!.config as Record<string, unknown>).mappings as Record<string, unknown>[])[0]
      : null;
    expect(mapping).toMatchObject({ function_code: 3, data_type: 'u16' });
  });
});

describe('what the schema says about a connection', () => {
  it('names the tags it provides and what it talks to', () => {
    let connection = validConnection();
    const rows = rowsOf(connection.config);
    const second = newRow(MODBUS_SCHEMA.properties!.mappings!);
    second.value.fields.uri = 'external.Plant.TT101.temperature';
    connection = { ...connection, config: { ...connection.config!, fields: { ...connection.config!.fields, mappings: { kind: 'rows', rows: [...rows.rows, second] } } } };
    expect(connectionTags(connection, SCHEMAS)).toEqual([
      'external.Plant.PT101.pressure',
      'external.Plant.TT101.temperature',
    ]);
    expect(connectionTarget(connection, SCHEMAS)).toBe('192.0.2.10:502 · unit 1');
  });

  it('says nothing about a target it cannot fully name', () => {
    expect(connectionTarget(newConnection('modbus', SCHEMAS), SCHEMAS)).toBeNull();
    expect(connectionTags(newConnection('opcua', SCHEMAS), SCHEMAS)).toBeNull();
  });
});
