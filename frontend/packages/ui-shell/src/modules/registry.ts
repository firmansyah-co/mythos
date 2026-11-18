// What the modules' workbench UIs contributed (ADR-011 §2.3), and whether they have loaded.

import { create } from 'zustand';
import type { DiagnosticsPanel, Editor, Panel, PortalSection, TreeSection, View } from '@mythos/workbench-sdk';

export interface ModuleLoad {
  id: string;
  /** Where its entry was imported from. */
  url: string;
  state: 'loading' | 'active' | 'failed';
  error?: string;
}

/** A contribution, with the module that made it. */
export interface Owned<T> {
  module: string;
  item: T;
}

export interface ModuleUiState {
  /** `idle` before the list of modules was read; `ready` once every entry settled. */
  phase: 'idle' | 'loading' | 'ready';
  loads: ModuleLoad[];
  treeSections: Owned<TreeSection>[];
  editors: Owned<Editor>[];
  views: Owned<View>[];
  portalSections: Owned<PortalSection>[];
  /** Inspector panels by owner. */
  inspector: Record<string, Owned<Panel>>;
  /** Task cards by owner. */
  taskCards: Record<string, Owned<Panel>>;
  /** What modules show in the inspector's Diagnostics tab, by owner. */
  diagnostics: Record<string, Owned<DiagnosticsPanel>>;
  /** Bumped when a module says its editor titles changed. */
  titles: number;
  /** Owners that hold unsaved changes. */
  unsaved: Record<string, string>;
}

const EMPTY: ModuleUiState = {
  phase: 'idle',
  loads: [],
  treeSections: [],
  editors: [],
  views: [],
  portalSections: [],
  inspector: {},
  taskCards: {},
  diagnostics: {},
  titles: 0,
  unsaved: {},
};

export const useModuleUi = create<ModuleUiState>(() => ({ ...EMPTY }));

export function resetModuleUi(): void {
  useModuleUi.setState({ ...EMPTY });
}

/** The editor a module registered for `kind`, if any. */
export function moduleEditor(state: ModuleUiState, kind: string): Owned<Editor> | undefined {
  return state.editors.find((entry) => entry.item.kind === kind);
}

export function moduleView(state: ModuleUiState, id: string): Owned<View> | undefined {
  return state.views.find((entry) => entry.item.id === id);
}

/** The tree sections in the order they asked for, then in the order they came. */
export function orderedTreeSections(sections: Owned<TreeSection>[]): Owned<TreeSection>[] {
  return sections
    .map((entry, index) => ({ entry, index }))
    .sort((a, b) => (a.entry.item.order ?? 0) - (b.entry.item.order ?? 0) || a.index - b.index)
    .map(({ entry }) => entry);
}

/** What the modules' diagnostics add to the count on the Diagnostics tab: only what they know. */
export function moduleProblemCount(state: ModuleUiState): number {
  return Object.values(state.diagnostics).reduce((sum, panel) => sum + (panel.item.count ?? 0), 0);
}
