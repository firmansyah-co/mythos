// What the controller says about itself: which project it runs, its field connections, and the
// tags it knows. Polled, because none of it is on the telemetry stream.

import { create } from 'zustand';
import { api, type IoStatus, type TagMetadata } from '../api/client';

/** How often the controller's own state is re-read while a project is open. */
export const CONTROLLER_POLL_INTERVAL_MS = 2000;

export interface ControllerState {
  /** `undefined` until the first answer; `null` when the controller runs no project. */
  activeProjectId: string | null | undefined;
  /** `null` until the first answer, and again whenever the controller cannot be reached. */
  io: IoStatus | null;
  /** Catalog metadata by tag URI. Empty whenever the controller cannot be reached. */
  tagMeta: Map<string, TagMetadata>;
  /** Why the controller's state is unknown, when it is. */
  error: string | null;

  refresh: () => Promise<void>;
}

export const useController = create<ControllerState>((set) => ({
  activeProjectId: undefined,
  io: null,
  tagMeta: new Map(),
  error: null,

  refresh: async () => {
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
        error: null,
      });
    } catch (cause) {
      // An unreachable controller is not a controller in its last known state. Drop what was
      // known so nothing goes on showing a connection as running.
      set({
        activeProjectId: undefined,
        io: null,
        tagMeta: new Map(),
        error: cause instanceof Error ? cause.message : String(cause),
      });
    }
  },
}));

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
