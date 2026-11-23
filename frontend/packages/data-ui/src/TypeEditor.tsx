// The editor of one user type: its Structured Text source, checked as a draft, saved and
// deleted through the data module. What is shown as saved is what the module gave back; a
// refusal is shown in the module's own words, with its diagnostics in the Diagnostics tab.

import { useEffect, useMemo, useRef } from 'react';
import { Check, Save, Trash2 } from 'lucide-react';
import type { DiagnosticsPanel } from '@mythos/workbench-sdk';
import { host, mayEngineer, type Diagnostic } from './bridge';
import { draw } from './draw';
import { isOfThisType, layoutText, NOT_CHECKING, NOT_KNOWN, placeOf, sameText } from './model';
import { isDirty, useData, type TypeDoc } from './store';

/** The owner of this editor's panel in the Diagnostics tab. */
export const DIAGNOSTICS_OWNER = 'data-type-check';

/** Said before a type is deleted: the data module's own words for what follows (it never sees programs). */
export const deleteQuestion = (name: string) =>
  `Delete user type ${name}? A program that declares it no longer compiles; nothing is rebuilt.`;

/** The text offset of `line`:`col` (from 1) in `text`. */
function offsetOf(text: string, line: number, col: number): number {
  let at = 0;
  for (let l = 1; l < line; l += 1) {
    const next = text.indexOf('\n', at);
    if (next < 0) return text.length;
    at = next + 1;
  }
  return Math.min(at + Math.max(col - 1, 0), text.length);
}

/** Put the caret of the type's source at a diagnostic of its own text. */
function goTo(name: string, d: Diagnostic): void {
  const area = [...document.querySelectorAll('textarea[data-type-source]')].find(
    (element) => (element as HTMLElement).dataset.typeSource === name
  );
  if (!(area instanceof HTMLTextAreaElement)) return;
  const at = offsetOf(area.value, d.line, d.col);
  area.focus();
  area.setSelectionRange(at, at);
}

/** How many problems the Diagnostics tab shows for `doc`; `null` before anything was checked or refused. */
export function problemCount(doc: TypeDoc | undefined): number | null {
  if (!doc) return null;
  const refused = doc.refusal?.refusal.diagnostics ?? null;
  const checked = doc.check === null ? null : doc.check.answer.ok ? 0 : doc.check.answer.diagnostics.length;
  if (refused === null && checked === null) return null;
  return (refused?.length ?? 0) + (checked ?? 0);
}

function DiagnosticItem({ d, name, testId }: { d: Diagnostic; name: string; testId: string }) {
  if (isOfThisType(d, name)) {
    return (
      <li data-testid={testId} data-own="true">
        <button type="button" className="btn btn--flat" onClick={() => goTo(name, d)}>
          <span className="mono">{placeOf(d, name)}</span> {d.message}
        </button>
      </li>
    );
  }
  // Of another file, or of the whole: listed with where it is, never placed in this text.
  return (
    <li data-testid={testId} data-own="false">
      <span className="mono">{placeOf(d, name)}</span> {d.message}
    </li>
  );
}

/** The Diagnostics tab's panel: the reasons of a refused save, and the last check. */
function DiagnosticsCard({ name }: { name: string }) {
  const doc = useData((state) => state.docs[name]);
  if (!doc) return null;
  const refusal = doc.refusal;
  const check = doc.check;
  return (
    <div className="data-check" data-testid="type-diagnostics">
      {refusal && (
        <>
          <p data-testid="refusal-error">
            {refusal.of === 'save' ? 'The save was refused' : 'The delete was refused'}: {refusal.refusal.error}
          </p>
          {refusal.refusal.diagnostics && (
            <ul className="data-diagnostics" aria-label="Why it was refused">
              {refusal.refusal.diagnostics.map((d, index) => (
                <DiagnosticItem key={index} d={d} name={name} testId="refusal-diagnostic" />
              ))}
            </ul>
          )}
        </>
      )}
      {check === null ? (
        <p className="muted" data-testid="check-none">
          Not checked yet.
        </p>
      ) : (
        <>
          {!sameText(check.text, doc.draft) && (
            <p className="muted" data-testid="check-stale">
              This is the check of an earlier text. The text has changed since.
            </p>
          )}
          {check.answer.ok ? (
            <p data-testid="check-ok">The project's types and data blocks check with this text.</p>
          ) : (
            <ul className="data-diagnostics" aria-label="Diagnostics">
              {check.answer.diagnostics.map((d, index) => (
                <DiagnosticItem key={index} d={d} name={name} testId="diagnostic" />
              ))}
            </ul>
          )}
        </>
      )}
    </div>
  );
}

function CheckBadge({ doc }: { doc: TypeDoc }) {
  if (doc.checking) {
    return (
      <span className="badge badge--neutral" data-testid="type-check">
        CHECKING
      </span>
    );
  }
  if (doc.check === null) {
    return (
      <span className="badge badge--neutral" data-testid="type-check">
        NOT CHECKED
      </span>
    );
  }
  const stale = !sameText(doc.check.text, doc.draft);
  const tone = stale ? 'neutral' : doc.check.answer.ok ? 'good' : 'bad';
  const text = doc.check.answer.ok ? 'CHECKS' : 'DOES NOT CHECK';
  return (
    <button type="button" className="btn btn--flat" onClick={() => host().showDiagnostics()} title="Show the diagnostics">
      <span className={`badge badge--${tone}`} data-testid="type-check">
        {stale ? `${text} (EARLIER TEXT)` : text}
      </span>
    </button>
  );
}

/** What the last save answered: whether it made the type, and which blocks it laid out anew. */
function SaveAnswer({ answer }: { answer: NonNullable<TypeDoc['saveAnswer']> }) {
  const { changed } = answer;
  return (
    <div className="notice" data-testid="save-answer">
      <p>
        {answer.created ? 'Saved; the module made the type.' : 'Saved.'}{' '}
        <span data-testid="save-changed">
          {changed === null
            ? 'Which blocks changed is not known (the project did not check before this save).'
            : changed.length === 0
              ? "No data block's layout changed."
              : `Data blocks laid out anew or new: ${changed.join(', ')}.`}
        </span>
      </p>
    </div>
  );
}

/** The blocks of this type, with their layout as the module last answered it. */
function BlocksOfType({ name }: { name: string }) {
  const blocks = useData((state) => state.blocks);
  const blocksOk = useData((state) => state.blocksOk);
  const blocksError = useData((state) => state.blocksError);
  if (blocks === null) {
    return blocksError === null ? (
      <p className="muted editor__footnote">Reading the data blocks…</p>
    ) : (
      <p className="notice notice--bad" data-testid="type-blocks-error">
        The data blocks could not be read: {blocksError}
      </p>
    );
  }
  const mine = blocks.filter((block) => block.type.toUpperCase() === name.toUpperCase());
  if (mine.length === 0) {
    return (
      <p className="muted editor__footnote" data-testid="type-blocks-none">
        No data block of the project is of this type.
      </p>
    );
  }
  return (
    <>
      {blocksOk === false && (
        <p className="muted editor__footnote" data-testid="type-blocks-not-known">
          Layouts and sizes are {NOT_KNOWN}: {NOT_CHECKING}.
        </p>
      )}
      <table className="grid" aria-label={`Data blocks of type ${name}`}>
        <thead>
          <tr>
            <th>Data block</th>
            <th>Layout</th>
            <th>Size</th>
            <th>Retain</th>
          </tr>
        </thead>
        <tbody>
          {mine.map((block) => (
            <tr key={block.name} data-testid="type-block">
              <td>{block.name}</td>
              <td className="mono">{layoutText(block)}</td>
              <td className="mono">{block.size === undefined ? NOT_KNOWN : `${block.size} bytes`}</td>
              <td>{block.retain ? 'retained' : 'not retained'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

export function TypeEditor({ name, onRemoved }: { name: string; onRemoved: (name: string) => void }) {
  const doc = useData((state) => state.docs[name]);
  const projectId = useData((state) => state.projectId);
  const open = useData((state) => state.open);
  const edit = useData((state) => state.edit);
  const check = useData((state) => state.check);
  const save = useData((state) => state.save);
  const remove = useData((state) => state.remove);
  const missing = doc === undefined;

  useEffect(() => {
    if (missing && projectId !== null) void open(name);
  }, [missing, projectId, name, open]);

  const mount = useMemo<DiagnosticsPanel['mount']>(() => (panel) => draw(panel, <DiagnosticsCard name={name} />), [name]);
  const count = problemCount(doc);
  const countRef = useRef(count);
  countRef.current = count;
  useEffect(() => {
    host().setDiagnostics(DIAGNOSTICS_OWNER, { title: `Check: ${name}`, mount, count: countRef.current });
    return () => host().setDiagnostics(DIAGNOSTICS_OWNER, null);
  }, [name, mount]);
  // Only the count changes after a check: the same mount, so nothing is drawn again.
  useEffect(() => {
    host().setDiagnostics(DIAGNOSTICS_OWNER, { title: `Check: ${name}`, mount, count });
  }, [name, mount, count]);

  if (!doc || doc.phase === 'reading') {
    return <p className="editor__loading muted">Reading {name}…</p>;
  }
  // A file that is not text (a 500) may be replaced or deleted, as the module allows; any other
  // failed read leaves nothing to edit.
  const replacing = doc.phase === 'unreadable' && doc.readError?.status === 500;
  if (doc.phase === 'unreadable' && !replacing) {
    return (
      <div className="notice notice--bad" role="alert" data-testid="type-read-error">
        <div>
          <h3>The user type {name} could not be read</h3>
          <p>{doc.readError?.error}</p>
        </div>
      </div>
    );
  }

  const may = mayEngineer();
  const dirty = isDirty(doc);
  const working = doc.busy !== null;
  const refusal = doc.refusal;

  return (
    <div className="editor__column" data-testid="type-editor">
      <div className="editor__tools">
        {may && (
          <button
            type="button"
            className="btn btn--primary"
            disabled={!dirty || working}
            onClick={() => void save(name)}
            data-testid="type-save"
          >
            <Save size={13} aria-hidden /> Save
          </button>
        )}
        {may && !doc.isNew && (
          <button
            type="button"
            className="btn"
            disabled={working}
            data-testid="type-delete"
            onClick={() => {
              if (!window.confirm(deleteQuestion(name))) return;
              void remove(name).then((gone) => {
                if (gone) onRemoved(name);
              });
            }}
          >
            <Trash2 size={13} aria-hidden /> Delete
          </button>
        )}
        {may && <span className="toolbar__sep" aria-hidden />}
        <button
          type="button"
          className="btn"
          disabled={working}
          data-testid="type-check-button"
          onClick={() => void check(name)}
        >
          <Check size={13} aria-hidden /> Check
        </button>
        <CheckBadge doc={doc} />
        {doc.isNew && (
          <span className="badge badge--warn" data-testid="type-new">
            NOT SAVED YET
          </span>
        )}
        {dirty && !doc.isNew && (
          <span className="badge badge--warn" data-testid="type-unsaved">
            UNSAVED
          </span>
        )}
        {!may && (
          <span className="muted" data-testid="type-read-only">
            Only an engineer changes user types.
          </span>
        )}
      </div>
      {replacing && (
        <div className="notice notice--bad" role="alert" data-testid="type-read-error">
          <p>
            The stored file of {name} could not be read as text: {doc.readError?.error}. The text below is not the stored
            one; saving replaces the file with it.
          </p>
        </div>
      )}
      {doc.readBack !== null && (
        <div className="notice notice--warn" role="alert" data-testid="type-read-back">
          <p>
            Saved; the stored text could not be read back: {doc.readBack}. The text below is what was sent, not what the
            module stored.
          </p>
        </div>
      )}
      {doc.elsewhere !== null && (
        <div className="notice notice--warn" role="alert" data-testid="type-elsewhere">
          <p>
            {doc.elsewhere === 'deleted'
              ? `${name} was deleted elsewhere since it was opened; saving makes it again.`
              : `${name} was changed elsewhere since it was opened; saving replaces that change.`}
          </p>
        </div>
      )}
      {refusal && (
        <div className="notice notice--bad" role="alert" data-testid={refusal.of === 'save' ? 'save-refused' : 'delete-refused'}>
          <p>{refusal.refusal.error}</p>
          {refusal.refusal.diagnostics && (
            <p>
              <button type="button" className="btn btn--flat" onClick={() => host().showDiagnostics()}>
                {refusal.refusal.diagnostics.length} reasons, in the Diagnostics tab
              </button>
            </p>
          )}
        </div>
      )}
      {doc.saveAnswer && <SaveAnswer answer={doc.saveAnswer} />}
      {doc.checkError !== null && (
        <div className="notice notice--bad" role="alert" data-testid="check-error">
          <p>The check could not be made: {doc.checkError}</p>
        </div>
      )}
      <div className="editor__scroll data-editor">
        <textarea
          className="mono data-editor__source"
          aria-label={`Source of ${name}`}
          data-testid="type-source"
          data-type-source={name}
          spellCheck={false}
          readOnly={!may}
          value={doc.draft}
          onChange={(event) => edit(name, event.target.value)}
        />
        <h3 className="editor__section">Data blocks of this type</h3>
        <BlocksOfType name={name} />
        <p className="muted editor__footnote">
          The data module does not see programs: a program that declares this type no longer compiles when the type
          changes or is deleted, and nothing is rebuilt.
        </p>
      </div>
    </div>
  );
}
