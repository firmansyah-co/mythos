// Who is using the workbench.
//
// The controller has accounts with roles (viewer < operator < engineer < admin). A browser signs
// in and gets a session cookie that the browser sends with every request, the telemetry stream
// included. The workbench shows nothing of the controller until the controller says who the
// user is; when a request is answered 401 the session has ended and the sign-in page returns.

import { create } from 'zustand';
import { api, ApiError, whenUnauthorized, type Identity } from '../api/client';

export type SessionPhase =
  /** Asking the controller. */
  | 'loading'
  /** The controller could not be asked; `error` says why. */
  | 'unreachable'
  /** No account exists yet: the first administrator is to be created with the setup code. */
  | 'setup'
  | 'signed-out'
  | 'signed-in'
  /** The server has no accounts at all (a development build without them). */
  | 'open';

interface SessionState {
  phase: SessionPhase;
  user: Identity | null;
  error: string | null;
  /** Why the user was signed out, when it was not their own doing. */
  notice: string | null;
  refresh: () => Promise<void>;
  signIn: (username: string, password: string) => Promise<void>;
  setUp: (code: string, username: string, displayName: string, password: string) => Promise<void>;
  signOut: () => Promise<void>;
  /** The controller answered 401: the session is over. */
  expired: () => void;
}

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

export const useSession = create<SessionState>((set, get) => ({
  phase: 'loading',
  user: null,
  error: null,
  notice: null,

  refresh: async () => {
    try {
      const status = await api.authStatus();
      if (!status.auth) set({ phase: 'open', user: null, error: null });
      else if (status.setup_required) set({ phase: 'setup', user: null, error: null });
      else if (status.user) set({ phase: 'signed-in', user: status.user, error: null });
      else set({ phase: 'signed-out', user: null, error: null });
    } catch (cause) {
      set({ phase: 'unreachable', user: null, error: message(cause) });
    }
  },

  signIn: async (username, password) => {
    const user = await api.signIn(username, password);
    set({ phase: 'signed-in', user, error: null, notice: null });
  },

  setUp: async (code, username, displayName, password) => {
    const user = await api.setUp(code, username, displayName, password);
    set({ phase: 'signed-in', user, error: null, notice: null });
  },

  signOut: async () => {
    try {
      await api.signOut();
    } catch (cause) {
      // Signing out locally still happens; the cookie expires on its own.
      if (!(cause instanceof ApiError)) throw cause;
    }
    set({ phase: 'signed-out', user: null, notice: null });
  },

  expired: () => {
    if (get().phase !== 'signed-in') return;
    set({
      phase: 'signed-out',
      user: null,
      notice: 'Your session has ended. Sign in again to continue.',
    });
  },
}));

whenUnauthorized(() => useSession.getState().expired());
