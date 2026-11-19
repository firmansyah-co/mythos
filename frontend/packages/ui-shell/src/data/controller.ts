// What the controller says about itself: which project it runs, its field connections, and the
// tags it knows. Polled, because none of it is on the telemetry stream.

import { create } from 'zustand';
import { api, type IoStatus, type RuntimeStatus, type TagMetadata } from '../api/client';

/** How often the controller's own state is re-read while a project is open. */
export const CONTROLLER_POLL_INTERVAL_MS = 2000;

export interface ControllerState {
  /** `undefined` until the first answer; `null` when the controller runs no project. */
  activeProjectId: string | null | undefined;
  /** `null` until the first answer, and again whenever the controller cannot be reached. */
  io: IoStatus | null;
  /** Catalog metadata by tag URI. Empty whenever the controller cannot be reached. */
  tagMeta: Map<string, TagMetadata>;
  /**
   * Every tag bound to a variable an engineer forces, sorted, as the runtimes last said.
   * `null` when not known: before the first answer, and whenever the runtimes cannot be read.
   */
  forcedTags: string[] | null;
  /** Why the controller's state is unknown, when it is. */
  error: string | null;

  refresh: () => Promise<void>;
}

export const useController = create<ControllerState>((set) => ({
  activeProjectId: undefined,
  io: null,
  tagMeta: new Map(),
  forcedTags: null,
  error: null,

  refresh: async () => {
    // The runtimes are read beside the rest, not with it. A failed read of them (a server
    // without the route, a runtime list that cannot be built) makes only the forced tags
    // unknown: it does not make the controller unreachable, which would put every value and
    // connection offline. It never rejects.
    const forced = readForcedTags();
    try {
      const [activeProjectId, io, tags] = await Promise.all([
        api.getActiveProjectId(),
        api.getIoStatus(),
        api.listTags(),
      ]);
      set({
        activeProjectId,
        io,
        tagMeta: new Map(tags.map((tag) => [tag.uri, tag])),
        forcedTags: await forced,
        error: null,
      });
    } catch (cause) {
      // An unreachable controller is not a controller in its last known state. Drop what was
      // known so nothing goes on showing a connection as running.
      set({
        activeProjectId: undefined,
        io: null,
        tagMeta: new Map(),
        forcedTags: null,
        error: cause instanceof Error ? cause.message : String(cause),
      });
    }
  },
}));

/**
 * The forced tags of every runtime, once each, sorted. `null` when any runtime does not say
 * (a server without forcing): the list is then not known, and an empty one would claim that
 * nothing is forced. A runtime whose module does not answer says `[]` with `forced: null`; it
 * adds nothing, because the controller knows none of its forces.
 */
export function forcedTagsOf(runtimes: readonly RuntimeStatus[]): string[] | null {
  const tags = new Set<string>();
  for (const runtime of runtimes) {
    const listed: unknown = runtime.forced_tags;
    if (!Array.isArray(listed) || !listed.every((tag) => typeof tag === 'string')) return null;
    for (const tag of listed as string[]) tags.add(tag);
  }
  return [...tags].sort((a, b) => a.localeCompare(b));
}

/** What `forcedTagsOf` gives for the controller's runtimes now, or `null` when they cannot be read. */
async function readForcedTags(): Promise<string[] | null> {
  try {
    return forcedTagsOf((await api.listRuntimes()).runtimes);
  } catch {
    return null;
  }
}

let pollTimer: ReturnType<typeof setInterval> | null = null;

export function startControllerPolling(): void {
  if (pollTimer !== null) return;
  void useController.getState().refresh();
  pollTimer = setInterval(() => {
    void useController.getState().refresh();
  }, CONTROLLER_POLL_INTERVAL_MS);
}

export function stopControllerPolling(): void {
  if (pollTimer === null) return;
  clearInterval(pollTimer);
  pollTimer = null;
}
