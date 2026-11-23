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
  type RuntimeInstance,
} from './bridge';
import { findBlock, sameName, sameText } from './model';

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

/**
 * A data block opened in an editor. What is saved is not kept here: it is the block of the list
 * the module last answered (`DataState.blocks`), so an editor without changes always shows that.
 */
export interface BlockDoc {
  name: string;
  /** Opened with "Add new data block": the module has not stored it, until a save answers 200. */
  isNew: boolean;
  /** The type chosen here; `null`: as saved, or for a new block, not chosen yet. */
  type: string | null;
  /** Retain as chosen here; `null`: as saved, or for a new block, not chosen yet (no default). */
  retain: boolean | null;
  busy: 'saving' | 'deleting' | null;
  /** The module's refusal of the last save or delete. */
  refusal: { of: 'save' | 'delete'; refusal: Refusal } | null;
  /** What the last save answered. */
  saveAnswer: { created: boolean; changed: string[] | null } | null;
  /** Saved or deleted by somebody else while this had changes here. */
  elsewhere: 'changed' | 'deleted' | null;
  /** An event for this block that came while a write of this editor was in flight. */
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
  /**
   * The instances of the open project's runtimes configuration as saved (none when the project
   * has no configuration); `null` while not read, or when the read failed (`runtimesError`).
   * Configured, which is not running: whether the controller runs it is not read here.
   */
  runtimes: RuntimeInstance[] | null;
  runtimesError: string | null;
  blockDocs: Record<string, BlockDoc>;

  load(projectId: string): Promise<void>;
  reset(): void;
  readTypes(): Promise<void>;
  readBlocks(): Promise<void>;
  readRuntimes(): Promise<void>;
  /** Read type `name` into an editor, unless it is open with changes. */
  open(name: string): Promise<void>;
  /** An editor on a type the module has not stored. */
  openNew(name: string): void;
  edit(name: string, text: string): void;
  check(name: string): Promise<void>;
  save(name: string): Promise<boolean>;
  remove(name: string): Promise<boolean>;
  /** An editor on data block `name`, as the block list answered it. */
  openBlock(name: string): void;
  /** An editor on a data block the module has not stored: no type and no retain chosen. */
  openNewBlock(name: string): void;
  editBlock(name: string, change: { type?: string; retain?: boolean }): void;
  saveBlock(name: string): Promise<boolean>;
  removeBlock(name: string): Promise<boolean>;
  /** Follow `DATA_CHANGED`. Returns the editor to close (deleted elsewhere), if any. */
  changed(event: DataChanged): { what: 'type' | 'block'; name: string } | null;
}

/** Whether `doc` holds text the module has not stored as it is shown. */
export function isDirty(doc: TypeDoc): boolean {
  if (doc.saved === null) return doc.isNew || doc.readBack !== null || doc.draft !== '';
  return !sameText(doc.draft, doc.saved);
}

export const anyDirty = (state: Pick<DataState, 'docs'>): boolean => Object.values(state.docs).some(isDirty);

/** The saved block `name`, as the module last answered the list; `null` when it has none or the list is not read. */
export const savedBlock = (state: Pick<DataState, 'blocks'>, name: string): BlockAnswer | null =>
  state.blocks === null ? null : findBlock(state.blocks, name);

/** Whether `doc` holds a choice the module has not stored. */
export function isBlockDirty(doc: BlockDoc, saved: BlockAnswer | null): boolean {
  if (doc.isNew) return true;
  if (saved === null) return doc.type !== null || doc.retain !== null;
  return (doc.type !== null && !sameName(doc.type, saved.type)) || (doc.retain !== null && doc.retain !== saved.retain);
}

export const anyBlockDirty = (state: Pick<DataState, 'blocks' | 'blockDocs'>): boolean =>
  Object.values(state.blockDocs).some((doc) => isBlockDirty(doc, savedBlock(state, doc.name)));

/** The type and retain a save of `doc` sends: as chosen here, else as saved; `null` when not chosen. */
export function chosen(doc: BlockDoc, saved: BlockAnswer | null): { type: string | null; retain: boolean | null } {
  return {
    type: doc.type ?? (doc.isNew ? null : (saved?.type ?? null)),
    retain: doc.retain ?? (doc.isNew ? null : (saved?.retain ?? null)),
  };
}

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

const freshBlock = (name: string): BlockDoc => ({
  name,
  isNew: false,
  type: null,
  retain: null,
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
let runtimesRead = 0;
// Echoes of block writes are counted apart from those of types: a block and a type may share a name.
const blockKey = (name: string) => `block:${name.toUpperCase()}`;
// The types whose write is sent and not answered yet: an event heard then may be that write's,
// which only its answer tells. Once answered (its read-back may still be out), an event is
// matched against the echoes at once.
const unanswered = new Set<string>();
// Which opening of a project this is: an answer to a request of an earlier one (the project was
// closed, or another opened, while it was out) is dropped, even for an editor of the same name.
let opening = 0;

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
  /** The open editor of block `name`: by its key, else in another case. */
  const blockDocKey = (name: string): string | null => {
    const keys = Object.keys(get().blockDocs);
    return keys.find((k) => k === name) ?? keys.find((k) => sameName(k, name)) ?? null;
  };
  const patchBlock = (name: string, change: Partial<BlockDoc>) =>
    set((state) => {
      const doc = state.blockDocs[name];
      return doc ? { blockDocs: { ...state.blockDocs, [name]: { ...doc, ...change } } } : {};
    });
  const dropBlock = (name: string) =>
    set((state) => {
      const { [name]: _gone, ...blockDocs } = state.blockDocs;
      return { blockDocs };
    });
  const blocksAnswered = (blocks: BlockAnswer[]) => {
    // A read started before this answer would draw an older list over it: it is dropped.
    blocksRead += 1;
    // A write is answered only when the state after it checks.
    set({ blocks, blocksOk: true, blocksDiagnostics: [], blocksError: null });
  };

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

  /** Apply an event of somebody else's write to an open block; the list is read again by the caller. */
  const followBlock = (key: string, change: DataChanged['change']): string | null => {
    const doc = get().blockDocs[key];
    if (!doc) return null;
    if (isBlockDirty(doc, savedBlock(get(), key))) {
      patchBlock(key, { elsewhere: change === 'deleted' ? 'deleted' : 'changed' });
      return null;
    }
    if (change === 'deleted') {
      dropBlock(key);
      return key;
    }
    // Nothing chosen here: the editor shows the list as read again.
    return null;
  };

  const settledBlock = (key: string, ok: boolean) => {
    const deferred = get().blockDocs[key]?.deferred ?? null;
    patchBlock(key, { deferred: null });
    if (ok) {
      if (deferred === null) echoes.set(blockKey(key), (echoes.get(blockKey(key)) ?? 0) + 1);
    } else if (deferred !== null) {
      followBlock(key, deferred);
    }
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
    const at = opening;
    if (projectId === null || !get().docs[name]) return;
    const before = get().docs[name];
    if (before && before.phase !== 'ready') patch(name, { phase: 'reading' });
    try {
      const stored = await api.readType(projectId, name);
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
      patch(name, { phase: 'unreadable', readError: refusalOf(cause), saved: null });
    }
  };

  /**
   * After a save's read-back: an event heard while it was out, once this save's own echo was
   * matched, was somebody else's write, and is followed now.
   */
  const afterReadBack = (name: string) => {
    const later = get().docs[name]?.deferred ?? null;
    if (later === null) return;
    patch(name, { deferred: null });
    if (later === 'deleted') {
      // Not stored any more: the text shown is not saved, and saving makes the type again.
      patch(name, { elsewhere: 'deleted', saved: null });
      return;
    }
    follow(name, later);
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
    runtimes: null,
    runtimesError: null,
    blockDocs: {},

    load: async (projectId) => {
      opening += 1;
      echoes.clear();
      unanswered.clear();
      set({
        projectId,
        types: null,
        typesError: null,
        blocks: null,
        blocksOk: null,
        blocksDiagnostics: [],
        blocksError: null,
        docs: {},
        runtimes: null,
        runtimesError: null,
        blockDocs: {},
      });
      await Promise.all([get().readTypes(), get().readBlocks(), get().readRuntimes()]);
    },

    reset: () => {
      opening += 1;
      echoes.clear();
      unanswered.clear();
      set({
        projectId: null,
        types: null,
        typesError: null,
        blocks: null,
        blocksOk: null,
        blocksDiagnostics: [],
        blocksError: null,
        docs: {},
        runtimes: null,
        runtimesError: null,
        blockDocs: {},
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

    readRuntimes: async () => {
      const projectId = get().projectId;
      if (projectId === null) return;
      const seq = (runtimesRead += 1);
      try {
        const answer = await api.runtimes(projectId);
        if (seq !== runtimesRead || get().projectId !== projectId) return;
        // `null`: the project has no runtimes configuration, so no runtime carries anything.
        set({ runtimes: answer.config === null ? [] : answer.config.instances, runtimesError: null });
      } catch (cause) {
        if (seq !== runtimesRead || get().projectId !== projectId) return;
        // Not read is not "none": which runtimes carry a block is then not known.
        set({ runtimes: null, runtimesError: refusalOf(cause).error });
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
      const at = opening;
      patch(name, { checkSeq: seq, checking: true });
      try {
        const answer = await api.check(projectId, name, text);
        // An answer for an older text than the last one sent is not this text's.
        if (opening !== at || get().docs[name]?.checkSeq !== seq) return;
        patch(name, { check: { text, answer }, checkError: null, checking: false });
      } catch (cause) {
        if (opening !== at || get().docs[name]?.checkSeq !== seq) return;
        patch(name, { checkError: refusalOf(cause).error, checking: false });
      }
    },

    save: async (name) => {
      const projectId = get().projectId;
      const doc = get().docs[name];
      if (projectId === null || !doc || doc.busy !== null) return false;
      const sent = doc.draft;
      const at = opening;
      patch(name, { busy: 'saving', refusal: null, saveAnswer: null });
      unanswered.add(name);
      let answer;
      try {
        answer = await api.saveType(projectId, name, sent);
      } catch (cause) {
        // Of a project closed since: nothing of it is drawn on what is open now.
        if (opening !== at) return false;
        unanswered.delete(name);
        // Refused: the typed text stays, unsaved, and nothing is taken as stored.
        patch(name, { busy: null, refusal: { of: 'save', refusal: refusalOf(cause) } });
        settled(name, false);
        return false;
      }
      if (opening !== at) return false;
      unanswered.delete(name);
      blocksAnswered(answer.blocks);
      patch(name, { isNew: false, elsewhere: null, saveAnswer: { created: answer.created, changed: answer.changed } });
      settled(name, true);
      // The answer has no source: what is stored is read back, and that is what is shown as saved.
      try {
        const stored = await api.readType(projectId, name);
        if (opening !== at) return true;
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
        if (opening !== at) return true;
        patch(name, { busy: null, phase: 'ready', readError: null, saved: null, readBack: refusalOf(cause).error });
      }
      afterReadBack(name);
      if (answer.created) void get().readTypes();
      return true;
    },

    remove: async (name) => {
      const projectId = get().projectId;
      const doc = get().docs[name];
      if (projectId === null || !doc || doc.busy !== null) return false;
      const at = opening;
      patch(name, { busy: 'deleting', refusal: null, saveAnswer: null });
      unanswered.add(name);
      let answer;
      try {
        answer = await api.deleteType(projectId, name);
      } catch (cause) {
        if (opening !== at) return false;
        unanswered.delete(name);
        patch(name, { busy: null, refusal: { of: 'delete', refusal: refusalOf(cause) } });
        settled(name, false);
        return false;
      }
      if (opening !== at) return false;
      unanswered.delete(name);
      settled(name, true);
      blocksAnswered(answer.blocks);
      drop(name);
      void get().readTypes();
      return true;
    },

    openBlock: (name) => {
      if (get().projectId === null || blockDocKey(name) !== null) return;
      set((state) => ({ blockDocs: { ...state.blockDocs, [name]: freshBlock(name) } }));
    },

    openNewBlock: (name) => {
      if (get().projectId === null || blockDocKey(name) !== null) return;
      set((state) => ({ blockDocs: { ...state.blockDocs, [name]: { ...freshBlock(name), isNew: true, type: null, retain: null } } }));
    },

    editBlock: (name, change) => patchBlock(name, change),

    saveBlock: async (name) => {
      const projectId = get().projectId;
      const doc = get().blockDocs[name];
      if (projectId === null || !doc || doc.busy !== null) return false;
      const { type, retain } = chosen(doc, savedBlock(get(), name));
      // The module asks both, and so does this editor: nothing is chosen for the user.
      if (type === null || retain === null) return false;
      const at = opening;
      patchBlock(name, { busy: 'saving', refusal: null, saveAnswer: null });
      let answer;
      try {
        answer = await api.saveBlock(projectId, name, type, retain);
      } catch (cause) {
        if (opening !== at) return false;
        // Refused: what was chosen stays, unsaved; the compiler's reasons are shown.
        patchBlock(name, { busy: null, refusal: { of: 'save', refusal: refusalOf(cause) } });
        settledBlock(name, false);
        return false;
      }
      if (opening !== at) return false;
      blocksAnswered(answer.blocks); // R5: the module's list after the save, not this request
      patchBlock(name, {
        busy: null,
        isNew: false,
        type: null,
        retain: null,
        elsewhere: null,
        saveAnswer: { created: answer.created, changed: answer.changed },
      });
      settledBlock(name, true);
      return true;
    },

    removeBlock: async (name) => {
      const projectId = get().projectId;
      const doc = get().blockDocs[name];
      if (projectId === null || !doc || doc.busy !== null) return false;
      const at = opening;
      patchBlock(name, { busy: 'deleting', refusal: null, saveAnswer: null });
      let answer;
      try {
        answer = await api.deleteBlock(projectId, name);
      } catch (cause) {
        if (opening !== at) return false;
        patchBlock(name, { busy: null, refusal: { of: 'delete', refusal: refusalOf(cause) } });
        settledBlock(name, false);
        return false;
      }
      if (opening !== at) return false;
      settledBlock(name, true);
      blocksAnswered(answer.blocks);
      dropBlock(name);
      return true;
    },

    changed: (event) => {
      if (get().projectId === null || event.project !== get().projectId) return null;
      void get().readTypes();
      void get().readBlocks();
      if (event.what === 'block') {
        const key = blockDocKey(event.name);
        if (key === null) return null;
        if (get().blockDocs[key]?.busy) {
          patchBlock(key, { deferred: event.change });
          return null;
        }
        const own = echoes.get(blockKey(key)) ?? 0;
        if (own > 0) {
          if (own === 1) echoes.delete(blockKey(key));
          else echoes.set(blockKey(key), own - 1);
          return null;
        }
        const close = followBlock(key, event.change);
        return close === null ? null : { what: 'block', name: close };
      }
      const name = event.name;
      const doc = get().docs[name];
      if (doc?.busy && unanswered.has(name)) {
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
      if (doc?.busy) {
        // Somebody else's write, heard while this editor's save is read back: followed after it.
        patch(name, { deferred: event.change });
        return null;
      }
      const close = follow(name, event.change);
      return close === null ? null : { what: 'type', name: close };
    },
  };
});
