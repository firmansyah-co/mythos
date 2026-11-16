import { ChevronRight, X } from 'lucide-react';
import { editorPath } from '../data/derive';
import { useTelemetry } from '../data/telemetry';
import { useWorkbench, type EditorRef } from '../data/workbench';
import { ConnectionDiagnostics } from '../editors/ConnectionDiagnostics';
import { ConnectionEditor } from '../editors/ConnectionEditor';
import { ConfigBar, DevicesAndNetworks } from '../editors/DevicesAndNetworks';
import { ProjectOverview } from '../editors/ProjectOverview';
import { TagTable } from '../editors/TagTable';
import { ModuleMount } from '../modules/ModuleMount';
import { moduleEditor, useModuleUi } from '../modules/registry';
import { useEditorNames } from './useEditorNames';

/** An editor a module registered; says so plainly when no loaded module has it. */
function ModuleEditorBody({ editor }: { editor: EditorRef }) {
  const entry = useModuleUi((state) => moduleEditor(state, editor.kind));
  const phase = useModuleUi((state) => state.phase);
  const project = useWorkbench((state) => state.project);
  if (!entry || !project) {
    return (
      <p className="muted" data-testid="module-editor-missing">
        {phase === 'ready'
          ? `No enabled module provides the editor “${editor.kind}”.`
          : 'The modules are loading.'}
      </p>
    );
  }
  return (
    <ModuleMount
      mount={entry.item.mount}
      context={{ project: { id: project.id, name: project.name }, target: editor.target }}
      deps={[project.id, editor.target]}
      className="module-mount module-mount--editor"
      testId={`module-editor-${editor.kind}`}
    />
  );
}

function EditorBody({ editor }: { editor: EditorRef }) {
  switch (editor.kind) {
    case 'overview':
      return <ProjectOverview />;
    case 'devices':
      return <DevicesAndNetworks />;
    case 'connection':
      return (
        <div className="editor__column">
          <ConfigBar />
          <ConnectionEditor connectionKey={editor.target ?? ''} />
        </div>
      );
    case 'tags':
      return <TagTable connectionId={editor.target} />;
    case 'diagnostics':
      return editor.target ? (
        <ConnectionDiagnostics connectionId={editor.target} />
      ) : null;
    default:
      return <ModuleEditorBody editor={editor} />;
  }
}


/** The work area: the active editor, under a header that says what it is and whether it is live. */
export function WorkArea() {
  const project = useWorkbench((state) => state.project);
  const editors = useWorkbench((state) => state.editors);
  const activeEditorId = useWorkbench((state) => state.activeEditorId);
  const closeEditor = useWorkbench((state) => state.closeEditor);
  const stream = useTelemetry((state) => state.stream);
  const names = useEditorNames();
  const modules = useModuleUi();

  const editor = editors.find((candidate) => candidate.id === activeEditorId);

  if (!project || !editor) {
    return (
      <section className="workarea workarea--empty" aria-label="Work area">
        <p className="muted">No editor is open. Double-click an item in the project tree.</p>
      </section>
    );
  }

  const path = editorPath(editor, project.name, names, modules);
  // The tag table and a module's live editors (an HMI screen) show values from the stream.
  const live = editor.kind === 'tags' || moduleEditor(modules, editor.kind)?.item.live === true;
  const online = stream === 'OPEN' && live;

  return (
    <section className="workarea" aria-label="Work area">
      <header className={`workarea__header${online ? ' is-online' : ''}`}>
        <div className="workarea__path">
          {path.map((segment, index) => (
            <span key={index} className="workarea__segment">
              {index > 0 && <ChevronRight size={12} aria-hidden />}
              {segment}
            </span>
          ))}
        </div>
        {online && <span className="workarea__online">ONLINE</span>}
        <button
          type="button"
          className="icon-btn workarea__close"
          aria-label="Close editor"
          onClick={() => closeEditor(editor.id)}
        >
          <X size={14} />
        </button>
      </header>
      <div className="workarea__body">
        <EditorBody editor={editor} />
      </div>
    </section>
  );
}
