// The open project's control programs: what the logic module has stored, the engineer's draft
// of each, what the last check and the last build said, and what the controller's runtimes are
// doing. Nothing here runs a program: loading is saving a built instance into the project's
// runtimes, which the core does.

import { create } from 'zustand';
import {
  api,
  log,
  type ApplyAnswer,
  type BuildResult,
  type CheckResult,
  type Language,
  type ProgramEntry,
  type RuntimeLoad,
  type RuntimeStatus,
  type Variable,
  type WatchAnswer,
} from './bridge';

export interface ProgramDoc {
  /** The settings being edited. */
  entry: ProgramEntry;
  /** The settings as the module has them, serialized, to tell whether they changed. */
  savedEntry: string;
  /** The source being edited; `null` until it has been read. */
  source: string | null;
  /** The source as the module has it. */
  savedSource: string | null;
  /** Why the source could not be read. */
  readError: string | null;
  /** The last check, and the text it was a check of. */
  check: CheckResult | null;
  checkedSource: string | null;
  /** The variables of the last text that compiled, and that text. */
  variables: Variable[] | null;
  variablesSource: string | null;
  /** The last build this workbench made, and whether it was loaded. */
  built: BuildResult | null;
  loaded: ApplyAnswer | null;
  /** Why the runtime did not start what was loaded, when the controller said it did not. */
  startFailure: string | null;
}

export type Busy = 'saving' | 'checking' | 'loading' | 'commanding' | 'forcing' | 'creating' | 'removing' | null;
export type Phase = 'idle' | 'reading' | 'ready' | 'unreadable';

/** The controller's runtimes as last read, or why they could not be. */
export interface RuntimesView {
  /** `null`: never read. */
  runtimes: RuntimeStatus[] | null;
  /** The runtime modules installed (their `provides`); `null`: never read. */
  modules: string[] | null;
  /** Why the last read failed. While set, `runtimes` is what was read before, not what is. */
  error: string | null;
  /** When the last read succeeded (ms). */
  readAtMs: number | null;
}

/** The variables of a running program as last read, or why they could not be. */
export interface WatchView {
  /** `null`: not read, or the last read failed: nothing read before is shown as a value. */
  answer: WatchAnswer | null;
  error: string | null;
}

export interface ProgramsState {
  projectId: string | null;
  phase: Phase;
  loadError: string | null;
  programs: ProgramDoc[];
  busy: Busy;
  /** Why the last request failed. */
  error: string | null;
  runtimes: RuntimesView;
  /** By program name. */
  watches: Record<string, WatchView>;

  load: (projectId: string) => Promise<void>;
  reset: () => void;
  create: (name: string, language?: Language) => Promise<string | null>;
  /** Read the program's source again, unless it has changes here: someone else saved it. */
  reread: (name: string) => Promise<void>;
  remove: (name: string) => Promise<boolean>;
  open: (name: string) => Promise<void>;
  setSource: (name: string, source: string) => void;
  setSettings: (name: string, change: Partial<ProgramEntry>) => void;
  discard: (name: string) => void;
  save: (name: string) => Promise<boolean>;
  check: (name: string) => Promise<void>;
  /**
   * Save, check, build, and put what was built into the project's runtimes. With `cold`, it
   * starts from its initial values, RETAIN ones included, instead of being changed online.
   */
  download: (name: string, cold?: boolean) => Promise<boolean>;
  /** Take the program out of the project's runtimes. */
  unload: (name: string) => Promise<boolean>;
  command: (name: string, run: boolean) => Promise<void>;
  /** Hold a variable of the running program at a value. */
  force: (name: string, variable: string, value: number | boolean) => Promise<boolean>;
  /** Release one forced variable of the running program, or every one with `null`. */
  release: (name: string, variable: string | null) => Promise<boolean>;
  readRuntimes: () => Promise<void>;
  /** Read these variables of the runtime that runs the program. */
  readWatch: (name: string, variables: string[]) => Promise<void>;
  /** Stop showing the program's values: nothing reads them any more. */
  dropWatch: (name: string) => void;
}

/** A value as text with every object's keys in one fixed order (the module sorts them). */
export function canonical(value: unknown): string {
  return JSON.stringify(value, (_key, inner: unknown) =>
    inner && typeof inner === 'object' && !Array.isArray(inner)
      ? Object.fromEntries(
          Object.entries(inner as Record<string, unknown>).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        )
      : inner
  );
}

/** How the controller says the runtime of program `name` started at this load, if it says. */
export function loadOf(answer: ApplyAnswer | null, name: string): RuntimeLoad | null {
  return answer?.changes?.runtimes?.find((load) => load.runtime === name) ?? null;
}

/**
 * How a loaded program started, in one sentence that claims what the controller said and
 * nothing more: changed online only when it says so.
 */
export function describeStart(load: RuntimeLoad | null): string {
  if (load === null) return 'The controller did not say how it started.';
  const notCarried = load.dropped.length + load.initialized.length;
  switch (load.start) {
    case 'online':
      return (
        `Changed online: it took over the running version's variables (${load.carried ?? 'UNKNOWN'} carried over) and scans on without a restart.` +
        (notCarried > 0
          ? ` ${load.dropped.length} of the running version's did not carry over, and ${load.initialized.length} of its own started from their initial values.`
          : '')
      );
    case 'warm':
      return `Not changed online${load.why ? `: ${load.why}` : ''}. Its RETAIN variables start from the values kept for them, every other variable from its initial value.`;
    case 'cold':
      return 'Loaded cold: every variable starts from its initial value, RETAIN ones included.';
    case 'unknown':
      return `How its variables started is not known${load.why ? `: ${load.why}` : ''}.`;
  }
}

export function isDirty(doc: ProgramDoc): boolean {
  return doc.source !== doc.savedSource || canonical(doc.entry) !== doc.savedEntry;
}

export function anyDirty(state: Pick<ProgramsState, 'programs'>): boolean {
  return state.programs.some(isDirty);
}

/** Whether the last check is of the text now in the editor. */
export function checkIsCurrent(doc: ProgramDoc): boolean {
  return doc.check !== null && doc.checkedSource === doc.source;
}

/**
 * How what the controller runs compares with this program, in words that claim no more than
 * is known. The runtime of a program has the program's name as its id.
 */
export type RunState =
  | { kind: 'unknown'; why: string }
  | { kind: 'not-loaded' }
  | { kind: 'loaded'; runtime: RuntimeStatus; stale: boolean };

export function runState(name: string, view: RuntimesView): RunState {
  if (view.runtimes === null) {
    return { kind: 'unknown', why: view.error ?? 'the runtimes have not been read yet' };
  }
  const runtime = view.runtimes.find((r) => r.id === name);
  if (view.error !== null) {
    // What was read before the failure is not what is: it is shown only as the last known.
    return runtime
      ? { kind: 'loaded', runtime, stale: true }
      : { kind: 'unknown', why: view.error };
  }
  return runtime ? { kind: 'loaded', runtime, stale: false } : { kind: 'not-loaded' };
}

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

const docOf = (entry: ProgramEntry): ProgramDoc => ({
  entry,
  savedEntry: canonical(entry),
  source: null,
  savedSource: null,
  readError: null,
  check: null,
  checkedSource: null,
  variables: null,
  variablesSource: null,
  built: null,
  loaded: null,
  startFailure: null,
});

/** `doc` with the answer of a check of `source`. A text that compiled gives the variables. */
const checked = (doc: ProgramDoc, result: CheckResult, source: string): ProgramDoc => ({
  ...doc,
  check: result,
  checkedSource: source,
  ...(result.ok && result.variables
    ? { variables: result.variables, variablesSource: source }
    : {}),
});

const EMPTY_RUNTIMES: RuntimesView = { runtimes: null, modules: null, error: null, readAtMs: null };

const EMPTY = {
  projectId: null,
  phase: 'idle' as Phase,
  loadError: null,
  programs: [] as ProgramDoc[],
  busy: null as Busy,
  error: null,
  runtimes: EMPTY_RUNTIMES,
  watches: {} as Record<string, WatchView>,
};

export const usePrograms = create<ProgramsState>((set, get) => {
  const change = (name: string, edit: (doc: ProgramDoc) => ProgramDoc) =>
    set((state) => ({
      programs: state.programs.map((doc) => (doc.entry.name === name ? edit(doc) : doc)),
    }));
  const find = (name: string) => get().programs.find((doc) => doc.entry.name === name);
  /** Whether the project the request was made for is still the one open. */
  const still = (projectId: string) => get().projectId === projectId;

  const failed = (what: string, cause: unknown) => {
    const text = `${what}: ${message(cause)}`;
    log.error(text);
    set({ busy: null, error: text });
  };

  /** Save what changed. Resolves with whether everything is saved. */
  const saveDoc = async (projectId: string, name: string): Promise<boolean> => {
    const doc = find(name);
    if (!doc || doc.source === null) return false;
    if (doc.source !== doc.savedSource) {
      const source = doc.source;
      await api.saveSource(projectId, name, source);
      change(name, (d) => ({ ...d, savedSource: source }));
    }
    const after = find(name);
    if (after && canonical(after.entry) !== after.savedEntry) {
      const saved = await api.saveSettings(projectId, name, after.entry);
      change(name, (d) => ({ ...d, entry: saved.program, savedEntry: canonical(saved.program) }));
    }
    return true;
  };

  return {
    ...EMPTY,

    load: async (projectId) => {
      set({ ...EMPTY, projectId, phase: 'reading', runtimes: get().runtimes });
      try {
        const listed = await api.list(projectId);
        if (!still(projectId)) return;
        set({ phase: 'ready', programs: listed.programs.map(docOf) });
      } catch (cause) {
        if (!still(projectId)) return;
        // Not an empty list: the programs are there or not, and this workbench does not know.
        set({ phase: 'unreadable', loadError: message(cause) });
        log.error(`The project's programs could not be read: ${message(cause)}`);
      }
    },

    reset: () => set({ ...EMPTY }),

    create: async (name, language = 'st') => {
      const projectId = get().projectId;
      if (!projectId) return null;
      set({ busy: 'creating', error: null });
      try {
        const made = await api.create(projectId, name, language);
        if (!still(projectId)) return null;
        set((state) => ({
          busy: null,
          programs: [
            ...state.programs,
            { ...docOf(made.program), source: made.source, savedSource: made.source },
          ],
        }));
        log.info(`Program ${made.program.name} was added.`);
        return made.program.name;
      } catch (cause) {
        failed(`Program ${name} was not added`, cause);
        return null;
      }
    },

    remove: async (name) => {
      const projectId = get().projectId;
      if (!projectId) return false;
      set({ busy: 'removing', error: null });
      try {
        await api.remove(projectId, name);
        set((state) => ({
          busy: null,
          programs: state.programs.filter((doc) => doc.entry.name !== name),
        }));
        log.info(`Program ${name} was removed from the project. A runtime that runs it was not touched.`);
        return true;
      } catch (cause) {
        failed(`Program ${name} was not removed`, cause);
        return false;
      }
    },

    open: async (name) => {
      const projectId = get().projectId;
      const doc = find(name);
      if (!projectId || !doc || doc.source !== null) return;
      try {
        const read = await api.read(projectId, name);
        if (!still(projectId)) return;
        change(name, (d) =>
          // Edits made while the read was on its way are kept.
          d.source !== null
            ? d
            : { ...d, source: read.source, savedSource: read.source, readError: null }
        );
      } catch (cause) {
        change(name, (d) => ({ ...d, readError: message(cause) }));
      }
    },

    reread: async (name) => {
      const projectId = get().projectId;
      const doc = find(name);
      if (!projectId || !doc || doc.source === null || isDirty(doc)) return;
      try {
        const read = await api.read(projectId, name);
        if (!still(projectId)) return;
        change(name, (d) =>
          // An edit made here while the read was on its way is kept.
          isDirty(d) || d.source === null
            ? d
            : { ...d, source: read.source, savedSource: read.source, readError: null }
        );
      } catch (cause) {
        log.error(`Program ${name} could not be read again: ${message(cause)}`);
      }
    },

    setSource: (name, source) => change(name, (doc) => ({ ...doc, source })),

    setSettings: (name, edit) =>
      change(name, (doc) => ({ ...doc, entry: { ...doc.entry, ...edit, name: doc.entry.name } })),

    discard: (name) =>
      change(name, (doc) => ({
        ...doc,
        source: doc.savedSource,
        entry: JSON.parse(doc.savedEntry) as ProgramEntry,
      })),

    save: async (name) => {
      const projectId = get().projectId;
      if (!projectId) return false;
      set({ busy: 'saving', error: null });
      try {
        const saved = await saveDoc(projectId, name);
        set({ busy: null });
        return saved;
      } catch (cause) {
        failed(`Program ${name} was not saved`, cause);
        return false;
      }
    },

    check: async (name) => {
      const projectId = get().projectId;
      const doc = find(name);
      if (!projectId || !doc || doc.source === null) return;
      const source = doc.source;
      set({ busy: 'checking', error: null });
      try {
        const result = await api.check(projectId, name, source);
        change(name, (d) => checked(d, result, source));
        set({ busy: null });
      } catch (cause) {
        // No result is kept: an old one would read as the answer for this text.
        change(name, (d) => ({ ...d, check: null, checkedSource: null }));
        failed(`Program ${name} was not checked`, cause);
      }
    },

    download: async (name, cold = false) => {
      const projectId = get().projectId;
      if (!projectId || !find(name)) return false;
      set({ busy: 'loading', error: null });
      try {
        if (!(await saveDoc(projectId, name))) {
          set({ busy: null, error: `Program ${name} has not been read yet.` });
          return false;
        }
        const source = find(name)?.source ?? '';
        // The check says what is wrong, with a line and a column; a refused build says only
        // that it was refused.
        const result = await api.check(projectId, name, source);
        change(name, (d) => checked(d, result, source));
        if (!result.ok || (result.binding_problems?.length ?? 0) > 0) {
          set({
            busy: null,
            error: result.ok
              ? `Program ${name} was not loaded: its bindings do not match its variables.`
              : `Program ${name} was not loaded: it does not compile.`,
          });
          return false;
        }
        const built = await api.build(projectId, name);
        change(name, (d) => ({ ...d, built, loaded: null, startFailure: null }));
        const applied = await api.setInstance(projectId, name, built.instance, cold);
        // Saved and applied is not started: the controller lists what did not start, and why.
        const refused = (applied.changes?.failed ?? []).find(([id]) => id === `runtime:${name}`);
        change(name, (d) => ({ ...d, loaded: applied, startFailure: refused ? refused[1] : null }));
        if (refused) {
          set({
            busy: null,
            error: `Program ${name} version ${built.version} was saved into the project's runtimes but did not start: ${refused[1]}`,
          });
          log.error(`Program ${name} did not start: ${refused[1]}`);
          void get().readRuntimes();
          return false;
        }
        set({ busy: null });
        log.info(
          applied.applied
            ? `Program ${name} version ${built.version} was loaded. ${describeStart(loadOf(applied, name))}`
            : `Program ${name} version ${built.version} was saved into the project's runtimes. The controller runs another project, so nothing was started.`
        );
        void get().readRuntimes();
        return true;
      } catch (cause) {
        failed(`Program ${name} was not loaded`, cause);
        return false;
      }
    },

    unload: async (name) => {
      const projectId = get().projectId;
      if (!projectId) return false;
      set({ busy: 'loading', error: null });
      try {
        await api.setInstance(projectId, name, null);
        change(name, (d) => ({ ...d, loaded: null, startFailure: null }));
        set({ busy: null });
        log.info(`Program ${name} was taken out of the project's runtimes.`);
        void get().readRuntimes();
        return true;
      } catch (cause) {
        failed(`Program ${name} was not unloaded`, cause);
        return false;
      }
    },

    command: async (name, run) => {
      set({ busy: 'commanding', error: null });
      try {
        await (run ? api.run(name) : api.stop(name));
        set({ busy: null });
        log.info(`Runtime ${name} was asked to ${run ? 'run' : 'stop'}.`);
        void get().readRuntimes();
      } catch (cause) {
        failed(`Runtime ${name} was not asked to ${run ? 'run' : 'stop'}`, cause);
      }
    },

    force: async (name, variable, value) => {
      set({ busy: 'forcing', error: null });
      try {
        await api.force(name, variable, value);
        set({ busy: null });
        log.info(`${variable} of ${name} is forced to ${String(value)}.`);
        void get().readRuntimes();
        return true;
      } catch (cause) {
        failed(`${variable} of ${name} was not forced`, cause);
        return false;
      }
    },

    release: async (name, variable) => {
      const what = variable === null ? `The forces of ${name} were` : `${variable} of ${name} was`;
      set({ busy: 'forcing', error: null });
      try {
        await api.release(name, variable);
        set({ busy: null });
        log.info(`${what} released.`);
        void get().readRuntimes();
        return true;
      } catch (cause) {
        failed(`${what} not released`, cause);
        return false;
      }
    },

    readRuntimes: async () => {
      try {
        const answer = await api.runtimes();
        set({
          runtimes: {
            runtimes: answer.runtimes,
            modules: answer.modules.map((module) => module.provides),
            error: null,
            readAtMs: Date.now(),
          },
        });
      } catch (cause) {
        // What was read before stays, marked by the error: it is no longer what is.
        set((state) => ({ runtimes: { ...state.runtimes, error: message(cause) } }));
      }
    },

    readWatch: async (name, variables) => {
      try {
        const answer = await api.watch(name, variables);
        set((state) => ({ watches: { ...state.watches, [name]: { answer, error: null } } }));
      } catch (cause) {
        // The values read before are dropped: shown on, they would read as the program's now.
        set((state) => ({ watches: { ...state.watches, [name]: { answer: null, error: message(cause) } } }));
      }
    },

    dropWatch: (name) =>
      set((state) => {
        const { [name]: _dropped, ...watches } = state.watches;
        return { watches };
      }),
  };
});
