// The open project's I/O configuration: what is saved on the controller, and the engineer's
// draft of it.

import { create } from 'zustand';
import { ApiError, ConfigRejected, api, type ConfigProblem, type IoConfig } from '../api/client';
import { useController } from './controller';
import { fromForm, newConnection, toForm, type ConnectionForm, type FieldError } from './ioForm';
import { log } from './log';

export type IoConfigPhase = 'idle' | 'loading' | 'ready' | 'saving' | 'unreadable';

export interface IoConfigState {
  projectId: string | null;
  phase: IoConfigPhase;
  /** Why the configuration could not be read, when it could not. */
  loadError: string | null;
  /** What the controller has stored. `null` when the project has no configuration. */
  saved: IoConfig | null;
  /** The draft being edited. */
  draft: ConnectionForm[];
  /** The draft as it was when last loaded or saved, to tell whether it has changed. */
  baseline: string;
  drivers: string[];
  /**
   * Errors in the draft's fields. Empty until a save has been refused for them; from then on
   * they are recomputed on every change, so a message never outlives the mistake it names.
   */
  fieldErrors: FieldError[];
  /** Whether a save has been refused for field errors, which turns on checking as you type. */
  checking: boolean;
  /** Problems the server named when it refused the draft. */
  problems: ConfigProblem[];
  /** Why the last save failed for a reason other than the configuration itself. */
  saveError: string | null;

  load: (projectId: string) => Promise<void>;
  reset: () => void;
  addConnection: () => string;
  removeConnection: (key: string) => void;
  updateConnection: (key: string, change: (connection: ConnectionForm) => ConnectionForm) => void;
  discard: () => void;
  save: () => Promise<boolean>;
}

/** The parts of a draft that are stored, without the editor's row keys. */
function fingerprint(draft: ConnectionForm[]): string {
  return JSON.stringify(draft, (key, value: unknown) => (key === 'key' ? undefined : value));
}

export function isDirty(state: Pick<IoConfigState, 'draft' | 'baseline'>): boolean {
  return fingerprint(state.draft) !== state.baseline;
}

const EMPTY = {
  projectId: null,
  phase: 'idle' as IoConfigPhase,
  loadError: null,
  saved: null,
  draft: [] as ConnectionForm[],
  baseline: fingerprint([]),
  fieldErrors: [] as FieldError[],
  checking: false,
  problems: [] as ConfigProblem[],
  saveError: null,
};

/**
 * What a change to the draft does to the messages on screen.
 *
 * Field errors follow the draft once checking is on. The controller's problems and a failed
 * save describe the configuration that was sent, which the draft no longer is, so they go.
 */
function afterEdit(draft: ConnectionForm[], checking: boolean) {
  const result = checking ? fromForm(draft) : null;
  return {
    draft,
    fieldErrors: result && !result.ok ? result.errors : [],
    problems: [] as ConfigProblem[],
    saveError: null,
  };
}

export const useIoConfig = create<IoConfigState>((set, get) => ({
  ...EMPTY,
  drivers: [],

  load: async (projectId) => {
    set({ ...EMPTY, projectId, phase: 'loading' });
    try {
      const [saved, drivers] = await Promise.all([api.getIoConfig(projectId), api.listDrivers()]);
      if (get().projectId !== projectId) return;
      const draft = toForm(saved);
      set({ phase: 'ready', saved, draft, baseline: fingerprint(draft), drivers });
    } catch (cause) {
      if (get().projectId !== projectId) return;
      set({
        phase: 'unreadable',
        loadError: cause instanceof Error ? cause.message : String(cause),
      });
    }
  },

  reset: () => set({ ...EMPTY }),

  addConnection: () => {
    const connection = newConnection();
    set((state) => afterEdit([...state.draft, connection], state.checking));
    return connection.key;
  },

  removeConnection: (key) =>
    set((state) =>
      afterEdit(
        state.draft.filter((connection) => connection.key !== key),
        state.checking
      )
    ),

  updateConnection: (key, change) =>
    set((state) =>
      afterEdit(
        state.draft.map((connection) => (connection.key === key ? change(connection) : connection)),
        state.checking
      )
    ),

  discard: () =>
    set((state) => {
      const draft = toForm(state.saved);
      return {
        draft,
        baseline: fingerprint(draft),
        fieldErrors: [],
        checking: false,
        problems: [],
        saveError: null,
      };
    }),

  save: async () => {
    const { projectId, draft } = get();
    if (projectId === null) return false;

    const result = fromForm(draft);
    if (!result.ok) {
      set({ fieldErrors: result.errors, checking: true, problems: [], saveError: null });
      return false;
    }

    set({ phase: 'saving', fieldErrors: [], checking: false, problems: [], saveError: null });
    try {
      const { applied } = await api.saveIoConfig(projectId, result.config);
      if (get().projectId !== projectId) return false;
      // The draft is kept as typed, so the rows keep their identity; only the baseline moves.
      set({ phase: 'ready', saved: result.config, baseline: fingerprint(draft) });
      log.info(
        applied
          ? 'I/O configuration saved and applied by the controller.'
          : 'I/O configuration saved. It is not applied: this project is not the active one.'
      );
      void useController.getState().refresh();
      return true;
    } catch (cause) {
      if (get().projectId !== projectId) return false;
      if (cause instanceof ConfigRejected) {
        set({ phase: 'ready', problems: cause.problems });
        log.error(`The controller refused the I/O configuration: ${cause.message}`);
      } else {
        const message =
          cause instanceof ApiError || cause instanceof Error ? cause.message : String(cause);
        set({ phase: 'ready', saveError: message });
        log.error(`Saving the I/O configuration failed: ${message}`);
      }
      return false;
    }
  },
}));
