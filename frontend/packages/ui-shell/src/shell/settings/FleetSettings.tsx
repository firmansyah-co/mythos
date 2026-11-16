import { useCallback, useEffect, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { api, type FleetMember } from '../../api/client';
import { describeError } from './format';

/** How a node is reached, in words. */
const VIA: Record<FleetMember['via'], string> = {
  direct: 'directly',
  call_home: 'it calls home',
  none: 'no way: it has no address and does not call home',
};

const short = (hash: string | null | undefined) => (hash ? `${hash.slice(0, 12)}…` : 'unknown');

/**
 * The fleet (ADR-012 §2.6): every node paired with this one, asked now, how it is reached, and
 * what it runs. A node that does not answer is shown as not answering, with why; nothing it said
 * before is shown as if it said it now.
 */
export function FleetSettings() {
  const [nodes, setNodes] = useState<FleetMember[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);

  const load = useCallback(async () => {
    setAsking(true);
    try {
      setNodes((await api.getFleet()).nodes);
      setError(null);
    } catch (cause) {
      setNodes(null);
      setError(describeError(cause));
    } finally {
      setAsking(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <>
      <div className="settings__heading">
        <h2>Fleet</h2>
        <button type="button" className="btn btn--flat" onClick={() => void load()} disabled={asking}>
          <RefreshCw size={13} aria-hidden /> Ask again
        </button>
      </div>
      <p className="muted">
        Every node paired with this one, asked now: how it is reached, who it says it is and what it runs. A node that
        cannot reach this one is reached when it calls home (Settings → Pairing, on that node).
      </p>
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
      {nodes === null ? (
        !error && <p className="muted">Asking every paired node…</p>
      ) : nodes.length === 0 ? (
        <p className="muted" data-testid="fleet-empty">
          This node is paired with no other.
        </p>
      ) : (
        <table className="grid settings__block">
          <thead>
            <tr>
              <th>Node</th>
              <th>Reached</th>
              <th>Answer</th>
              <th>Runs</th>
            </tr>
          </thead>
          <tbody>
            {nodes.map((member) => (
              <tr key={member.paired.id} data-testid="fleet-node">
                <td>
                  <strong>{member.paired.name}</strong> <span className="muted">{member.paired.role}</span>
                  <div className="mono muted">{member.paired.id}</div>
                </td>
                <td data-testid="fleet-via">{VIA[member.via]}</td>
                <td>
                  {member.reachable && member.node ? (
                    <span className="badge badge--good" data-testid="fleet-answer">
                      {member.node.role} · {member.node.version}
                    </span>
                  ) : (
                    <span className="badge badge--bad" data-testid="fleet-answer" title={member.error}>
                      NOT ANSWERING
                    </span>
                  )}
                  {!member.reachable && member.error && <div className="muted">{member.error}</div>}
                </td>
                <td data-testid="fleet-running">
                  {!member.reachable ? (
                    <span className="muted">unknown</span>
                  ) : member.running ? (
                    <>
                      <span className="mono">{member.running.project}</span>
                      <div className="mono muted" title={member.running.hash ?? undefined}>
                        {short(member.running.hash)}
                      </div>
                    </>
                  ) : (
                    <span className="muted">no project</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}
