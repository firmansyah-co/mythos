// The tags the open project's control programs publish (`internal.*`, ADR-013 §2.4), read from
// the project's runtimes configuration: what an HMI element or another program may bind to,
// beside the tags of the project's I/O configuration. They come from the configuration, not from
// what runs, so a project the controller does not run offers them too.

import { create } from 'zustand';
import { api } from '../api/client';
import { log } from './log';

export interface ProgramTagsState {
  projectId: string | null;
  /** Sorted. Empty until the first read; a failed read keeps the last one (it is configuration). */
  published: string[];
  /** Why the last read failed, when it did. */
  error: string | null;
  load: (projectId: string) => Promise<void>;
  reset: () => void;
}

const same = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i]);

export const useProgramTags = create<ProgramTagsState>((set, get) => ({
  projectId: null,
  published: [],
  error: null,

  load: async (projectId) => {
    if (get().projectId !== projectId) set({ projectId, published: [], error: null });
    try {
      const config = await api.getRuntimesConfig(projectId);
      if (get().projectId !== projectId) return;
      const published = [
        ...new Set(
          (config?.instances ?? []).flatMap((instance) =>
            (instance.program?.published ?? []).map((binding) => binding.tag)
          )
        ),
      ]
        .filter((uri) => typeof uri === 'string' && uri !== '')
        .sort((a, b) => a.localeCompare(b));
      // A new array only when the list changed, so modules are not told of a change that is none.
      if (!same(published, get().published) || get().error !== null) set({ published, error: null });
    } catch (cause) {
      if (get().projectId !== projectId) return;
      const error = cause instanceof Error ? cause.message : String(cause);
      if (get().error !== error) log.error(`The project's programs' tags could not be read: ${error}`);
      set({ error });
    }
  },

  reset: () => set({ projectId: null, published: [], error: null }),
}));
