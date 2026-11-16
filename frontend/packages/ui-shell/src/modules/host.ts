// The `WorkbenchHost` each module's UI is activated with (ADR-011 §2.3).

import type {
  ControllerView,
  Dispose,
  Editor,
  Identity,
  PortalSection,
  Project,
  TelemetrySnapshot,
  TreeSection,
  View,
  WorkbenchHost,
} from '@mythos/workbench-sdk';
import { ApiError, apiRequest, type ProjectConfig } from '../api/client';
import { useController } from '../data/controller';
import { useIoConfig } from '../data/ioConfig';
import { connectionTags } from '../data/ioForm';
import { log } from '../data/log';
import { useSession } from '../data/session';
import { goOnline, onStreamEvent, useTelemetry } from '../data/telemetry';
import { CORE_EDITOR_KINDS, editorId, useWorkbench } from '../data/workbench';
import { useModuleUi, type ModuleUiState, type Owned } from './registry';

function asProject(project: ProjectConfig | null): Project | null {
  return project ? { id: project.id, name: project.name } : null;
}

/** Add `entry` to the list `key`, refusing a second claim on the same name. */
function claim<K extends 'treeSections' | 'editors' | 'views' | 'portalSections'>(
  key: K,
  entry: ModuleUiState[K][number],
  name: (item: ModuleUiState[K][number]['item']) => string
): Dispose {
  const taken = (useModuleUi.getState()[key] as Owned<unknown>[]).find(
    (other) => name(other.item as never) === name(entry.item as never)
  );
  if (taken) {
    throw new Error(
      `module '${entry.module}' cannot register '${name(entry.item as never)}': module '${taken.module}' already did`
    );
  }
  useModuleUi.setState((state) => ({ [key]: [...state[key], entry] }) as Partial<ModuleUiState>);
  return () =>
    useModuleUi.setState(
      (state) =>
        ({ [key]: (state[key] as Owned<unknown>[]).filter((other) => other !== entry) }) as Partial<
          ModuleUiState
        >
    );
}

// One snapshot per change of what it is made from, so a module comparing snapshots sees a new
// one only when something changed.
let lastSources: unknown[] = [];
let lastView: ControllerView | null = null;

/** The controller as a module sees it (`host.controller.get()`). */
export function controllerView(): ControllerView {
  const controller = useController.getState();
  const io = useIoConfig.getState();
  const sources = [controller.activeProjectId, controller.io, controller.error, io.draft, io.schemas];
  if (lastView && sources.every((source, index) => source === lastSources[index])) return lastView;
  lastSources = sources;
  lastView = {
    activeProjectId: controller.activeProjectId,
    io: controller.io,
    error: controller.error,
    configuredTags: [
      ...new Set(io.draft.flatMap((connection) => connectionTags(connection, io.schemas) ?? [])),
    ]
      .filter((uri) => uri !== '')
      .sort((a, b) => a.localeCompare(b)),
  };
  return lastView;
}

let lastTelemetry: TelemetrySnapshot | null = null;

/** `host.telemetry.snapshot()`: a new object only when the cache or the stream changed. */
export function telemetrySnapshot(): TelemetrySnapshot {
  const { tags, stream, lastBatchAtMs } = useTelemetry.getState();
  if (
    lastTelemetry &&
    lastTelemetry.tags === tags &&
    lastTelemetry.stream === stream &&
    lastTelemetry.lastBatchAtMs === lastBatchAtMs
  ) {
    return lastTelemetry;
  }
  lastTelemetry = { tags, stream, lastBatchAtMs };
  return lastTelemetry;
}

export function createHost(module: string): WorkbenchHost {
  return {
    sdk: 1,

    project: () => asProject(useWorkbench.getState().project),
    onProject: (listener) =>
      useWorkbench.subscribe((state, previous) => {
        if (state.project?.id !== previous.project?.id || state.project?.name !== previous.project?.name) {
          listener(asProject(state.project));
        }
      }),

    user: () => (useSession.getState().user as Identity | null) ?? null,

    api: async <T,>(path: string, init?: RequestInit) => {
      try {
        return await apiRequest<T>(path, init);
      } catch (cause) {
        if (cause instanceof ApiError) throw new Error(cause.message);
        throw cause;
      }
    },

    telemetry: {
      get: (uri) => useTelemetry.getState().tags.get(uri),
      subscribe: (listener) =>
        useTelemetry.subscribe((state, previous) => {
          if (
            state.tags !== previous.tags ||
            state.stream !== previous.stream ||
            state.lastBatchAtMs !== previous.lastBatchAtMs
          ) {
            listener();
          }
        }),
      online: () => useTelemetry.getState().stream === 'OPEN',
      stream: () => useTelemetry.getState().stream,
      snapshot: telemetrySnapshot,
      goOnline: () => goOnline(),
    },

    controller: {
      get: controllerView,
      subscribe: (listener) => {
        let last = controllerView();
        const check = () => {
          const next = controllerView();
          if (next !== last) {
            last = next;
            listener();
          }
        };
        const stops = [useController.subscribe(check), useIoConfig.subscribe(check)];
        return () => stops.forEach((stop) => stop());
      },
    },

    events: { subscribe: (name, listener) => onStreamEvent(name, listener) },

    log: {
      info: (message) => log.info(message),
      error: (message) => log.error(message),
    },

    openEditor: (kind, target) => useWorkbench.getState().openEditor(kind, target),
    closeEditor: (kind, target) => useWorkbench.getState().closeEditor(editorId(kind, target)),
    titlesChanged: () => useModuleUi.setState((state) => ({ titles: state.titles + 1 })),
    showView: (id, target) => useWorkbench.getState().showModuleView(id, target),
    exitView: () => useWorkbench.getState().exitModuleView(),

    setInspector: (owner, panel) => {
      useModuleUi.setState((state) => {
        const inspector = { ...state.inspector };
        if (panel) inspector[owner] = { module, item: panel };
        else delete inspector[owner];
        return { inspector };
      });
      const workbench = useWorkbench.getState();
      const selected = workbench.selection?.kind === 'module' && workbench.selection.owner === owner;
      if (panel) workbench.select({ kind: 'module', owner });
      else if (selected) workbench.select(null);
    },

    setUnsaved: (owner, unsaved) =>
      useModuleUi.setState((state) => {
        const next = { ...state.unsaved };
        if (unsaved) next[owner] = module;
        else delete next[owner];
        return { unsaved: next };
      }),

    setTaskCard: (owner, panel) =>
      useModuleUi.setState((state) => {
        const taskCards = { ...state.taskCards };
        if (panel) taskCards[owner] = { module, item: panel };
        else delete taskCards[owner];
        return { taskCards };
      }),

    register: {
      treeSection: (section: TreeSection) =>
        claim('treeSections', { module, item: section }, (item) => item.id),
      editor: (editor: Editor) => {
        if ((CORE_EDITOR_KINDS as readonly string[]).includes(editor.kind)) {
          throw new Error(`module '${module}' cannot register the workbench's own editor '${editor.kind}'`);
        }
        return claim('editors', { module, item: editor }, (item) => item.kind);
      },
      view: (view: View) => {
        if (RESERVED_VIEWS.includes(view.id)) {
          throw new Error(`module '${module}' cannot register the view '${view.id}'`);
        }
        return claim('views', { module, item: view }, (item) => item.id);
      },
      portalSection: (section: PortalSection) => {
        if (RESERVED_PORTAL.includes(section.id)) {
          throw new Error(`module '${module}' cannot register the Portal's own '${section.id}'`);
        }
        return claim('portalSections', { module, item: section }, (item) => item.id);
      },
    },
  };
}

/** View ids that are words of the workbench's own address (`#/project/…`). */
export const RESERVED_VIEWS: readonly string[] = ['project', 'settings', 'portal'];

/** The Portal's own pages. */
export const RESERVED_PORTAL: readonly string[] = ['open', 'create'];
