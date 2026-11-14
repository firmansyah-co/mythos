import type { IoStatus } from '../api/client';
import { connectionById } from '../data/derive';
import { formatClock } from '../data/quality';
import { useWorkbench } from '../data/workbench';
import { RuntimeGate } from '../shell/RuntimeGate';
import { ConnectionStateBadge } from './ConnectionStateBadge';

/** One connection's state, and every tag it could not read in its last cycle. */
export function ConnectionDiagnostics({ connectionId }: { connectionId: string }) {
  return <RuntimeGate>{(io) => <Detail io={io} connectionId={connectionId} />}</RuntimeGate>;
}

function Detail({ io, connectionId }: { io: IoStatus; connectionId: string }) {
  const select = useWorkbench((state) => state.select);

  const connection = connectionById(io, connectionId);
  if (!connection) {
    return (
      <div className="notice notice--warn">
        <div>
          <h3>Connection “{connectionId}” is not known to the controller</h3>
          <p>It is not in the I/O configuration the controller is running.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="editor__scroll">
      <dl className="props props--wide">
        <dt>State</dt>
        <dd>
          <ConnectionStateBadge connection={connection} />
        </dd>
        {connection.detail && (
          <>
            <dt>Detail</dt>
            <dd>{connection.detail}</dd>
          </>
        )}
        <dt>Driver</dt>
        <dd>{connection.driver}</dd>
        <dt>Poll interval</dt>
        <dd className="mono">{connection.poll_interval_ms} ms</dd>
        <dt>Cycle deadline</dt>
        <dd className="mono">{connection.cycle_timeout_ms} ms</dd>
        <dt>Cycles completed</dt>
        <dd className="mono">{connection.cycles_completed}</dd>
        <dt>Last cycle finished</dt>
        <dd className="mono">
          {connection.last_cycle_unix_ms === null ? '—' : formatClock(connection.last_cycle_unix_ms)}
        </dd>
        <dt>Tags read in last cycle</dt>
        <dd className="mono">
          {connection.tags_good === null ? '—' : connection.tags_good} of {connection.tags.length}
        </dd>
        {connection.last_cycle_error && (
          <>
            <dt>Last cycle error</dt>
            <dd>{connection.last_cycle_error}</dd>
          </>
        )}
      </dl>

      <h3 className="editor__section">Tags not read in the last cycle</h3>
      {connection.faults.length === 0 ? (
        <p className="editor__empty muted" data-testid="no-faults">
          {connection.tags_good === null
            ? 'No poll cycle has finished yet.'
            : 'Every tag was read.'}
        </p>
      ) : (
        <table className="grid" aria-label="Tags not read">
          <thead>
            <tr>
              <th>Tag</th>
              <th>Reason</th>
            </tr>
          </thead>
          <tbody>
            {connection.faults.map((fault) => (
              <tr key={fault.uri} onClick={() => select({ kind: 'tag', uri: fault.uri })}>
                <td className="mono">{fault.uri}</td>
                <td className="grow" title={fault.reason}>
                  {fault.reason}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
