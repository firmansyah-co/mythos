// The "Data" section of the project tree: the open project's user types and data blocks, each
// list as the data module answered it, and a row to add a type or a block. A list that could not
// be read says so, with the module's words, and lists nothing. Under Data blocks, each block a
// configured runtime carries that the project no longer has is said.

import { useState } from 'react';
import type { ReactNode } from 'react';
import { ChevronDown, ChevronRight, FileCode2, LayoutGrid, Plus, Square, Type } from 'lucide-react';
import { mayEngineer, useProject } from './bridge';
import { goneBlocks, sameName } from './model';
import { anyBlockDirty, anyDirty, isBlockDirty, isDirty, savedBlock, useData } from './store';

interface NodeProps {
  depth: number;
  icon: ReactNode;
  label: string;
  mark?: ReactNode;
  expanded?: boolean;
  onToggle?: () => void;
  onOpen?: () => void;
  testId?: string;
}

function TreeNode({ depth, icon, label, mark, expanded, onToggle, onOpen, testId }: NodeProps) {
  return (
    <div
      role="treeitem"
      aria-selected={false}
      aria-expanded={expanded}
      tabIndex={0}
      data-testid={testId}
      className="tree__node"
      style={{ paddingLeft: 6 + depth * 16 }}
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
        {expanded === undefined ? null : expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
      </span>
      <span className="tree__icon">{icon}</span>
      <span className="tree__label" title={label}>
        {label}
      </span>
      {mark}
    </div>
  );
}

/** The mark of a new type or block that has never been saved: its editor may be closed, its text is kept. */
const newMark = (
  <span className="tree__mark tree__mark--warn" title="Not saved yet">
    *
  </span>
);

const unsavedMark = (
  <span className="tree__mark tree__mark--warn" title="Unsaved changes">
    *
  </span>
);

/**
 * An "Add new …" row: becomes a name field, and opens an editor on that name on Enter. Nothing
 * is made here: the module stores it when the editor's first save is answered, and a name it
 * refuses is refused there, in its own words.
 */
function AddRow({ label, field, testId, onName }: { label: string; field: string; testId: string; onName: (name: string) => void }) {
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
        data-testid={testId}
        className="tree__node"
        style={{ paddingLeft: 6 + 3 * 16 }}
        onClick={() => setName('')}
        onKeyDown={(event) => event.key === 'Enter' && setName('')}
      >
        <span className="tree__twisty" />
        <span className="tree__icon">
          <Plus size={14} />
        </span>
        <span className="tree__label muted">{label}</span>
      </div>
    );
  }

  return (
    <div className="tree__node" style={{ paddingLeft: 6 + 3 * 16 }}>
      <span className="tree__twisty" />
      <input
        className="data-tree__input"
        autoFocus
        aria-label={field}
        placeholder={field}
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

export function DataTree({ openType, openBlock }: { openType: (name: string) => void; openBlock: (name: string) => void }) {
  const project = useProject((state) => state.project);
  const types = useData((state) => state.types);
  const typesError = useData((state) => state.typesError);
  const blocks = useData((state) => state.blocks);
  const blocksError = useData((state) => state.blocksError);
  const docs = useData((state) => state.docs);
  const blockDocs = useData((state) => state.blockDocs);
  const runtimes = useData((state) => state.runtimes);
  const runtimesError = useData((state) => state.runtimesError);
  const openNew = useData((state) => state.openNew);
  const openNewBlock = useData((state) => state.openNewBlock);
  const dirty = useData((state) => anyDirty(state) || anyBlockDirty(state));
  const [open, setOpen] = useState({ data: true, types: true, blocks: true });

  if (!project) return null;
  const may = mayEngineer();
  const toggle = (which: keyof typeof open) => setOpen({ ...open, [which]: !open[which] });

  return (
    <>
      <TreeNode
        depth={1}
        icon={<LayoutGrid size={14} />}
        label="Data"
        testId="tree-data"
        expanded={open.data}
        onToggle={() => toggle('data')}
        mark={dirty ? unsavedMark : undefined}
      />
      {open.data && (
        <>
          <TreeNode
            depth={2}
            icon={<Type size={14} />}
            label="User types"
            testId="tree-types"
            expanded={open.types}
            onToggle={() => toggle('types')}
          />
          {open.types && types !== null && may && (
            <AddRow
              label="Add new type"
              field="Type name"
              testId="tree-add-type"
              onName={(name) => {
                // A name the module lists, in any case (the module's names are one name in any
                // case), is opened as it is stored; any other starts a new type.
                const listed = types.find((t) => t === name) ?? types.find((t) => sameName(t, name));
                if (listed === undefined) openNew(name);
                openType(listed ?? name);
              }}
            />
          )}
          {open.types &&
            types?.map((name) => {
              const doc = docs[name];
              return (
                <TreeNode
                  key={name}
                  depth={3}
                  icon={<FileCode2 size={14} />}
                  label={name}
                  testId="tree-type"
                  onOpen={() => openType(name)}
                  mark={doc && isDirty(doc) ? unsavedMark : undefined}
                />
              );
            })}
          {open.types &&
            Object.values(docs)
              .filter((doc) => doc.isNew && !(types ?? []).some((t) => sameName(t, doc.name)))
              .map((doc) => (
                <TreeNode
                  key={`new:${doc.name}`}
                  depth={3}
                  icon={<FileCode2 size={14} />}
                  label={`${doc.name} (not saved yet)`}
                  testId="tree-type-new"
                  onOpen={() => openType(doc.name)}
                  mark={newMark}
                />
              ))}
          {open.types && types === null && typesError === null && <p className="tree__note">Reading the user types…</p>}
          {open.types && typesError !== null && (
            <p className="tree__note" role="alert" data-testid="tree-data-error" data-list="types">
              The user types could not be read: {typesError}
            </p>
          )}
          {open.types && types !== null && types.length === 0 && (
            <p className="tree__note" data-testid="tree-types-empty">
              No user types yet.
            </p>
          )}

          <TreeNode
            depth={2}
            icon={<Square size={14} />}
            label="Data blocks"
            testId="tree-blocks"
            expanded={open.blocks}
            onToggle={() => toggle('blocks')}
          />
          {open.blocks && blocks !== null && may && (
            <AddRow
              label="Add new data block"
              field="Data block name"
              testId="tree-add-block"
              onName={(name) => {
                // A name the module lists is opened as it is; any other starts a new block.
                const listed = savedBlock({ blocks }, name);
                if (listed === null) openNewBlock(name);
                openBlock(listed?.name ?? name);
              }}
            />
          )}
          {open.blocks &&
            blocks?.map((block) => {
              const doc = blockDocs[block.name];
              return (
                <TreeNode
                  key={block.name}
                  depth={3}
                  icon={<Square size={14} />}
                  label={block.name}
                  testId="tree-block"
                  onOpen={() => openBlock(block.name)}
                  mark={
                    <>
                      <span className="tree__mark" data-testid="tree-block-type" title="Its type">
                        {block.type}
                      </span>
                      {doc && isBlockDirty(doc, block) ? unsavedMark : null}
                    </>
                  }
                />
              );
            })}
          {open.blocks &&
            Object.values(blockDocs)
              .filter((doc) => doc.isNew && !(blocks ?? []).some((b) => sameName(b.name, doc.name)))
              .map((doc) => (
                <TreeNode
                  key={`new:${doc.name}`}
                  depth={3}
                  icon={<Square size={14} />}
                  label={`${doc.name} (not saved yet)`}
                  testId="tree-block-new"
                  onOpen={() => openBlock(doc.name)}
                  mark={newMark}
                />
              ))}
          {open.blocks &&
            blocks !== null &&
            runtimes !== null &&
            goneBlocks(blocks, runtimes).map(({ runtime, block }) => (
              <p key={`${runtime}/${block}`} className="tree__note" data-testid="tree-block-gone">
                Runtime {runtime} carries data block {block}, which the project no longer has.
              </p>
            ))}
          {open.blocks && runtimesError !== null && (
            <p className="tree__note" data-testid="tree-runtimes-error">
              Whether a runtime carries a data block the project no longer has is not known: the runtimes configuration
              could not be read: {runtimesError}
            </p>
          )}
          {open.blocks && blocks === null && blocksError === null && <p className="tree__note">Reading the data blocks…</p>}
          {open.blocks && blocksError !== null && (
            <p className="tree__note" role="alert" data-testid="tree-data-error" data-list="blocks">
              The data blocks could not be read: {blocksError}
            </p>
          )}
          {open.blocks && blocks !== null && blocks.length === 0 && (
            <p className="tree__note" data-testid="tree-blocks-empty">
              No data blocks yet.
            </p>
          )}
        </>
      )}
    </>
  );
}
