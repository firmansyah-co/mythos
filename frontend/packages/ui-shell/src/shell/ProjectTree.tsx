import { useState } from 'react';
import type { ReactNode } from 'react';
import {
  Activity,
  ChevronDown,
  ChevronRight,
  Cable,
  FolderOpen,
  Monitor,
  MonitorSmartphone,
  Network,
  Plus,
  Settings2,
  Tags,
} from 'lucide-react';
import { connectionLabel, currentProblems } from '../data/derive';
import { isDirty, useIoConfig } from '../data/ioConfig';
import { useRuntimeScope } from '../data/scope';
import { anyScreenDirty, useScreens } from '../data/screens';
import { useWorkbench, type EditorKind } from '../data/workbench';

interface NodeProps {
  depth: number;
  icon: ReactNode;
  label: string;
  /** A short state shown at the right edge, e.g. a fault count. */
  mark?: ReactNode;
  selected?: boolean;
  expanded?: boolean;
  onToggle?: () => void;
  onSelect?: () => void;
  onOpen?: () => void;
  testId?: string;
}

function TreeNode({
  depth,
  icon,
  label,
  mark,
  selected,
  expanded,
  onToggle,
  onSelect,
  onOpen,
  testId,
}: NodeProps) {
  return (
    <div
      role="treeitem"
      aria-selected={selected ?? false}
      aria-expanded={expanded}
      tabIndex={0}
      data-testid={testId}
      className={`tree__node${selected ? ' is-selected' : ''}`}
      style={{ paddingLeft: 6 + depth * 16 }}
      onClick={onSelect}
      onDoubleClick={onOpen}
      onKeyDown={(event) => {
        if (event.key === 'Enter') onOpen?.();
        if (event.key === 'ArrowRight' && expanded === false) onToggle?.();
        if (event.key === 'ArrowLeft' && expanded === true) onToggle?.();
      }}
    >
      <span
        className="tree__twisty"
        onClick={(event) => {
          event.stopPropagation();
          onToggle?.();
        }}
      >
        {expanded === undefined ? null : expanded ? (
          <ChevronDown size={13} />
        ) : (
          <ChevronRight size={13} />
        )}
      </span>
      <span className="tree__icon">{icon}</span>
      <span className="tree__label" title={label}>
        {label}
      </span>
      {mark}
    </div>
  );
}

/** The "Add new screen" row: becomes a name field, and creates the screen on Enter. */
function AddScreen() {
  const create = useScreens((state) => state.create);
  const busy = useScreens((state) => state.busy);
  const openEditor = useWorkbench((state) => state.openEditor);
  const [name, setName] = useState<string | null>(null);

  const submit = async () => {
    const trimmed = (name ?? '').trim();
    if (trimmed === '') return;
    const id = await create(trimmed);
    if (id) {
      setName(null);
      openEditor('screen', id);
    }
  };

  if (name === null) {
    return (
      <div
        role="treeitem"
        aria-selected={false}
        tabIndex={0}
        data-testid="tree-add-screen"
        className="tree__node tree__node--action"
        style={{ paddingLeft: 6 + 2 * 16 }}
        onClick={() => setName('')}
        onKeyDown={(event) => event.key === 'Enter' && setName('')}
      >
        <span className="tree__twisty" />
        <span className="tree__icon">
          <Plus size={14} />
        </span>
        <span className="tree__label">Add new screen</span>
      </div>
    );
  }

  return (
    <div className="tree__node tree__node--input" style={{ paddingLeft: 6 + 2 * 16 }}>
      <span className="tree__twisty" />
      <input
        autoFocus
        aria-label="Screen name"
        placeholder="Screen name"
        value={name}
        disabled={busy === 'creating'}
        onChange={(event) => setName(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') void submit();
          if (event.key === 'Escape') setName(null);
        }}
        onBlur={() => name.trim() === '' && setName(null)}
      />
    </div>
  );
}

/**
 * The project tree.
 *
 * The connections listed are the open project's own configuration, so every project has them.
 * The runtime nodes under a connection (its live tags and diagnostics) appear only when the
 * open project is the one the controller runs: for any other project they would show another
 * project's field devices.
 */
export function ProjectTree() {
  const project = useWorkbench((state) => state.project);
  const selection = useWorkbench((state) => state.selection);
  const activeEditorId = useWorkbench((state) => state.activeEditorId);
  const select = useWorkbench((state) => state.select);
  const openEditor = useWorkbench((state) => state.openEditor);
  const scope = useRuntimeScope();
  const draft = useIoConfig((state) => state.draft);
  const dirty = useIoConfig(isDirty);
  const screens = useScreens((state) => state.screens);
  const screensPhase = useScreens((state) => state.phase);
  const screensDirty = useScreens(anyScreenDirty);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  if (!project) return null;

  const io = scope.kind === 'active' ? scope.io : null;
  const problems = currentProblems(io);
  const isOpen = (key: string) => !collapsed.has(key);
  const toggle = (key: string) =>
    setCollapsed((previous) => {
      const next = new Set(previous);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  const open = (kind: EditorKind, connectionId?: string) => () => openEditor(kind, connectionId);
  const editorIs = (id: string) => activeEditorId === id;

  return (
    <div className="tree" role="tree" aria-label="Project tree">
      <TreeNode
        depth={0}
        icon={<FolderOpen size={14} />}
        label={project.name}
        testId="tree-project"
        expanded={isOpen('project')}
        onToggle={() => toggle('project')}
        selected={selection?.kind === 'project'}
        onSelect={() => select({ kind: 'project' })}
        onOpen={open('overview')}
      />

      {isOpen('project') && (
        <TreeNode
          depth={1}
          icon={<Network size={14} />}
          label="Devices & networks"
          testId="tree-devices"
          selected={editorIs('devices') && selection === null}
          onSelect={() => select(null)}
          onOpen={open('devices')}
          mark={
            dirty ? (
              <span className="tree__mark tree__mark--warn" title="Unsaved changes">
                *
              </span>
            ) : undefined
          }
        />
      )}

      {isOpen('project') &&
        draft.map((connection) => {
          const key = `connection:${connection.key}`;
          const id = connection.id.trim();
          // The runtime half of a connection exists only for a running project, and only once
          // the controller has loaded a connection with this name.
          const status = io?.connections.find((candidate) => candidate.id === id);
          const faults = problems.filter((p) => p.connectionId === id).length;
          return (
            <div key={connection.key} role="group">
              <TreeNode
                depth={1}
                icon={<Cable size={14} />}
                label={`${connectionLabel(connection.id)} [${connection.driver}]`}
                testId="tree-connection"
                expanded={isOpen(key)}
                onToggle={() => toggle(key)}
                selected={
                  status !== undefined &&
                  selection?.kind === 'connection' &&
                  selection.id === status.id
                }
                onSelect={() => select(status ? { kind: 'connection', id: status.id } : null)}
                onOpen={open('connection', connection.key)}
                mark={
                  status && faults > 0 ? (
                    <span className="tree__mark tree__mark--warn" title={`${faults} problem(s)`}>
                      {faults}
                    </span>
                  ) : undefined
                }
              />
              {isOpen(key) && (
                <TreeNode
                  depth={2}
                  icon={<Settings2 size={14} />}
                  label="Configuration"
                  selected={editorIs(`connection:${connection.key}`) && selection === null}
                  onSelect={() => select(null)}
                  onOpen={open('connection', connection.key)}
                />
              )}
              {isOpen(key) && status?.state === 'RUNNING' && (
                <TreeNode
                  depth={2}
                  icon={<Tags size={14} />}
                  label={`Tags (${status.tags.length})`}
                  testId="tree-runtime-tags"
                  selected={editorIs(`tags:${status.id}`) && selection === null}
                  onSelect={() => select(null)}
                  onOpen={open('tags', status.id)}
                />
              )}
              {isOpen(key) && status && (
                <TreeNode
                  depth={2}
                  icon={<Activity size={14} />}
                  label="Online & diagnostics"
                  testId="tree-runtime-diagnostics"
                  selected={editorIs(`diagnostics:${status.id}`) && selection === null}
                  onSelect={() => select(null)}
                  onOpen={open('diagnostics', status.id)}
                />
              )}
            </div>
          );
        })}

      {isOpen('project') && io && (
        <TreeNode
          depth={1}
          icon={<Tags size={14} />}
          label="All tags"
          testId="tree-all-tags"
          selected={editorIs('tags') && selection === null}
          onSelect={() => select(null)}
          onOpen={open('tags')}
        />
      )}

      {isOpen('project') && (
        <TreeNode
          depth={1}
          icon={<MonitorSmartphone size={14} />}
          label="HMI screens"
          testId="tree-screens"
          expanded={isOpen('screens')}
          onToggle={() => toggle('screens')}
          onSelect={() => select(null)}
          mark={
            screensDirty ? (
              <span className="tree__mark tree__mark--warn" title="Unsaved changes">
                *
              </span>
            ) : undefined
          }
        />
      )}
      {isOpen('project') && isOpen('screens') && <AddScreen />}
      {isOpen('project') &&
        isOpen('screens') &&
        screens.map((screen) => (
          <TreeNode
            key={screen.id}
            depth={2}
            icon={<Monitor size={14} />}
            label={screen.name}
            testId="tree-screen"
            selected={editorIs(`screen:${screen.id}`)}
            onSelect={() => select(null)}
            onOpen={open('screen', screen.id)}
          />
        ))}
      {isOpen('project') && isOpen('screens') && screensPhase === 'unreadable' && (
        <p className="tree__note" data-testid="tree-screens-unreadable">
          The screens could not be read.
        </p>
      )}

      {isOpen('project') && scope.kind !== 'active' && (
        <p className="tree__note" data-testid="tree-not-active">
          {scope.kind === 'unknown' && 'The controller has not answered yet.'}
          {scope.kind === 'unreachable' && 'The controller cannot be reached.'}
          {scope.kind === 'not-active' &&
            'Not running on the controller. Open the project overview to activate it.'}
        </p>
      )}
    </div>
  );
}
