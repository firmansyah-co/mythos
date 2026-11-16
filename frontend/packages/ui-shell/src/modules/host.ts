// The `WorkbenchHost` each module's UI is activated with (ADR-011 §2.3).

import type {
  Dispose,
  Editor,
  Identity,
  Project,
  TreeSection,
  View,
  WorkbenchHost,
} from '@mythos/workbench-sdk';
import { ApiError, apiRequest, type ProjectConfig } from '../api/client';
import { log } from '../data/log';
import { useSession } from '../data/session';
import { onStreamEvent, useTelemetry } from '../data/telemetry';
import { CORE_EDITOR_KINDS, editorId, useWorkbench } from '../data/workbench';
import { useModuleUi, type ModuleUiState, type Owned } from './registry';

function asProject(project: ProjectConfig | null): Project | null {
  return project ? { id: project.id, name: project.name } : null;
}

/** Add `entry` to the list `key`, refusing a second claim on the same name. */
function claim<K extends 'treeSections' | 'editors' | 'views'>(
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
          if (state.tags !== previous.tags || state.stream !== previous.stream) listener();
        }),
      online: () => useTelemetry.getState().stream === 'OPEN',
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
    },
  };
}

/** View ids that are words of the workbench's own address (`#/project/…`). */
export const RESERVED_VIEWS: readonly string[] = ['project', 'settings', 'portal'];
