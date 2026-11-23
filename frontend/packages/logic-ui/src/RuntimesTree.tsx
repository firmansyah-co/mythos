// The "Runtimes" section of the project tree: the runtimes configured in the open project, as the
// logic module listed them in its file's order, and a row to add one. A list that could not be
// read says so, with the module's words, and lists nothing. No id is refused here: the module
// answers its own rule. A program running alone is not listed: it is under Programs.

import { useState } from 'react';
import { Cpu, Plus } from 'lucide-react';
import { mayEngineer, useProject } from './bridge';
import { sameId } from './runtimes';
import { anyRuntimeDirty, hasChanges, useRuntimes } from './runtimestore';
import { TreeNode } from './ProgramsTree';

const unsavedMark = (
  <span className="tree__mark tree__mark--warn" title="Unsaved changes">
    *
  </span>
);

/**
 * The "Add new runtime" row: becomes an id field, and opens an editor on that id on Enter.
 * Nothing is written here: the module stores the runtime when the editor's first save is
 * answered, and an id it refuses is refused there, in its own words.
 */
function AddRuntime({ onId }: { onId: (id: string) => void }) {
  const [id, setId] = useState<string | null>(null);
  const trimmed = (id ?? '').trim();

  const submit = () => {
    if (trimmed === '') return;
    setId(null);
    onId(trimmed);
  };

  if (id === null) {
    return (
      <div
        role="treeitem"
        aria-selected={false}
        tabIndex={0}
        data-testid="tree-add-runtime"
        className="tree__node tree__node--action"
        style={{ paddingLeft: 6 + 2 * 16 }}
        onClick={() => setId('')}
        onKeyDown={(event) => event.key === 'Enter' && setId('')}
      >
        <span className="tree__twisty" />
        <span className="tree__icon">
          <Plus size={14} />
        </span>
        <span className="tree__label">Add new runtime</span>
      </div>
    );
  }

  return (
    <div className="tree__node tree__node--input" style={{ paddingLeft: 6 + 2 * 16 }}>
      <span className="tree__twisty" />
      <input
        autoFocus
        aria-label="Runtime id"
        placeholder="Runtime id"
        value={id}
        onChange={(event) => setId(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') submit();
          if (event.key === 'Escape') setId(null);
        }}
        onBlur={() => {
          if (trimmed === '') setId(null);
        }}
      />
    </div>
  );
}

export function RuntimesTree({ openEditor }: { openEditor: (id: string) => void }) {
  const project = useProject((state) => state.project);
  const list = useRuntimes((state) => state.list);
  const listError = useRuntimes((state) => state.listError);
  const docs = useRuntimes((state) => state.docs);
  const openNew = useRuntimes((state) => state.openNew);
  const dirty = useRuntimes(anyRuntimeDirty);
  const [open, setOpen] = useState(true);

  if (!project) return null;
  const ids = list?.runtimes.map((r) => r.id) ?? null;
  const problems = list?.problems ?? [];
  // A new runtime is listed apart until the module lists it under its very id.
  const unsaved = Object.values(docs).filter((doc) => doc.isNew && !(ids ?? []).includes(doc.id));

  return (
    <>
      <TreeNode
        depth={1}
        icon={<Cpu size={14} />}
        label="Runtimes"
        testId="tree-runtimes"
        expanded={open}
        onToggle={() => setOpen(!open)}
        mark={
          <>
            {dirty && unsavedMark}
            {problems.length > 0 && (
              <span className="badge badge--bad" data-testid="tree-runtimes-problems" title={problems.join('\n')}>
                {problems.length} {problems.length === 1 ? 'PROBLEM' : 'PROBLEMS'}
              </span>
            )}
          </>
        }
      />
      {open && ids !== null && mayEngineer() && (
        <AddRuntime
          onId={(id) => {
            // An id the module lists, in any case (its ids are one id in any case), opens the
            // stored runtime; any other starts a new one, which writes nothing.
            const listed = ids.find((n) => n === id) ?? ids.find((n) => sameId(n, id));
            if (listed === undefined) openNew(id);
            openEditor(listed ?? id);
          }}
        />
      )}
      {open &&
        ids?.map((id) => {
          const doc = docs[id];
          return (
            <TreeNode
              key={id}
              depth={2}
              icon={<Cpu size={14} />}
              label={id}
              testId="tree-runtime"
              onOpen={() => openEditor(id)}
              mark={doc && hasChanges(doc) ? unsavedMark : undefined}
            />
          );
        })}
      {open &&
        unsaved.map((doc) => (
          <TreeNode
            key={`new:${doc.id}`}
            depth={2}
            icon={<Cpu size={14} />}
            label={`${doc.id} (not saved yet)`}
            testId="tree-runtime-new"
            onOpen={() => openEditor(doc.id)}
            mark={unsavedMark}
          />
        ))}
      {open && list === null && listError === null && (
        <p className="tree__note" data-testid="tree-runtimes-reading">
          Reading the runtimes…
        </p>
      )}
      {open && listError !== null && (
        <p className="tree__note" role="alert" data-testid="tree-runtimes-error">
          The runtimes could not be read: {listError}
        </p>
      )}
      {open && list !== null && list.runtimes.length === 0 && (
        <p className="tree__note" data-testid="tree-runtimes-empty">
          No runtime is configured: every program runs alone.
        </p>
      )}
    </>
  );
}
