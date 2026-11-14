// What the operator runtime shows: the screens that were last released for the open project.
// Never the draft: an operator sees only what an engineer chose to release.

import { create } from 'zustand';
import { api } from '../api/client';
import { parseCanvas, type ScreenCanvas } from './screenModel';

export interface RuntimeScreen {
  id: string;
  name: string;
  isRoot: boolean;
  canvas: ScreenCanvas;
}

export interface RuntimeState {
  projectId: string | null;
  phase: 'idle' | 'loading' | 'ready' | 'unreadable';
  loadError: string | null;
  screens: RuntimeScreen[];
  /** Read the released screens. With screens already on display they stay until the new ones arrive. */
  load: (projectId: string) => Promise<void>;
  reset: () => void;
}

export const useRuntime = create<RuntimeState>((set, get) => ({
  projectId: null,
  phase: 'idle',
  loadError: null,
  screens: [],

  load: async (projectId) => {
    const refreshing = get().projectId === projectId && get().phase === 'ready';
    if (!refreshing) set({ projectId, phase: 'loading', loadError: null, screens: [] });
    try {
      const released = await api.listScreens(projectId, 'active');
      if (get().projectId !== projectId) return;
      set({
        phase: 'ready',
        loadError: null,
        screens: released.map((screen) => ({
          id: screen.id,
          name: screen.name,
          isRoot: screen.is_root,
          canvas: parseCanvas(screen.canvasData),
        })),
      });
    } catch (cause) {
      if (get().projectId !== projectId) return;
      // Screens that can no longer be confirmed as the released ones are taken off the display.
      set({
        phase: 'unreadable',
        loadError: cause instanceof Error ? cause.message : String(cause),
        screens: [],
      });
    }
  },

  reset: () => set({ projectId: null, phase: 'idle', loadError: null, screens: [] }),
}));

/** The screen the runtime opens on: the one asked for, else the root screen, else the first. */
export function screenToShow(screens: RuntimeScreen[], wanted: string | null): RuntimeScreen | undefined {
  return (
    screens.find((screen) => screen.id === wanted) ??
    screens.find((screen) => screen.isRoot) ??
    screens[0]
  );
}
