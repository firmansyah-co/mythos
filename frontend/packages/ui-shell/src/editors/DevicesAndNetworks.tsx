import type { IoStatus } from '../api/client';
import { formatClock } from '../data/quality';
import { useWorkbench } from '../data/workbench';
import { RuntimeGate } from '../shell/RuntimeGate';
import { ConnectionStateBadge } from './ConnectionStateBadge';

/** The field connections the controller runs for the open project. */
export function DevicesAndNetworks() {
  return <RuntimeGate>{(io) => <Connections io={io} />}</RuntimeGate>;
}

function Connections({ io }: { io: IoStatus }) {
  const selection = useWorkbench((state) => state.selection);
  const select = useWorkbench((state) => state.select);
  const openEditor = useWorkbench((state) => state.openEditor);

  if (io.connections.length === 0) {
    return (
      <div className="notice" data-testid="no-connections">
        <div>
          <h3>No field connections are running</h3>
          <p>{io.note ?? 'The controller gave no reason.'}</p>
          <p className="muted">
            Connections are the project's I/O configuration (<code>io.json</code>). The workbench
            has no editor for it yet; it can be saved with{' '}
            <code>PUT /api/v1/projects/&lt;id&gt;/io</code> and is applied at once.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="editor__scroll">
      <table className="grid" aria-label="Field connections">
        <thead>
          <tr>
            <th>Connection</th>
            <th>Driver</th>
            <th>State</th>
            <th className="num">Tags read</th>
            <th className="num">Poll interval</th>
            <th className="num">Cycles</th>
            <th>Last cycle</th>
            <th>Detail</th>
          </tr>
        </thead>
        <tbody>
          {io.connections.map((connection) => (
            <tr
              key={connection.id}
              aria-selected={selection?.kind === 'connection' && selection.id === connection.id}
              onClick={() => select({ kind: 'connection', id: connection.id })}
              onDoubleClick={() => openEditor('tags', connection.id)}
            >
              <td>{connection.id}</td>
              <td>{connection.driver}</td>
              <td>
                <ConnectionStateBadge connection={connection} />
              </td>
              <td className="num mono">
                {connection.tags_good === null ? '—' : connection.tags_good} / {connection.tags.length}
              </td>
              <td className="num mono">{connection.poll_interval_ms} ms</td>
              <td className="num mono">{connection.cycles_completed}</td>
              <td className="mono">
                {connection.last_cycle_unix_ms === null
                  ? '—'
                  : formatClock(connection.last_cycle_unix_ms)}
              </td>
              <td className="grow" title={connection.detail ?? connection.last_cycle_error ?? ''}>
                {connection.detail ?? connection.last_cycle_error ?? ''}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {io.source && (
        <p className="editor__footnote muted">
          Loaded from <code className="mono">{io.source}</code>
        </p>
      )}
    </div>
  );
}
