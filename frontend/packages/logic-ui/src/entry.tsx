// The logic module's workbench UI (ADR-013 §2.6, ADR-011 §2.3): the "Programs", "Runtimes" and
// "Library" sections of the project tree, the program editor, the runtime editor (ADR-013 §2.19,
// C8d) and the library source editor (ADR-014 D4c). Built into `entry.js` (see vite.config.ts)
// and served by the controller beside the module.

import type { Dispose, WorkbenchHost, WorkbenchModule } from '@mythos/workbench-sdk';
import { bind } from './bridge';
import { draw } from './draw';
import { DATA_CHANGED_EVENT, LIBRARY_CHANGED_EVENT, LIBRARY_EDITOR, libraryEventOf, projectOf } from './library';
import { LibraryEditor } from './LibraryEditor';
import { anyDirty as anyLibraryDirty, useLibrary } from './librarystore';
import { LibraryTree } from './LibraryTree';
import { ProgramEditor } from './ProgramEditor';
import { ProgramsTree } from './ProgramsTree';
import { anyDirty, followsSavedSource, usePrograms } from './programs';
import { RuntimeEditor } from './RuntimeEditor';
import { RUNTIME_EDITOR, RUNTIMES_CHANGED_EVENT, runtimesEventOf } from './runtimes';
import { anyRuntimeDirty, useRuntimes } from './runtimestore';
import { RuntimesTree } from './RuntimesTree';
// Extracted by the build into `logic.css` beside `entry.js`, which `stylesheet()` links.
import './logic.css';

export const MODULE_ID = 'org.mythos.logic';
/** The editor's kind; its target is the program's name. */
export const EDITOR = 'logic-program';
/** Sent by the logic module when a project's programs change. */
export const PROGRAMS_CHANGED_EVENT = 'LOGIC_PROGRAMS_CHANGED';
/** The owner of the library sources' unsaved changes. */
export const LIBRARY_UNSAVED_OWNER = 'logic-library';
/** The owner of the configured runtimes' unsaved changes. */
export const RUNTIMES_UNSAVED_OWNER = 'logic-runtimes';

/** The stylesheet built beside `entry.js`, while the module is active. */
function stylesheet(): Dispose {
  // Tests draw without stylesheets: their DOM loads no CSS.
  if (typeof document === 'undefined' || import.meta.env.MODE === 'test') return () => undefined;
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = new URL('./logic.css', import.meta.url).href;
  link.dataset.module = MODULE_ID;
  document.head.append(link);
  return () => link.remove();
}

export function activate(host: WorkbenchHost): Dispose {
  const stops: Dispose[] = [stylesheet(), bind(host)];

  // The open project's programs, runtimes and library are read when it opens, and dropped when it
  // closes.
  const load = (projectId: string | null) => {
    if (projectId === null) {
      usePrograms.getState().reset();
      useRuntimes.getState().reset();
      useLibrary.getState().reset();
    } else {
      void usePrograms.getState().load(projectId);
      void useRuntimes.getState().load(projectId);
      void useLibrary.getState().load(projectId);
    }
  };
  load(host.project()?.id ?? null);
  stops.push(host.onProject((project) => load(project?.id ?? null)));

  // Closing the project asks first while a program has changes that are not saved.
  stops.push(
    usePrograms.subscribe((state, previous) => {
      if (state.programs !== previous.programs) host.setUnsaved('logic-programs', anyDirty(state));
    })
  );
  stops.push(() => host.setUnsaved('logic-programs', false));
  stops.push(
    useLibrary.subscribe((state, previous) => {
      if (state.docs !== previous.docs) host.setUnsaved(LIBRARY_UNSAVED_OWNER, anyLibraryDirty(state));
    })
  );
  stops.push(() => host.setUnsaved(LIBRARY_UNSAVED_OWNER, false));
  stops.push(
    useRuntimes.subscribe((state, previous) => {
      if (state.docs !== previous.docs) host.setUnsaved(RUNTIMES_UNSAVED_OWNER, anyRuntimeDirty(state));
    })
  );
  stops.push(() => host.setUnsaved(RUNTIMES_UNSAVED_OWNER, false));

  // A runtime was saved or deleted (here, by another engineer, by an agent over MCP): the list is
  // read again, and an open editor follows it unless it has changes.
  stops.push(
    host.events.subscribe(RUNTIMES_CHANGED_EVENT, (data) => {
      const runtimes = useRuntimes.getState();
      if (runtimes.projectId === null || projectOf(data) !== runtimes.projectId) return;
      const event = runtimesEventOf(data);
      if (event === null) return;
      const close = runtimes.changed(event);
      if (close !== null) host.closeEditor(RUNTIME_EDITOR, close);
    })
  );

  // A library source was saved or deleted (here, by another engineer, by an agent over MCP): the
  // list is read again, and an open editor follows it unless it has changes.
  stops.push(
    host.events.subscribe(LIBRARY_CHANGED_EVENT, (data) => {
      const library = useLibrary.getState();
      const project = (data as { project?: unknown } | null)?.project;
      if (library.projectId === null || project !== library.projectId) return;
      const event = libraryEventOf(data);
      if (event === null) return;
      const close = library.changed(event);
      if (close !== null) host.closeEditor(LIBRARY_EDITOR, close);
    })
  );

  // Every program is compiled with the library: a check made before the library changed is of
  // an earlier one. Open programs say so, and those read are checked again.
  stops.push(
    host.events.subscribe(LIBRARY_CHANGED_EVENT, (data) => {
      const programs = usePrograms.getState();
      if (programs.projectId === null || projectOf(data) !== programs.projectId) return;
      if (libraryEventOf(data) === null) return;
      programs.libraryChanged();
    })
  );

  // The data module changed the project's user types or data blocks: every program and the
  // library are compiled with them, so the checks made before are of earlier ones.
  stops.push(
    host.events.subscribe(DATA_CHANGED_EVENT, (data) => {
      const project = projectOf(data);
      const programs = usePrograms.getState();
      if (programs.projectId !== null && project === programs.projectId) programs.dataChanged();
      const library = useLibrary.getState();
      if (library.projectId !== null && project === library.projectId) library.dataChanged();
    })
  );

  // Somebody else (another engineer, an agent over MCP) changed the programs: read them again,
  // unless that would throw away what is being edited here.
  stops.push(
    host.events.subscribe(PROGRAMS_CHANGED_EVENT, (data) => {
      const state = usePrograms.getState();
      const project = (data as { project?: unknown } | null)?.project;
      if (state.projectId === null || project !== state.projectId) return;
      const program = (data as { program?: unknown }).program;
      const what = (data as { what?: unknown }).what;
      // A diagram's view follows what an agent or another workbench saved, unless it has changes
      // here.
      const ladder = state.programs.find((doc) => doc.entry.name === program && followsSavedSource(doc.entry.language));
      if (what === 'source' && ladder && ladder.source !== null) {
        void state.reread(ladder.entry.name);
        return;
      }
      if (state.busy !== null || anyDirty(state)) return;
      const listed = new Set(state.programs.map((doc) => doc.entry.name));
      // This workbench's own saves come back as events too; only a list that changed is read.
      if ((what === 'created' && !listed.has(String(program))) || (what === 'deleted' && listed.has(String(program)))) {
        void state.load(state.projectId);
      }
    })
  );

  stops.push(
    host.register.treeSection({
      id: 'logic-programs',
      order: 20,
      mount: (container) => draw(container, <ProgramsTree openEditor={(name) => host.openEditor(EDITOR, name)} />),
    })
  );

  stops.push(
    host.register.treeSection({
      id: 'logic-runtimes',
      // After Programs (20), before Library (25).
      order: 22,
      mount: (container) => draw(container, <RuntimesTree openEditor={(id) => host.openEditor(RUNTIME_EDITOR, id)} />),
    })
  );

  stops.push(
    host.register.treeSection({
      id: 'logic-library',
      // After Programs (20), before the data module's Data (30).
      order: 25,
      mount: (container) => draw(container, <LibraryTree openEditor={(name) => host.openEditor(LIBRARY_EDITOR, name)} />),
    })
  );

  stops.push(
    host.register.editor({
      kind: EDITOR,
      live: true,
      title: (name) => ['Programs', name ?? ''],
      mount: (container, { target }) =>
        draw(container, <ProgramEditor name={target ?? ''} onRemoved={(name) => host.closeEditor(EDITOR, name)} />),
    })
  );

  stops.push(
    host.register.editor({
      kind: LIBRARY_EDITOR,
      title: (name) => ['Library', name ?? ''],
      mount: (container, { target }) =>
        draw(container, <LibraryEditor name={target ?? ''} onClose={(name) => host.closeEditor(LIBRARY_EDITOR, name)} />),
    })
  );

  stops.push(
    host.register.editor({
      kind: RUNTIME_EDITOR,
      title: (id) => ['Runtimes', id ?? ''],
      mount: (container, { target }) =>
        draw(container, <RuntimeEditor id={target ?? ''} onClose={(id) => host.closeEditor(RUNTIME_EDITOR, id)} />),
    })
  );

  return () => {
    for (const stop of stops.reverse()) stop();
    usePrograms.getState().reset();
    useRuntimes.getState().reset();
    useLibrary.getState().reset();
  };
}

const logic: WorkbenchModule = { id: MODULE_ID, activate };
export default logic;
