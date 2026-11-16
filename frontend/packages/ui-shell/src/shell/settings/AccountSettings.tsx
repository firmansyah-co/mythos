import { useCallback, useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { Copy, KeyRound, Trash2 } from 'lucide-react';
import { api, type AccessToken } from '../../api/client';
import { useSession } from '../../data/session';
import { formatTime, describeError } from './format';

const MIN_PASSWORD_LENGTH = 10;

function ChangePassword() {
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [repeat, setRepeat] = useState('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);

  const problem =
    next.length > 0 && next.length < MIN_PASSWORD_LENGTH
      ? `The new password must be at least ${MIN_PASSWORD_LENGTH} characters.`
      : repeat.length > 0 && repeat !== next
      ? 'The two new passwords differ.'
      : null;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setResult(null);
    try {
      await api.changePassword(current, next);
      setResult({ ok: true, text: 'Password changed. Your other sessions have been signed out.' });
      setCurrent('');
      setNext('');
      setRepeat('');
    } catch (cause) {
      setResult({ ok: false, text: describeError(cause) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="form settings__block" onSubmit={submit} aria-label="Change password">
      <h3>Change password</h3>
      <label className="form__row">
        <span>Current password</span>
        <input type="password" autoComplete="current-password" value={current} onChange={(e) => setCurrent(e.target.value)} />
      </label>
      <label className="form__row">
        <span>New password</span>
        <input type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} />
      </label>
      <label className="form__row">
        <span>Repeat new password</span>
        <input type="password" autoComplete="new-password" value={repeat} onChange={(e) => setRepeat(e.target.value)} />
      </label>
      {(problem || (result && !result.ok)) && (
        <p className="form__error" role="alert">
          {problem ?? result?.text}
        </p>
      )}
      {result?.ok && <p className="notice">{result.text}</p>}
      <div className="portal__actions">
        <button
          type="submit"
          className="btn btn--primary"
          disabled={busy || !current || !next || next !== repeat || Boolean(problem)}
        >
          Change password
        </button>
      </div>
    </form>
  );
}

function AccessTokens() {
  const [tokens, setTokens] = useState<AccessToken[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState('');
  const [created, setCreated] = useState<{ name: string; secret: string } | null>(null);

  const load = useCallback(async () => {
    try {
      setTokens(await api.listTokens());
      setError(null);
    } catch (cause) {
      setError(describeError(cause));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const create = async (event: FormEvent) => {
    event.preventDefault();
    try {
      const made = await api.createToken(name);
      setCreated({ name: made.token.name, secret: made.secret });
      setName('');
      await load();
    } catch (cause) {
      setError(describeError(cause));
    }
  };

  const revoke = async (token: AccessToken) => {
    try {
      await api.revokeToken(token.id);
      await load();
    } catch (cause) {
      setError(describeError(cause));
    }
  };

  return (
    <section className="form settings__block" aria-label="Access tokens">
      <h3>Access tokens</h3>
      <p className="muted">
        A token lets a program act as you, with your role: an AI agent over MCP
        (<span className="mono">Authorization: Bearer &lt;token&gt;</span>) or a script. Revoke a
        token you no longer use.
      </p>
      {created && (
        <div className="notice" data-testid="new-token">
          <div>
            Token <strong>{created.name}</strong> was created. Copy it now: it is not shown again.
            <div className="settings__secret">
              <code className="mono">{created.secret}</code>
              <button
                type="button"
                className="btn btn--flat"
                onClick={() => void navigator.clipboard?.writeText(created.secret)}
              >
                <Copy size={13} aria-hidden /> Copy
              </button>
            </div>
          </div>
        </div>
      )}
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
      {tokens === null ? (
        <p className="muted">Loading…</p>
      ) : tokens.length === 0 ? (
        <p className="muted">You have no access tokens.</p>
      ) : (
        <table className="grid" aria-label="Your access tokens">
          <thead>
            <tr>
              <th>Name</th>
              <th>Created</th>
              <th>Last used</th>
              <th aria-label="Revoke" />
            </tr>
          </thead>
          <tbody>
            {tokens.map((token) => (
              <tr key={token.id}>
                <td>{token.name}</td>
                <td>{formatTime(token.created_unix_s)}</td>
                <td>{token.last_used_unix_s ? formatTime(token.last_used_unix_s) : 'never'}</td>
                <td>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={`Revoke ${token.name}`}
                    onClick={() => void revoke(token)}
                  >
                    <Trash2 size={13} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <form className="settings__inline" onSubmit={create}>
        <input
          aria-label="New token name"
          placeholder="What is it for? (e.g. Claude Code)"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button type="submit" className="btn" disabled={!name.trim()}>
          <KeyRound size={13} aria-hidden /> Create token
        </button>
      </form>
    </section>
  );
}

/** The signed-in account: who it is, its password, its access tokens. */
export function AccountSettings() {
  const user = useSession((state) => state.user);
  if (!user) return null;
  return (
    <>
      <h2>Account</h2>
      <dl className="settings__facts">
        <dt>Username</dt>
        <dd className="mono">{user.username}</dd>
        <dt>Display name</dt>
        <dd>{user.display_name}</dd>
        <dt>Role</dt>
        <dd data-testid="my-role">{user.role}</dd>
      </dl>
      <ChangePassword />
      <AccessTokens />
    </>
  );
}
