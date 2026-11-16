import { useCallback, useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { api, hasRole, ROLES, type PairedNode, type Role } from '../../api/client';
import { useSession } from '../../data/session';
import { describeError, formatTime } from './format';

const spoken = (fingerprint: string) => fingerprint.match(/.{1,4}/g)?.join(' ') ?? fingerprint;

/** Make a one-use code another node pairs with this one by. */
function MakeCode() {
  const [role, setRole] = useState<Role>('engineer');
  const [made, setMade] = useState<{ code: string; expires_unix_s: number; granted: Role } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const make = async () => {
    setError(null);
    try {
      setMade(await api.makePairingCode(role));
    } catch (cause) {
      setError(describeError(cause));
    }
  };
  return (
    <div className="settings__block" data-testid="pairing-make-code">
      <h3>Let another node pair with this one</h3>
      <p className="muted">
        A code is used once and lasts ten minutes. Read it, with this node’s certificate fingerprint (Settings → Node),
        to whoever pairs. The node that uses it may act here as:
      </p>
      <div className="settings__inline">
        <select aria-label="Role granted" value={role} onChange={(e) => setRole(e.target.value as Role)}>
          {ROLES.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
        <button type="button" className="btn" onClick={() => void make()}>
          Make a pairing code
        </button>
      </div>
      {made && (
        <p className="settings__secret" data-testid="pairing-code">
          <code>{made.code}</code> — for a node acting here as {made.granted}, until {formatTime(made.expires_unix_s)}
        </p>
      )}
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

/** Pair with another node, at an address, presenting the certificate given. */
function PairWith({ onPaired }: { onPaired: () => void }) {
  const [address, setAddress] = useState('');
  const [fingerprint, setFingerprint] = useState('');
  const [code, setCode] = useState('');
  const [grant, setGrant] = useState<Role>('viewer');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setResult(null);
    try {
      const paired = await api.pairWith(address.trim(), fingerprint, code, grant);
      setResult({
        ok: true,
        text: `Paired with ${paired.peer.name}. It may act here as ${paired.peer.granted}; this node there as ${paired.granted_there}.`,
      });
      setCode('');
      onPaired();
    } catch (cause) {
      setResult({ ok: false, text: describeError(cause) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <form className="form settings__block" onSubmit={submit} aria-label="Pair with a node">
      <h3>Pair with another node</h3>
      <p className="muted">
        Compare the fingerprint with the one that node shows in its Settings → Node: a node that presents another
        certificate is refused before the code is sent.
      </p>
      <label className="form__row">
        <span>Address</span>
        <input aria-label="Address" placeholder="192.168.10.21:8443" value={address} onChange={(e) => setAddress(e.target.value)} />
      </label>
      <label className="form__row">
        <span>Fingerprint</span>
        <input aria-label="Fingerprint" className="mono" value={fingerprint} onChange={(e) => setFingerprint(e.target.value)} />
      </label>
      <label className="form__row">
        <span>Pairing code</span>
        <input aria-label="Pairing code" value={code} onChange={(e) => setCode(e.target.value)} />
      </label>
      <label className="form__row">
        <span>It may act here as</span>
        <select aria-label="Grant" value={grant} onChange={(e) => setGrant(e.target.value as Role)}>
          {ROLES.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
      </label>
      {result && (
        <p className={result.ok ? 'notice' : 'form__error'} role={result.ok ? 'status' : 'alert'} data-testid="pair-result">
          {result.text}
        </p>
      )}
      <div className="portal__actions">
        <button type="submit" className="btn btn--primary" disabled={busy || !address.trim() || !fingerprint.trim() || !code.trim()}>
          Pair
        </button>
      </div>
    </form>
  );
}

/** The nodes this one is paired with (Settings → Pairing, ADR-012 §2.3). */
export function PairingSettings() {
  const user = useSession((state) => state.user);
  const mayChange = !user || hasRole(user.role, 'admin');
  const [peers, setPeers] = useState<PairedNode[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reach, setReach] = useState<Record<string, string>>({});

  const load = useCallback(async () => {
    try {
      setPeers((await api.listPairings()).peers);
      setError(null);
    } catch (cause) {
      setError(describeError(cause));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const check = async (peer: PairedNode) => {
    setReach((r) => ({ ...r, [peer.id]: 'asking…' }));
    try {
      const status = await api.pairedNodeStatus(peer.id);
      setReach((r) => ({
        ...r,
        [peer.id]: status.reachable ? `reachable: ${status.node?.name} (${status.node?.role})` : `not reachable: ${status.error}`,
      }));
    } catch (cause) {
      setReach((r) => ({ ...r, [peer.id]: describeError(cause) }));
    }
  };

  const unpair = async (peer: PairedNode) => {
    if (!window.confirm(`Unpair ${peer.name}? Its token stops working here at once.`)) return;
    try {
      const gone = await api.unpair(peer.id);
      setReach((r) => ({ ...r, [peer.id]: gone.told ? '' : `unpaired here; it was not told: ${gone.told_error}` }));
    } catch (cause) {
      setError(describeError(cause));
    }
    await load();
  };

  return (
    <>
      <div className="settings__heading">
        <h2>Pairing</h2>
      </div>
      <p className="muted">
        Paired nodes trust each other by pinned certificates and act on each other with the role each granted. A node
        heard on the network is not paired: pairing is always done here, by an administrator.
      </p>
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
      {peers === null ? (
        !error && <p className="muted">Loading…</p>
      ) : peers.length === 0 ? (
        <p className="muted" data-testid="pairings-empty">
          This node is paired with no other.
        </p>
      ) : (
        <table className="grid settings__block">
          <thead>
            <tr>
              <th>Node</th>
              <th>Address</th>
              <th>Certificate</th>
              <th>May act here as</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {peers.map((peer) => (
              <tr key={peer.id} data-testid="paired-node">
                <td>
                  <strong>{peer.name}</strong> <span className="muted">{peer.role}</span>
                  <div className="mono muted">{peer.id}</div>
                  {reach[peer.id] && <div data-testid="paired-node-reach">{reach[peer.id]}</div>}
                </td>
                <td className="mono">{peer.address ?? 'cannot be reached'}</td>
                <td className="mono" title={peer.fingerprint}>
                  {spoken(peer.fingerprint).slice(0, 19)}…
                </td>
                <td>{peer.granted}</td>
                <td className="settings__row-actions">
                  <button type="button" className="btn btn--flat" onClick={() => void check(peer)}>
                    Reach
                  </button>
                  {mayChange && (
                    <button type="button" className="btn btn--flat" onClick={() => void unpair(peer)}>
                      Unpair
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {mayChange ? (
        <>
          <MakeCode />
          <PairWith onPaired={() => void load()} />
        </>
      ) : (
        <p className="muted">Only an administrator pairs and unpairs nodes.</p>
      )}
    </>
  );
}
