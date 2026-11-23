// The editor of one library source: its Structured Text in the program editor's text editor,
// checked with the whole library before it is saved, and deleted through the logic module. What
// is shown as saved is what the module gave back; a refusal is shown in the module's own words.
// A check that is ok says what was checked, and nothing of programs: they are checked apart.

import { useEffect, useMemo, useRef } from 'react';
import { Check, Save, Trash2, Undo2 } from 'lucide-react';
import type { DiagnosticsPanel } from '@mythos/workbench-sdk';
import { host, mayEngineer, type Diagnostic } from './bridge';
import { draw } from './draw';
import {
  deleteQuestion,
  isOfThisSource,
  LIBRARY_EDITOR,
  librarySourceOf,
  placeOf,
  sameText,
  saveAnywayQuestion,
  WITHHELD,
} from './library';
import { isDirty, useLibrary, type LibraryDoc } from './librarystore';
import { SourceEditor, type SourceEditorHandle } from './SourceEditor';

/** The owner of this editor's panel in the Diagnostics tab. */
export const LIBRARY_DIAGNOSTICS_OWNER = 'logic-library-check';

const NONE: Diagnostic[] = [];

/** How many diagnostics the Diagnostics tab lists for `doc`; `null` before anything was checked. */
export function problemCount(doc: LibraryDoc | undefined): number | null {
  if (!doc || doc.check === null) return null;
  return doc.check.answer.ok ? 0 : doc.check.answer.diagnostics.length;
}

/** One diagnostic of a check, as listed: a place to go to when it is one of the library's own texts. */
function DiagnosticItem({ d, name, listed }: { d: Diagnostic; name: string; listed: string[] | null }) {
  const other = librarySourceOf(d, listed);
  if (d.line > 0 && isOfThisSource(d, name)) {
    return (
      <li>
        <button
          type="button"
          className="logic-diagnostic"
          data-testid="library-diagnostic"
          data-kind="own"
          onClick={() => useLibrary.getState().goTo(name, d.line, d.col)}
        >
          <span className="mono logic-diagnostic__at">{placeOf(d, name)}</span>
          {d.message}
        </button>
      </li>
    );
  }
  if (other !== null && other !== name) {
    return (
      <li>
        <button
          type="button"
          className="logic-diagnostic"
          data-testid="library-diagnostic"
          data-kind="library"
          onClick={() => {
            if (d.line > 0) useLibrary.getState().goTo(other, d.line, d.col);
            host().openEditor(LIBRARY_EDITOR, other);
          }}
        >
          <span className="mono logic-diagnostic__at">{placeOf(d, name)}</span>
          {d.message}
        </button>
      </li>
    );
  }
  // Of a type's file, of the whole, or of a source not listed: where it is, never placed in a text.
  return (
    <li className="logic-diagnostic logic-diagnostic--plain" data-testid="library-diagnostic" data-kind="plain">
      <span className="mono logic-diagnostic__at">{placeOf(d, name)}</span>
      {d.message}
    </li>
  );
}

/** The Diagnostics tab's panel: the last check of the library with this source's text. */
function CheckCard({ name }: { name: string }) {
  const doc = useLibrary((state) => state.docs[name]);
  const listed = useLibrary((state) => state.names);
  if (!doc) return null;
  const { check } = doc;
  if (check === null) {
    return (
      <p className="logic-check muted" data-testid="library-check-none">
        Not checked yet.
      </p>
    );
  }
  const problems = check.answer.project_problems ?? [];
  return (
    <div className="logic-check" data-testid="library-check-result">
      {!sameText(check.text, doc.draft) && (
        <p className="muted" data-testid="library-check-stale">
          This is the check of an earlier text. The text has changed since.
        </p>
      )}
      {check.answer.ok ? (
        <>
          <p data-testid="library-check-ok">The library checks with this text, with the project's types and data blocks.</p>
          <p className="muted" data-testid="library-check-footnote">
            Programs are not checked here: a program that declares a name the library declares, or uses what this text no
            longer has, is refused at its own check.
          </p>
        </>
      ) : (
        <ul className="logic-diagnostics" aria-label="Diagnostics">
          {check.answer.diagnostics.map((d, index) => (
            <DiagnosticItem key={index} d={d} name={name} listed={listed} />
          ))}
        </ul>
      )}
      {problems.length > 0 && (
        <>
          <p className="muted" data-testid="library-project-problems-note">
            The project's types and data blocks do not check, so the library was checked without them.
          </p>
          <ul className="logic-diagnostics" aria-label="Problems in the project's types and data blocks">
            {problems.map((d, index) => (
              <li key={index} className="logic-diagnostic logic-diagnostic--plain" data-testid="library-project-problem">
                <span className="mono logic-diagnostic__at">{placeOf(d, name)}</span>
                {d.message}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function CheckBadge({ doc }: { doc: LibraryDoc }) {
  if (doc.checking || doc.check === null) {
    return (
      <span className="badge badge--neutral" data-testid="library-check">
        {doc.checking ? 'CHECKING' : 'NOT CHECKED'}
      </span>
    );
  }
  const stale = !sameText(doc.check.text, doc.draft);
  const tone = stale ? 'neutral' : doc.check.answer.ok ? 'good' : 'bad';
  const text = doc.check.answer.ok ? 'CHECKS' : 'DOES NOT CHECK';
  return (
    <button type="button" className="btn btn--flat" onClick={() => host().showDiagnostics()} title="Show the diagnostics">
      <span className={`badge badge--${tone}`} data-testid="library-check">
        {stale ? `${text} (EARLIER TEXT)` : text}
      </span>
    </button>
  );
}

export function LibraryEditor({ name, onClose }: { name: string; onClose: (name: string) => void }) {
  const doc = useLibrary((state) => state.docs[name]);
  const projectId = useLibrary((state) => state.projectId);
  const pending = useLibrary((state) => state.pending);
  const { open, edit, discard, check, save, remove, wentTo } = useLibrary.getState();
  const editor = useRef<SourceEditorHandle>(null);
  const missing = doc === undefined;
  const phase = doc?.phase;

  useEffect(() => {
    if (missing && projectId !== null) void open(name);
  }, [missing, projectId, name, open]);

  // A place asked for in this source (a diagnostic's) is gone to once its text is shown.
  useEffect(() => {
    if (pending === null || pending.name !== name || phase === undefined || phase === 'reading') return;
    if (phase === 'ready') editor.current?.goTo(pending.line, pending.col);
    wentTo();
  }, [pending, name, phase, wentTo]);

  const mount = useMemo<DiagnosticsPanel['mount']>(() => (panel) => draw(panel, <CheckCard name={name} />), [name]);
  const count = problemCount(doc);
  const countRef = useRef(count);
  countRef.current = count;
  useEffect(() => {
    host().setDiagnostics(LIBRARY_DIAGNOSTICS_OWNER, { title: `Check: ${name}`, mount, count: countRef.current });
    return () => host().setDiagnostics(LIBRARY_DIAGNOSTICS_OWNER, null);
  }, [name, mount]);
  // Only the count changes after a check: the same mount, so nothing is drawn again.
  useEffect(() => {
    host().setDiagnostics(LIBRARY_DIAGNOSTICS_OWNER, { title: `Check: ${name}`, mount, count });
  }, [name, mount, count]);

  if (!doc || doc.phase === 'reading') {
    return <p className="editor__loading muted">Reading {name}…</p>;
  }
  // A stored file that cannot be read (a 500: not text) may be replaced or deleted, as the module
  // allows; any other failed read (a 404) leaves nothing to edit.
  const replacing = doc.phase === 'unreadable' && doc.readError?.status === 500;
  if (doc.phase === 'unreadable' && !replacing) {
    return (
      <div className="notice notice--bad" role="alert" data-testid="library-read-error">
        <div>
          <h3>The library source {name} could not be read</h3>
          <p>{doc.readError?.error}</p>
        </div>
      </div>
    );
  }

  const may = mayEngineer();
  const dirty = isDirty(doc);
  const working = doc.busy !== null;
  const refusal = doc.refusal;
  const current = doc.check !== null && sameText(doc.check.text, doc.draft) ? doc.check.answer : null;
  const drawn = current === null || current.ok ? NONE : current.diagnostics.filter((d) => d.line > 0 && isOfThisSource(d, name));

  return (
    <div className="editor__column" data-testid="library-editor">
      <div className="editor__tools">
        {may && (
          <button
            type="button"
            className="btn btn--primary"
            disabled={!(dirty || replacing) || working}
            onClick={() => void save(name)}
            data-testid="library-save"
          >
            <Save size={13} aria-hidden /> Save
          </button>
        )}
        {may && (
          <button
            type="button"
            className="btn"
            disabled={!dirty || working}
            data-testid="library-discard"
            onClick={() => {
              if (discard(name)) onClose(name);
            }}
          >
            <Undo2 size={13} aria-hidden /> Discard
          </button>
        )}
        {may && !doc.isNew && (
          <button
            type="button"
            className="btn"
            disabled={working}
            data-testid="library-delete"
            onClick={() => {
              if (!window.confirm(deleteQuestion(name))) return;
              void remove(name).then((gone) => {
                if (gone) onClose(name);
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
          data-testid="library-check-button"
          onClick={() => void check(name)}
        >
          <Check size={13} aria-hidden /> Check
        </button>
        <CheckBadge doc={doc} />
        {doc.isNew && (
          <span className="badge badge--warn" data-testid="library-new">
            NOT SAVED YET
          </span>
        )}
        {dirty && !doc.isNew && (
          <span className="badge badge--warn" data-testid="library-unsaved">
            UNSAVED
          </span>
        )}
        {!may && (
          <span className="muted" data-testid="library-read-only">
            Only an engineer changes the library.
          </span>
        )}
      </div>
      {replacing && (
        <div className="notice notice--bad" role="alert" data-testid="library-read-error">
          <p>
            The stored file of {name} could not be read as text: {doc.readError?.error}. The text below is not the stored
            text; saving replaces the file.
          </p>
        </div>
      )}
      {doc.readBack !== null && (
        <div className="notice notice--warn" role="alert" data-testid="library-read-back">
          <p>
            Saved; the stored text could not be read back: {doc.readBack}. The text below is what was sent, not what the
            module stored.
          </p>
        </div>
      )}
      {doc.elsewhere !== null && (
        <div className="notice notice--warn" role="alert" data-testid="library-elsewhere">
          <p>
            {doc.elsewhere === 'deleted'
              ? `${name} was deleted elsewhere since it was opened; saving makes it again.`
              : `${name} was changed elsewhere since it was opened; saving replaces that change.`}
          </p>
        </div>
      )}
      {doc.held && (
        <div className="notice notice--warn" role="alert" data-testid="save-held">
          <p>Not saved: with this text the library does not check, and every program of the project would be refused.</p>
          {may && (
            <p>
              <button
                type="button"
                className="btn"
                disabled={working}
                data-testid="library-save-anyway"
                onClick={() => {
                  if (!window.confirm(saveAnywayQuestion(name))) return;
                  void save(name, true);
                }}
              >
                Save anyway
              </button>
            </p>
          )}
        </div>
      )}
      {refusal && (
        <div className="notice notice--bad" role="alert" data-testid={refusal.of === 'save' ? 'save-refused' : 'delete-refused'}>
          <p>{refusal.refusal.error}</p>
          {refusal.refusal.withheld && <p data-testid="refusal-withheld">{WITHHELD}</p>}
        </div>
      )}
      {doc.saveAnswer && (
        <div className="notice" data-testid="library-saved">
          <p>{doc.saveAnswer.created ? 'Saved; the module made the source.' : 'Saved.'}</p>
        </div>
      )}
      {doc.checkRefusal !== null && (
        <div className="notice notice--bad" role="alert" data-testid="check-refused">
          <p>{doc.checkRefusal.error}</p>
        </div>
      )}
      <div className="logic-editor">
        <div className="logic-editor__source">
          <SourceEditor ref={editor} source={doc.draft} readOnly={!may} diagnostics={drawn} onChange={(text) => edit(name, text)} />
        </div>
      </div>
    </div>
  );
}
