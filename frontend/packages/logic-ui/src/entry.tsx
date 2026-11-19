// The logic module's workbench UI (ADR-013 §2.6, ADR-011 §2.3): the "Programs" section of the
// project tree and the program editor. Built into `entry.js` (see vite.config.ts) and served by
// the controller beside the module.

import type { Dispose, WorkbenchHost, WorkbenchModule } from '@mythos/workbench-sdk';
import { bind } from './bridge';
import { draw } from './draw';
import { ProgramEditor } from './ProgramEditor';
import { ProgramsTree } from './ProgramsTree';
import { anyDirty, usePrograms } from './programs';
// Extracted by the build into `logic.css` beside `entry.js`, which `stylesheet()` links.
import './logic.css';

export const MODULE_ID = 'org.mythos.logic';
/** The editor's kind; its target is the program's name. */
export const EDITOR = 'logic-program';
/** Sent by the logic module when a project's programs change. */
export const PROGRAMS_CHANGED_EVENT = 'LOGIC_PROGRAMS_CHANGED';

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

  // The open project's programs are read when it opens, and dropped when it closes.
  const load = (projectId: string | null) => {
    if (projectId === null) usePrograms.getState().reset();
    else void usePrograms.getState().load(projectId);
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
      // here (a Function Block Diagram is not edited here yet, ADR-013 L3c-1).
      const ladder = state.programs.find(
        (doc) => doc.entry.name === program && (doc.entry.language === 'ld' || doc.entry.language === 'fbd')
      );
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
    host.register.editor({
      kind: EDITOR,
      live: true,
      title: (name) => ['Programs', name ?? ''],
      mount: (container, { target }) =>
        draw(container, <ProgramEditor name={target ?? ''} onRemoved={(name) => host.closeEditor(EDITOR, name)} />),
    })
  );

  return () => {
    for (const stop of stops.reverse()) stop();
    usePrograms.getState().reset();
  };
}

const logic: WorkbenchModule = { id: MODULE_ID, activate };
export default logic;
