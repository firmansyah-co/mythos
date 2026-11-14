import { Plus, Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { useIoConfig } from '../data/ioConfig';
import {
  DATA_TYPES,
  WORD_ORDERS,
  newMapping,
  registerCount,
  type ConnectionForm,
  type FieldError,
  type MappingForm,
  type ModbusForm,
} from '../data/ioForm';
import { connectionLabel } from '../data/derive';
import { useWorkbench, editorId } from '../data/workbench';

function errorFor(
  errors: FieldError[],
  connectionKey: string,
  field: string,
  mappingKey?: string
): string | undefined {
  return errors.find(
    (e) => e.connectionKey === connectionKey && e.field === field && e.mappingKey === mappingKey
  )?.message;
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

/** Edits one connection of the draft I/O configuration. */
export function ConnectionEditor({ connectionKey }: { connectionKey: string }) {
  const connection = useIoConfig((state) => state.draft.find((c) => c.key === connectionKey));
  const drivers = useIoConfig((state) => state.drivers);
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
  const updateModbus = (change: Partial<ModbusForm>) =>
    updateConnection(connectionKey, (c) => (c.modbus ? { ...c, modbus: { ...c.modbus, ...change } } : c));
  const updateMapping = (key: string, change: Partial<MappingForm>) =>
    updateConnection(connectionKey, (c) =>
      c.modbus
        ? {
            ...c,
            modbus: {
              ...c.modbus,
              mappings: c.modbus.mappings.map((m) => (m.key === key ? { ...m, ...change } : m)),
            },
          }
        : c
    );

  const error = (field: string, mappingKey?: string) =>
    errorFor(fieldErrors, connectionKey, field, mappingKey);
  const knownDriver = drivers.includes(connection.driver);

  return (
    <div className="editor__scroll" data-testid="connection-editor">
      <h3 className="editor__section">General</h3>
      <div className="form-grid">
        <Field label="Name" error={error('id')} hint="Identifies the connection within the project.">
          <input
            value={connection.id}
            aria-label="Name"
            onChange={(event) => update({ id: event.target.value })}
          />
        </Field>
        <Field
          label="Driver"
          hint={knownDriver ? undefined : 'The controller does not list this driver.'}
        >
          <select
            value={connection.driver}
            aria-label="Driver"
            onChange={(event) => update({ driver: event.target.value })}
            disabled={!connection.modbus}
          >
            {[...new Set([...drivers, connection.driver])].map((driver) => (
              <option key={driver} value={driver}>
                {driver}
              </option>
            ))}
          </select>
        </Field>
        <Field
          label="Poll interval (ms)"
          error={error('pollIntervalMs')}
          hint="How often the device is read. Required."
        >
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

      {connection.modbus ? (
        <>
          <h3 className="editor__section">Modbus TCP</h3>
          <div className="form-grid">
            <Field label="Host" error={error('host')} hint="Address of the device or gateway.">
              <input
                className="mono"
                value={connection.modbus.host}
                aria-label="Host"
                onChange={(event) => updateModbus({ host: event.target.value })}
              />
            </Field>
            <Field label="Port" error={error('port')}>
              <input
                className="mono"
                inputMode="numeric"
                value={connection.modbus.port}
                aria-label="Port"
                onChange={(event) => updateModbus({ port: event.target.value })}
              />
            </Field>
            <Field label="Unit ID" error={error('slaveId')}>
              <input
                className="mono"
                inputMode="numeric"
                value={connection.modbus.slaveId}
                aria-label="Unit ID"
                onChange={(event) => updateModbus({ slaveId: event.target.value })}
              />
            </Field>
            <Field label="Request timeout (ms)" error={error('timeoutMs')}>
              <input
                className="mono"
                inputMode="numeric"
                value={connection.modbus.timeoutMs}
                aria-label="Request timeout (ms)"
                onChange={(event) => updateModbus({ timeoutMs: event.target.value })}
              />
            </Field>
          </div>

          <h3 className="editor__section">
            Tags
            <button
              type="button"
              className="btn btn--flat editor__section-action"
              onClick={() =>
                updateModbus({ mappings: [...connection.modbus!.mappings, newMapping()] })
              }
            >
              <Plus size={13} aria-hidden /> Add tag
            </button>
          </h3>
          {connection.modbus.mappings.length === 0 ? (
            <p className="editor__empty muted">
              This connection has no tags. A connection must provide at least one.
            </p>
          ) : (
            <table className="grid grid--form" aria-label="Tag mappings">
              <thead>
                <tr>
                  <th>Tag</th>
                  <th>Register</th>
                  <th>Address</th>
                  <th>Data type</th>
                  <th>Word order</th>
                  <th>Scale</th>
                  <th aria-label="Remove" />
                </tr>
              </thead>
              <tbody>
                {connection.modbus.mappings.map((mapping) => {
                  const wide = registerCount(mapping.dataType) === 2;
                  const rowErrors = ['uri', 'address', 'scale']
                    .map((field) => error(field, mapping.key))
                    .filter((message): message is string => Boolean(message));
                  return (
                    <tr key={mapping.key} data-testid="mapping-row">
                      <td className="grow">
                        <input
                          className="mono"
                          placeholder="external.<area>.<device>.<variable>"
                          value={mapping.uri}
                          aria-label="Tag"
                          aria-invalid={Boolean(error('uri', mapping.key))}
                          onChange={(event) => updateMapping(mapping.key, { uri: event.target.value })}
                        />
                        {rowErrors.length > 0 && (
                          <span className="field__error" role="alert">
                            {rowErrors.join(' · ')}
                          </span>
                        )}
                      </td>
                      <td>
                        <select
                          value={mapping.register}
                          aria-label="Register"
                          onChange={(event) =>
                            updateMapping(mapping.key, {
                              register: event.target.value === 'input' ? 'input' : 'holding',
                            })
                          }
                        >
                          <option value="holding">Holding</option>
                          <option value="input">Input</option>
                        </select>
                      </td>
                      <td>
                        <input
                          className="mono num-input"
                          inputMode="numeric"
                          value={mapping.address}
                          aria-label="Address"
                          aria-invalid={Boolean(error('address', mapping.key))}
                          onChange={(event) =>
                            updateMapping(mapping.key, { address: event.target.value })
                          }
                        />
                      </td>
                      <td>
                        <select
                          value={mapping.dataType}
                          aria-label="Data type"
                          onChange={(event) => {
                            const dataType = DATA_TYPES.find((t) => t === event.target.value);
                            if (dataType) updateMapping(mapping.key, { dataType });
                          }}
                        >
                          {DATA_TYPES.map((type) => (
                            <option key={type} value={type}>
                              {type}
                            </option>
                          ))}
                        </select>
                      </td>
                      <td>
                        <select
                          value={mapping.wordOrder}
                          aria-label="Word order"
                          disabled={!wide}
                          title={wide ? undefined : 'Only two-register types have a word order'}
                          onChange={(event) => {
                            const wordOrder = WORD_ORDERS.find((o) => o === event.target.value);
                            if (wordOrder) updateMapping(mapping.key, { wordOrder });
                          }}
                        >
                          <option value="high_first">High word first</option>
                          <option value="low_first">Low word first</option>
                        </select>
                      </td>
                      <td>
                        <input
                          className="mono num-input"
                          inputMode="decimal"
                          value={mapping.scale}
                          aria-label="Scale"
                          aria-invalid={Boolean(error('scale', mapping.key))}
                          onChange={(event) => updateMapping(mapping.key, { scale: event.target.value })}
                        />
                      </td>
                      <td>
                        <button
                          type="button"
                          className="icon-btn"
                          aria-label={`Remove tag ${mapping.uri || '(unnamed)'}`}
                          onClick={() =>
                            updateModbus({
                              mappings: connection.modbus!.mappings.filter((m) => m.key !== mapping.key),
                            })
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
      ) : (
        <>
          <h3 className="editor__section">Driver configuration</h3>
          <div className="notice" data-testid="raw-config">
            <div>
              <h3>The workbench has no form for this driver configuration</h3>
              <p>It is kept exactly as stored and saved back unchanged.</p>
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
