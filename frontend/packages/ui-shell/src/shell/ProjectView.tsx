import { ChevronLeft, ChevronRight, LayoutGrid, MonitorPlay, Plug, Unplug, X } from 'lucide-react';
import { editorTitle } from '../data/derive';
import { isDirty, useIoConfig } from '../data/ioConfig';
import { useLog } from '../data/log';
import { useRuntimeScope } from '../data/scope';
import { anyScreenDirty, useScreens } from '../data/screens';
import { goOffline, goOnline, useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';
import { Inspector } from './Inspector';
import { MenuBar, type Menu } from './MenuBar';
import { ProjectTree } from './ProjectTree';
import { OnlineTools, Toolbox } from './TaskCards';
import { useEditorNames } from './useEditorNames';
import { useResizable } from './useResizable';
import { WorkArea } from './WorkArea';

const STREAM_WORDS = {
  IDLE: { label: 'OFFLINE', tone: 'neutral' },
  CONNECTING: { label: 'CONNECTING…', tone: 'warn' },
  OPEN: { label: 'ONLINE', tone: 'online' },
  CLOSED: { label: 'CONNECTION LOST — RETRYING', tone: 'bad' },
} as const;

/** A collapsed side pane: a strip that names the pane and reopens it. */
function CollapsedPane({
  label,
  side,
  onExpand,
}: {
  label: string;
  side: 'left' | 'right';
  onExpand: () => void;
}) {
  return (
    <button
      type="button"
      className={`pane-strip pane-strip--${side}`}
      aria-label={`Show ${label}`}
      onClick={onExpand}
    >
      <span className="pane-strip__label">{label}</span>
    </button>
  );
}

export function ProjectView({ onAbout }: { onAbout: () => void }) {
  const project = useWorkbench((state) => state.project);
  const panes = useWorkbench((state) => state.panes);
  const editors = useWorkbench((state) => state.editors);
  const activeEditorId = useWorkbench((state) => state.activeEditorId);
  const togglePane = useWorkbench((state) => state.togglePane);
  const focusEditor = useWorkbench((state) => state.focusEditor);
  const closeEditor = useWorkbench((state) => state.closeEditor);
  const closeProject = useWorkbench((state) => state.closeProject);
  const showPortal = useWorkbench((state) => state.showPortal);
  const showSettings = useWorkbench((state) => state.showSettings);
  const startRuntime = useWorkbench((state) => state.startRuntime);
  const stream = useTelemetry((state) => state.stream);
  const wantOnline = useTelemetry((state) => state.wantOnline);
  const lastLog = useLog((state) => state.entries[state.entries.length - 1]);
  const running = useRuntimeScope().kind === 'active';
  const ioDirty = useIoConfig(isDirty);
  const screensDirty = useScreens(anyScreenDirty);
  const dirty = ioDirty || screensDirty;
  const names = useEditorNames();
  const activeEditor = editors.find((editor) => editor.id === activeEditorId);

  /** Close the project, unless there are unsaved changes the engineer wants to keep. */
  const close = () => {
    if (
      dirty &&
      !window.confirm('This project has unsaved changes. Close it and lose them?')
    ) {
      return;
    }
    goOffline();
    closeProject();
  };

  const tree = useResizable({ storageKey: 'mythos.pane.tree', initial: 280, min: 180, max: 520, grow: 'right' });
  const cards = useResizable({ storageKey: 'mythos.pane.cards', initial: 260, min: 200, max: 480, grow: 'left' });
  const inspector = useResizable({ storageKey: 'mythos.pane.inspector', initial: 230, min: 110, max: 560, grow: 'up' });

  if (!project) return null;

  const menus: Menu[] = [
    {
      label: 'Project',
      items: [
        { label: 'Open or create a project…', run: showPortal },
        { label: 'Close project', run: close },
        { label: 'Settings…', run: () => showSettings() },
      ],
    },
    {
      label: 'View',
      items: [
        { label: 'Project tree', checked: panes.tree, run: () => togglePane('tree') },
        { label: 'Inspector window', checked: panes.inspector, run: () => togglePane('inspector') },
        { label: 'Task cards', checked: panes.taskCards, run: () => togglePane('taskCards') },
      ],
    },
    {
      label: 'Online',
      items: [
        { label: 'Go online', disabled: wantOnline || !running, run: goOnline },
        { label: 'Go offline', disabled: !wantOnline, run: goOffline },
        { label: 'Start runtime', run: () => startRuntime() },
      ],
    },
    { label: 'Help', items: [{ label: 'About MythOS Workbench', run: onAbout }] },
  ];

  const words = STREAM_WORDS[stream];

  return (
    <div className="pv" data-testid="project-view">
      <header className="titlebar">
        <img className="titlebar__logo" src="/favicon.svg" alt="" />
        <span className="titlebar__product">MythOS Workbench</span>
        <span className="titlebar__sep">—</span>
        <span className="titlebar__project" title={project.id}>
          {project.name}
        </span>
      </header>

      <MenuBar menus={menus} />

      <div className="toolbar" role="toolbar" aria-label="Online">
        <button
          type="button"
          className="btn btn--flat"
          disabled={wantOnline || !running}
          title={running ? undefined : 'Only a project the controller is running can be read online'}
          onClick={goOnline}
        >
          <Plug size={14} aria-hidden /> Go online
        </button>
        <button type="button" className="btn btn--flat" disabled={!wantOnline} onClick={goOffline}>
          <Unplug size={14} aria-hidden /> Go offline
        </button>
        <span className="toolbar__sep" aria-hidden />
        <button
          type="button"
          className="btn btn--flat"
          title="Show the released screens as an operator sees them"
          onClick={() => startRuntime()}
        >
          <MonitorPlay size={14} aria-hidden /> Start runtime
        </button>
        <span className="toolbar__sep" aria-hidden />
        <span className={`badge badge--${words.tone}`} data-testid="toolbar-stream">
          {words.label}
        </span>
      </div>

      <div className="pv__body">
        {panes.tree ? (
          <>
            <aside className="pane pane--tree" style={{ width: tree.size }}>
              <header className="pane__header">
                <span className="pane__title">Project tree</span>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label="Collapse project tree"
                  onClick={() => togglePane('tree')}
                >
                  <ChevronLeft size={14} />
                </button>
              </header>
              <div className="pane__body">
                <ProjectTree />
              </div>
            </aside>
            <div className="splitter splitter--v" role="separator" aria-orientation="vertical" {...tree.handle} />
          </>
        ) : (
          <CollapsedPane label="Project tree" side="left" onExpand={() => togglePane('tree')} />
        )}

        <div className="pv__center">
          <WorkArea />
          {panes.inspector && (
            <div className="splitter splitter--h" role="separator" aria-orientation="horizontal" {...inspector.handle} />
          )}
          <div
            className="pv__inspector"
            style={panes.inspector ? { height: inspector.size } : undefined}
          >
            <Inspector collapsed={!panes.inspector} />
          </div>
        </div>

        {panes.taskCards ? (
          <>
            <div className="splitter splitter--v" role="separator" aria-orientation="vertical" {...cards.handle} />
            <aside className="pane pane--cards" style={{ width: cards.size }}>
              <header className="pane__header">
                <span className="pane__title">Task cards</span>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label="Collapse task cards"
                  onClick={() => togglePane('taskCards')}
                >
                  <ChevronRight size={14} />
                </button>
              </header>
              <div className="pane__body">
                {activeEditor?.kind === 'screen' && activeEditor.target && (
                  <Toolbox screenId={activeEditor.target} />
                )}
                <OnlineTools />
              </div>
            </aside>
          </>
        ) : (
          <CollapsedPane label="Task cards" side="right" onExpand={() => togglePane('taskCards')} />
        )}
      </div>

      <footer className="editorbar">
        <button type="button" className="editorbar__portal" onClick={showPortal}>
          <LayoutGrid size={13} aria-hidden /> Portal view
        </button>
        <div className="editorbar__tabs" role="tablist" aria-label="Open editors">
          {editors.map((editor) => (
            <div
              key={editor.id}
              className={`editorbar__tab${editor.id === activeEditorId ? ' is-active' : ''}`}
            >
              <button
                type="button"
                role="tab"
                aria-selected={editor.id === activeEditorId}
                className="editorbar__tab-label"
                onClick={() => focusEditor(editor.id)}
              >
                {editorTitle(editor, names)}
              </button>
              <button
                type="button"
                className="editorbar__tab-close"
                aria-label={`Close ${editorTitle(editor, names)}`}
                onClick={() => closeEditor(editor.id)}
              >
                <X size={11} />
              </button>
            </div>
          ))}
        </div>
        <span className={`editorbar__status${lastLog ? ` is-${lastLog.level}` : ''}`} title={lastLog?.text}>
          {lastLog?.text ?? ''}
        </span>
      </footer>
    </div>
  );
}
