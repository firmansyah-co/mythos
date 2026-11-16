// The HMI UI's view of the workbench, through the `WorkbenchHost` it was activated with
// (ADR-011 §2.3). The stores here mirror what the host reports, under the names the HMI code
// used when it was part of the workbench, so that code reads the same as it did.

import { create } from 'zustand';
import type {
  ControllerView,
  Dispose,
  Project,
  TelemetrySnapshot,
  WorkbenchHost,
} from '@mythos/workbench-sdk';

let current: WorkbenchHost | null = null;

/** The host the module was activated with. */
export function host(): WorkbenchHost {
  if (!current) throw new Error('the HMI UI is not activated');
  return current;
}

const EMPTY_TELEMETRY: TelemetrySnapshot = { tags: new Map(), stream: 'IDLE', lastBatchAtMs: null };
const EMPTY_CONTROLLER: ControllerView = {
  activeProjectId: undefined,
  io: null,
  error: null,
  configuredTags: [],
};

/** The live tag cache and the stream's state, as the workbench holds them. */
export const useTelemetry = create<TelemetrySnapshot>(() => EMPTY_TELEMETRY);
/** The controller, as the workbench polls it. */
export const useController = create<ControllerView>(() => EMPTY_CONTROLLER);

/** What is selected on a screen being edited. */
export type Selection = { kind: 'element'; screenId: string; elementId: string } | null;

export interface WorkbenchState {
  project: Project | null;
  selection: Selection;
  select: (selection: Selection) => void;
  closeEditor: (id: string) => void;
  /** The screen the operator runtime was asked for (its address's target). */
  runtimeScreenId: string | null;
  startRuntime: (screenId?: string) => void;
  exitRuntime: () => void;
}

/** Set by `entry.tsx`: what the inspector shows for a selected element. */
let onSelect: (selection: Selection) => void = () => undefined;
export function whenSelected(handler: (selection: Selection) => void): void {
  onSelect = handler;
}

export const useWorkbench = create<WorkbenchState>((set) => ({
  project: null,
  selection: null,
  select: (selection) => {
    set({ selection });
    onSelect(selection);
  },
  closeEditor: (id) => {
    const [kind, ...rest] = id.split(':');
    host().closeEditor(kind ?? id, rest.length ? rest.join(':') : undefined);
  },
  runtimeScreenId: null,
  startRuntime: (screenId) => {
    set({ runtimeScreenId: screenId ?? null });
    host().showView('runtime', screenId);
  },
  exitRuntime: () => host().exitView(),
}));

/** The id the workbench gives an editor of `kind` on `target`. */
export function editorId(kind: string, target?: string): string {
  return target ? `${kind}:${target}` : kind;
}

export function goOnline(): void {
  host().telemetry.goOnline();
}

export function onStreamEvent(name: string, handler: (data: unknown) => void): Dispose {
  return host().events.subscribe(name, handler);
}

export const log = {
  info: (text: string) => current?.log.info(text),
  warn: (text: string) => current?.log.info(text),
  error: (text: string) => current?.log.error(text),
};

/** An HMI screen as the module stores it. `canvasData` is the workbench's own format. */
export interface StoredScreen {
  id: string;
  name: string;
  is_root: boolean;
  canvasData: unknown;
}

const screensOf = (projectId: string) => `/api/v1/projects/${encodeURIComponent(projectId)}/hmi`;
const json = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
});

/** The HMI module's routes. */
export const api = {
  listScreens: (projectId: string, view: 'draft' | 'active') =>
    host().api<StoredScreen[]>(`${screensOf(projectId)}/screens?view=${view}`),
  createScreen: (projectId: string, name: string) =>
    host().api<StoredScreen>(`${screensOf(projectId)}/screens`, json('POST', { name, is_root: false })),
  saveScreenDraft: async (projectId: string, screenId: string, canvas: unknown) => {
    await host().api(
      `${screensOf(projectId)}/screens/${encodeURIComponent(screenId)}/draft`,
      json('PUT', canvas)
    );
  },
  deleteScreen: async (projectId: string, screenId: string) => {
    await host().api(`${screensOf(projectId)}/screens/${encodeURIComponent(screenId)}`, {
      method: 'DELETE',
    });
  },
  /** Release every screen's draft: what operators see becomes what was last saved. */
  releaseScreens: (projectId: string) =>
    host().api<Record<string, unknown>>(`${screensOf(projectId)}/commit`, { method: 'POST' }),
};

/**
 * Follow `next` from now on: mirror its project, telemetry and controller into the stores
 * above. Returns what stops it. Called by `activate`, and by tests with a host of their own.
 */
export function bind(next: WorkbenchHost): Dispose {
  current = next;
  useTelemetry.setState(next.telemetry.snapshot(), true);
  useController.setState(next.controller.get(), true);
  useWorkbench.setState({ project: next.project(), selection: null });
  const stops = [
    next.telemetry.subscribe(() => useTelemetry.setState(next.telemetry.snapshot(), true)),
    next.controller.subscribe(() => useController.setState(next.controller.get(), true)),
    next.onProject((project) => useWorkbench.setState({ project, selection: null })),
  ];
  return () => {
    for (const stop of stops) stop();
    if (current === next) current = null;
    useTelemetry.setState(EMPTY_TELEMETRY, true);
    useController.setState(EMPTY_CONTROLLER, true);
    useWorkbench.setState({ project: null, selection: null, runtimeScreenId: null });
  };
}
