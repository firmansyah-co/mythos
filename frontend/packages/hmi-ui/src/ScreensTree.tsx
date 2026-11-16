// The "HMI screens" section of the project tree: the open project's screens, and a row to add one.

import { useState } from 'react';
import type { ReactNode } from 'react';
import { ChevronDown, ChevronRight, Monitor, MonitorSmartphone, Plus } from 'lucide-react';
import { useWorkbench } from './bridge';
import { anyScreenDirty, useScreens } from './screens';

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
function AddScreen({ openEditor }: { openEditor: (kind: string, id: string) => void }) {
  const create = useScreens((state) => state.create);
  const busy = useScreens((state) => state.busy);
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


export function ScreensTree({ openEditor }: { openEditor: (kind: string, id: string) => void }) {
  const project = useWorkbench((state) => state.project);
  const screens = useScreens((state) => state.screens);
  const screensPhase = useScreens((state) => state.phase);
  const screensDirty = useScreens(anyScreenDirty);
  const [open, setOpen] = useState(true);

  if (!project) return null;

  return (
    <>
      <TreeNode
        depth={1}
        icon={<MonitorSmartphone size={14} />}
        label="HMI screens"
        testId="tree-screens"
        expanded={open}
        onToggle={() => setOpen(!open)}
        mark={
          screensDirty ? (
            <span className="tree__mark tree__mark--warn" title="Unsaved changes">
              *
            </span>
          ) : undefined
        }
      />
      {open && <AddScreen openEditor={openEditor} />}
      {open &&
        screens.map((screen) => (
          <TreeNode
            key={screen.id}
            depth={2}
            icon={<Monitor size={14} />}
            label={screen.name}
            testId="tree-screen"
            onOpen={() => openEditor('screen', screen.id)}
          />
        ))}
      {open && screensPhase === 'unreadable' && (
        <p className="tree__note" data-testid="tree-screens-unreadable">
          The screens could not be read.
        </p>
      )}
    </>
  );
}
