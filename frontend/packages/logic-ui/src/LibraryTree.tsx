// The "Library" section of the project tree: the open project's library sources as the logic
// module listed them, and a row to add one. A list that could not be read says so, with the
// module's words, and lists nothing. No name is refused here: the module answers its own rule.

import { useState } from 'react';
import { FileCode2, Library, Plus } from 'lucide-react';
import { mayEngineer, useProject } from './bridge';
import { sameName } from './library';
import { anyDirty, isDirty, useLibrary, type StoredCheck } from './librarystore';
import { TreeNode } from './ProgramsTree';

const unsavedMark = (
  <span className="tree__mark tree__mark--warn" title="Unsaved changes">
    *
  </span>
);

const stMark = (
  <span className="tree__mark" data-testid="tree-library-language" title="Structured Text">
    ST
  </span>
);

/**
 * The state of the stored library, on the "Library" node: what the module answered to
 * `POST /library/check {}`. A library that does not check refuses every program, so it is said
 * with the count the module gave; one that could not be checked is NOT CHECKED, with why; one
 * that checks carries no mark, since that says nothing of the programs (each is checked apart).
 */
function StoredState({ stored }: { stored: StoredCheck | null }) {
  if (stored === null) return null;
  if (stored.answer === null) {
    return (
      <span className="badge badge--neutral" data-testid="tree-library-state" title={`The library could not be checked: ${stored.error}`}>
        NOT CHECKED
      </span>
    );
  }
  if (stored.answer.ok) return null;
  const count = stored.answer.diagnostics.length;
  return (
    <span
      className="badge badge--bad"
      data-testid="tree-library-state"
      title="Every program of the project is refused while the library does not check."
    >
      DOES NOT CHECK · {count} {count === 1 ? 'PROBLEM' : 'PROBLEMS'}
    </span>
  );
}

/**
 * The "Add new source" row: becomes a name field, and opens an editor on that name on Enter.
 * Nothing is written here: the module stores the source when the editor's first save is
 * answered, and a name it refuses is refused there, in its own words.
 */
function AddSource({ onName }: { onName: (name: string) => void }) {
  const [name, setName] = useState<string | null>(null);
  const trimmed = (name ?? '').trim();

  const submit = () => {
    if (trimmed === '') return;
    setName(null);
    onName(trimmed);
  };

  if (name === null) {
    return (
      <div
        role="treeitem"
        aria-selected={false}
        tabIndex={0}
        data-testid="tree-add-library"
        className="tree__node tree__node--action"
        style={{ paddingLeft: 6 + 2 * 16 }}
        onClick={() => setName('')}
        onKeyDown={(event) => event.key === 'Enter' && setName('')}
      >
        <span className="tree__twisty" />
        <span className="tree__icon">
          <Plus size={14} />
        </span>
        <span className="tree__label">Add new source</span>
      </div>
    );
  }

  return (
    <div className="tree__node tree__node--input" style={{ paddingLeft: 6 + 2 * 16 }}>
      <span className="tree__twisty" />
      <input
        autoFocus
        aria-label="Library source name"
        placeholder="Library source name"
        value={name}
        onChange={(event) => setName(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') submit();
          if (event.key === 'Escape') setName(null);
        }}
        onBlur={() => {
          if (trimmed === '') setName(null);
        }}
      />
    </div>
  );
}

export function LibraryTree({ openEditor }: { openEditor: (name: string) => void }) {
  const project = useProject((state) => state.project);
  const names = useLibrary((state) => state.names);
  const namesError = useLibrary((state) => state.namesError);
  const docs = useLibrary((state) => state.docs);
  const openNew = useLibrary((state) => state.openNew);
  const dirty = useLibrary(anyDirty);
  const stored = useLibrary((state) => state.stored);
  const [open, setOpen] = useState(true);

  if (!project) return null;
  const unsaved = Object.values(docs).filter((doc) => doc.isNew && !(names ?? []).some((n) => sameName(n, doc.name)));

  return (
    <>
      <TreeNode
        depth={1}
        icon={<Library size={14} />}
        label="Library"
        testId="tree-library"
        expanded={open}
        onToggle={() => setOpen(!open)}
        mark={
          <>
            {dirty && unsavedMark}
            <StoredState stored={stored} />
          </>
        }
      />
      {open && names !== null && mayEngineer() && (
        <AddSource
          onName={(name) => {
            // A name the module lists, in any case (its names are one name in any case), opens
            // the stored source; any other starts a new one, which writes nothing.
            const listed = names.find((n) => n === name) ?? names.find((n) => sameName(n, name));
            if (listed === undefined) openNew(name);
            openEditor(listed ?? name);
          }}
        />
      )}
      {open &&
        names?.map((name) => {
          const doc = docs[name];
          return (
            <TreeNode
              key={name}
              depth={2}
              icon={<FileCode2 size={14} />}
              label={name}
              testId="tree-library-source"
              onOpen={() => openEditor(name)}
              mark={
                <>
                  {doc && isDirty(doc) && unsavedMark}
                  {stMark}
                </>
              }
            />
          );
        })}
      {open &&
        unsaved.map((doc) => (
          <TreeNode
            key={`new:${doc.name}`}
            depth={2}
            icon={<FileCode2 size={14} />}
            label={`${doc.name} (not saved yet)`}
            testId="tree-library-new"
            onOpen={() => openEditor(doc.name)}
            mark={
              <>
                {unsavedMark}
                {stMark}
              </>
            }
          />
        ))}
      {open && names === null && namesError === null && (
        <p className="tree__note" data-testid="tree-library-reading">
          Reading the library…
        </p>
      )}
      {open && namesError !== null && (
        <p className="tree__note" role="alert" data-testid="tree-library-error">
          The library could not be read: {namesError}
        </p>
      )}
      {open && names !== null && names.length === 0 && (
        <p className="tree__note" data-testid="tree-library-empty">
          No library sources yet.
        </p>
      )}
    </>
  );
}
