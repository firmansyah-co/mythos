import { useState } from 'react';
import type { FormEvent, ReactNode } from 'react';
import { APP_CHANNEL, APP_VERSION } from '../lib/appVersion';
import { useSession } from '../data/session';

const MIN_PASSWORD_LENGTH = 10;

function Frame({ children }: { children: ReactNode }) {
  return (
    <div className="portal" data-testid="sign-in">
      <aside className="portal__side">
        <div className="portal__brand">
          <img src="/favicon.svg" alt="" />
          <div>
            <div className="portal__product">MythOS Workbench</div>
            <div className="portal__version mono">
              v{APP_VERSION} · channel {APP_CHANNEL}
            </div>
          </div>
        </div>
      </aside>
      <main className="portal__main">
        <section className="portal__panel">{children}</section>
      </main>
    </div>
  );
}

function SignInForm() {
  const signIn = useSession((state) => state.signIn);
  const notice = useSession((state) => state.notice);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await signIn(username, password);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setPassword('');
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <h2>Sign in</h2>
      {notice && <div className="notice">{notice}</div>}
      <form className="form" onSubmit={submit}>
        <label className="form__row">
          <span>Username</span>
          <input
            autoComplete="username"
            autoFocus
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
        </label>
        <label className="form__row">
          <span>Password</span>
          <input
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        {error && (
          <p className="form__error" role="alert">
            {error}
          </p>
        )}
        <div className="portal__actions">
          <button type="submit" className="btn btn--primary" disabled={busy || !username || !password}>
            {busy ? 'Signing in…' : 'Sign in'}
          </button>
        </div>
      </form>
    </>
  );
}

function FirstAdministrator() {
  const setUp = useSession((state) => state.setUp);
  const [code, setCode] = useState('');
  const [username, setUsername] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [password, setPassword] = useState('');
  const [repeat, setRepeat] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const problem =
    password.length > 0 && password.length < MIN_PASSWORD_LENGTH
      ? `The password must be at least ${MIN_PASSWORD_LENGTH} characters.`
      : repeat.length > 0 && repeat !== password
      ? 'The two passwords differ.'
      : null;
  const ready = code.trim() && username.trim() && password && repeat === password && !problem;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      await setUp(code, username, displayName, password);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <h2>Create the first administrator</h2>
      <div className="notice">
        This controller has no account yet. Its setup code is printed in the server's log
        (“No account exists yet … setup code …”). Only someone who can read the controller's
        console can claim it.
      </div>
      <form className="form" onSubmit={submit}>
        <label className="form__row">
          <span>Setup code</span>
          <input className="mono" autoFocus value={code} onChange={(e) => setCode(e.target.value)} />
        </label>
        <label className="form__row">
          <span>Username</span>
          <input
            autoComplete="username"
            value={username}
            onChange={(e) => setUsername(e.target.value.toLowerCase())}
          />
        </label>
        <label className="form__row">
          <span>Display name</span>
          <input value={displayName} onChange={(e) => setDisplayName(e.target.value)} />
        </label>
        <label className="form__row">
          <span>Password</span>
          <input
            type="password"
            autoComplete="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <label className="form__row">
          <span>Repeat password</span>
          <input
            type="password"
            autoComplete="new-password"
            value={repeat}
            onChange={(e) => setRepeat(e.target.value)}
          />
        </label>
        {(problem || error) && (
          <p className="form__error" role="alert">
            {problem ?? error}
          </p>
        )}
        <div className="portal__actions">
          <button type="submit" className="btn btn--primary" disabled={busy || !ready}>
            {busy ? 'Creating…' : 'Create administrator'}
          </button>
        </div>
      </form>
    </>
  );
}

/** What is shown until the controller knows who the user is. */
export function SignIn() {
  const phase = useSession((state) => state.phase);
  const error = useSession((state) => state.error);
  const refresh = useSession((state) => state.refresh);

  if (phase === 'loading') {
    return (
      <Frame>
        <p className="muted">Asking the controller…</p>
      </Frame>
    );
  }
  if (phase === 'unreachable') {
    return (
      <Frame>
        <h2>The controller cannot be reached</h2>
        <p className="form__error" role="alert">
          {error}
        </p>
        <button type="button" className="btn" onClick={() => void refresh()}>
          Try again
        </button>
      </Frame>
    );
  }
  return <Frame>{phase === 'setup' ? <FirstAdministrator /> : <SignInForm />}</Frame>;
}
