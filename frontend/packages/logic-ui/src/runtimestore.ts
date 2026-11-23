// The open project's configured runtimes as the logic module answered them, and the runtimes being
// edited. Nothing here is made up: a list that could not be read is `null` with why, never empty;
// a runtime's saved entry is what the module gave back, never what was typed; a check's answer for
// an older draft than the last one sent is dropped; a draft that is not a complete body is never
// sent. Configured is all this store knows: whether the controller runs a runtime is not read here.

import { create } from 'zustand';
import { refusalOf, type Refusal } from './library';
import { canonical } from './programs';
import {
  api,
  bodyOf,
  sameId,
  sentOf,
  newRuntime,
  type RuntimeCheck,
  type RuntimeEntry,
  type RuntimesChanged,
  type RuntimesList,
} from './runtimes';
import type { RuntimesAnswer } from './bridge';

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
  busy: 'saving' | 'deleting' | null;
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
}

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
        patch(id, { check: { body: key, answer }, checkRefusal: null, checking: false });
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
      if (doc?.busy && unanswered.has(id)) {
        // This editor's own write may be what it is about: decided once the write is answered.
        patch(id, { deferred: event.what });
        return null;
      }
      const echo = echoes.get(id) ?? 0;
      if (echo > 0) {
        echoes.set(id, echo - 1);
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
