// The editable form of an I/O configuration, and the conversion to and from the stored one.
//
// A connection has fields the controller defines (its name, driver, poll interval and cycle
// deadline) and a driver configuration that the driver module defines. The latter is drawn from
// the module's JSON Schema (see `schemaForm`); when there is no schema the workbench can draw,
// the stored configuration is shown and saved back unchanged.
//
// Every numeric field is held as the text the engineer typed. It becomes a number only when
// the configuration is saved, and text that is not a number is reported as an error against
// its field. Nothing here turns an empty or mistyped field into a zero.

import type { ConnectionConfig, IoConfig } from '../api/client';
import {
  formFromValue,
  isSupported,
  summary,
  tagUris,
  valueFromForm,
  type JsonSchema,
  type ObjectForm,
} from './schemaForm';

/** The configuration schema of each driver that publishes one, by driver name. */
export type DriverSchemas = Record<string, JsonSchema>;

export interface ConnectionForm {
  /** Identity of this row in the editor. Not stored: the connection's own id can be edited. */
  key: string;
  id: string;
  driver: string;
  pollIntervalMs: string;
  /** Empty means "use the server's default". */
  cycleTimeoutMs: string;
  /** The driver configuration as a form, when its driver publishes a schema it can draw. */
  config: ObjectForm | null;
  /** The driver configuration exactly as stored, when there is no form for it. */
  raw: unknown;
}

let nextKey = 1;
const newKey = () => `k${nextKey++}`;

/** The schema for `driver`, when there is one this workbench can draw. */
export function schemaFor(schemas: DriverSchemas, driver: string): JsonSchema | null {
  const schema = schemas[driver];
  return isSupported(schema) ? schema : null;
}

/** A driver configuration as a form, or `null` when the driver has no drawable schema. */
export function driverForm(schemas: DriverSchemas, driver: string, stored?: unknown): ObjectForm | null {
  const schema = schemaFor(schemas, driver);
  return schema ? formFromValue(schema, stored) : null;
}

/**
 * A new, unsaved connection using `driver`. Its driver fields start with the defaults the
 * module's schema declares and are otherwise empty; the name and poll interval have no answer
 * and start empty.
 */
export function newConnection(driver: string, schemas: DriverSchemas): ConnectionForm {
  const config = driverForm(schemas, driver);
  return {
    key: newKey(),
    id: '',
    driver,
    pollIntervalMs: '',
    cycleTimeoutMs: '',
    config,
    raw: config ? null : {},
  };
}

export function toForm(config: IoConfig | null, schemas: DriverSchemas): ConnectionForm[] {
  return (config?.connections ?? []).map((connection) => {
    const form = driverForm(schemas, connection.driver, connection.config ?? {});
    return {
      key: newKey(),
      id: connection.id,
      driver: connection.driver,
      pollIntervalMs: String(connection.poll_interval_ms),
      cycleTimeoutMs:
        connection.cycle_timeout_ms === undefined ? '' : String(connection.cycle_timeout_ms),
      config: form,
      raw: form ? null : connection.config,
    };
  });
}

/** The tags a connection is configured to provide, or `null` when its driver has no form. */
export function connectionTags(connection: ConnectionForm, schemas: DriverSchemas): string[] | null {
  const schema = schemaFor(schemas, connection.driver);
  return schema && connection.config ? tagUris(schema, connection.config) : null;
}

/** What a connection talks to, in one line, or `null` when that cannot be said. */
export function connectionTarget(connection: ConnectionForm, schemas: DriverSchemas): string | null {
  const schema = schemaFor(schemas, connection.driver);
  return schema && connection.config ? summary(schema, connection.config) : null;
}

/** An error against one field of one connection, found before anything is sent. */
export interface FieldError {
  connectionKey: string;
  /**
   * `id`, `pollIntervalMs` or `cycleTimeoutMs` for the connection's own fields; for a driver
   * field, `config.` followed by its path, with a row's key for a field in a list
   * (`config.mappings.r3.address`).
   */
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
export function fromForm(connections: ConnectionForm[], schemas: DriverSchemas): FormResult {
  const errors: FieldError[] = [];
  const out: ConnectionConfig[] = [];

  for (const connection of connections) {
    const fail = (field: string, message: string) =>
      errors.push({ connectionKey: connection.key, field, message });

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
    const schema = schemaFor(schemas, connection.driver);
    if (connection.config && schema) {
      const result = valueFromForm(schema, connection.config);
      if (result.ok) driverConfig = result.value;
      else for (const error of result.errors) fail(['config', ...error.path].join('.'), error.message);
    } else if (connection.config) {
      fail('driver', `The controller no longer describes driver '${connection.driver}'`);
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
