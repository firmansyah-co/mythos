import { Plus, Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { useIoConfig } from '../data/ioConfig';
import { driverForm, schemaFor, type ConnectionForm, type FieldError } from '../data/ioForm';
import {
  choicesOf,
  isScalar,
  newRow,
  orderedProperties,
  type FieldForm,
  type JsonSchema,
  type ObjectForm,
  type RowsForm,
} from '../data/schemaForm';
import { connectionLabel } from '../data/derive';
import { useWorkbench, editorId } from '../data/workbench';

function errorFor(errors: FieldError[], connectionKey: string, field: string): string | undefined {
  return errors.find((e) => e.connectionKey === connectionKey && e.field === field)?.message;
}

function Field({
  label,
  hint,
  error,
  children,
}: {
  label: string;
  hint?: string;
  error?: string;
  children: ReactNode;
}) {
  return (
    <label className={`field${error ? ' has-error' : ''}`}>
      <span className="field__label">{label}</span>
      {children}
      {error ? (
        <span className="field__error" role="alert">
          {error}
        </span>
      ) : (
        hint && <span className="field__hint">{hint}</span>
      )}
    </label>
  );
}

/** The input for one scalar of a driver configuration: a choice, or text. */
function ScalarInput({
  name,
  schema,
  value,
  invalid,
  onChange,
  compact,
}: {
  name: string;
  schema: JsonSchema;
  value: string;
  invalid: boolean;
  onChange: (value: string) => void;
  compact?: boolean;
}) {
  const label = schema.title ?? name;
  const choices = choicesOf(schema);
  if (choices) {
    return (
      <select value={value} aria-label={label} aria-invalid={invalid} onChange={(e) => onChange(e.target.value)}>
        {value === '' && <option value="">—</option>}
        {choices.map((choice) => (
          <option key={choice.value} value={choice.value}>
            {choice.label}
          </option>
        ))}
      </select>
    );
  }
  const numeric = schema.type === 'integer' || schema.type === 'number';
  return (
    <input
      className={`mono${compact && numeric ? ' num-input' : ''}`}
      inputMode={schema.type === 'integer' ? 'numeric' : schema.type === 'number' ? 'decimal' : undefined}
      placeholder={schema['x-mythos-role'] === 'tag-uri' ? schema.description : undefined}
      value={value}
      aria-label={label}
      aria-invalid={invalid}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

/** A list of objects as a table: one row per item, one column per field. */
function RowsTable({
  name,
  schema,
  rows,
  error,
  onChange,
}: {
  name: string;
  schema: JsonSchema;
  rows: RowsForm;
  error: (path: string[]) => string | undefined;
  onChange: (rows: RowsForm) => void;
}) {
  const items = schema.items ?? {};
  const columns = orderedProperties(items);
  const title = schema.title ?? name;
  const itemTitle = (items.title ?? 'row').toLowerCase();
  const nameOf = (row: ObjectForm) => {
    const [named] = columns.find(([, p]) => p['x-mythos-role'] === 'tag-uri') ?? columns[0] ?? [];
    const value = named ? row.fields[named] : undefined;
    return typeof value === 'string' && value.trim() !== '' ? value.trim() : '(unnamed)';
  };
  const setRow = (key: string, value: ObjectForm) =>
    onChange({ kind: 'rows', rows: rows.rows.map((row) => (row.key === key ? { key, value } : row)) });

  return (
    <>
      <h3 className="editor__section">
        {title}
        <button
          type="button"
          className="btn btn--flat editor__section-action"
          onClick={() => onChange({ kind: 'rows', rows: [...rows.rows, newRow(schema)] })}
        >
          <Plus size={13} aria-hidden /> Add {itemTitle}
        </button>
      </h3>
      {rows.rows.length === 0 ? (
        <p className="editor__empty muted">This connection has no {title.toLowerCase()}.</p>
      ) : (
        <table className="grid grid--form" aria-label={title}>
          <thead>
            <tr>
              {columns.map(([column, property]) => (
                <th key={column} title={property.description}>
                  {property.title ?? column}
                </th>
              ))}
              <th aria-label="Remove" />
            </tr>
          </thead>
          <tbody>
            {rows.rows.map((row) => {
              const rowErrors = columns
                .map(([column]) => error([name, row.key, column]))
                .filter((message): message is string => Boolean(message));
              return (
                <tr key={row.key} data-testid="mapping-row">
                  {columns.map(([column, property], index) => (
                    <td key={column} className={index === 0 ? 'grow' : undefined}>
                      <ScalarInput
                        name={column}
                        schema={property}
                        value={typeof row.value.fields[column] === 'string' ? (row.value.fields[column] as string) : ''}
                        invalid={Boolean(error([name, row.key, column]))}
                        compact
                        onChange={(value) =>
                          setRow(row.key, { ...row.value, fields: { ...row.value.fields, [column]: value } })
                        }
                      />
                      {index === 0 && rowErrors.length > 0 && (
                        <span className="field__error" role="alert">
                          {rowErrors.join(' · ')}
                        </span>
                      )}
                    </td>
                  ))}
                  <td>
                    <button
                      type="button"
                      className="icon-btn"
                      aria-label={`Remove ${itemTitle} ${nameOf(row.value)}`}
                      onClick={() =>
                        onChange({ kind: 'rows', rows: rows.rows.filter((r) => r.key !== row.key) })
                      }
                    >
                      <Trash2 size={13} />
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </>
  );
}

/** A driver configuration drawn from its module's schema. */
function SchemaFields({
  schema,
  form,
  error,
  onChange,
}: {
  schema: JsonSchema;
  form: ObjectForm;
  error: (path: string[]) => string | undefined;
  onChange: (form: ObjectForm) => void;
}) {
  const properties = orderedProperties(schema);
  const set = (name: string, value: FieldForm) => onChange({ ...form, fields: { ...form.fields, [name]: value } });
  const scalars = properties.filter(([, property]) => isScalar(property));
  const lists = properties.filter(([, property]) => !isScalar(property));
  return (
    <>
      <div className="form-grid">
        {scalars.map(([name, property]) => (
          <Field key={name} label={property.title ?? name} error={error([name])} hint={property.description}>
            <ScalarInput
              name={name}
              schema={property}
              value={typeof form.fields[name] === 'string' ? (form.fields[name] as string) : ''}
              invalid={Boolean(error([name]))}
              onChange={(value) => set(name, value)}
            />
          </Field>
        ))}
      </div>
      {lists.map(([name, property]) => {
        const field = form.fields[name];
        return field && typeof field === 'object' && field.kind === 'rows' ? (
          <RowsTable
            key={name}
            name={name}
            schema={property}
            rows={field}
            error={error}
            onChange={(rows) => set(name, rows)}
          />
        ) : null;
      })}
    </>
  );
}

/** Edits one connection of the draft I/O configuration. */
export function ConnectionEditor({ connectionKey }: { connectionKey: string }) {
  const connection = useIoConfig((state) => state.draft.find((c) => c.key === connectionKey));
  const drivers = useIoConfig((state) => state.drivers);
  const driverInfo = useIoConfig((state) => state.driverInfo);
  const schemas = useIoConfig((state) => state.schemas);
  const fieldErrors = useIoConfig((state) => state.fieldErrors);
  const updateConnection = useIoConfig((state) => state.updateConnection);
  const removeConnection = useIoConfig((state) => state.removeConnection);
  const closeEditor = useWorkbench((state) => state.closeEditor);

  if (!connection) {
    return (
      <div className="notice">
        <div>
          <h3>This connection is no longer in the configuration</h3>
          <p>It was removed, or the changes that added it were discarded.</p>
        </div>
      </div>
    );
  }

  const update = (change: Partial<ConnectionForm>) =>
    updateConnection(connectionKey, (c) => ({ ...c, ...change }));
  const changeDriver = (driver: string) =>
    updateConnection(connectionKey, (c) => {
      // Another driver's configuration means nothing to this one: start from its own form.
      const config = driverForm(schemas, driver);
      return { ...c, driver, config, raw: config ? null : {} };
    });

  const error = (field: string) => errorFor(fieldErrors, connectionKey, field);
  const configError = (path: string[]) => error(['config', ...path].join('.'));
  const knownDriver = drivers.includes(connection.driver);
  const schema = schemaFor(schemas, connection.driver);
  const manifest = driverInfo.find((d) => d.name === connection.driver)?.module?.manifest;

  return (
    <div className="editor__scroll" data-testid="connection-editor">
      <h3 className="editor__section">General</h3>
      <div className="form-grid">
        <Field label="Name" error={error('id')} hint="Identifies the connection within the project.">
          <input value={connection.id} aria-label="Name" onChange={(event) => update({ id: event.target.value })} />
        </Field>
        <Field
          label="Driver"
          error={error('driver')}
          hint={
            knownDriver
              ? manifest
                ? `${manifest.name} ${manifest.version} — ${manifest.id}`
                : undefined
              : 'The controller does not list this driver.'
          }
        >
          <select value={connection.driver} aria-label="Driver" onChange={(event) => changeDriver(event.target.value)}>
            {[...new Set([...drivers, connection.driver])].map((driver) => (
              <option key={driver} value={driver}>
                {driver || '—'}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Poll interval (ms)" error={error('pollIntervalMs')} hint="How often the device is read. Required.">
          <input
            className="mono"
            inputMode="numeric"
            value={connection.pollIntervalMs}
            aria-label="Poll interval (ms)"
            onChange={(event) => update({ pollIntervalMs: event.target.value })}
          />
        </Field>
        <Field
          label="Cycle deadline (ms)"
          error={error('cycleTimeoutMs')}
          hint="Optional. Empty uses five poll intervals, at least 2000 ms."
        >
          <input
            className="mono"
            inputMode="numeric"
            value={connection.cycleTimeoutMs}
            aria-label="Cycle deadline (ms)"
            onChange={(event) => update({ cycleTimeoutMs: event.target.value })}
          />
        </Field>
      </div>

      {connection.config && schema ? (
        <>
          <h3 className="editor__section">{manifest?.name ?? schema.title ?? connection.driver}</h3>
          <SchemaFields
            schema={schema}
            form={connection.config}
            error={configError}
            onChange={(config) => update({ config })}
          />
        </>
      ) : (
        <>
          <h3 className="editor__section">Driver configuration</h3>
          <div className="notice" data-testid="raw-config">
            <div>
              <h3>The workbench has no form for this driver configuration</h3>
              <p>
                {knownDriver
                  ? 'Its module does not describe its configuration in a form the workbench can draw.'
                  : 'The controller has no module for this driver.'}{' '}
                It is kept exactly as stored and saved back unchanged.
              </p>
              <pre className="mono raw">{JSON.stringify(connection.raw, null, 2)}</pre>
            </div>
          </div>
        </>
      )}

      <div className="editor__danger">
        <button
          type="button"
          className="btn"
          onClick={() => {
            removeConnection(connectionKey);
            closeEditor(editorId('connection', connectionKey));
          }}
        >
          <Trash2 size={13} aria-hidden /> Remove {connectionLabel(connection.id)} from the configuration
        </button>
        <span className="muted">Takes effect when the configuration is saved.</span>
      </div>
    </div>
  );
}
