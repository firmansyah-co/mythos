import { ChevronRight, X } from 'lucide-react';
import { editorPath } from '../data/derive';
import { useIoConfig } from '../data/ioConfig';
import { useTelemetry } from '../data/telemetry';
import { useWorkbench, type EditorRef } from '../data/workbench';
import { ConnectionDiagnostics } from '../editors/ConnectionDiagnostics';
import { ConnectionEditor } from '../editors/ConnectionEditor';
import { ConfigBar, DevicesAndNetworks } from '../editors/DevicesAndNetworks';
import { ProjectOverview } from '../editors/ProjectOverview';
import { TagTable } from '../editors/TagTable';

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
          <ConnectionEditor connectionKey={editor.connectionId ?? ''} />
        </div>
      );
    case 'tags':
      return <TagTable connectionId={editor.connectionId} />;
    case 'diagnostics':
      return editor.connectionId ? (
        <ConnectionDiagnostics connectionId={editor.connectionId} />
      ) : null;
  }
}


/** The work area: the active editor, under a header that says what it is and whether it is live. */
export function WorkArea() {
  const project = useWorkbench((state) => state.project);
  const editors = useWorkbench((state) => state.editors);
  const activeEditorId = useWorkbench((state) => state.activeEditorId);
  const closeEditor = useWorkbench((state) => state.closeEditor);
  const stream = useTelemetry((state) => state.stream);
  const draft = useIoConfig((state) => state.draft);
  const nameOf = (key: string) => draft.find((connection) => connection.key === key)?.id;

  const editor = editors.find((candidate) => candidate.id === activeEditorId);

  if (!project || !editor) {
    return (
      <section className="workarea workarea--empty" aria-label="Work area">
        <p className="muted">No editor is open. Double-click an item in the project tree.</p>
      </section>
    );
  }

  const path = editorPath(editor, project.name, nameOf);
  // The tag table is the editor whose values arrive on the telemetry stream.
  const online = stream === 'OPEN' && editor.kind === 'tags';

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
