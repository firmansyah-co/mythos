// The "Programs" section of the project tree: the open project's control programs, each with
// its language, and a row to add one in a language chosen there.

import { useState } from 'react';
import type { ReactNode } from 'react';
import { ChevronDown, ChevronRight, Cpu, FileCode2, Plus } from 'lucide-react';
import { mayEngineer, useProject, type Language } from './bridge';
import { anyDirty, isDirty, usePrograms } from './programs';

/** What a program may be called: what the logic module accepts. */
export const NAME = /^[A-Za-z][A-Za-z0-9_]{0,63}$/;

/** A language as the tree names it. */
export const LANGUAGE_LABEL: Record<string, string> = { st: 'ST', ld: 'LD' };
const LANGUAGE_TITLE: Record<string, string> = { st: 'Structured Text', ld: 'Ladder Diagram' };

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

/** The "Add new program" row: becomes a name field, and creates the program on Enter. */
function AddProgram({ openEditor }: { openEditor: (name: string) => void }) {
  const create = usePrograms((state) => state.create);
  const busy = usePrograms((state) => state.busy);
  const [name, setName] = useState<string | null>(null);
  const [language, setLanguage] = useState<Language>('st');
  const trimmed = (name ?? '').trim();
  const invalid = trimmed !== '' && !NAME.test(trimmed);

  const submit = async () => {
    if (trimmed === '' || invalid) return;
    const made = await create(trimmed, language);
    if (made) {
      setName(null);
      openEditor(made);
    }
  };

  if (name === null) {
    return (
      <div
        role="treeitem"
        aria-selected={false}
        tabIndex={0}
        data-testid="tree-add-program"
        className="tree__node tree__node--action"
        style={{ paddingLeft: 6 + 2 * 16 }}
        onClick={() => setName('')}
        onKeyDown={(event) => event.key === 'Enter' && setName('')}
      >
        <span className="tree__twisty" />
        <span className="tree__icon">
          <Plus size={14} />
        </span>
        <span className="tree__label">Add new program</span>
      </div>
    );
  }

  return (
    <>
      <div className="tree__node tree__node--input" style={{ paddingLeft: 6 + 2 * 16 }}>
        <span className="tree__twisty" />
        <input
          autoFocus
          aria-label="Program name"
          aria-invalid={invalid}
          placeholder="Program name"
          value={name}
          disabled={busy === 'creating'}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') void submit();
            if (event.key === 'Escape') setName(null);
          }}
          onBlur={(event) => {
            // Moving to the language keeps the name being typed.
            if (trimmed === '' && !event.currentTarget.parentElement?.contains(event.relatedTarget as Node | null)) {
              setName(null);
            }
          }}
        />
        <select
          aria-label="Program language"
          value={language}
          disabled={busy === 'creating'}
          onChange={(event) => setLanguage(event.target.value === 'ld' ? 'ld' : 'st')}
        >
          <option value="st">ST</option>
          <option value="ld">LD</option>
        </select>
      </div>
      {invalid && (
        <p className="tree__note" data-testid="tree-program-name-invalid">
          A letter, then letters, digits or _.
        </p>
      )}
    </>
  );
}

export function ProgramsTree({ openEditor }: { openEditor: (name: string) => void }) {
  const project = useProject((state) => state.project);
  const programs = usePrograms((state) => state.programs);
  const phase = usePrograms((state) => state.phase);
  const error = usePrograms((state) => state.error);
  const dirty = usePrograms(anyDirty);
  const [open, setOpen] = useState(true);

  if (!project) return null;

  return (
    <>
      <TreeNode
        depth={1}
        icon={<Cpu size={14} />}
        label="Programs"
        testId="tree-programs"
        expanded={open}
        onToggle={() => setOpen(!open)}
        mark={
          dirty ? (
            <span className="tree__mark tree__mark--warn" title="Unsaved changes">
              *
            </span>
          ) : undefined
        }
      />
      {open && phase === 'ready' && mayEngineer() && <AddProgram openEditor={openEditor} />}
      {open &&
        programs.map((doc) => (
          <TreeNode
            key={doc.entry.name}
            depth={2}
            icon={<FileCode2 size={14} />}
            label={doc.entry.name}
            testId="tree-program"
            onOpen={() => openEditor(doc.entry.name)}
            mark={
              <>
                {isDirty(doc) && (
                  <span className="tree__mark tree__mark--warn" title="Unsaved changes">
                    *
                  </span>
                )}
                <span
                  className="tree__mark"
                  data-testid="tree-program-language"
                  title={LANGUAGE_TITLE[doc.entry.language] ?? doc.entry.language}
                >
                  {LANGUAGE_LABEL[doc.entry.language] ?? doc.entry.language.toUpperCase()}
                </span>
              </>
            }
          />
        ))}
      {open && phase === 'reading' && <p className="tree__note">Reading the programs…</p>}
      {open && phase === 'unreadable' && (
        <p className="tree__note" data-testid="tree-programs-unreadable">
          The programs could not be read.
        </p>
      )}
      {open && phase === 'ready' && error && (
        <p className="tree__note" data-testid="tree-programs-error">
          {error}
        </p>
      )}
    </>
  );
}
