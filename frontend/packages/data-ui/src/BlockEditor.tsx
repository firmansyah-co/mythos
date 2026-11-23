// The editor of one data block: its type and whether it is retained, saved and deleted through
// the data module; its layout, size and members exactly as the module answered them; and how
// each runtime of the project's runtimes configuration that carries it compares with it. What is
// shown as saved is the module's last list, never what was chosen here.

import { useEffect } from 'react';
import { RefreshCw, Save, Trash2 } from 'lucide-react';
import { mayEngineer, type BlockAnswer, type Diagnostic } from './bridge';
import { carried, compareBlock, comparisonText, layoutText, NOT_CHECKING, NOT_KNOWN, retainOf, sameName } from './model';
import { chosen, isBlockDirty, savedBlock, useData, type BlockDoc } from './store';

/** Said before a block is deleted: the data module's own words for what follows (it never sees programs). */
export const deleteBlockQuestion = (name: string) =>
  `Delete data block ${name}? A program that declares it no longer compiles; nothing is rebuilt.`;

/** The heading of the comparison: configured, which is not running. */
export const RUNTIMES_HEADING = "Configured in the project's runtimes";

/** Where a reason of the compiler is: a file and a place, or the whole. A block has no text of its own. */
function whereOf(d: Diagnostic): string {
  const source = d.source ?? '';
  if (d.line === 0) return source === '' ? 'about the whole' : source;
  return source === '' ? `${d.line}:${d.col}` : `${source} ${d.line}:${d.col}`;
}

/** Why the saved block's layout is not known, when it is not. */
function whyNotKnown(saved: BlockAnswer | null, blocksOk: boolean | null): string {
  if (saved === null) return 'the block is not saved';
  return blocksOk === false ? NOT_CHECKING : 'the module answered none';
}

/** The type and retain fields. Nothing is chosen for a new block: the module asks both. */
function Choice({ doc, saved, may }: { doc: BlockDoc; saved: BlockAnswer | null; may: boolean }) {
  const types = useData((state) => state.types);
  const typesError = useData((state) => state.typesError);
  const editBlock = useData((state) => state.editBlock);
  const { type, retain } = chosen(doc, saved);
  const disabled = !may || doc.busy !== null;
  // The types the module listed, and the chosen or saved one when the list lacks it.
  const options = [...(types ?? [])];
  for (const extra of [type, saved?.type ?? null]) {
    if (extra !== null && !options.some((t) => sameName(t, extra))) options.push(extra);
  }
  return (
    <div className="data-block-form">
      <label className="field">
        <span className="field__label">Type</span>
        <select
          aria-label="Type"
          data-testid="block-type"
          value={type ?? ''}
          disabled={disabled}
          onChange={(event) => editBlock(doc.name, { type: event.target.value })}
        >
          {type === null && (
            <option value="" disabled>
              Choose a type
            </option>
          )}
          {options.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
        {typesError !== null && (
          <span className="field__error" data-testid="block-types-error">
            The user types could not be read: {typesError}
          </span>
        )}
      </label>
      <fieldset className="field data-block-form__retain" disabled={disabled} data-testid="block-retain">
        <span className="field__label">Retain</span>
        <label className="settings__check">
          <input
            type="radio"
            name={`retain-${doc.name}`}
            data-testid="block-retain-yes"
            checked={retain === true}
            onChange={() => editBlock(doc.name, { retain: true })}
          />
          Retained: its values are kept across a warm restart
        </label>
        <label className="settings__check">
          <input
            type="radio"
            name={`retain-${doc.name}`}
            data-testid="block-retain-no"
            checked={retain === false}
            onChange={() => editBlock(doc.name, { retain: false })}
          />
          Not retained
        </label>
        {retain === null && (
          <span className="field__hint" data-testid="block-retain-none">
            Not chosen: the data module asks for it.
          </span>
        )}
      </fieldset>
    </div>
  );
}

/** The saved block's layout, size and members, as the module answered them; NOT KNOWN with why otherwise. */
function Layout({ saved, dirty }: { saved: BlockAnswer | null; dirty: boolean }) {
  const blocksOk = useData((state) => state.blocksOk);
  const known = saved !== null && saved.layout !== undefined;
  const why = whyNotKnown(saved, blocksOk);
  return (
    <>
      {dirty && saved !== null && (
        <p className="muted editor__footnote" data-testid="block-layout-of-saved">
          The layout below is of the saved block, not of what is chosen above.
        </p>
      )}
      <dl className="props">
        <dt>Layout</dt>
        <dd className="mono" data-testid="block-layout">
          {saved === null ? NOT_KNOWN : layoutText(saved)}
        </dd>
        <dt>Size</dt>
        <dd className="mono" data-testid="block-size">
          {saved?.size === undefined ? NOT_KNOWN : `${saved.size} bytes`}
        </dd>
      </dl>
      {!known && (
        <p className="muted editor__footnote" data-testid="block-not-known">
          Layout, size and members are {NOT_KNOWN}: {why}.
        </p>
      )}
      {saved?.members === undefined ? (
        <p className="muted editor__footnote" data-testid="block-members-unknown">
          Members: {NOT_KNOWN}: {why}.
        </p>
      ) : (
        <table className="grid" aria-label="Members">
          <thead>
            <tr>
              <th>Member</th>
              <th>Type</th>
              <th>Offset</th>
              <th>Size</th>
            </tr>
          </thead>
          <tbody>
            {saved.members.map((member) => (
              <tr key={member.name} data-testid="block-member">
                <td>{member.name}</td>
                <td className="mono">{member.type}</td>
                <td className="mono">{member.offset}</td>
                <td className="mono">{member.size}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p className="muted editor__footnote">
        Members are the block&apos;s top-level members only; the members of a member are not listed.
      </p>
    </>
  );
}

/** Each configured runtime carrying the block, and what agrees or differs. Read only. */
function Runtimes({ name, saved }: { name: string; saved: BlockAnswer | null }) {
  const runtimes = useData((state) => state.runtimes);
  const runtimesError = useData((state) => state.runtimesError);
  const blocksOk = useData((state) => state.blocksOk);
  const readRuntimes = useData((state) => state.readRuntimes);

  let body;
  if (runtimesError !== null) {
    body = (
      <p className="notice notice--bad" data-testid="runtimes-error">
        The project&apos;s runtimes configuration could not be read: {runtimesError}. Which runtimes carry this block is{' '}
        {NOT_KNOWN}.
      </p>
    );
  } else if (runtimes === null) {
    body = <p className="muted editor__footnote">Reading the project&apos;s runtimes configuration…</p>;
  } else {
    const rows = runtimes.flatMap((instance) => {
      const configured = carried(instance, name);
      return configured === null ? [] : [{ id: instance.id, configured }];
    });
    body =
      rows.length === 0 ? (
        <p className="muted editor__footnote" data-testid="runtimes-none">
          No runtime of this project carries this block.
        </p>
      ) : (
        <>
          <table className="grid" aria-label="Runtimes that carry this block">
            <thead>
              <tr>
                <th>Runtime</th>
                <th>Type there</th>
                <th>Layout there</th>
                <th>Retain there</th>
                <th>Against the saved block</th>
              </tr>
            </thead>
            <tbody>
              {rows.map(({ id, configured }) => (
                <tr key={id} data-testid={`runtime-row-${id}`}>
                  <td>{id}</td>
                  <td className="mono">{configured.type}</td>
                  <td className="mono">{configured.layout}</td>
                  <td>{retainOf(configured) ? 'retained' : 'not retained'}</td>
                  <td data-testid="runtime-verdict">{comparisonText(compareBlock(saved, blocksOk === true, configured))}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="muted editor__footnote" data-testid="runtimes-footnote">
            An initial value changed in the type is not seen here: build the programs again to be sure.
          </p>
        </>
      );
  }

  return (
    <>
      <h3 className="editor__section" data-testid="runtimes-heading">
        {RUNTIMES_HEADING}
      </h3>
      <p className="muted editor__footnote">
        <span data-testid="runtimes-note">
          The configuration as the project saved it. Whether the controller runs this project and that configuration is
          not shown here.
        </span>{' '}
        <button type="button" className="btn btn--flat" data-testid="runtimes-read" onClick={() => void readRuntimes()}>
          <RefreshCw size={13} aria-hidden /> Read again
        </button>
      </p>
      {body}
    </>
  );
}

export function BlockEditor({ name, onRemoved }: { name: string; onRemoved: (name: string) => void }) {
  const doc = useData((state) => state.blockDocs[name]);
  const projectId = useData((state) => state.projectId);
  const blocks = useData((state) => state.blocks);
  const blocksError = useData((state) => state.blocksError);
  const saved = useData((state) => savedBlock(state, name));
  const openBlock = useData((state) => state.openBlock);
  const saveBlock = useData((state) => state.saveBlock);
  const removeBlock = useData((state) => state.removeBlock);
  const readRuntimes = useData((state) => state.readRuntimes);
  const missing = doc === undefined;

  useEffect(() => {
    if (missing && projectId !== null) openBlock(name);
  }, [missing, projectId, name, openBlock]);
  // The configuration is read again each time the editor opens: nothing announces a change of it.
  useEffect(() => {
    if (projectId !== null) void readRuntimes();
  }, [projectId, readRuntimes]);

  if (!doc) return <p className="editor__loading muted">Opening {name}…</p>;
  if (!doc.isNew && blocks === null) {
    return blocksError === null ? (
      <p className="editor__loading muted">Reading the data blocks…</p>
    ) : (
      <div className="notice notice--bad" role="alert" data-testid="block-read-error">
        <p>The data blocks could not be read: {blocksError}</p>
      </div>
    );
  }

  const may = mayEngineer();
  const dirty = isBlockDirty(doc, saved);
  const working = doc.busy !== null;
  const { type, retain } = chosen(doc, saved);
  const refusal = doc.refusal;

  if (!doc.isNew && saved === null && !dirty && doc.elsewhere === null) {
    return (
      <div className="notice notice--bad" role="alert" data-testid="block-missing">
        <p>The project has no data block {name}.</p>
      </div>
    );
  }

  return (
    <div className="editor__column" data-testid="block-editor">
      <div className="editor__tools">
        {may && (
          <button
            type="button"
            className="btn btn--primary"
            disabled={!dirty || working || type === null || retain === null}
            onClick={() => void saveBlock(name)}
            data-testid="block-save"
          >
            <Save size={13} aria-hidden /> Save
          </button>
        )}
        {may && !doc.isNew && (
          <button
            type="button"
            className="btn"
            disabled={working}
            data-testid="block-delete"
            onClick={() => {
              if (!window.confirm(deleteBlockQuestion(name))) return;
              void removeBlock(name).then((gone) => {
                if (gone) onRemoved(name);
              });
            }}
          >
            <Trash2 size={13} aria-hidden /> Delete
          </button>
        )}
        {may && <span className="toolbar__sep" aria-hidden />}
        {doc.isNew && (
          <span className="badge badge--warn" data-testid="block-new">
            NOT SAVED YET
          </span>
        )}
        {dirty && !doc.isNew && (
          <span className="badge badge--warn" data-testid="block-unsaved">
            UNSAVED
          </span>
        )}
        {!may && (
          <span className="muted" data-testid="block-read-only">
            Only an engineer changes data blocks.
          </span>
        )}
      </div>
      {doc.elsewhere !== null && (
        <div className="notice notice--warn" role="alert" data-testid="block-elsewhere">
          <p>
            {doc.elsewhere === 'deleted'
              ? `${name} was deleted elsewhere since it was opened; to make it again, choose its type and retain and save.`
              : `${name} was changed elsewhere since it was opened; saving replaces that change.`}
          </p>
        </div>
      )}
      {refusal && (
        <div className="notice notice--bad" role="alert" data-testid={refusal.of === 'save' ? 'save-refused' : 'delete-refused'}>
          <div>
            <p data-testid="block-refusal-error">{refusal.refusal.error}</p>
            {refusal.refusal.diagnostics && (
              <ul className="data-diagnostics" aria-label="Why it was refused">
                {refusal.refusal.diagnostics.map((d, index) => (
                  <li key={index} data-testid="block-refusal-diagnostic">
                    <span className="mono">{whereOf(d)}</span> {d.message}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      )}
      {doc.saveAnswer && (
        <div className="notice" data-testid="block-save-answer">
          <p>
            {doc.saveAnswer.created ? 'Saved; the module made the block.' : 'Saved.'}{' '}
            <span data-testid="block-save-changed">
              {doc.saveAnswer.changed === null
                ? 'Which blocks changed is not known (the project did not check before this save).'
                : doc.saveAnswer.changed.length === 0
                  ? "No data block's layout changed."
                  : `Data blocks laid out anew or new: ${doc.saveAnswer.changed.join(', ')}.`}
            </span>
          </p>
        </div>
      )}
      <div className="editor__scroll">
        <h3 className="editor__section">Data block</h3>
        <Choice doc={doc} saved={saved} may={may} />
        <h3 className="editor__section">Layout, as the module answered it</h3>
        <Layout saved={saved} dirty={dirty} />
        <Runtimes name={name} saved={saved} />
        <p className="muted editor__footnote">
          The data module does not see programs: a program that declares this block no longer compiles when its type
          changes or it is deleted, and nothing is rebuilt.
        </p>
      </div>
    </div>
  );
}
