// The editable form of an I/O configuration, and the conversion to and from the stored one.
//
// Every numeric field is held as the text the engineer typed. It becomes a number only when
// the configuration is saved, and text that is not a number is reported as an error against
// its field. Nothing here turns an empty or mistyped field into a zero.

import type { ConnectionConfig, IoConfig } from '../api/client';

export type RegisterKind = 'holding' | 'input';
export const DATA_TYPES = ['u16', 'i16', 'u32', 'i32', 'f32'] as const;
export type DataType = (typeof DATA_TYPES)[number];
export const WORD_ORDERS = ['high_first', 'low_first'] as const;
export type WordOrder = (typeof WORD_ORDERS)[number];

export interface MappingForm {
  key: string;
  uri: string;
  register: RegisterKind;
  address: string;
  dataType: DataType;
  wordOrder: WordOrder;
  scale: string;
}

export interface ModbusForm {
  host: string;
  port: string;
  slaveId: string;
  timeoutMs: string;
  mappings: MappingForm[];
}

export interface ConnectionForm {
  /** Identity of this row in the editor. Not stored: the connection's own id can be edited. */
  key: string;
  id: string;
  driver: string;
  pollIntervalMs: string;
  /** Empty means "use the server's default". */
  cycleTimeoutMs: string;
  /** Present when the driver configuration is one this workbench has a form for. */
  modbus: ModbusForm | null;
  /** The driver configuration exactly as stored, when there is no form for it. */
  raw: unknown;
}

let nextKey = 1;
const newKey = () => `k${nextKey++}`;

/** A register count per value, to say which addresses a mapping occupies. */
export function registerCount(dataType: DataType): number {
  return dataType === 'u16' || dataType === 'i16' ? 1 : 2;
}

export function newMapping(): MappingForm {
  return {
    key: newKey(),
    uri: '',
    register: 'holding',
    address: '',
    dataType: 'u16',
    wordOrder: 'high_first',
    scale: '1',
  };
}

/**
 * A new, unsaved Modbus TCP connection.
 *
 * The fields that are pre-filled are the ones the protocol or the server already defines: 502
 * is the registered Modbus TCP port, and unit 1 and a 1000 ms request timeout are what the
 * server applies when they are omitted. The host, the poll interval and the tags have no such
 * answer and start empty.
 */
export function newConnection(): ConnectionForm {
  return {
    key: newKey(),
    id: '',
    driver: 'modbus',
    pollIntervalMs: '',
    cycleTimeoutMs: '',
    modbus: { host: '', port: '502', slaveId: '1', timeoutMs: '1000', mappings: [newMapping()] },
    raw: null,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function text(value: unknown, fallback: string): string {
  return typeof value === 'number' || typeof value === 'string' ? String(value) : fallback;
}

/** Read a stored Modbus configuration into the form, or `null` when it is not Modbus TCP. */
function modbusForm(config: unknown): ModbusForm | null {
  if (!isRecord(config) || config.transport !== 'tcp') return null;
  const mappings = Array.isArray(config.mappings) ? config.mappings : [];
  return {
    host: text(config.host, ''),
    // Absent fields show what the server applies when they are absent.
    port: text(config.port, '502'),
    slaveId: text(config.slave_id, '1'),
    timeoutMs: text(config.timeout_ms, '1000'),
    mappings: mappings.filter(isRecord).map((mapping) => ({
      key: newKey(),
      uri: text(mapping.uri, ''),
      register: mapping.function_code === 4 ? 'input' : 'holding',
      address: text(mapping.address, ''),
      dataType: DATA_TYPES.find((t) => t === mapping.data_type) ?? 'u16',
      wordOrder: WORD_ORDERS.find((o) => o === mapping.word_order) ?? 'high_first',
      scale: text(mapping.scale, '1'),
    })),
  };
}

export function toForm(config: IoConfig | null): ConnectionForm[] {
  return (config?.connections ?? []).map((connection) => {
    const modbus = connection.driver === 'modbus' ? modbusForm(connection.config) : null;
    return {
      key: newKey(),
      id: connection.id,
      driver: connection.driver,
      pollIntervalMs: String(connection.poll_interval_ms),
      cycleTimeoutMs:
        connection.cycle_timeout_ms === undefined ? '' : String(connection.cycle_timeout_ms),
      modbus,
      raw: modbus ? null : connection.config,
    };
  });
}

/** An error against one field of one connection, found before anything is sent. */
export interface FieldError {
  connectionKey: string;
  /** The mapping the field belongs to, when it belongs to one. */
  mappingKey?: string;
  field: string;
  message: string;
}

/** Parse a whole number in a range. Anything else is an error, never a default. */
function wholeNumber(
  raw: string,
  label: string,
  min: number,
  max: number
): { value: number } | { error: string } {
  const trimmed = raw.trim();
  if (trimmed === '') return { error: `${label} is required` };
  if (!/^-?\d+$/.test(trimmed)) return { error: `${label} must be a whole number` };
  const value = Number(trimmed);
  if (value < min || value > max) return { error: `${label} must be between ${min} and ${max}` };
  return { value };
}

export type FormResult = { ok: true; config: IoConfig } | { ok: false; errors: FieldError[] };

/** Turn the form into a configuration to save, or list what is wrong with it. */
export function fromForm(connections: ConnectionForm[]): FormResult {
  const errors: FieldError[] = [];
  const out: ConnectionConfig[] = [];

  for (const connection of connections) {
    const fail = (field: string, message: string, mappingKey?: string) =>
      errors.push({ connectionKey: connection.key, mappingKey, field, message });

    if (connection.id.trim() === '') fail('id', 'A name is required');

    const poll = wholeNumber(connection.pollIntervalMs, 'Poll interval', 10, 3_600_000);
    if ('error' in poll) fail('pollIntervalMs', poll.error);

    let cycleTimeout: number | undefined;
    if (connection.cycleTimeoutMs.trim() !== '') {
      const parsed = wholeNumber(connection.cycleTimeoutMs, 'Cycle deadline', 1, 3_600_000);
      if ('error' in parsed) fail('cycleTimeoutMs', parsed.error);
      else cycleTimeout = parsed.value;
    }

    let driverConfig: unknown = connection.raw;
    if (connection.modbus) {
      const modbus = connection.modbus;
      if (modbus.host.trim() === '') fail('host', 'A host is required');
      const port = wholeNumber(modbus.port, 'Port', 1, 65535);
      if ('error' in port) fail('port', port.error);
      const slaveId = wholeNumber(modbus.slaveId, 'Unit ID', 1, 247);
      if ('error' in slaveId) fail('slaveId', slaveId.error);
      const timeout = wholeNumber(modbus.timeoutMs, 'Request timeout', 10, 10_000);
      if ('error' in timeout) fail('timeoutMs', timeout.error);

      const mappings = modbus.mappings.map((mapping) => {
        if (mapping.uri.trim() === '') fail('uri', 'A tag name is required', mapping.key);
        const address = wholeNumber(mapping.address, 'Address', 0, 65535);
        if ('error' in address) fail('address', address.error, mapping.key);
        const scaleText = mapping.scale.trim();
        const scale = Number(scaleText);
        if (scaleText === '' || !Number.isFinite(scale) || scale === 0) {
          fail('scale', 'Scale must be a number other than zero', mapping.key);
        }
        return {
          uri: mapping.uri.trim(),
          address: 'value' in address ? address.value : null,
          function_code: mapping.register === 'input' ? 4 : 3,
          data_type: mapping.dataType,
          word_order: mapping.wordOrder,
          scale,
        };
      });

      driverConfig = {
        transport: 'tcp',
        host: modbus.host.trim(),
        port: 'value' in port ? port.value : null,
        slave_id: 'value' in slaveId ? slaveId.value : null,
        timeout_ms: 'value' in timeout ? timeout.value : null,
        mappings,
      };
    }

    out.push({
      id: connection.id.trim(),
      driver: connection.driver,
      poll_interval_ms: 'value' in poll ? poll.value : Number.NaN,
      ...(cycleTimeout === undefined ? {} : { cycle_timeout_ms: cycleTimeout }),
      config: driverConfig,
    });
  }

  // The partially built `out` is discarded whenever there is an error, so the placeholders in
  // it for fields that failed to parse never leave this function.
  return errors.length > 0 ? { ok: false, errors } : { ok: true, config: { connections: out } };
}
