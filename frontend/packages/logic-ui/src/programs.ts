// The open project's control programs: what the logic module has stored, the engineer's draft
// of each, what the last check and the last build said, and what the controller's runtimes are
// doing. Nothing here runs a program: loading is saving a built instance into the project's
// runtimes, which the core does.

import { create } from 'zustand';
import {
  api,
  host,
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
  /** When the last read that succeeded was sent (ms, `monotonicNow`). */
  readAtMs: number | null;
  /**
   * The runtimes configured in the logic module (its `logic/tasks.json`, ADR-013 §2.19), read
   * with the controller's: for each program a configured runtime runs, by its name in lower
   * case, that runtime's id. A program in none runs alone, as a runtime of its own name.
   * `null`: not known, never read or the last read failed (`configuredError`). Left out of a
   * view, the editor reads the controller's runtimes alone, as before there were configured
   * runtimes.
   */
  configured?: Record<string, string> | null;
  /** Why the configured runtimes could not be read. */
  configuredError?: string | null;
}

/** The variables of a running program as last read, or why they could not be. */
export interface WatchView {
  /** `null`: not read, or the last read failed: nothing read before is shown as a value. */
  answer: WatchAnswer | null;
  error: string | null;
  /**
   * When the request `answer` answers was sent (ms, `monotonicNow`); `null` with no answer. Not
   * when it came: the time it spent in transit is time its values aged, so the send time is a
   * bound on their age that never says them younger than they are.
   */
  sentAtMs: number | null;
}

/** How often the running program's variables are read while its editor is open. */
export const WATCH_POLL_MS = 500;

/**
 * The clock every age the editor judges is measured on: monotonic, so that setting the wall
 * clock (back an hour, or forward) neither keeps old values current nor puts off judging them.
 */
export const monotonicNow = (): number => performance.now();

/**
 * How long the runtimes may go unanswered before what was read of them is the last known state,
 * not the current one: the rule of Settings → Runtimes (`RUNTIMES_STALE_AFTER_MS`, 3 s for a list
 * read once a second), reused for the same list read as often.
 */
export const RUNTIMES_STALE_AFTER_MS = 3000;

/** Whether the values of a watch answer are the program's current ones, and until when. */
export type Currency = { current: true; untilMs: number } | { current: false; why: string };

const isAmount = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value) && value >= 0;
const millis = (us: number) => `${(us / 1000).toFixed(3).replace(/\.?0+$/, '')} ms`;

/**
 * Whether the values of `answer`, asked for at `sentAtMs`, are what the program does at `nowMs`
 * (both `monotonicNow`; ADR-013 §2.8). Both bounds are worked out from what the controller says, none is a
 * constant of this view's:
 *
 * - the read is recent: its age (`age_ms` and the time since it came) is within `fresh_ms`, the
 *   oldest answer the route gives as current, and one interval of asking (`WATCH_POLL_MS`). An
 *   answer older than that should have been replaced and was not: asking has stopped getting
 *   answers (a request left hanging, a core no longer reading the runtime);
 * - in RUN, the program was scanning when read: its last scan had completed within two of its
 *   intervals and its longest scan (`scan_age_us` against `interval_ms` and `max_scan_us`), the
 *   interval taken only from a `runtime` of the version that answered (another version's
 *   interval is not this program's: a list read before a load still names the old one). A
 *   scan starting less than an interval late is on time (the VM's scan thread counts an overrun
 *   from one interval), so on time a program completes one at least that often; past it, its
 *   scan thread has stopped or fallen behind, and the values are older than the program's own
 *   cycle (a runtime still saying RUN, before its watchdog's ten intervals have passed).
 *
 * What it needs and is not told makes the values not current, never current.
 */
export function currencyOf(answer: WatchAnswer, sentAtMs: number, runtime: RuntimeStatus, nowMs: number): Currency {
  const age: unknown = answer.age_ms;
  const fresh: unknown = answer.fresh_ms;
  if (!isAmount(age)) return { current: false, why: 'The controller does not say how old these values are.' };
  if (!isAmount(fresh)) return { current: false, why: 'The controller does not say for how long its values are current.' };
  const allowed = fresh + WATCH_POLL_MS;
  const read = age + Math.max(0, nowMs - sentAtMs);
  if (read > allowed) {
    return {
      current: false,
      why: `They were read ${Math.round(read)} ms ago and no newer answer has come: an answer is current for ${fresh} ms, and one is asked for every ${WATCH_POLL_MS} ms.`,
    };
  }
  if (answer.state.mode === 'run') {
    const scan: unknown = answer.state.scan_age_us;
    const longest: unknown = answer.state.max_scan_us;
    const interval: unknown = runtime.interval_ms;
    if (!isAmount(scan)) return { current: false, why: 'The runtime does not say when the program last completed a scan.' };
    if (!isAmount(longest)) return { current: false, why: 'The runtime does not say how long the program’s longest scan took.' };
    if (runtime.version !== answer.state.version) {
      return {
        current: false,
        why: `The runtimes were read of version ${runtime.version} and the values are of version ${answer.state.version}: the interval of the version that answered is not known yet.`,
      };
    }
    if (!isAmount(interval) || interval === 0) return { current: false, why: 'The program’s interval is not known.' };
    const bound = 2 * interval * 1000 + longest;
    if (scan > bound) {
      return {
        current: false,
        why: `When read, the program had completed no scan for ${millis(scan)}, past the ${millis(bound)} within which a program scanning every ${interval} ms completes one (two intervals and its longest scan): it is not scanning on time.`,
      };
    }
  }
  return { current: true, untilMs: sentAtMs + allowed - age };
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
  /**
   * Check the text in the editor. `quiet`: the check the Ladder Diagram editor makes by itself
   * after an edit, which neither marks the editor busy nor reports a failure as an error.
   */
  check: (name: string, quiet?: boolean) => Promise<void>;
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

/**
 * Whether an open program follows a source saved elsewhere (an agent over MCP, another workbench)
 * by reading it again: a diagram, LD, FBD or SFC, whose view draws the saved text. A chart above
 * all: until its editor (ADR-013 L3d-3) it is only ever written elsewhere, and a view left on an
 * older text would never be live (its check's version is not the runtime's).
 */
export const followsSavedSource = (language: string): boolean => language === 'ld' || language === 'fbd' || language === 'sfc';

export function anyDirty(state: Pick<ProgramsState, 'programs'>): boolean {
  return state.programs.some(isDirty);
}

/** Whether the last check is of the text now in the editor. */
export function checkIsCurrent(doc: ProgramDoc): boolean {
  return doc.check !== null && doc.checkedSource === doc.source;
}

/**
 * How what the controller runs compares with this program, in words that claim no more than
 * is known. The runtime of a program running alone has the program's name as its id. A program
 * that the logic module's configuration puts in a runtime (ADR-013 §2.19) is `unknown` with that
 * runtime's id in `runtime`, whether that runtime is loaded or not: this editor does not show
 * such a runtime, and its commands, which act on the runtime named like the program, are not
 * offered for it (Load of such a program is refused by the logic module).
 */
export type RunState =
  | { kind: 'unknown'; why: string; runtime?: string }
  | { kind: 'not-loaded' }
  | { kind: 'loaded'; runtime: RuntimeStatus; stale: boolean };

/** The configured runtime that runs program `name`, if the logic module says one does. */
const configuredOf = (name: string, view: RuntimesView): string | undefined => {
  const configured = view.configured;
  const key = name.toLowerCase();
  return configured && Object.hasOwn(configured, key) ? configured[key] : undefined;
};

export function runState(name: string, view: RuntimesView): RunState {
  if (view.runtimes === null) {
    return { kind: 'unknown', why: view.error ?? 'the runtimes have not been read yet' };
  }
  const runtime = view.runtimes.find((r) => r.id === name);
  // Which runtime runs a program is the logic module's configuration, never a guess from the
  // controller's runtimes: those name the PROGRAM a source declares, which need not be the
  // program's name (a program alone may declare any, and a copy of another's text declares that
  // one's).
  const owner = view.error === null ? configuredOf(name, view) : undefined;
  if (owner !== undefined) {
    const running = view.runtimes.find((r) => r.id === owner);
    let why = running
      ? running.program.split(', ').length > 1
        ? `program ${name} runs in runtime ${owner} with other programs, which this editor does not show yet`
        : `program ${name} runs in runtime ${owner}, which this editor does not show yet`
      : `program ${name} is in runtime ${owner}, which is not loaded and which this editor does not show yet`;
    // Its build alone, loaded before it was put in the runtime: it runs, and Load cannot replace it.
    if (runtime) why += `; its build alone, loaded before, still runs as runtime ${name}`;
    return { kind: 'unknown', runtime: owner, why };
  }
  if (view.error !== null) {
    // What was read before the failure is not what is: it is shown only as the last known.
    return runtime
      ? { kind: 'loaded', runtime, stale: true }
      : { kind: 'unknown', why: view.error };
  }
  if (!runtime && view.configured === null) {
    // It may run in a configured runtime: not said to be not loaded.
    return {
      kind: 'unknown',
      why: `whether program ${name} runs in a runtime configured in the logic module is not known: ${view.configuredError ?? 'the configured runtimes have not been read yet'}`,
    };
  }
  return runtime ? { kind: 'loaded', runtime, stale: false } : { kind: 'not-loaded' };
}

/**
 * Why no command for the program is offered, when it runs in a runtime of another id: every
 * command of the editor acts on the runtime named like the program. `null` otherwise.
 */
export function inOtherRuntime(state: RunState): string | null {
  return state.kind === 'unknown' && state.runtime !== undefined ? state.why : null;
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

const EMPTY_RUNTIMES: RuntimesView = {
  runtimes: null,
  modules: null,
  error: null,
  readAtMs: null,
  configured: null,
  configuredError: null,
};

/** The configured runtimes' answer of the logic module, as far as this editor reads it. */
interface ConfiguredAnswer {
  runtimes: Array<{ id: string; tasks: Array<{ programs: string[] }> }>;
}

/**
 * Which configured runtime of the logic module runs each program of `projectId`, by program
 * name in lower case (the module compares names without case), or why that is not known.
 */
async function readConfigured(
  projectId: string | null
): Promise<Pick<RuntimesView, 'configured' | 'configuredError'>> {
  if (!projectId) return { configured: null, configuredError: 'no project is open' };
  try {
    const answer = await host().api<ConfiguredAnswer>(
      `/api/v1/projects/${encodeURIComponent(projectId)}/logic/runtimes`
    );
    const configured: Record<string, string> = {};
    for (const runtime of answer.runtimes) {
      for (const task of runtime.tasks) {
        for (const program of task.programs) configured[program.toLowerCase()] ??= runtime.id;
      }
    }
    return { configured, configuredError: null };
  } catch (cause) {
    return { configured: null, configuredError: message(cause) };
  }
}

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

/**
 * Requests overlap (each is asked on time, whether the one before has come back or not), and an
 * answer may come after a newer one: each is numbered when asked, and one older than what was
 * last applied, or than a drop or a reset, is not applied. Numbers grow across every program.
 */
let asked = 0;
/** The newest request applied (or dropped) for each program's watch, by program name. */
const watchApplied = new Map<string, number>();
/** The newest runtimes request asked, and the newest applied. */
let runtimesAsked = 0;
let runtimesApplied = 0;
/** Nothing asked before this was applied: the store was reset after it was asked. */
let floor = 0;
/** Fires when no read of the runtimes has come back for `RUNTIMES_STALE_AFTER_MS`. */
let runtimesOverdue: ReturnType<typeof setTimeout> | null = null;

export const usePrograms = create<ProgramsState>((set, get) => {
  /** (Re)start waiting for the runtimes: past the bound with a read still out, what was read is no longer what is. */
  const awaitRuntimes = () => {
    if (runtimesOverdue !== null) clearTimeout(runtimesOverdue);
    runtimesOverdue = setTimeout(() => {
      runtimesOverdue = null;
      // Only while a read is out: an editor that stopped reading has nothing overdue.
      if (runtimesAsked <= Math.max(runtimesApplied, floor)) return;
      set((state) =>
        state.runtimes.error !== null
          ? state
          : {
              runtimes: {
                ...state.runtimes,
                error: `the controller has not answered for ${RUNTIMES_STALE_AFTER_MS / 1000} s`,
              },
            }
      );
    }, RUNTIMES_STALE_AFTER_MS);
  };

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

  /**
   * Whether a command for program `name` is refused before it is sent, and said so: the program
   * runs in a runtime of another id, and every command here acts on the runtime named like the
   * program (Load and Unload replace or drop that instance; Run, Stop and the forces address it),
   * so none would have the effect it reports.
   */
  const refusedElsewhere = (name: string, what: string): boolean => {
    const why = inOtherRuntime(runState(name, get().runtimes));
    if (why === null) return false;
    set({ error: `${what}: ${why}.` });
    return true;
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
      // Another project's configured runtimes are not this one's.
      set({
        ...EMPTY,
        projectId,
        phase: 'reading',
        runtimes: { ...get().runtimes, configured: null, configuredError: null },
      });
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

    reset: () => {
      floor = asked;
      if (runtimesOverdue !== null) clearTimeout(runtimesOverdue);
      runtimesOverdue = null;
      set({ ...EMPTY });
    },

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
        let followed = false;
        change(name, (d) => {
          // An edit made here while the read was on its way is kept.
          if (isDirty(d) || d.source === null) return d;
          followed = d.source !== read.source;
          return { ...d, source: read.source, savedSource: read.source, readError: null };
        });
        // A text followed from elsewhere is checked at once, quietly (nobody here asked: the
        // editor is not busy, a failure is logged, and an answer for an older text is dropped),
        // so what everyone sees, a viewer's diagram as an engineer's, is checked text and can be
        // drawn live, rather than an unchecked one that never would be.
        if (followed) void get().check(name, true);
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

    check: async (name, quiet = false) => {
      const projectId = get().projectId;
      const doc = find(name);
      if (!projectId || !doc || doc.source === null) return;
      const source = doc.source;
      if (!quiet) set({ busy: 'checking', error: null });
      try {
        const result = await api.check(projectId, name, source);
        // A quiet check answered after the text moved on is of a text no longer shown: dropped.
        change(name, (d) => (quiet && d.source !== source ? d : checked(d, result, source)));
        if (!quiet) set({ busy: null });
      } catch (cause) {
        // No result is kept: an old one would read as the answer for this text.
        change(name, (d) => ({ ...d, check: null, checkedSource: null }));
        if (quiet) log.error(`Program ${name} was not checked: ${message(cause)}`);
        else failed(`Program ${name} was not checked`, cause);
      }
    },

    download: async (name, cold = false) => {
      const projectId = get().projectId;
      if (!projectId || !find(name)) return false;
      if (refusedElsewhere(name, `Program ${name} was not loaded`)) return false;
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
      if (refusedElsewhere(name, `Program ${name} was not unloaded`)) return false;
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
      if (refusedElsewhere(name, `Runtime ${name} was not asked to ${run ? 'run' : 'stop'}`)) return;
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
      if (refusedElsewhere(name, `${variable} of ${name} was not forced`)) return false;
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
      if (refusedElsewhere(name, `${what} not released`)) return false;
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
      const number = ++asked;
      runtimesAsked = number;
      const sentAtMs = monotonicNow();
      if (runtimesOverdue === null) awaitRuntimes();
      const newest = () => {
        if (number <= Math.max(runtimesApplied, floor)) return false;
        runtimesApplied = number;
        return true;
      };
      try {
        const projectId = get().projectId;
        const [answer, configured] = await Promise.all([api.runtimes(), readConfigured(projectId)]);
        if (!newest()) return;
        awaitRuntimes();
        set({
          runtimes: {
            runtimes: answer.runtimes,
            modules: answer.modules.map((module) => module.provides),
            error: null,
            readAtMs: sentAtMs,
            // Read for another project than the one open now: not known.
            ...(get().projectId === projectId
              ? configured
              : { configured: null, configuredError: 'the configured runtimes are being read' }),
          },
        });
      } catch (cause) {
        if (!newest()) return;
        // What was read before stays, marked by the error: it is no longer what is.
        set((state) => ({ runtimes: { ...state.runtimes, error: message(cause) } }));
      }
    },

    readWatch: async (name, variables) => {
      const number = ++asked;
      const sentAtMs = monotonicNow();
      const newest = () => {
        if (number <= Math.max(watchApplied.get(name) ?? 0, floor)) return false;
        watchApplied.set(name, number);
        return true;
      };
      try {
        const answer = await api.watch(name, variables);
        if (!newest()) return;
        set((state) => ({ watches: { ...state.watches, [name]: { answer, error: null, sentAtMs } } }));
      } catch (cause) {
        if (!newest()) return;
        // The values read before are dropped: shown on, they would read as the program's now.
        set((state) => ({ watches: { ...state.watches, [name]: { answer: null, error: message(cause), sentAtMs: null } } }));
      }
    },

    dropWatch: (name) => {
      // An answer to a request asked before the drop is not shown after it.
      watchApplied.set(name, asked);
      set((state) => {
        const { [name]: _dropped, ...watches } = state.watches;
        return { watches };
      });
    },
  };
});
