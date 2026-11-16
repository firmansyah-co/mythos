import { useCallback, useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { Trash2, UserPlus } from 'lucide-react';
import { api, ROLES, type Role, type UserAccount } from '../../api/client';
import { useSession } from '../../data/session';
import { describeError, formatTime } from './format';

const MIN_PASSWORD_LENGTH = 10;

/** What each role may do, as the controller enforces it. */
export const ROLE_MEANING: Record<Role, string> = {
  viewer: 'Reads everything: projects, tags, live values, I/O status.',
  operator:
    'What a viewer may. Reserved for operating the plant (writing setpoints), which does not exist yet.',
  engineer: 'Changes projects: I/O configuration, screens, data types, faceplates, modules, activation.',
  admin: 'Manages accounts, and everything an engineer may do.',
};

function NewUser({ onCreated }: { onCreated: () => void }) {
  const [username, setUsername] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [role, setRole] = useState<Role>('viewer');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const problem =
    password.length > 0 && password.length < MIN_PASSWORD_LENGTH
      ? `The password must be at least ${MIN_PASSWORD_LENGTH} characters.`
      : null;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await api.createUser({ username, display_name: displayName, role, password });
      setUsername('');
      setDisplayName('');
      setPassword('');
      setRole('viewer');
      onCreated();
    } catch (cause) {
      setError(describeError(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="form settings__block" onSubmit={submit} aria-label="New account">
      <h3>New account</h3>
      <label className="form__row">
        <span>Username</span>
        <input value={username} onChange={(e) => setUsername(e.target.value.toLowerCase())} />
      </label>
      <label className="form__row">
        <span>Display name</span>
        <input value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
      </label>
      <label className="form__row">
        <span>Role</span>
        <select value={role} onChange={(e) => setRole(e.target.value as Role)} aria-label="Role">
          {ROLES.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
        <span className="field__hint">{ROLE_MEANING[role]}</span>
      </label>
      <label className="form__row">
        <span>Initial password</span>
        <input type="password" autoComplete="new-password" value={password} onChange={(e) => setPassword(e.target.value)} />
      </label>
      {(problem || error) && (
        <p className="form__error" role="alert">
          {problem ?? error}
        </p>
      )}
      <div className="portal__actions">
        <button type="submit" className="btn btn--primary" disabled={busy || !username || !password || Boolean(problem)}>
          <UserPlus size={13} aria-hidden /> Create account
        </button>
      </div>
    </form>
  );
}

function UserRow({ user, me, onChanged, onError }: {
  user: UserAccount;
  me: string;
  onChanged: () => void;
  onError: (message: string) => void;
}) {
  const change = async (update: Parameters<typeof api.updateUser>[1]) => {
    try {
      await api.updateUser(user.username, update);
      onChanged();
    } catch (cause) {
      onError(`${user.username}: ${describeError(cause)}`);
    }
  };
  const resetPassword = async () => {
    const password = window.prompt(
      `New password for ${user.username} (at least ${MIN_PASSWORD_LENGTH} characters). Their sessions end.`
    );
    if (password) await change({ password });
  };
  const remove = async () => {
    if (!window.confirm(`Delete the account ${user.username} and its access tokens?`)) return;
    try {
      await api.deleteUser(user.username);
      onChanged();
    } catch (cause) {
      onError(`${user.username}: ${describeError(cause)}`);
    }
  };

  return (
    <tr data-testid="user-row">
      <td className="mono">
        {user.username}
        {user.username === me ? ' (you)' : ''}
      </td>
      <td>{user.display_name}</td>
      <td>
        <select
          value={user.role}
          aria-label={`Role of ${user.username}`}
          onChange={(e) => void change({ role: e.target.value as Role })}
        >
          {ROLES.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
      </td>
      <td>
        <label className="settings__check">
          <input
            type="checkbox"
            checked={user.disabled}
            aria-label={`${user.username} disabled`}
            onChange={(e) => void change({ disabled: e.target.checked })}
          />
          disabled
        </label>
      </td>
      <td>{formatTime(user.created_unix_s)}</td>
      <td className="settings__row-actions">
        <button type="button" className="btn btn--flat" onClick={() => void resetPassword()}>
          Set password
        </button>
        <button type="button" className="icon-btn" aria-label={`Delete ${user.username}`} onClick={() => void remove()}>
          <Trash2 size={13} />
        </button>
      </td>
    </tr>
  );
}

/** Every account of the controller (administrators). */
export function UsersSettings() {
  const me = useSession((state) => state.user?.username ?? '');
  const [users, setUsers] = useState<UserAccount[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setUsers((await api.listUsers()).users);
    } catch (cause) {
      setError(describeError(cause));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <>
      <h2>Users</h2>
      <dl className="settings__facts">
        {ROLES.map((role) => (
          <div key={role} className="settings__fact">
            <dt>{role}</dt>
            <dd>{ROLE_MEANING[role]}</dd>
          </div>
        ))}
      </dl>
      {error && (
        <p className="form__error" role="alert">
          {error}
        </p>
      )}
      {users === null ? (
        <p className="muted">Loading…</p>
      ) : (
        <table className="grid settings__users" aria-label="Accounts">
          <thead>
            <tr>
              <th>Username</th>
              <th>Display name</th>
              <th>Role</th>
              <th>State</th>
              <th>Created</th>
              <th aria-label="Actions" />
            </tr>
          </thead>
          <tbody>
            {users.map((user) => (
              <UserRow
                key={user.username}
                user={user}
                me={me}
                onChanged={() => {
                  setError(null);
                  void load();
                  void useSession.getState().refresh();
                }}
                onError={setError}
              />
            ))}
          </tbody>
        </table>
      )}
      <NewUser onCreated={() => void load()} />
    </>
  );
}
