// The open project's configured runtimes as the logic module answered them, and the runtimes being
// edited. Nothing here is made up: a list that could not be read is `null` with why, never empty;
// a runtime's saved entry is what the module gave back, never what was typed; a check's answer for
// an older draft than the last one sent is dropped; a draft that is not a complete body is never
// sent. Configured is all this store knows: whether the controller runs a runtime is not read here.

import { create } from 'zustand';
import { refusalOf, type Refusal } from './library';
import { canonical, usePrograms } from './programs';
import {
  api,
  bodyOf,
  elsewhereOf,
  listedNameOf,
  loadQuestion,
  planLoad,
  sameId,
  sentOf,
  newRuntime,
  type Instance,
  type LoadAnswer,
  type RuntimeBuild,
  type RuntimeCheck,
  type RuntimeEntry,
  type RuntimesChanged,
  type RuntimesList,
} from './runtimes';
import { api as core, problemsOf, useController, type ApplyAnswer, type ConfigProblem, type RuntimesAnswer } from './bridge';

/** The programs a stored instance names, by their names in it: its `programs`, else its `program`. */
const heldBy = (instance: Instance): string[] =>
  Array.isArray(instance.programs) ? instance.programs.map((p) => p.name) : instance.program !== undefined ? [instance.program.name] : [];

/** Why a load, a command or an unload stopped: in this UI's words, then the refuser's own. */
export interface Stopped {
  words: string;
  /** The module's or the core's refusal, verbatim; `null` when nothing was refused (a check that is not ok). */
  refusal: Refusal | null;
  /** The core's `problems` with a refused save of the project's runtimes. */
  problems: ConfigProblem[] | null;
}

/**
 * The versions of the runtime's programs last built or checked ok here, in lower case by the name
 * the controller lists each under: for a build, the PROGRAM name its instance gives it; for a
 * check, which does not give that name, the program's name in the project.
 */
export interface VersionsHere {
  of: 'build' | 'check';
  byName: Record<string, string>;
}

/** A runtime opened in an editor. */
export interface RuntimeDoc {
  id: string;
  /**
   * `reading`: being read; `ready`: read, or new; `unreadable`: the read failed (a 404: there is no
   * such runtime); `implied`: the module answered a program running alone, not a configured runtime.
   */
  phase: 'reading' | 'ready' | 'unreadable' | 'implied';
  readError: Refusal | null;
  /** Opened with "Add new runtime": the module has not stored it, until a save answers 200. */
  isNew: boolean;
  /** The stored runtime, as the module last gave it; `null` when not known (or not stored). */
  saved: RuntimeEntry | null;
  /** A save answered 200 but the stored runtime could not be read back: why. */
  readBack: string | null;
  /** The runtime in the form. */
  draft: RuntimeEntry;
  /** The number of the last check asked for; an answer to an earlier one is dropped. */
  checkSeq: number;
  /** The last check applied, and the draft it checked (canonical). */
  check: { body: string; answer: RuntimeCheck } | null;
  /** The module refused the last check (a body it does not take, an unknown id), or it could not be made. */
  checkRefusal: Refusal | null;
  checking: boolean;
  busy: 'saving' | 'deleting' | 'loading' | 'unloading' | null;
  /** The module's refusal of the last save or delete. */
  refusal: { of: 'save' | 'delete'; refusal: Refusal } | null;
  /** What the last save answered: whether the module made the runtime. */
  saveAnswer: { created: boolean } | null;
  /** Saved or deleted by somebody else while this had changes here. */
  elsewhere: 'changed' | 'deleted' | null;
  /** The id the event named, when it is another spelling than this new runtime's: not its change. */
  elsewhereAs: string | null;
  /** An event for this runtime that came while a write of this editor was sent and not answered. */
  deferred: RuntimesChanged['what'] | null;
  /** An event of somebody else's write that came while this editor was busy: followed after. */
  later: RuntimesChanged['what'] | null;
  /** The last build made here (by a load), as the module answered it. */
  built: RuntimeBuild | null;
  /** The versions of the last build, or of the last check that was ok, made here; `null`: none. */
  versions: VersionsHere | null;
  /** What the core answered the last load made here. */
  loaded: LoadAnswer | null;
  /** Why the last load stopped, when it did. */
  loadStopped: Stopped | null;
  /** What the last command (Run, Stop, Unload) answered: done, or why not. */
  commanded: { words: string; stopped: Stopped | null; answer: ApplyAnswer | null } | null;
}

export interface RuntimesState {
  projectId: string | null;
  /** The module's list; `null` while not read, or when the read failed. */
  list: RuntimesList | null;
  listError: string | null;
  /** The installed runtime modules (`GET /api/v1/runtimes`); `null` while not read, or when that failed. */
  modules: RuntimesAnswer['modules'] | null;
  modulesError: string | null;
  docs: Record<string, RuntimeDoc>;

  load(projectId: string): Promise<void>;
  reset(): void;
  readList(): Promise<void>;
  readModules(): Promise<void>;
  /** Read runtime `id` into an editor, unless it is open with changes. */
  open(id: string): Promise<void>;
  /** An editor on a runtime the module has not stored, with nothing chosen. Writes nothing. */
  openNew(id: string): void;
  edit(id: string, draft: RuntimeEntry): void;
  /** The draft back to the stored runtime. A new one, or one deleted elsewhere, is dropped: true then (its editor closes). */
  discard(id: string): boolean;
  check(id: string): Promise<void>;
  /** Save the draft; the module checks every rule itself. */
  save(id: string): Promise<boolean>;
  /** Delete the runtime. Its editor is kept until `forget`, so that it is closed before it goes. */
  remove(id: string): Promise<boolean>;
  forget(id: string): void;
  /** Follow `LOGIC_RUNTIMES_CHANGED` of the open project. Returns the editor to close (deleted elsewhere), if any. */
  changed(event: RuntimesChanged): string | null;
  /**
   * Save the draft if it has changes, check it, build it, build each configured runtime that holds
   * one of its programs, ask, and save the plan into the project's runtimes. Stops at the first
   * refusal, with its words, and sends nothing to the core's configuration before the engineer
   * agreed. Resolves with whether the core's configuration was saved.
   */
  loadRuntime(id: string, cold: boolean): Promise<boolean>;
  /** Ask the controller to put runtime `id` in RUN or in STOP (asked first by the caller). */
  command(id: string, run: boolean): Promise<void>;
  /** Take runtime `id` out of the project's runtimes, every other instance kept (asked first by the caller). */
  unload(id: string): Promise<boolean>;
}

/** The versions of a build's programs, in lower case by the PROGRAM name the controller lists each under. */
const versionsOfBuild = (build: RuntimeBuild): VersionsHere => ({
  of: 'build',
  byName: Object.fromEntries(build.programs.map((p) => [listedNameOf(build.instance, p.name).toLowerCase(), p.version])),
});
/** The versions of an ok check's programs, by name in lower case; `null` when it gives none. */
const versionsOfCheck = (check: RuntimeCheck): VersionsHere | null =>
  check.ok
    ? {
        of: 'check',
        byName: Object.fromEntries(check.programs.flatMap((p) => (p.version === undefined ? [] : [[p.name.toLowerCase(), p.version]]))),
      }
    : null;

/** Whether `doc` holds a runtime the module has not stored as it is shown. */
export function isRuntimeDirty(doc: RuntimeDoc): boolean {
  if (doc.saved === null) return true;
  return canonical(bodyOf(doc.draft)) !== canonical(bodyOf(doc.saved));
}

/** Whether an editor holds what is not stored: only an editor with a form can. */
export const hasChanges = (doc: RuntimeDoc): boolean => doc.phase === 'ready' && isRuntimeDirty(doc);
export const anyRuntimeDirty = (state: Pick<RuntimesState, 'docs'>): boolean => Object.values(state.docs).some(hasChanges);

/** The draft as a check or a save sends it, for comparison: what was checked is this text. */
export const draftKey = (draft: RuntimeEntry): string => canonical(bodyOf(draft));

const fresh = (id: string): RuntimeDoc => ({
  id,
  phase: 'reading',
  readError: null,
  isNew: false,
  saved: null,
  readBack: null,
  draft: newRuntime(id),
  checkSeq: 0,
  check: null,
  checkRefusal: null,
  checking: false,
  busy: null,
  refusal: null,
  saveAnswer: null,
  elsewhere: null,
  elsewhereAs: null,
  deferred: null,
  later: null,
  built: null,
  versions: null,
  loaded: null,
  loadStopped: null,
  commanded: null,
});

// The events of this workbench's own writes, still to come: one per write answered 200 whose
// event has not been heard yet. Not state: nothing is drawn from it.
const echoes = new Map<string, number>();
// The runtimes whose write is sent and not answered yet: an event heard then may be that write's.
const unanswered = new Set<string>();
// Numbered reads of the list and of the modules: an answer for an earlier read is dropped.
let listRead = 0;
let modulesRead = 0;
// Which opening of a project this is: an answer to a request of an earlier one is dropped.
let opening = 0;

const EMPTY = { projectId: null, list: null, listError: null, modules: null, modulesError: null, docs: {} };

export const useRuntimes = create<RuntimesState>((set, get) => {
  const patch = (id: string, change: Partial<RuntimeDoc>) =>
    set((state) => {
      const doc = state.docs[id];
      return doc ? { docs: { ...state.docs, [id]: { ...doc, ...change } } } : {};
    });
  const drop = (id: string) =>
    set((state) => {
      const { [id]: _gone, ...docs } = state.docs;
      return { docs };
    });
  /** The open editor of runtime `id`: by its key, else in another case. */
  const docKey = (id: string): string => {
    const keys = Object.keys(get().docs);
    return keys.find((k) => k === id) ?? keys.find((k) => sameId(k, id)) ?? id;
  };

  const reread = async (id: string) => {
    const projectId = get().projectId;
    const at = opening;
    if (projectId === null || !get().docs[id]) return;
    try {
      const answer = await api.getRuntime(projectId, id);
      if (opening !== at) return;
      const doc = get().docs[id];
      if (!doc) return;
      if (answer.implied) {
        // A program running alone: there is no configured runtime to edit.
        patch(id, { phase: 'implied', readError: null, isNew: false, saved: null, readBack: null, draft: answer.runtime });
        return;
      }
      // Changed while it was being read: kept, now measured against what is stored.
      const keep = doc.phase === 'ready' && isRuntimeDirty(doc) && !doc.isNew;
      patch(id, {
        phase: 'ready',
        readError: null,
        isNew: false,
        saved: answer.runtime,
        readBack: null,
        draft: keep ? doc.draft : answer.runtime,
      });
    } catch (cause) {
      if (opening !== at) return;
      const doc = get().docs[id];
      const refusal = refusalOf(cause);
      if (doc && doc.phase === 'ready' && isRuntimeDirty(doc)) {
        // Changed here while a change made elsewhere was read: kept. Gone (404): saving makes it
        // again; otherwise what is stored is not known, and it was changed: saving replaces it.
        patch(id, refusal.status === 404 ? { elsewhere: 'deleted', saved: null } : { elsewhere: 'changed' });
        return;
      }
      patch(id, { phase: 'unreadable', readError: refusal, saved: null });
    }
  };

  /** Apply an event of somebody else's write to an open runtime. */
  const follow = (id: string, what: RuntimesChanged['what']): string | null => {
    const doc = get().docs[id];
    if (!doc) return null;
    if (hasChanges(doc)) {
      // What is changed here is kept: it is said that saving replaces the other change.
      patch(id, what === 'deleted' ? { elsewhere: 'deleted', saved: null } : { elsewhere: 'changed' });
      return null;
    }
    if (what === 'deleted') {
      drop(id);
      return id;
    }
    void reread(id);
    return null;
  };

  /** After a write of `id` was answered: its event, if it came first, was this write's. */
  const settled = (id: string, ok: boolean) => {
    const deferred = get().docs[id]?.deferred ?? null;
    patch(id, { deferred: null });
    if (ok) {
      if (deferred === null) echoes.set(id, (echoes.get(id) ?? 0) + 1);
    } else if (deferred !== null) {
      // A refused write sends nothing: what came was somebody else's.
      patch(id, { later: deferred });
    }
  };

  /** Once not busy: an event of somebody else's write heard meanwhile is followed now. */
  const afterBusy = (id: string) => {
    const later = get().docs[id]?.later ?? null;
    if (later === null) return;
    patch(id, { later: null });
    const doc = get().docs[id];
    if (later === 'deleted' && doc && !hasChanges(doc)) {
      // Not stored any more: the form shown is not saved, and saving makes the runtime again.
      patch(id, { elsewhere: 'deleted', saved: null });
      return;
    }
    follow(id, later);
  };

  /** The steps of a load after the draft is saved: see `loadRuntime`. */
  const loading = async (projectId: string, id: string, cold: boolean, at: number): Promise<boolean> => {
    const gone = () => opening !== at;
    const refuse = (words: string, cause?: unknown): false => {
      if (!gone()) {
        patch(id, {
          loadStopped: { words, refusal: cause === undefined ? null : refusalOf(cause), problems: cause === undefined ? null : problemsOf(cause) },
        });
      }
      return false;
    };
    // The stored runtime is checked as a build would compile it; an older check still out is dropped.
    const seq = (get().docs[id]?.checkSeq ?? 0) + 1;
    patch(id, { checkSeq: seq, checking: false });
    let checked: RuntimeCheck;
    try {
      checked = await api.check(projectId, id, {});
    } catch (cause) {
      if (gone()) return false;
      patch(id, { check: null, checkRefusal: refusalOf(cause) });
      return refuse(`Runtime ${id} could not be checked, so nothing was built or loaded.`, cause);
    }
    if (gone()) return false;
    const stored = get().docs[id]?.saved ?? null;
    const versions = versionsOfCheck(checked);
    patch(id, { check: { body: stored === null ? '' : draftKey(stored), answer: checked }, checkRefusal: null, ...(versions ? { versions } : {}) });
    if (!checked.ok) {
      return refuse(`Runtime ${id} does not check as saved, so nothing was built or loaded. What the module found is in the Diagnostics tab.`);
    }
    let build: RuntimeBuild;
    try {
      build = await api.build(projectId, id);
    } catch (cause) {
      return refuse(`Runtime ${id} was not built, so nothing was loaded.`, cause);
    }
    if (gone()) return false;
    patch(id, { built: build, versions: versionsOfBuild(build) });
    let saved: Instance[];
    // The revision of this one read goes with the save: if another client (an agent, another
    // workbench, a program's Load) saves the project's runtimes while the builds run or the
    // question is open, the core refuses this save instead of taking out or setting back what
    // that client saved.
    let revision: string | undefined;
    try {
      const read = await api.config(projectId);
      saved = read.config?.instances ?? [];
      revision = read.revision;
    } catch (cause) {
      return refuse("The project's runtimes could not be read, so nothing was loaded.", cause);
    }
    if (gone()) return false;
    // Without a revision the save would be made whatever is stored by then: a save another client
    // makes while the question is open would be overwritten, unasked. Nothing is asked or sent.
    if (revision === undefined) {
      return refuse(
        "The core gave no revision with the project's runtimes, so this Load could not keep a save made by someone else while it asks from being overwritten: nothing was loaded. Load needs a core that gives one."
      );
    }
    // Each other instance that holds one of its programs: a configured runtime is built again
    // by the module, without the program, and goes into the same save; any other is taken out.
    // A runtime built again may itself now run a program another instance holds: the same
    // again for it, each configured runtime built once.
    const view = usePrograms.getState().runtimes;
    const status = { runtimes: view.runtimes, error: view.error, elsewhere: elsewhereOf(useController.getState().activeProjectId, projectId) };
    const rebuilt: RuntimeBuild[] = [];
    const tried = new Set<string>();
    let list: RuntimesList | null = null;
    for (;;) {
      const holders = planLoad(saved, build, rebuilt, cold, status).takenOut;
      if (holders.length === 0) break;
      if (list === null) {
        try {
          list = await api.list(projectId);
        } catch (cause) {
          return refuse(
            `${holders.map((h) => h.id).join(', ')} ${holders.length === 1 ? 'holds' : 'hold'} a program of ${id}, and whether each is a configured runtime is not known: the configured runtimes could not be read. Nothing was loaded.`,
            cause
          );
        }
        if (gone()) return false;
      }
      const configured = list.runtimes;
      const toBuild = holders.flatMap((holder) => {
        const entry = configured.find((r) => sameId(r.id, holder.id));
        return entry === undefined || tried.has(entry.id.toLowerCase()) ? [] : [{ holder, entry }];
      });
      if (toBuild.length === 0) break;
      for (const { holder, entry } of toBuild) {
        tried.add(entry.id.toLowerCase());
        try {
          rebuilt.push(await api.build(projectId, entry.id));
        } catch (cause) {
          return refuse(`Runtime ${holder.id} holds a program of ${id} and was not built again without it, so nothing was loaded.`, cause);
        }
        if (gone()) return false;
      }
    }
    const plan = planLoad(saved, build, rebuilt, cold, status);
    if (!window.confirm(loadQuestion(plan))) return false;
    if (gone()) return false;
    let answer: ApplyAnswer;
    try {
      answer = await api.setInstances(projectId, plan.instances, cold, revision);
    } catch (cause) {
      if (refusalOf(cause).status === 409) {
        return refuse("The project's runtimes were changed since this Load read them, so nothing was loaded. Load again.", cause);
      }
      return refuse("The project's runtimes were not saved, so nothing was loaded.", cause);
    }
    if (gone()) return false;
    patch(id, { loaded: { plan, answer } });
    void usePrograms.getState().readRuntimes();
    return true;
  };

  return {
    ...EMPTY,

    load: async (projectId) => {
      opening += 1;
      echoes.clear();
      unanswered.clear();
      set({ ...EMPTY, projectId });
      await get().readList();
    },

    reset: () => {
      opening += 1;
      echoes.clear();
      unanswered.clear();
      set({ ...EMPTY });
    },

    readList: async () => {
      const projectId = get().projectId;
      if (projectId === null) return;
      const seq = (listRead += 1);
      try {
        const answer = await api.list(projectId);
        if (seq !== listRead || get().projectId !== projectId) return;
        set({ list: answer, listError: null });
      } catch (cause) {
        if (seq !== listRead || get().projectId !== projectId) return;
        // Not read is not "none": nothing is listed, and why is said.
        set({ list: null, listError: refusalOf(cause).error });
      }
    },

    readModules: async () => {
      const seq = (modulesRead += 1);
      const at = opening;
      try {
        const answer = await api.installed();
        if (seq !== modulesRead || opening !== at) return;
        set({ modules: answer.modules, modulesError: null });
      } catch (cause) {
        if (seq !== modulesRead || opening !== at) return;
        set({ modules: null, modulesError: refusalOf(cause).error });
      }
    },

    open: async (id) => {
      if (get().projectId === null) return;
      const doc = get().docs[id];
      if (doc && (doc.isNew || hasChanges(doc) || doc.phase === 'reading' || doc.busy !== null)) return;
      if (!doc) set((state) => ({ docs: { ...state.docs, [id]: fresh(id) } }));
      await reread(id);
    },

    openNew: (id) => {
      if (get().projectId === null || get().docs[id]) return;
      set((state) => ({ docs: { ...state.docs, [id]: { ...fresh(id), phase: 'ready', isNew: true } } }));
    },

    edit: (id, draft) => {
      const doc = get().docs[id];
      if (!doc || doc.phase !== 'ready') return;
      // The id is the editor's, never the form's.
      patch(id, { draft: { ...draft, id: doc.draft.id } });
    },

    discard: (id) => {
      const doc = get().docs[id];
      if (!doc || doc.busy !== null) return false;
      if (doc.isNew || doc.elsewhere === 'deleted') {
        drop(id);
        return true;
      }
      if (doc.elsewhere === 'changed' || doc.readBack !== null || doc.saved === null) {
        // The runtime last read is not known to be what is stored: it is read again.
        patch(id, { phase: 'reading', elsewhere: null, readBack: null });
        void reread(id);
        return false;
      }
      patch(id, { draft: doc.saved });
      return false;
    },

    check: async (id) => {
      const projectId = get().projectId;
      const doc = get().docs[id];
      if (projectId === null || !doc || doc.phase !== 'ready') return;
      // A draft that is not a complete body is not sent: the form says what it lacks.
      const body = sentOf(doc.draft);
      if (body === null) return;
      const seq = doc.checkSeq + 1;
      const key = draftKey(doc.draft);
      const at = opening;
      patch(id, { checkSeq: seq, checking: true });
      // An answer for an older draft than the last one sent, or of another opening, is not this draft's.
      const stale = () => opening !== at || get().docs[id]?.checkSeq !== seq;
      try {
        const answer = await api.check(projectId, id, body);
        if (stale()) return;
        const versions = versionsOfCheck(answer);
        patch(id, { check: { body: key, answer }, checkRefusal: null, checking: false, ...(versions ? { versions } : {}) });
      } catch (cause) {
        if (stale()) return;
        patch(id, { check: null, checkRefusal: refusalOf(cause), checking: false });
      }
    },

    save: async (id) => {
      const projectId = get().projectId;
      const doc = get().docs[id];
      if (projectId === null || !doc || doc.phase !== 'ready' || doc.busy !== null) return false;
      const body = sentOf(doc.draft);
      if (body === null) return false;
      const sent = draftKey(doc.draft);
      const at = opening;
      patch(id, { busy: 'saving', refusal: null, saveAnswer: null });
      unanswered.add(id);
      let answer: { runtime: RuntimeEntry; created: boolean };
      try {
        answer = await api.save(projectId, id, body);
      } catch (cause) {
        if (opening !== at) return false;
        unanswered.delete(id);
        // Refused: the draft stays, unsaved, and nothing is taken as stored.
        patch(id, { busy: null, refusal: { of: 'save', refusal: refusalOf(cause) } });
        settled(id, false);
        afterBusy(id);
        return false;
      }
      if (opening !== at) return false;
      unanswered.delete(id);
      patch(id, { isNew: false, elsewhere: null, elsewhereAs: null, saveAnswer: { created: answer.created } });
      settled(id, true);
      // What is stored is read back, and that is what is shown as saved (program names in the
      // spelling the module stores them in).
      try {
        const back = await api.getRuntime(projectId, id);
        if (opening !== at) return true;
        const now = get().docs[id];
        patch(id, {
          busy: null,
          phase: 'ready',
          readError: null,
          saved: back.runtime,
          readBack: null,
          draft: now && draftKey(now.draft) !== sent ? now.draft : back.runtime,
        });
      } catch (cause) {
        if (opening !== at) return true;
        // The save's own answer is what the module stored; that it could not be read back is said.
        const now = get().docs[id];
        patch(id, {
          busy: null,
          saved: answer.runtime,
          readBack: refusalOf(cause).error,
          draft: now && draftKey(now.draft) !== sent ? now.draft : answer.runtime,
        });
      }
      afterBusy(id);
      void get().readList();
      return true;
    },

    remove: async (id) => {
      const projectId = get().projectId;
      const doc = get().docs[id];
      if (projectId === null || !doc || doc.busy !== null || doc.isNew) return false;
      const at = opening;
      patch(id, { busy: 'deleting', refusal: null, saveAnswer: null });
      unanswered.add(id);
      try {
        await api.remove(projectId, id);
      } catch (cause) {
        if (opening !== at) return false;
        unanswered.delete(id);
        patch(id, { busy: null, refusal: { of: 'delete', refusal: refusalOf(cause) } });
        settled(id, false);
        afterBusy(id);
        return false;
      }
      if (opening !== at) return false;
      unanswered.delete(id);
      settled(id, true);
      void get().readList();
      return true;
    },

    forget: (id) => drop(id),

    loadRuntime: async (id, cold) => {
      const projectId = get().projectId;
      const doc = get().docs[id];
      if (projectId === null || !doc || doc.phase !== 'ready' || doc.busy !== null) return false;
      // A draft that is not a complete body is neither saved nor loaded: the form says what it lacks.
      if (sentOf(doc.draft) === null) return false;
      const at = opening;
      patch(id, { loaded: null, loadStopped: null, commanded: null });
      // What is built is what the module stores: the draft is saved first.
      if (isRuntimeDirty(doc)) {
        const saved = await get().save(id);
        if (opening !== at) return false;
        if (!saved) {
          patch(id, { loadStopped: { words: `Runtime ${id} was not saved, so nothing was built or loaded.`, refusal: null, problems: null } });
          return false;
        }
      }
      if (!get().docs[id] || get().docs[id]?.busy !== null) return false;
      patch(id, { busy: 'loading' });
      try {
        return await loading(projectId, id, cold, at);
      } finally {
        if (opening === at) {
          patch(id, { busy: null });
          afterBusy(id);
        }
      }
    },

    command: async (id, run) => {
      const at = opening;
      if (!get().docs[id]) return;
      patch(id, { commanded: null });
      try {
        await (run ? core.run(id) : core.stop(id));
        if (opening !== at) return;
        patch(id, { commanded: { words: `Runtime ${id} was asked to ${run ? 'run' : 'stop'}.`, stopped: null, answer: null } });
      } catch (cause) {
        if (opening !== at) return;
        patch(id, {
          commanded: {
            words: '',
            stopped: { words: `Runtime ${id} was not asked to ${run ? 'run' : 'stop'}.`, refusal: refusalOf(cause), problems: null },
            answer: null,
          },
        });
      }
      void usePrograms.getState().readRuntimes();
    },

    unload: async (id) => {
      const projectId = get().projectId;
      const doc = get().docs[id];
      if (projectId === null || !doc || doc.busy !== null) return false;
      const at = opening;
      patch(id, { busy: 'unloading', commanded: null, loaded: null, loadStopped: null });
      const stop = (words: string, cause?: unknown): false => {
        if (opening === at) {
          patch(id, {
            commanded: {
              words: '',
              stopped: { words, refusal: cause === undefined ? null : refusalOf(cause), problems: cause === undefined ? null : problemsOf(cause) },
              answer: null,
            },
          });
        }
        return false;
      };
      try {
        let saved: Instance[];
        let revision: string | undefined;
        try {
          const read = await api.config(projectId);
          saved = read.config?.instances ?? [];
          revision = read.revision;
        } catch (cause) {
          return stop(`The project's runtimes could not be read, so runtime ${id} was not taken out.`, cause);
        }
        if (opening !== at) return false;
        // The question was asked before this read, of what the controller lists: what is taken out
        // is what is stored now, and it is said below, so that a runtime saved by someone else
        // while the question was open is not taken out without a word of it.
        const stored = saved.find((instance) => instance.id === id);
        if (stored === undefined) {
          return stop(`The project's runtimes hold no runtime ${id}, so nothing was taken out.`);
        }
        let answer: ApplyAnswer;
        try {
          // Every other instance is sent back as it was read, in its order, with the revision of
          // that read when the core gave one: the core then refuses the save if another client
          // saved since.
          answer = await api.setInstances(projectId, saved.filter((instance) => instance.id !== id), false, revision);
        } catch (cause) {
          return stop(`The project's runtimes were not saved, so runtime ${id} was not taken out.`, cause);
        }
        if (opening !== at) return false;
        const held = heldBy(stored);
        const what = `As stored when it was taken out, it held ${held.length === 0 ? 'no program it names' : held.join(', ')}.`;
        patch(id, {
          commanded: {
            words: answer.applied
              ? `Runtime ${id} was taken out of the project's runtimes. ${what}`
              : `Runtime ${id} was taken out of the project's runtimes. ${what} The controller runs another project, so nothing was stopped.`,
            stopped: null,
            answer,
          },
        });
        void usePrograms.getState().readRuntimes();
        return true;
      } finally {
        if (opening === at) {
          patch(id, { busy: null });
          afterBusy(id);
        }
      }
    },

    changed: (event) => {
      // Whether the event is of the open project is asked by the caller.
      void get().readList();
      const id = docKey(event.runtime);
      const doc = get().docs[id];
      if (doc?.isNew && id !== event.runtime) {
        // Another spelling of this new runtime's id was saved or deleted elsewhere: not its change.
        patch(id, { elsewhere: event.what === 'deleted' ? 'deleted' : 'changed', elsewhereAs: event.runtime });
        return null;
      }
      // An earlier write of this editor answered 200 whose event has not come yet: the module
      // sends its events in the order it wrote, so this is that write's, even while a later
      // write is out.
      const echo = echoes.get(id) ?? 0;
      if (echo > 0) {
        echoes.set(id, echo - 1);
        return null;
      }
      if (doc?.busy && unanswered.has(id)) {
        // This editor's own write may be what it is about: decided once the write is answered.
        patch(id, { deferred: event.what });
        return null;
      }
      if (doc?.busy) {
        patch(id, { later: event.what });
        return null;
      }
      return follow(id, event.what);
    },
  };
});
