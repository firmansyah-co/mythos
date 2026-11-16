// What the workbench itself is showing: which view, which project, which editors, what is
// selected. None of it is process data.

import { create } from 'zustand';
import type { ProjectConfig } from '../api/client';

/** The workbench's own editors. */
export type CoreEditorKind =
  | 'overview'
  | 'devices'
  | 'connection'
  | 'tags'
  | 'diagnostics'
  | 'screen';

/** An editor kind: the workbench's own, or one a module registered (ADR-011 §2.3). */
export type EditorKind = CoreEditorKind | (string & {});

export const CORE_EDITOR_KINDS: readonly CoreEditorKind[] = [
  'overview',
  'devices',
  'connection',
  'tags',
  'diagnostics',
  'screen',
];

export interface EditorRef {
  /** Stable identity: opening the same thing twice focuses the existing editor. */
  id: string;
  kind: EditorKind;
  /**
   * What the editor is scoped to, for the kinds that are scoped. For `tags` and `diagnostics`
   * it is a connection's id; for `connection` it is the key of the draft row, because the id
   * itself is one of the things that editor changes; for `screen` it is the screen's id.
   */
  target?: string;
}

export type Selection =
  | { kind: 'project' }
  | { kind: 'connection'; id: string }
  | { kind: 'tag'; uri: string }
  | { kind: 'element'; screenId: string; elementId: string }
  /** What a module showed in the inspector, by the owner it gave. */
  | { kind: 'module'; owner: string }
  | null;

export type InspectorTab = 'properties' | 'info' | 'diagnostics';

export function editorId(kind: EditorKind, target?: string): string {
  return target ? `${kind}:${target}` : kind;
}

export interface WorkbenchState {
  view: 'portal' | 'project' | 'runtime' | 'settings' | 'module';
  /** The full-window view of a module shown when `view` is `module`. */
  moduleView: { id: string; target?: string } | null;
  /** The settings section shown, by its id in the settings registry. */
  settingsSection: string;
  /** Where closing the settings returns to. */
  settingsReturn: 'portal' | 'project';
  /** The screen the operator runtime is showing. `null` shows the project's first screen. */
  runtimeScreenId: string | null;
  project: ProjectConfig | null;
  editors: EditorRef[];
  activeEditorId: string | null;
  selection: Selection;
  inspectorTab: InspectorTab;
  panes: { tree: boolean; inspector: boolean; taskCards: boolean };

  openProject: (project: ProjectConfig) => void;
  closeProject: () => void;
  showPortal: () => void;
  showProject: () => void;
  startRuntime: (screenId?: string) => void;
  exitRuntime: () => void;
  showModuleView: (id: string, target?: string) => void;
  exitModuleView: () => void;
  showSettings: (section?: string) => void;
  closeSettings: () => void;
  openEditor: (kind: EditorKind, target?: string) => void;
  closeEditor: (id: string) => void;
  focusEditor: (id: string) => void;
  select: (selection: Selection) => void;
  setInspectorTab: (tab: InspectorTab) => void;
  togglePane: (pane: 'tree' | 'inspector' | 'taskCards') => void;
}

export const useWorkbench = create<WorkbenchState>((set) => ({
  view: 'portal',
  settingsSection: 'account',
  settingsReturn: 'portal',
  runtimeScreenId: null,
  moduleView: null,
  project: null,
  editors: [],
  activeEditorId: null,
  selection: null,
  inspectorTab: 'properties',
  panes: { tree: true, inspector: true, taskCards: true },

  openProject: (project) =>
    set({
      view: 'project',
      project,
      editors: [{ id: editorId('overview'), kind: 'overview' }],
      activeEditorId: editorId('overview'),
      selection: { kind: 'project' },
    }),

  closeProject: () =>
    set({
      view: 'portal',
      project: null,
      editors: [],
      activeEditorId: null,
      selection: null,
      moduleView: null,
    }),

  showPortal: () => set({ view: 'portal' }),
  showProject: () => set((state) => (state.project ? { view: 'project' } : {})),
  startRuntime: (screenId) =>
    set((state) => (state.project ? { view: 'runtime', runtimeScreenId: screenId ?? null } : {})),
  exitRuntime: () => set((state) => (state.view === 'runtime' ? { view: 'project' } : {})),
  showModuleView: (id, target) =>
    set((state) => (state.project ? { view: 'module', moduleView: { id, target } } : {})),
  exitModuleView: () =>
    set((state) => (state.view === 'module' ? { view: 'project', moduleView: null } : {})),
  showSettings: (section) =>
    set((state) => ({
      view: 'settings',
      settingsSection: section ?? state.settingsSection,
      settingsReturn:
        state.view === 'settings'
          ? state.settingsReturn
          : state.view === 'portal' || !state.project
          ? 'portal'
          : 'project',
    })),
  closeSettings: () =>
    set((state) =>
      state.view === 'settings'
        ? { view: state.settingsReturn === 'project' && state.project ? 'project' : 'portal' }
        : {}
    ),

  openEditor: (kind, target) =>
    set((state) => {
      const id = editorId(kind, target);
      const exists = state.editors.some((editor) => editor.id === id);
      return {
        editors: exists ? state.editors : [...state.editors, { id, kind, target }],
        activeEditorId: id,
      };
    }),

  closeEditor: (id) =>
    set((state) => {
      const index = state.editors.findIndex((editor) => editor.id === id);
      if (index < 0) return {};
      const editors = state.editors.filter((editor) => editor.id !== id);
      const activeEditorId =
        state.activeEditorId === id
          ? (editors[Math.min(index, editors.length - 1)]?.id ?? null)
          : state.activeEditorId;
      return { editors, activeEditorId };
    }),

  focusEditor: (id) => set({ activeEditorId: id }),
  select: (selection) => set({ selection }),
  setInspectorTab: (inspectorTab) => set({ inspectorTab }),
  togglePane: (pane) => set((state) => ({ panes: { ...state.panes, [pane]: !state.panes[pane] } })),
}));
