// The open project's user types and data blocks as the data module answered them, and the
// types being edited. Nothing here is made up: a list that could not be read is `null` with
// why, never empty; a type's saved text is what the module gave back, never what was typed.

import { create } from 'zustand';
import {
  api,
  refusalOf,
  type BlockAnswer,
  type CheckAnswer,
  type DataChanged,
  type Diagnostic,
  type Refusal,
} from './bridge';
import { sameText } from './model';

/** A user type opened in an editor. */
export interface TypeDoc {
  name: string;
  /** `reading`: its text is being read; `ready`: read, or new; `unreadable`: the read failed. */
  phase: 'reading' | 'ready' | 'unreadable';
  /** Why the read failed. A 500 is a file that is not text, which a save may replace. */
  readError: Refusal | null;
  /** Opened with "Add new type": the module has not stored it, until a save answers 200. */
  isNew: boolean;
  /** The stored text, as the module last gave it; `null` when not known. */
  saved: string | null;
  /** A save answered 200 but the stored text could not be read back: why. */
  readBack: string | null;
  /** The text in the editor. */
  draft: string;
  /** The number of the last check asked for; an answer to an earlier one is dropped. */
  checkSeq: number;
  /** The last check that was applied, and the text it checked. */
  check: { text: string; answer: CheckAnswer } | null;
  checkError: string | null;
  checking: boolean;
  busy: 'saving' | 'deleting' | null;
  /** The module's refusal of the last save or delete. */
  refusal: { of: 'save' | 'delete'; refusal: Refusal } | null;
  /** What the last save answered. */
  saveAnswer: { created: boolean; changed: string[] | null } | null;
  /** Saved or deleted by somebody else while this had changes here. */
  elsewhere: 'changed' | 'deleted' | null;
  /** An event for this type that came while a write of this editor was in flight. */
  deferred: DataChanged['change'] | null;
}

export interface DataState {
  projectId: string | null;
  /** The types the module listed; `null` while not read, or when the read failed (`typesError`). */
  types: string[] | null;
  typesError: string | null;
  /** The blocks as the module last answered them; `null` while not read, or when the read failed. */
  blocks: BlockAnswer[] | null;
  /** Whether the state checked when the blocks were answered; `null` with no answer. */
  blocksOk: boolean | null;
  blocksDiagnostics: Diagnostic[];
  blocksError: string | null;
  docs: Record<string, TypeDoc>;

  load(projectId: string): Promise<void>;
  reset(): void;
  readTypes(): Promise<void>;
  readBlocks(): Promise<void>;
  /** Read type `name` into an editor, unless it is open with changes. */
  open(name: string): Promise<void>;
  /** An editor on a type the module has not stored. */
  openNew(name: string): void;
  edit(name: string, text: string): void;
  check(name: string): Promise<void>;
  save(name: string): Promise<boolean>;
  remove(name: string): Promise<boolean>;
  /** Follow `DATA_CHANGED`. Returns the type whose editor is to close (deleted elsewhere), if any. */
  changed(event: DataChanged): string | null;
}

/** Whether `doc` holds text the module has not stored as it is shown. */
export function isDirty(doc: TypeDoc): boolean {
  if (doc.saved === null) return doc.isNew || doc.readBack !== null || doc.draft !== '';
  return !sameText(doc.draft, doc.saved);
}

export const anyDirty = (state: Pick<DataState, 'docs'>): boolean => Object.values(state.docs).some(isDirty);

const fresh = (name: string): TypeDoc => ({
  name,
  phase: 'reading',
  readError: null,
  isNew: false,
  saved: null,
  readBack: null,
  draft: '',
  checkSeq: 0,
  check: null,
  checkError: null,
  checking: false,
  busy: null,
  refusal: null,
  saveAnswer: null,
  elsewhere: null,
  deferred: null,
});

/** What a new type's editor starts with: the one declaration the module asks of its file. */
export const newTypeText = (name: string): string => `TYPE ${name} :\nSTRUCT\nEND_STRUCT;\nEND_TYPE\n`;

// The events of this workbench's own writes, still to come: one per write answered 200 whose
// event has not been heard yet. Not state: nothing is drawn from it.
const echoes = new Map<string, number>();
// Numbered reads of each list: an answer for an earlier read, or another project, is dropped.
let typesRead = 0;
let blocksRead = 0;

export const useData = create<DataState>((set, get) => {
  const patch = (name: string, change: Partial<TypeDoc>) =>
    set((state) => {
      const doc = state.docs[name];
      return doc ? { docs: { ...state.docs, [name]: { ...doc, ...change } } } : {};
    });
  const drop = (name: string) =>
    set((state) => {
      const { [name]: _gone, ...docs } = state.docs;
      return { docs };
    });
  const blocksAnswered = (blocks: BlockAnswer[]) =>
    // A write is answered only when the state after it checks.
    set({ blocks, blocksOk: true, blocksDiagnostics: [], blocksError: null });

  /** Apply an event of somebody else's write to an open type. */
  const follow = (name: string, change: DataChanged['change']): string | null => {
    const doc = get().docs[name];
    if (!doc) return null;
    if (isDirty(doc)) {
      // What is typed here is kept: it is said that saving replaces the other change.
      patch(name, { elsewhere: change === 'deleted' ? 'deleted' : 'changed' });
      return null;
    }
    if (change === 'deleted') {
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
      follow(name, deferred);
    }
  };

  const reread = async (name: string) => {
    const projectId = get().projectId;
    if (projectId === null || !get().docs[name]) return;
    const before = get().docs[name];
    if (before && before.phase !== 'ready') patch(name, { phase: 'reading' });
    try {
      const stored = await api.readType(projectId, name);
      if (get().projectId !== projectId) return;
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
      if (get().projectId !== projectId) return;
      patch(name, { phase: 'unreadable', readError: refusalOf(cause), saved: null });
    }
  };

  return {
    projectId: null,
    types: null,
    typesError: null,
    blocks: null,
    blocksOk: null,
    blocksDiagnostics: [],
    blocksError: null,
    docs: {},

    load: async (projectId) => {
      echoes.clear();
      set({
        projectId,
        types: null,
        typesError: null,
        blocks: null,
        blocksOk: null,
        blocksDiagnostics: [],
        blocksError: null,
        docs: {},
      });
      await Promise.all([get().readTypes(), get().readBlocks()]);
    },

    reset: () => {
      echoes.clear();
      set({
        projectId: null,
        types: null,
        typesError: null,
        blocks: null,
        blocksOk: null,
        blocksDiagnostics: [],
        blocksError: null,
        docs: {},
      });
    },

    readTypes: async () => {
      const projectId = get().projectId;
      if (projectId === null) return;
      const seq = (typesRead += 1);
      try {
        const answer = await api.types(projectId);
        if (seq !== typesRead || get().projectId !== projectId) return;
        set({ types: answer.types.map((t) => t.name), typesError: null });
      } catch (cause) {
        if (seq !== typesRead || get().projectId !== projectId) return;
        set({ types: null, typesError: refusalOf(cause).error });
      }
    },

    readBlocks: async () => {
      const projectId = get().projectId;
      if (projectId === null) return;
      const seq = (blocksRead += 1);
      try {
        const answer = await api.blocks(projectId);
        if (seq !== blocksRead || get().projectId !== projectId) return;
        set({ blocks: answer.blocks, blocksOk: answer.ok, blocksDiagnostics: answer.diagnostics, blocksError: null });
      } catch (cause) {
        if (seq !== blocksRead || get().projectId !== projectId) return;
        set({ blocks: null, blocksOk: null, blocksDiagnostics: [], blocksError: refusalOf(cause).error });
      }
    },

    open: async (name) => {
      if (get().projectId === null) return;
      const doc = get().docs[name];
      if (doc && (doc.isNew || isDirty(doc) || doc.phase === 'reading')) return;
      if (!doc) set((state) => ({ docs: { ...state.docs, [name]: fresh(name) } }));
      await reread(name);
    },

    openNew: (name) => {
      if (get().docs[name]) return;
      set((state) => ({
        docs: { ...state.docs, [name]: { ...fresh(name), phase: 'ready', isNew: true, draft: newTypeText(name) } },
      }));
    },

    edit: (name, text) => patch(name, { draft: text }),

    check: async (name) => {
      const projectId = get().projectId;
      const doc = get().docs[name];
      if (projectId === null || !doc) return;
      const seq = doc.checkSeq + 1;
      const text = doc.draft;
      patch(name, { checkSeq: seq, checking: true });
      try {
        const answer = await api.check(projectId, name, text);
        // An answer for an older text than the last one sent is not this text's.
        if (get().docs[name]?.checkSeq !== seq) return;
        patch(name, { check: { text, answer }, checkError: null, checking: false });
      } catch (cause) {
        if (get().docs[name]?.checkSeq !== seq) return;
        patch(name, { checkError: refusalOf(cause).error, checking: false });
      }
    },

    save: async (name) => {
      const projectId = get().projectId;
      const doc = get().docs[name];
      if (projectId === null || !doc || doc.busy !== null) return false;
      const sent = doc.draft;
      patch(name, { busy: 'saving', refusal: null, saveAnswer: null });
      let answer;
      try {
        answer = await api.saveType(projectId, name, sent);
      } catch (cause) {
        // Refused: the typed text stays, unsaved, and nothing is taken as stored.
        patch(name, { busy: null, refusal: { of: 'save', refusal: refusalOf(cause) } });
        settled(name, false);
        return false;
      }
      if (get().projectId !== projectId) return false;
      blocksAnswered(answer.blocks);
      patch(name, { isNew: false, elsewhere: null, saveAnswer: { created: answer.created, changed: answer.changed } });
      settled(name, true);
      // The answer has no source: what is stored is read back, and that is what is shown as saved.
      try {
        const stored = await api.readType(projectId, name);
        if (get().projectId !== projectId) return true;
        const now = get().docs[name];
        patch(name, {
          busy: null,
          phase: 'ready',
          readError: null,
          saved: stored.source,
          readBack: null,
          draft: now && now.draft !== sent ? now.draft : stored.source,
        });
      } catch (cause) {
        patch(name, { busy: null, phase: 'ready', readError: null, saved: null, readBack: refusalOf(cause).error });
      }
      if (answer.created) void get().readTypes();
      return true;
    },

    remove: async (name) => {
      const projectId = get().projectId;
      const doc = get().docs[name];
      if (projectId === null || !doc || doc.busy !== null) return false;
      patch(name, { busy: 'deleting', refusal: null, saveAnswer: null });
      let answer;
      try {
        answer = await api.deleteType(projectId, name);
      } catch (cause) {
        patch(name, { busy: null, refusal: { of: 'delete', refusal: refusalOf(cause) } });
        settled(name, false);
        return false;
      }
      if (get().projectId !== projectId) return false;
      settled(name, true);
      blocksAnswered(answer.blocks);
      drop(name);
      void get().readTypes();
      return true;
    },

    changed: (event) => {
      if (get().projectId === null || event.project !== get().projectId) return null;
      void get().readTypes();
      void get().readBlocks();
      if (event.what !== 'type') return null;
      const name = event.name;
      const doc = get().docs[name];
      if (doc?.busy) {
        // This editor's own write may be what it is about: decided once the write is answered.
        patch(name, { deferred: event.change });
        return null;
      }
      const own = echoes.get(name) ?? 0;
      if (own > 0) {
        if (own === 1) echoes.delete(name);
        else echoes.set(name, own - 1);
        return null;
      }
      return follow(name, event.change);
    },
  };
});
