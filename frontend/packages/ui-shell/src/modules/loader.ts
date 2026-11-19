// Imports the workbench UI of every enabled module after sign-in (ADR-011 §2.3).

import type { Dispose, WorkbenchModule } from '@mythos/workbench-sdk';
import { api } from '../api/client';
import { log } from '../data/log';
import { createHost } from './host';
import { resetModuleUi, useModuleUi, type ModuleLoad } from './registry';

/** How an entry is imported. Replaced in tests; the browser's own `import()` otherwise. */
export type Importer = (url: string) => Promise<{ default?: unknown }>;

const browserImport: Importer = (url) => import(/* @vite-ignore */ url);

let disposers: Dispose[] = [];
let generation = 0;

function setLoad(id: string, patch: Partial<ModuleLoad>): void {
  useModuleUi.setState((state) => ({
    loads: state.loads.map((load) => (load.id === id ? { ...load, ...patch } : load)),
  }));
}

function isModule(value: unknown): value is WorkbenchModule {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Record<string, unknown>;
  return typeof candidate.id === 'string' && typeof candidate.activate === 'function';
}

/** Take every module's UI down again: on sign-out, and before loading anew. */
export function unloadModuleUis(): void {
  generation += 1;
  for (const dispose of disposers.reverse()) {
    try {
      dispose();
    } catch {
      // A module that fails to take itself down is gone from the registry all the same.
    }
  }
  disposers = [];
  resetModuleUi();
}

/**
 * Read which modules have a UI and import each. A module whose entry fails to load or to
 * activate is reported in the session log and in `loads`; the others load regardless.
 */
export async function loadModuleUis(importer: Importer = browserImport): Promise<void> {
  unloadModuleUis();
  const mine = generation;
  useModuleUi.setState({ phase: 'loading' });

  let rows: { id: string; ui?: string | null }[];
  try {
    rows = (await api.listModules()).modules ?? [];
  } catch (cause) {
    if (mine !== generation) return;
    log.error(
      `The modules' workbench UIs could not be listed: ${cause instanceof Error ? cause.message : String(cause)}`
    );
    useModuleUi.setState({ phase: 'ready' });
    return;
  }
  if (mine !== generation) return;

  const withUi = rows.filter((row): row is { id: string; ui: string } => typeof row.ui === 'string');
  useModuleUi.setState({
    loads: withUi.map((row) => ({ id: row.id, url: row.ui, state: 'loading' })),
  });

  await Promise.all(
    withUi.map(async (row) => {
      try {
        const loaded = await importer(row.ui);
        if (mine !== generation) return;
        const module = loaded.default;
        if (!isModule(module)) throw new Error('its entry has no default export with id and activate');
        if (module.id !== row.id) {
          throw new Error(`its entry says it is '${module.id}'`);
        }
        const registered: Dispose[] = [];
        const host = createHost(row.id);
        const register = host.register;
        // Remember what it registered, so that taking it down needs nothing from the module.
        host.register = {
          treeSection: (section) => remember(registered, register.treeSection(section)),
          editor: (editor) => remember(registered, register.editor(editor)),
          view: (view) => remember(registered, register.view(view)),
          portalSection: (section) => remember(registered, register.portalSection(section)),
        };
        let undo: Dispose | void;
        try {
          undo = module.activate(host);
        } catch (cause) {
          for (const dispose of registered) dispose();
          throw cause;
        }
        disposers.push(() => {
          if (typeof undo === 'function') undo();
          for (const dispose of registered) dispose();
          useModuleUi.setState((state) => ({
            inspector: withoutModule(state.inspector, row.id),
            taskCards: withoutModule(state.taskCards, row.id),
            diagnostics: withoutModule(state.diagnostics, row.id),
            unsaved: Object.fromEntries(
              Object.entries(state.unsaved).filter(([, owner]) => owner !== row.id)
            ),
          }));
        });
        setLoad(row.id, { state: 'active' });
      } catch (cause) {
        if (mine !== generation) return;
        const message = cause instanceof Error ? cause.message : String(cause);
        setLoad(row.id, { state: 'failed', error: message });
        log.error(`The workbench UI of module '${row.id}' did not load: ${message}`);
      }
    })
  );
  if (mine === generation) useModuleUi.setState({ phase: 'ready' });
}

function remember(list: Dispose[], dispose: Dispose): Dispose {
  let done = false;
  const once = () => {
    if (done) return;
    done = true;
    dispose();
  };
  list.push(once);
  return once;
}

function withoutModule<T extends { module: string }>(
  record: Record<string, T>,
  module: string
): Record<string, T> {
  return Object.fromEntries(Object.entries(record).filter(([, entry]) => entry.module !== module));
}
