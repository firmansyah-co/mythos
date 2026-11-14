// The open project's HMI screens: what the controller has stored, the engineer's draft of each,
// and what was last released to operators.

import { create } from 'zustand';
import { api } from '../api/client';
import { log } from './log';
import {
  newIoField,
  newTextElement,
  parseCanvas,
  place,
  serializeCanvas,
  type KnownElement,
  type ScreenCanvas,
} from './screenModel';

export interface ScreenDoc {
  id: string;
  name: string;
  /** The draft being edited. */
  canvas: ScreenCanvas;
  /** The draft as the controller has it, serialized, to tell whether it has changed. */
  baseline: string;
}

export type ScreensPhase = 'idle' | 'loading' | 'ready' | 'unreadable';
export type ReleaseState = 'never' | 'current' | 'differs' | 'unknown';

export interface ScreensState {
  projectId: string | null;
  phase: ScreensPhase;
  loadError: string | null;
  screens: ScreenDoc[];
  /** Each released screen's canvas, serialized, by id. `null` when it could not be read. */
  released: Map<string, string> | null;
  /** The request in flight, if any. */
  busy: 'saving' | 'releasing' | 'creating' | 'removing' | null;
  /** Why the last request failed. */
  error: string | null;

  load: (projectId: string) => Promise<void>;
  reset: () => void;
  create: (name: string) => Promise<string | null>;
  remove: (screenId: string) => Promise<boolean>;
  addElement: (screenId: string, kind: 'text' | 'io_field') => string | null;
  updateElement: (screenId: string, elementId: string, change: Partial<KnownElement>) => void;
  removeElement: (screenId: string, elementId: string) => void;
  discard: (screenId: string) => void;
  save: (screenId: string) => Promise<boolean>;
  release: () => Promise<boolean>;
}

/**
 * A value as text with every object's keys in one fixed order.
 *
 * Two canvases are compared by content. The controller stores JSON objects with their keys
 * sorted, so the same canvas comes back in a different key order than it was sent in, and a
 * plain `JSON.stringify` comparison would call an identical screen different.
 */
export function canonical(value: unknown): string {
  return JSON.stringify(value, (_key, inner: unknown) =>
    inner && typeof inner === 'object' && !Array.isArray(inner)
      ? Object.fromEntries(
          Object.entries(inner as Record<string, unknown>).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        )
      : inner
  );
}

const fingerprint = (canvas: ScreenCanvas) => canonical(serializeCanvas(canvas));

export function isScreenDirty(doc: ScreenDoc): boolean {
  return fingerprint(doc.canvas) !== doc.baseline;
}

export function anyScreenDirty(state: Pick<ScreensState, 'screens'>): boolean {
  return state.screens.some(isScreenDirty);
}

/**
 * How a screen's saved draft compares with what operators have.
 *
 * Compared against the saved draft, not the one being edited: a release publishes what is
 * saved, so unsaved edits are not part of the answer.
 */
export function releaseState(doc: ScreenDoc, released: Map<string, string> | null): ReleaseState {
  if (released === null) return 'unknown';
  const live = released.get(doc.id);
  if (live === undefined) return 'never';
  return live === doc.baseline ? 'current' : 'differs';
}

const EMPTY = {
  projectId: null,
  phase: 'idle' as ScreensPhase,
  loadError: null,
  screens: [] as ScreenDoc[],
  released: null as Map<string, string> | null,
  busy: null,
  error: null,
};

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

/** A stored canvas in the form this workbench saves it, so that an unedited screen is clean. */
const normalized = (raw: unknown) => fingerprint(parseCanvas(raw));

export const useScreens = create<ScreensState>((set, get) => {
  const edit = (screenId: string, change: (canvas: ScreenCanvas) => ScreenCanvas) =>
    set((state) => ({
      screens: state.screens.map((doc) =>
        doc.id === screenId ? { ...doc, canvas: change(doc.canvas) } : doc
      ),
    }));

  return {
    ...EMPTY,

    load: async (projectId) => {
      set({ ...EMPTY, projectId, phase: 'loading' });
      try {
        const [draft, active] = await Promise.all([
          api.listScreens(projectId, 'draft'),
          // What is released is a separate question from what is drafted. If it cannot be read,
          // the drafts are still shown and the release state is reported as unknown.
          api.listScreens(projectId, 'active').catch(() => null),
        ]);
        if (get().projectId !== projectId) return;
        set({
          phase: 'ready',
          screens: draft.map((screen) => {
            const canvas = parseCanvas(screen.canvasData);
            return { id: screen.id, name: screen.name, canvas, baseline: fingerprint(canvas) };
          }),
          released: active && new Map(active.map((s) => [s.id, normalized(s.canvasData)])),
        });
      } catch (cause) {
        if (get().projectId !== projectId) return;
        set({ phase: 'unreadable', loadError: message(cause) });
      }
    },

    reset: () => set({ ...EMPTY }),

    create: async (name) => {
      const { projectId } = get();
      if (projectId === null) return null;
      set({ busy: 'creating', error: null });
      try {
        const screen = await api.createScreen(projectId, name);
        if (get().projectId !== projectId) return null;
        const canvas = parseCanvas(screen.canvasData);
        set((state) => ({
          busy: null,
          screens: [
            ...state.screens,
            { id: screen.id, name: screen.name, canvas, baseline: fingerprint(canvas) },
          ],
        }));
        log.info(`Screen “${screen.name}” created.`);
        return screen.id;
      } catch (cause) {
        set({ busy: null, error: message(cause) });
        log.error(`Creating the screen failed: ${message(cause)}`);
        return null;
      }
    },

    remove: async (screenId) => {
      const { projectId, screens } = get();
      const doc = screens.find((candidate) => candidate.id === screenId);
      if (projectId === null || !doc) return false;
      set({ busy: 'removing', error: null });
      try {
        await api.deleteScreen(projectId, screenId);
        if (get().projectId !== projectId) return false;
        set((state) => ({
          busy: null,
          screens: state.screens.filter((candidate) => candidate.id !== screenId),
        }));
        log.info(`Screen “${doc.name}” removed from the draft. It stays released until the next release.`);
        return true;
      } catch (cause) {
        set({ busy: null, error: message(cause) });
        log.error(`Removing the screen failed: ${message(cause)}`);
        return false;
      }
    },

    addElement: (screenId, kind) => {
      const doc = get().screens.find((candidate) => candidate.id === screenId);
      if (!doc) return null;
      const element = kind === 'text' ? newTextElement(doc.canvas) : newIoField(doc.canvas);
      edit(screenId, (canvas) => ({ ...canvas, elements: [...canvas.elements, element] }));
      return element.id;
    },

    updateElement: (screenId, elementId, change) =>
      edit(screenId, (canvas) => ({
        ...canvas,
        elements: canvas.elements.map((element) =>
          element.id === elementId && element.type !== 'unknown'
            ? place({ ...element, ...change } as KnownElement, canvas)
            : element
        ),
      })),

    removeElement: (screenId, elementId) =>
      edit(screenId, (canvas) => ({
        ...canvas,
        elements: canvas.elements.filter((element) => element.id !== elementId),
      })),

    discard: (screenId) =>
      set((state) => ({
        screens: state.screens.map((doc) =>
          doc.id === screenId ? { ...doc, canvas: parseCanvas(JSON.parse(doc.baseline)) } : doc
        ),
      })),

    save: async (screenId) => {
      const { projectId, screens } = get();
      const doc = screens.find((candidate) => candidate.id === screenId);
      if (projectId === null || !doc) return false;
      const stored = serializeCanvas(doc.canvas);
      set({ busy: 'saving', error: null });
      try {
        await api.saveScreenDraft(projectId, screenId, stored);
        if (get().projectId !== projectId) return false;
        set((state) => ({
          busy: null,
          screens: state.screens.map((candidate) =>
            candidate.id === screenId ? { ...candidate, baseline: canonical(stored) } : candidate
          ),
        }));
        log.info(`Screen “${doc.name}” saved. It is not released until the screens are released.`);
        return true;
      } catch (cause) {
        set({ busy: null, error: message(cause) });
        log.error(`Saving screen “${doc.name}” failed: ${message(cause)}`);
        return false;
      }
    },

    release: async () => {
      const { projectId, screens } = get();
      if (projectId === null) return false;
      // A release publishes what the controller has saved. With unsaved edits on screen, what
      // would be released is not what the engineer is looking at.
      if (screens.some(isScreenDirty)) {
        set({ error: 'Save or discard the unsaved screens before releasing.' });
        return false;
      }
      set({ busy: 'releasing', error: null });
      try {
        const released = await api.releaseScreens(projectId);
        if (get().projectId !== projectId) return false;
        set({
          busy: null,
          released: new Map(Object.entries(released).map(([id, canvas]) => [id, normalized(canvas)])),
        });
        log.info(`${Object.keys(released).length} screen(s) released.`);
        return true;
      } catch (cause) {
        set({ busy: null, error: message(cause) });
        log.error(`Releasing the screens failed: ${message(cause)}`);
        return false;
      }
    },
  };
});
