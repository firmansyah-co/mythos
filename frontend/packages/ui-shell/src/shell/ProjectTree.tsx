import { useState } from 'react';
import type { ReactNode } from 'react';
import {
  Activity,
  ChevronDown,
  ChevronRight,
  Cable,
  FolderOpen,
  Network,
  Tags,
} from 'lucide-react';
import { currentProblems } from '../data/derive';
import { useRuntimeScope } from '../data/scope';
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

/**
 * The project tree.
 *
 * Runtime nodes (connections and their tags) appear only when the open project is the one the
 * controller runs. For any other project they would show another project's field devices.
 */
export function ProjectTree() {
  const project = useWorkbench((state) => state.project);
  const selection = useWorkbench((state) => state.selection);
  const activeEditorId = useWorkbench((state) => state.activeEditorId);
  const select = useWorkbench((state) => state.select);
  const openEditor = useWorkbench((state) => state.openEditor);
  const scope = useRuntimeScope();
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

      {isOpen('project') && io && (
        <>
          <TreeNode
            depth={1}
            icon={<Network size={14} />}
            label="Devices & networks"
            testId="tree-devices"
            selected={editorIs('devices') && selection === null}
            onSelect={() => select(null)}
            onOpen={open('devices')}
          />

          {io.connections.map((connection) => {
            const key = `connection:${connection.id}`;
            const faults = problems.filter((p) => p.connectionId === connection.id).length;
            return (
              <div key={connection.id} role="group">
                <TreeNode
                  depth={1}
                  icon={<Cable size={14} />}
                  label={`${connection.id} [${connection.driver}]`}
                  testId="tree-connection"
                  expanded={isOpen(key)}
                  onToggle={() => toggle(key)}
                  selected={selection?.kind === 'connection' && selection.id === connection.id}
                  onSelect={() => select({ kind: 'connection', id: connection.id })}
                  onOpen={open('tags', connection.id)}
                  mark={
                    faults > 0 ? (
                      <span className="tree__mark tree__mark--warn" title={`${faults} problem(s)`}>
                        {faults}
                      </span>
                    ) : undefined
                  }
                />
                {isOpen(key) && connection.state === 'RUNNING' && (
                  <TreeNode
                    depth={2}
                    icon={<Tags size={14} />}
                    label={`Tags (${connection.tags.length})`}
                    selected={editorIs(`tags:${connection.id}`) && selection === null}
                    onSelect={() => select(null)}
                    onOpen={open('tags', connection.id)}
                  />
                )}
                {isOpen(key) && (
                  <TreeNode
                    depth={2}
                    icon={<Activity size={14} />}
                    label="Online & diagnostics"
                    selected={editorIs(`diagnostics:${connection.id}`) && selection === null}
                    onSelect={() => select(null)}
                    onOpen={open('diagnostics', connection.id)}
                  />
                )}
              </div>
            );
          })}

          <TreeNode
            depth={1}
            icon={<Tags size={14} />}
            label="All tags"
            testId="tree-all-tags"
            selected={editorIs('tags') && selection === null}
            onSelect={() => select(null)}
            onOpen={open('tags')}
          />
        </>
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
