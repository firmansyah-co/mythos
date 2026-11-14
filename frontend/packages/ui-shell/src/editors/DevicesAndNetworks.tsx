import { Plus, RotateCcw, Save } from 'lucide-react';
import type { ConnectionStatus } from '../api/client';
import { connectionLabel } from '../data/derive';
import { isDirty, useIoConfig } from '../data/ioConfig';
import type { ConnectionForm } from '../data/ioForm';
import { useProjectIo, useRuntimeScope } from '../data/scope';
import { useWorkbench } from '../data/workbench';
import { ConnectionStateBadge } from './ConnectionStateBadge';

function target(connection: ConnectionForm): string {
  if (!connection.modbus) return '—';
  const { host, port, slaveId } = connection.modbus;
  return host.trim() === '' ? '—' : `${host}:${port} · unit ${slaveId}`;
}

/** The save bar shared by the configuration editors: what is pending, and what went wrong. */
export function ConfigBar() {
  const phase = useIoConfig((state) => state.phase);
  const dirty = useIoConfig(isDirty);
  const fieldErrors = useIoConfig((state) => state.fieldErrors);
  const problems = useIoConfig((state) => state.problems);
  const saveError = useIoConfig((state) => state.saveError);
  const save = useIoConfig((state) => state.save);
  const discard = useIoConfig((state) => state.discard);
  const addConnection = useIoConfig((state) => state.addConnection);
  const openEditor = useWorkbench((state) => state.openEditor);
  const saving = phase === 'saving';

  return (
    <>
      <div className="editor__tools">
        <button
          type="button"
          className="btn"
          onClick={() => openEditor('connection', addConnection())}
        >
          <Plus size={13} aria-hidden /> Add connection
        </button>
        <span className="toolbar__sep" aria-hidden />
        <button
          type="button"
          className="btn btn--primary"
          disabled={!dirty || saving}
          onClick={() => void save()}
        >
          <Save size={13} aria-hidden /> {saving ? 'Saving…' : 'Save configuration'}
        </button>
        <button type="button" className="btn" disabled={!dirty || saving} onClick={discard}>
          <RotateCcw size={13} aria-hidden /> Discard changes
        </button>
        {dirty && (
          <span className="badge badge--warn" data-testid="unsaved">
            UNSAVED CHANGES
          </span>
        )}
      </div>

      {fieldErrors.length > 0 && (
        <div className="notice notice--bad" role="alert" data-testid="field-errors">
          <div>
            <h3>The configuration was not sent</h3>
            <p>
              {fieldErrors.length} field{fieldErrors.length === 1 ? ' is' : 's are'} missing or not
              valid. Open each connection to see which.
            </p>
          </div>
        </div>
      )}
      {problems.length > 0 && (
        <div className="notice notice--bad" role="alert" data-testid="server-problems">
          <div>
            <h3>The controller refused the configuration, and nothing was saved</h3>
            {problems.map((problem, index) => (
              <p key={index}>
                {problem.connection_id ? <strong>{problem.connection_id}: </strong> : null}
                {problem.message}
              </p>
            ))}
          </div>
        </div>
      )}
      {saveError && (
        <div className="notice notice--bad" role="alert" data-testid="save-error">
          <div>
            <h3>The configuration was not saved</h3>
            <p>{saveError}</p>
          </div>
        </div>
      )}
    </>
  );
}

/**
 * The project's field connections: what is configured, and, for a project the controller is
 * running, how each one is doing.
 */
export function DevicesAndNetworks() {
  const phase = useIoConfig((state) => state.phase);
  const loadError = useIoConfig((state) => state.loadError);
  const draft = useIoConfig((state) => state.draft);
  const dirty = useIoConfig(isDirty);
  const scope = useRuntimeScope();
  const io = useProjectIo();
  const selection = useWorkbench((state) => state.selection);
  const select = useWorkbench((state) => state.select);
  const openEditor = useWorkbench((state) => state.openEditor);

  if (phase === 'idle' || phase === 'loading') {
    return <p className="editor__loading muted">Reading the project's I/O configuration…</p>;
  }
  if (phase === 'unreadable') {
    return (
      <div className="notice notice--bad" role="alert" data-testid="config-unreadable">
        <div>
          <h3>The I/O configuration could not be read</h3>
          <p>{loadError}</p>
        </div>
      </div>
    );
  }

  const runtime = new Map<string, ConnectionStatus>(
    (io?.connections ?? []).map((connection) => [connection.id, connection])
  );

  return (
    <div className="editor__column">
      <ConfigBar />

      {scope.kind === 'not-active' && (
        <div className="notice notice--warn" data-testid="config-not-active">
          <div>
            <h3>This project is not running on the controller</h3>
            <p>
              Its configuration can be edited and saved. It takes effect when the project is made
              the active one.
            </p>
          </div>
        </div>
      )}

      {draft.length === 0 ? (
        <div className="notice" data-testid="no-connections">
          <div>
            <h3>No field connections are configured</h3>
            <p>Add a connection to have the controller read a device.</p>
          </div>
        </div>
      ) : (
        <div className="editor__scroll">
          <table className="grid" aria-label="Field connections">
            <thead>
              <tr>
                <th>Connection</th>
                <th>Driver</th>
                <th>Target</th>
                <th className="num">Tags</th>
                <th className="num">Poll interval</th>
                <th>On the controller</th>
                <th>Detail</th>
              </tr>
            </thead>
            <tbody>
              {draft.map((connection) => {
                const status = runtime.get(connection.id.trim());
                return (
                  <tr
                    key={connection.key}
                    data-testid="connection-row"
                    aria-selected={
                      selection?.kind === 'connection' && selection.id === connection.id.trim()
                    }
                    onClick={() =>
                      select(status ? { kind: 'connection', id: status.id } : null)
                    }
                    onDoubleClick={() => openEditor('connection', connection.key)}
                  >
                    <td>{connectionLabel(connection.id)}</td>
                    <td>{connection.driver}</td>
                    <td className="mono">{target(connection)}</td>
                    <td className="num mono">
                      {connection.modbus ? connection.modbus.mappings.length : '—'}
                    </td>
                    <td className="num mono">
                      {connection.pollIntervalMs.trim() === '' ? '—' : `${connection.pollIntervalMs} ms`}
                    </td>
                    <td>
                      {status ? (
                        <ConnectionStateBadge connection={status} />
                      ) : scope.kind === 'active' ? (
                        <span className="badge badge--neutral">NOT LOADED</span>
                      ) : (
                        <span className="muted">—</span>
                      )}
                    </td>
                    <td
                      className="grow"
                      title={status?.detail ?? status?.last_cycle_error ?? ''}
                    >
                      {status?.detail ?? status?.last_cycle_error ?? ''}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {dirty && scope.kind === 'active' && (
            <p className="editor__footnote muted">
              The state shown is that of the saved configuration. Unsaved changes are not on the
              controller.
            </p>
          )}
          {io?.source && (
            <p className="editor__footnote muted">
              Stored at <code className="mono">{io.source}</code>
            </p>
          )}
        </div>
      )}
    </div>
  );
}
