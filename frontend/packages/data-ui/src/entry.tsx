// The data module's workbench UI (ADR-014 §2.8, ADR-011 §2.3): the "Data" section of the
// project tree and the editor of a user type. Built into `entry.js` (see vite.config.ts) and
// served by the controller beside the module.

import type { Dispose, WorkbenchHost, WorkbenchModule } from '@mythos/workbench-sdk';
import { bind, DATA_CHANGED_EVENT, type DataChanged } from './bridge';
import { DataTree } from './DataTree';
import { draw } from './draw';
import { anyDirty, useData } from './store';
import { TypeEditor } from './TypeEditor';
// Extracted by the build into `data.css` beside `entry.js`, which `stylesheet()` links.
import './data.css';

export const MODULE_ID = 'org.mythos.data';
/** The editor of a user type; its target is the type's name. */
export const TYPE_EDITOR = 'data-type';
/** The owner of the types' unsaved changes. */
export const UNSAVED_OWNER = 'data-types';

/** The stylesheet built beside `entry.js`, while the module is active. */
function stylesheet(): Dispose {
  // Tests draw without stylesheets: their DOM loads no CSS.
  if (typeof document === 'undefined' || import.meta.env.MODE === 'test') return () => undefined;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./data.css', import.meta.url).href;
  link.dataset.module = MODULE_ID;
  document.head.append(link);
  return () => link.remove();
}

/** `DATA_CHANGED`'s data, when it has the shape the module sends; otherwise nothing. */
function eventOf(data: unknown): DataChanged | null {
  if (data === null || typeof data !== 'object') return null;
  const d = data as Record<string, unknown>;
  if (typeof d.project !== 'string' || typeof d.name !== 'string') return null;
  if (d.what !== 'type' && d.what !== 'block') return null;
  if (d.change !== 'saved' && d.change !== 'deleted') return null;
  return { project: d.project, what: d.what, name: d.name, change: d.change };
}

export function activate(host: WorkbenchHost): Dispose {
  const stops: Dispose[] = [stylesheet(), bind(host)];

  // The open project's types and blocks are read when it opens, and dropped when it closes.
  const load = (projectId: string | null) => {
    if (projectId === null) useData.getState().reset();
    else void useData.getState().load(projectId);
  };
  load(host.project()?.id ?? null);
  stops.push(host.onProject((project) => load(project?.id ?? null)));

  // Closing the project asks first while a type has changes that are not saved.
  stops.push(
    useData.subscribe((state, previous) => {
      if (state.docs !== previous.docs) host.setUnsaved(UNSAVED_OWNER, anyDirty(state));
    })
  );
  stops.push(() => host.setUnsaved(UNSAVED_OWNER, false));

  // Somebody else (another engineer, an agent over MCP) changed the types or blocks.
  stops.push(
    host.events.subscribe(DATA_CHANGED_EVENT, (data) => {
      const event = eventOf(data);
      if (!event) return;
      const close = useData.getState().changed(event);
      if (close !== null) host.closeEditor(TYPE_EDITOR, close);
    })
  );

  stops.push(
    host.register.treeSection({
      id: 'data',
      order: 30,
      mount: (container) => draw(container, <DataTree openType={(name) => host.openEditor(TYPE_EDITOR, name)} />),
    })
  );

  stops.push(
    host.register.editor({
      kind: TYPE_EDITOR,
      title: (name) => ['User types', name ?? ''],
      mount: (container, { target }) =>
        draw(container, <TypeEditor name={target ?? ''} onRemoved={(name) => host.closeEditor(TYPE_EDITOR, name)} />),
    })
  );

  return () => {
    for (const stop of stops.reverse()) stop();
    useData.getState().reset();
  };
}

const data: WorkbenchModule = { id: MODULE_ID, activate };
export default data;
