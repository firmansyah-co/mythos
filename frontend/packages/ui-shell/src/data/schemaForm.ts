// A form drawn from a module's JSON Schema, and the conversion to and from the stored value.
//
// A driver module publishes the schema of its configuration in its manifest. The workbench
// draws the connection form from it, so a new driver needs no workbench change. The module stays
// the authority on what is valid: the checks here are the ones the schema states, to point at a
// field before anything is sent, and the controller checks again.
//
// As everywhere in this workbench, a number is held as the text that was typed and becomes a
// number only when it is saved; text that is not a number is an error against its field, never a
// zero. A field the schema gives a default starts with that default, because the module applies
// it when the field is absent. A field without one starts empty. Keys of the stored value that
// the schema does not describe are kept and saved back unchanged.

export interface JsonSchema {
  type?: string | string[];
  title?: string;
  description?: string;
  enum?: unknown[];
  /** Labels for `enum`, in the same order. */
  'x-enum-labels'?: string[];
  default?: unknown;
  minimum?: number;
  maximum?: number;
  minLength?: number;
  pattern?: string;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  items?: JsonSchema;
  /** `tag-uri`: the field names a tag the connection provides. */
  'x-mythos-role'?: string;
  /** How to summarise a configuration in one line: `{field}` is replaced by its value. */
  'x-mythos-summary'?: string;
  /**
   * The order to show an object's properties in. JSON objects carry no order a server is bound
   * to keep, so a schema that cares says so here. Properties it does not name follow, sorted.
   */
  'x-mythos-order'?: string[];
}

/** An object schema's properties, in the order to show them. */
export function orderedProperties(schema: JsonSchema): [string, JsonSchema][] {
  const properties = schema.properties ?? {};
  const order = (schema['x-mythos-order'] ?? []).filter((name) => name in properties);
  const rest = Object.keys(properties)
    .filter((name) => !order.includes(name))
    .sort();
  return [...order, ...rest].flatMap((name): [string, JsonSchema][] => {
    const property = properties[name];
    return property ? [[name, property]] : [];
  });
}

/** An object's fields, as edited. */
export interface ObjectForm {
  kind: 'object';
  fields: Record<string, FieldForm>;
  /** Stored keys the schema does not describe, kept as they were. */
  extra: Record<string, unknown>;
}

/** An array of objects, as edited: one row per item. */
export interface RowsForm {
  kind: 'rows';
  rows: { key: string; value: ObjectForm }[];
}

/**
 * A scalar is the text in its field. For a choice it is the chosen value as text, turned back
 * into the schema's own value (a number stays a number) when saved; `''` is "nothing chosen".
 */
export type FieldForm = string | RowsForm | ObjectForm;

export interface SchemaFieldError {
  /** Property names and row keys from the configuration's root to the field. */
  path: string[];
  message: string;
}

let nextRow = 1;
const rowKey = () => `r${nextRow++}`;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function typeOf(schema: JsonSchema): string | undefined {
  return Array.isArray(schema.type) ? schema.type.find((t) => t !== 'null') : schema.type;
}

export function isScalar(schema: JsonSchema): boolean {
  if (schema.enum) return schema.enum.length > 0;
  const type = typeOf(schema);
  return type === 'string' || type === 'integer' || type === 'number' || type === 'boolean';
}

function isRowsSchema(schema: JsonSchema): boolean {
  return (
    typeOf(schema) === 'array' &&
    !!schema.items &&
    typeOf(schema.items) === 'object' &&
    isFlatObject(schema.items)
  );
}

function isFlatObject(schema: JsonSchema): boolean {
  return (
    typeOf(schema) === 'object' &&
    Object.values(schema.properties ?? {}).every((property) => isScalar(property))
  );
}

/**
 * Whether this workbench can draw a form for `schema`: an object whose properties are scalars
 * (text, numbers, booleans, choices) or lists of objects of scalars. Anything else is shown as
 * the stored JSON and saved back unchanged.
 */
export function isSupported(schema: JsonSchema | undefined): schema is JsonSchema {
  if (!schema || typeOf(schema) !== 'object' || !schema.properties) return false;
  return Object.values(schema.properties).every(
    (property) => isScalar(property) || isRowsSchema(property)
  );
}

/** The choices of a scalar field: an `enum`, or true/false for a boolean. */
export function choicesOf(schema: JsonSchema): { value: string; label: string }[] | null {
  const values = schema.enum ?? (typeOf(schema) === 'boolean' ? [true, false] : null);
  if (!values) return null;
  return values.map((value, index) => ({
    value: String(value),
    label: schema['x-enum-labels']?.[index] ?? String(value),
  }));
}

function scalarText(schema: JsonSchema, value: unknown): string {
  if (value === undefined || value === null) return '';
  if (choicesOf(schema)) return String(value);
  return typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean'
    ? String(value)
    : JSON.stringify(value);
}

function objectForm(schema: JsonSchema, stored: unknown, fresh: boolean): ObjectForm {
  const value = isRecord(stored) ? stored : {};
  const properties = schema.properties ?? {};
  const required = new Set(schema.required ?? []);
  const fields: Record<string, FieldForm> = {};
  for (const [name, property] of Object.entries(properties)) {
    const present = name in value;
    if (isRowsSchema(property)) {
      const items = Array.isArray(value[name]) ? (value[name] as unknown[]) : [];
      const rows = items.map((item) => ({
        key: rowKey(),
        value: objectForm(property.items!, item, false),
      }));
      // A new configuration starts with one empty row of a list it cannot do without.
      if (!present && fresh && required.has(name)) {
        rows.push({ key: rowKey(), value: objectForm(property.items!, undefined, true) });
      }
      fields[name] = { kind: 'rows', rows };
    } else {
      fields[name] = scalarText(property, present ? value[name] : property.default);
    }
  }
  const extra = Object.fromEntries(Object.entries(value).filter(([name]) => !(name in properties)));
  return { kind: 'object', fields, extra };
}

/** The form for a stored configuration, or for a new one when `stored` is `undefined`. */
export function formFromValue(schema: JsonSchema, stored: unknown): ObjectForm {
  return objectForm(schema, stored, stored === undefined);
}

/** A new, empty row of a list field. */
export function newRow(schema: JsonSchema): { key: string; value: ObjectForm } {
  return { key: rowKey(), value: objectForm(schema.items ?? {}, undefined, true) };
}

function label(name: string, schema: JsonSchema): string {
  return schema.title ?? name;
}

type Parsed = { value: unknown } | { omit: true } | { error: string };

function parseScalar(name: string, schema: JsonSchema, text: string, required: boolean): Parsed {
  const title = label(name, schema);
  const trimmed = text.trim();
  if (trimmed === '') return required ? { error: `${title} is required` } : { omit: true };

  if (choicesOf(schema)) {
    const values = schema.enum ?? [true, false];
    const matches = values.filter((choice) => String(choice) === trimmed);
    // Two choices with the same text (1 and "1") cannot be told apart from a field: refuse.
    return matches.length === 1
      ? { value: matches[0] }
      : { error: `${title} must be one of the listed choices` };
  }

  const type = typeOf(schema);
  if (type === 'integer' || type === 'number') {
    const ok = type === 'integer' ? /^-?\d+$/.test(trimmed) : trimmed !== '' && Number.isFinite(Number(trimmed));
    if (!ok) return { error: `${title} must be ${type === 'integer' ? 'a whole number' : 'a number'}` };
    const value = Number(trimmed);
    const { minimum: min, maximum: max } = schema;
    if (min !== undefined && max !== undefined && (value < min || value > max)) {
      return { error: `${title} must be between ${min} and ${max}` };
    }
    if (min !== undefined && value < min) return { error: `${title} must be at least ${min}` };
    if (max !== undefined && value > max) return { error: `${title} must be at most ${max}` };
    return { value };
  }

  if (schema.minLength !== undefined && trimmed.length < schema.minLength) {
    return { error: `${title} is too short` };
  }
  if (schema.pattern !== undefined) {
    let pattern: RegExp | null = null;
    try {
      pattern = new RegExp(schema.pattern);
    } catch {
      pattern = null;
    }
    if (pattern && !pattern.test(trimmed)) {
      return {
        error: schema.description
          ? `${title} must have the form ${schema.description}`
          : `${title} does not have the expected form`,
      };
    }
  }
  return { value: trimmed };
}

function objectValue(
  schema: JsonSchema,
  form: ObjectForm,
  path: string[],
  errors: SchemaFieldError[]
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...form.extra };
  const required = new Set(schema.required ?? []);
  for (const [name, property] of Object.entries(schema.properties ?? {})) {
    const field = form.fields[name];
    if (field !== undefined && typeof field === 'object' && field.kind === 'rows') {
      out[name] = field.rows.map((row) =>
        objectValue(property.items ?? {}, row.value, [...path, name, row.key], errors)
      );
      continue;
    }
    const parsed = parseScalar(name, property, typeof field === 'string' ? field : '', required.has(name));
    if ('error' in parsed) errors.push({ path: [...path, name], message: parsed.error });
    else if ('value' in parsed) out[name] = parsed.value;
  }
  return out;
}

/** The value to save, or every field that is wrong with the form. */
export function valueFromForm(
  schema: JsonSchema,
  form: ObjectForm
): { ok: true; value: Record<string, unknown> } | { ok: false; errors: SchemaFieldError[] } {
  const errors: SchemaFieldError[] = [];
  const value = objectValue(schema, form, [], errors);
  return errors.length > 0 ? { ok: false, errors } : { ok: true, value };
}

/** The tags a configuration names, from the fields the schema marks as tag URIs. */
export function tagUris(schema: JsonSchema, form: ObjectForm): string[] {
  const uris: string[] = [];
  for (const [name, property] of Object.entries(schema.properties ?? {})) {
    const field = form.fields[name];
    if (property['x-mythos-role'] === 'tag-uri' && typeof field === 'string') {
      uris.push(field.trim());
    }
    if (typeof field === 'object' && field.kind === 'rows' && property.items) {
      for (const row of field.rows) uris.push(...tagUris(property.items, row.value));
    }
  }
  return uris.filter((uri) => uri !== '');
}

/**
 * One line describing a configuration, from the schema's `x-mythos-summary`. `null` when the
 * schema has none, or when a field it names is empty: a summary with a hole would describe a
 * device that is not there.
 */
export function summary(schema: JsonSchema, form: ObjectForm): string | null {
  const template = schema['x-mythos-summary'];
  if (!template) return null;
  let complete = true;
  const text = template.replace(/\{([^}]+)\}/g, (_, name: string) => {
    const field = form.fields[name];
    const value = typeof field === 'string' ? field.trim() : '';
    if (value === '') complete = false;
    return value;
  });
  return complete ? text : null;
}
