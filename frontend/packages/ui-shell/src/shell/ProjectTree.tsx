import { useMemo, useState } from 'react';
import type { ReactNode } from 'react';
import {
  Activity,
  ChevronDown,
  ChevronRight,
  Cable,
  Factory,
  FolderOpen,
  Network,
  Settings2,
  Tags,
} from 'lucide-react';
import { connectionLabel, currentProblems } from '../data/derive';
import { isDirty, useIoConfig } from '../data/ioConfig';
import { useRuntimeScope } from '../data/scope';
import { useWorkbench, type EditorKind } from '../data/workbench';
import { ModuleMount } from '../modules/ModuleMount';
import { orderedTreeSections, useModuleUi } from '../modules/registry';

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
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const registered = useModuleUi((state) => state.treeSections);
  const sections = useMemo(() => orderedTreeSections(registered), [registered]);

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
          icon={<Factory size={14} />}
          label="Plant"
          testId="tree-plant"
          selected={editorIs('plant') && selection === null}
          onSelect={() => select(null)}
          onOpen={open('plant')}
        />
      )}

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

      {isOpen('project') &&
        sections.map((section) => (
          <ModuleMount
            key={`${section.module}:${section.item.id}`}
            mount={section.item.mount}
            context={{ project: { id: project.id, name: project.name } }}
            deps={[project.id, project.name]}
            className="tree__module"
            testId={`tree-module-${section.item.id}`}
          />
        ))}

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
