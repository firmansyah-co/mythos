import type { ConnectionStatus } from '../api/client';

/** A connection's state in words: running with every tag read, running with faults, or not running. */
export function ConnectionStateBadge({ connection }: { connection: ConnectionStatus }) {
  if (connection.state === 'CONFIG_ERROR') {
    return <span className="badge badge--bad">NOT STARTED</span>;
  }
  if (connection.state === 'STOPPED') {
    return <span className="badge badge--neutral">STOPPED</span>;
  }
  if (connection.tags_good === null) {
    return <span className="badge badge--neutral">NO CYCLE YET</span>;
  }
  if (connection.tags_good === connection.tags.length && !connection.last_cycle_error) {
    return <span className="badge badge--good">RUNNING</span>;
  }
  return <span className="badge badge--warn">RUNNING, FAULTS</span>;
}
