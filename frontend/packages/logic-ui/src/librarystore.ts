// The open project's library as the logic module answered it, and the sources being edited.
// Nothing here is made up: a list that could not be read is `null` with why, never empty; a
// source's saved text is what the module gave back, never what was typed; a check's answer for
// an older text than the last one sent is dropped.

import { create } from 'zustand';
import { libraryApi as api, refusalOf, sameName, sameText, type LibraryChanged, type LibraryCheck, type Refusal } from './library';

/** A library source opened in an editor. */
export interface LibraryDoc {
  name: string;
  /** `reading`: its text is being read; `ready`: read, or new; `unreadable`: the read failed. */
  phase: 'reading' | 'ready' | 'unreadable';
  /** Why the read failed. A 500 is a file that is not text (or not readable), which a save may replace. */
  readError: Refusal | null;
  /** Opened with "Add new source": the module has not stored it, until a save answers 200. */
  isNew: boolean;
  /** The stored text, as the module last gave it; `null` when not known. */
  saved: string | null;
  /** A save answered 200 but the stored text could not be read back: why. */
  readBack: string | null;
  /** The text in the editor. */
  draft: string;
  /** The number of the last check asked for; an answer to an earlier one is dropped. */
  checkSeq: number;
  /**
   * The last check that was applied, and the text it checked. `earlierTypes`: the project's
   * types or data blocks changed (`DATA_CHANGED`) after it was asked, so it is a check with
   * earlier ones.
   */
  check: { text: string; answer: LibraryCheck; earlierTypes: boolean } | null;
  /** The module refused the last check (its name rule, another spelling) or it could not be made. */
  checkRefusal: Refusal | null;
  checking: boolean;
  /** `saving` from the check before the save until the read-back is in. */
  busy: 'saving' | 'deleting' | null;
  /** The module's refusal of the last save (or of its check) or delete. */
  refusal: { of: 'save' | 'delete'; refusal: Refusal } | null;
  /** The last save was held: with its text the library does not check. */
  held: boolean;
  /** What the last save answered: whether the module made the source. */
  saveAnswer: { created: boolean } | null;
  /** Saved or deleted by somebody else while this had changes here. */
  elsewhere: 'changed' | 'deleted' | null;
  /** An event for this source that came while a write of this editor was sent and not answered. */
  deferred: LibraryChanged['what'] | null;
  /** An event of somebody else's write that came while this editor was busy: followed after. */
  later: LibraryChanged['what'] | null;
}

/**
 * The last check of the library as it is stored (`POST /library/check {}`): the module's answer,
 * or why it could not be made.
 */
export type StoredCheck = { answer: LibraryCheck; error: null } | { answer: null; error: string };

export interface LibraryState {
  projectId: string | null;
  /** The sources the module listed, in its order; `null` while not read, or when the read failed. */
  names: string[] | null;
  namesError: string | null;
  docs: Record<string, LibraryDoc>;
  /** A place to put the caret in, once the editor of `name` shows its text. */
  pending: { name: string; line: number; col: number } | null;
  /**
   * The stored library's last check; `null` while none has answered since the list was read or
   * the library or the project's types last changed.
   */
  stored: StoredCheck | null;

  load(projectId: string): Promise<void>;
  reset(): void;
  readList(): Promise<void>;
  /** Read source `name` into an editor, unless it is open with changes. */
  open(name: string): Promise<void>;
  /** An editor on a source the module has not stored, with an empty text. Writes nothing. */
  openNew(name: string): void;
  edit(name: string, text: string): void;
  /** The draft back to the saved text. A new source is dropped: returns true then. */
  discard(name: string): boolean;
  check(name: string): Promise<void>;
  /** Check the draft, and save it when the library checks with it, or `anyway`. */
  save(name: string, anyway?: boolean): Promise<boolean>;
  remove(name: string): Promise<boolean>;
  /** Put the caret at `line`:`col` of source `name` once its editor shows it. */
  goTo(name: string, line: number, col: number): void;
  /** The pending place was gone to (or cannot be). */
  wentTo(): void;
  /** Follow `LOGIC_LIBRARY_CHANGED` of the open project. Returns the editor to close (deleted elsewhere), if any. */
  changed(event: LibraryChanged): string | null;
  /** Check the library as it is stored. */
  checkStored(): Promise<void>;
  /** The data module changed the open project's types or blocks: every check made before is of earlier ones. */
  dataChanged(): void;
}

/** Whether `doc` holds text the module has not stored as it is shown. */
export function isDirty(doc: LibraryDoc): boolean {
  if (doc.saved === null) return doc.isNew || doc.readBack !== null || doc.draft !== '';
  return !sameText(doc.draft, doc.saved);
}

export const anyDirty = (state: Pick<LibraryState, 'docs'>): boolean => Object.values(state.docs).some(isDirty);

const fresh = (name: string): LibraryDoc => ({
  name,
  phase: 'reading',
  readError: null,
  isNew: false,
  saved: null,
  readBack: null,
  draft: '',
  checkSeq: 0,
  check: null,
  checkRefusal: null,
  checking: false,
  busy: null,
  refusal: null,
  held: false,
  saveAnswer: null,
  elsewhere: null,
  deferred: null,
  later: null,
});

// The events of this workbench's own writes, still to come: one per write answered 200 whose
// event has not been heard yet. Not state: nothing is drawn from it.
const echoes = new Map<string, number>();
// The sources whose write is sent and not answered yet: an event heard then may be that write's,
// which only its answer tells.
const unanswered = new Set<string>();
// Numbered reads of the list: an answer for an earlier read is dropped.
let listRead = 0;
// Which opening of a project this is: an answer to a request of an earlier one is dropped.
let opening = 0;
// Numbered checks of the stored library: an answer for an earlier one is dropped.
let storedAsked = 0;
// How many times the project's types and blocks were heard to change: a check asked before the
// last change is of earlier ones.
let dataChanges = 0;

const EMPTY = { projectId: null, names: null, namesError: null, docs: {}, pending: null, stored: null };

export const useLibrary = create<LibraryState>((set, get) => {
  const patch = (name: string, change: Partial<LibraryDoc>) =>
    set((state) => {
      const doc = state.docs[name];
      return doc ? { docs: { ...state.docs, [name]: { ...doc, ...change } } } : {};
    });
  const drop = (name: string) =>
    set((state) => {
      const { [name]: _gone, ...docs } = state.docs;
      return { docs };
    });
  /** The open editor of source `name`: by its key, else in another case. */
  const docKey = (name: string): string => {
    const keys = Object.keys(get().docs);
    return keys.find((k) => k === name) ?? keys.find((k) => sameName(k, name)) ?? name;
  };

  const reread = async (name: string) => {
    const projectId = get().projectId;
    const at = opening;
    if (projectId === null || !get().docs[name]) return;
    try {
      const stored = await api.readSource(projectId, name);
      if (opening !== at) return;
      const doc = get().docs[name];
      if (!doc) return;
      // Typed while it was being read: kept, now measured against what is stored.
      const keep = doc.phase === 'ready' && isDirty(doc);
      patch(name, {
        phase: 'ready',
        readError: null,
        isNew: false,
        saved: stored.source,
        readBack: null,
        draft: keep ? doc.draft : stored.source,
      });
    } catch (cause) {
      if (opening !== at) return;
      patch(name, { phase: 'unreadable', readError: refusalOf(cause), saved: null, draft: '' });
    }
  };

  /** Apply an event of somebody else's write to an open source. */
  const follow = (name: string, what: LibraryChanged['what']): string | null => {
    const doc = get().docs[name];
    if (!doc) return null;
    if (isDirty(doc)) {
      // What is typed here is kept: it is said that saving replaces the other change.
      patch(name, what === 'deleted' ? { elsewhere: 'deleted', saved: null } : { elsewhere: 'changed' });
      return null;
    }
    if (what === 'deleted') {
      drop(name);
      return name;
    }
    void reread(name);
    return null;
  };

  /** After a write of `name` was answered: its event, if it came first, was this write's. */
  const settled = (name: string, ok: boolean) => {
    const deferred = get().docs[name]?.deferred ?? null;
    patch(name, { deferred: null });
    if (ok) {
      if (deferred === null) echoes.set(name, (echoes.get(name) ?? 0) + 1);
    } else if (deferred !== null) {
      // A refused write sends nothing: what came was somebody else's.
      patch(name, { later: deferred });
    }
  };

  /** Once not busy: an event of somebody else's write heard meanwhile is followed now. */
  const afterBusy = (name: string) => {
    const later = get().docs[name]?.later ?? null;
    if (later === null) return;
    patch(name, { later: null });
    const doc = get().docs[name];
    if (later === 'deleted' && doc && !isDirty(doc)) {
      // Not stored any more: the text shown is not saved, and saving makes the source again.
      patch(name, { elsewhere: 'deleted', saved: null });
      return;
    }
    follow(name, later);
  };

  /** Send the `PUT` of `sent`, then read back what is stored. */
  const write = async (projectId: string, name: string, sent: string, at: number): Promise<boolean> => {
    unanswered.add(name);
    let answer;
    try {
      answer = await api.save(projectId, name, sent);
    } catch (cause) {
      if (opening !== at) return false;
      unanswered.delete(name);
      // Refused: the typed text stays, unsaved, and nothing is taken as stored.
      patch(name, { busy: null, refusal: { of: 'save', refusal: refusalOf(cause) } });
      settled(name, false);
      afterBusy(name);
      return false;
    }
    if (opening !== at) return false;
    unanswered.delete(name);
    patch(name, { isNew: false, elsewhere: null, held: false, saveAnswer: { created: answer.created } });
    settled(name, true);
    // The answer has no source: what is stored is read back, and that is what is shown as saved.
    try {
      const back = await api.readSource(projectId, name);
      if (opening !== at) return true;
      const now = get().docs[name];
      patch(name, {
        busy: null,
        phase: 'ready',
        readError: null,
        saved: back.source,
        readBack: null,
        draft: now && now.draft !== sent ? now.draft : back.source,
      });
    } catch (cause) {
      if (opening !== at) return true;
      patch(name, { busy: null, phase: 'ready', readError: null, saved: null, readBack: refusalOf(cause).error });
    }
    afterBusy(name);
    if (answer.created) void get().readList();
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
      // What every program is compiled with: whether it checks as stored, said on the tree.
      if (get().projectId === projectId && get().names !== null) await get().checkStored();
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
        set({ names: answer.sources.map((s) => s.name), namesError: null });
      } catch (cause) {
        if (seq !== listRead || get().projectId !== projectId) return;
        // Not read is not "none": nothing is listed, and why is said.
        set({ names: null, namesError: refusalOf(cause).error });
      }
    },

    open: async (name) => {
      if (get().projectId === null) return;
      const doc = get().docs[name];
      if (doc && (doc.isNew || isDirty(doc) || doc.phase === 'reading' || doc.busy !== null)) return;
      if (!doc) set((state) => ({ docs: { ...state.docs, [name]: fresh(name) } }));
      await reread(name);
    },

    openNew: (name) => {
      if (get().projectId === null || get().docs[name]) return;
      set((state) => ({ docs: { ...state.docs, [name]: { ...fresh(name), phase: 'ready', isNew: true } } }));
    },

    edit: (name, text) => patch(name, { draft: text }),

    discard: (name) => {
      const doc = get().docs[name];
      if (!doc || doc.busy !== null) return false;
      if (doc.isNew) {
        drop(name);
        return true;
      }
      patch(name, { draft: doc.saved ?? '', held: false });
      return false;
    },

    check: async (name) => {
      const projectId = get().projectId;
      const doc = get().docs[name];
      if (projectId === null || !doc) return;
      const seq = doc.checkSeq + 1;
      const text = doc.draft;
      const at = opening;
      const data = dataChanges;
      patch(name, { checkSeq: seq, checking: true });
      // An answer for an older text than the last one sent, or of another opening, is not this text's.
      const stale = () => opening !== at || get().docs[name]?.checkSeq !== seq;
      try {
        const answer = await api.check(projectId, name, text);
        if (stale()) return;
        patch(name, { check: { text, answer, earlierTypes: data !== dataChanges }, checkRefusal: null, checking: false });
      } catch (cause) {
        if (stale()) return;
        patch(name, { checkRefusal: refusalOf(cause), checking: false });
      }
    },

    save: async (name, anyway = false) => {
      const projectId = get().projectId;
      const doc = get().docs[name];
      if (projectId === null || !doc || doc.busy !== null) return false;
      const sent = doc.draft;
      const at = opening;
      patch(name, { busy: 'saving', refusal: null, held: false, saveAnswer: null });
      if (!anyway) {
        // The module writes any text, and a library that does not check refuses every program:
        // the text is checked first, as the module asks of a caller.
        const seq = doc.checkSeq + 1;
        const data = dataChanges;
        patch(name, { checkSeq: seq, checking: true });
        let checked: LibraryCheck;
        try {
          checked = await api.check(projectId, name, sent);
        } catch (cause) {
          if (opening !== at) return false;
          // Said once, as the save's refusal: the check was asked for by the save.
          if (get().docs[name]?.checkSeq === seq) patch(name, { checking: false });
          patch(name, { busy: null, refusal: { of: 'save', refusal: refusalOf(cause) } });
          afterBusy(name);
          return false;
        }
        if (opening !== at) return false;
        if (get().docs[name]?.checkSeq === seq) {
          patch(name, { check: { text: sent, answer: checked, earlierTypes: data !== dataChanges }, checkRefusal: null, checking: false });
        }
        if (!checked.ok) {
          patch(name, { busy: null, held: true });
          afterBusy(name);
          return false;
        }
      }
      return write(projectId, name, sent, at);
    },

    remove: async (name) => {
      const projectId = get().projectId;
      const doc = get().docs[name];
      if (projectId === null || !doc || doc.busy !== null) return false;
      const at = opening;
      patch(name, { busy: 'deleting', refusal: null, held: false, saveAnswer: null });
      unanswered.add(name);
      try {
        await api.remove(projectId, name);
      } catch (cause) {
        if (opening !== at) return false;
        unanswered.delete(name);
        patch(name, { busy: null, refusal: { of: 'delete', refusal: refusalOf(cause) } });
        settled(name, false);
        afterBusy(name);
        return false;
      }
      if (opening !== at) return false;
      unanswered.delete(name);
      settled(name, true);
      drop(name);
      void get().readList();
      return true;
    },

    goTo: (name, line, col) => set({ pending: { name, line, col } }),
    wentTo: () => set({ pending: null }),

    changed: (event) => {
      // Whether the event is of the open project is asked by the caller.
      void get().readList();
      void get().checkStored();
      const name = docKey(event.name);
      const doc = get().docs[name];
      if (doc?.busy && unanswered.has(name)) {
        // This editor's own write may be what it is about: decided once the write is answered.
        patch(name, { deferred: event.what });
        return null;
      }
      const echo = echoes.get(name) ?? 0;
      if (echo > 0) {
        echoes.set(name, echo - 1);
        return null;
      }
      if (doc?.busy) {
        // Somebody else's write, heard while this editor checks before a save or reads one back.
        patch(name, { later: event.what });
        return null;
      }
      return follow(name, event.what);
    },

    checkStored: async () => {
      const projectId = get().projectId;
      if (projectId === null) return;
      const seq = (storedAsked += 1);
      const at = opening;
      // What was said of the library before is not said of it now: nothing until this answers.
      set({ stored: null });
      const newest = () => seq === storedAsked && opening === at;
      try {
        const answer = await api.checkStored(projectId);
        if (newest()) set({ stored: { answer, error: null } });
      } catch (cause) {
        // Not checked is never "checks": the module's words, and no verdict.
        if (newest()) set({ stored: { answer: null, error: refusalOf(cause).error } });
      }
    },

    dataChanged: () => {
      dataChanges += 1;
      set((state) => ({
        docs: Object.fromEntries(
          Object.entries(state.docs).map(([name, doc]) => [
            name,
            doc.check === null ? doc : { ...doc, check: { ...doc.check, earlierTypes: true } },
          ])
        ),
      }));
      void get().checkStored();
    },
  };
});
