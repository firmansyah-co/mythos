// What the workbench itself is showing: which view, which project, which editors, what is
// selected. None of it is process data.

import { create } from 'zustand';
import type { ProjectConfig } from '../api/client';

export type EditorKind = 'overview' | 'devices' | 'tags' | 'diagnostics';

export interface EditorRef {
  /** Stable identity: opening the same thing twice focuses the existing editor. */
  id: string;
  kind: EditorKind;
  /** The connection this editor is scoped to, for the kinds that have one. */
  connectionId?: string;
}

export type Selection =
  | { kind: 'project' }
  | { kind: 'connection'; id: string }
  | { kind: 'tag'; uri: string }
  | null;

export type InspectorTab = 'properties' | 'info' | 'diagnostics';

export function editorId(kind: EditorKind, connectionId?: string): string {
  return connectionId ? `${kind}:${connectionId}` : kind;
}

export interface WorkbenchState {
  view: 'portal' | 'project';
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
  openEditor: (kind: EditorKind, connectionId?: string) => void;
  closeEditor: (id: string) => void;
  focusEditor: (id: string) => void;
  select: (selection: Selection) => void;
  setInspectorTab: (tab: InspectorTab) => void;
  togglePane: (pane: 'tree' | 'inspector' | 'taskCards') => void;
}

export const useWorkbench = create<WorkbenchState>((set) => ({
  view: 'portal',
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
    set({ view: 'portal', project: null, editors: [], activeEditorId: null, selection: null }),

  showPortal: () => set({ view: 'portal' }),
  showProject: () => set((state) => (state.project ? { view: 'project' } : {})),

  openEditor: (kind, connectionId) =>
    set((state) => {
      const id = editorId(kind, connectionId);
      const exists = state.editors.some((editor) => editor.id === id);
      return {
        editors: exists ? state.editors : [...state.editors, { id, kind, connectionId }],
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
