// The editor of one control program: its Structured Text, or its Ladder Diagram drawn read-only
// with its live state (LadderView), what the compiler says of it, its task settings, its
// variables with their bindings to tags, and what the controller's runtime is doing with it.
//
// Three things are kept apart on purpose, because an engineer acts on the difference: the text
// in the editor, the text that is saved, and the program that runs. Nothing that runs changes
// until "Load" builds the saved text and puts it into the project's runtimes.

import { useEffect, useMemo, useRef, useState } from 'react';
import { Check, CirclePlay, CircleStop, Download, Lock, LockOpen, Save, Trash2, Undo2, Upload } from 'lucide-react';
import { ForcedMark, TagValue } from '@mythos/workbench-sdk/react';
import {
  addBinding,
  bindingsOf,
  bindingsOfVariable,
  KIND_LABEL,
  orphans,
  parseSafeNumber,
  problemOf,
  removeBinding,
  setSafe,
  setTag,
  setWritable,
  type BindingRow,
  type Kind,
} from './bindings';
import {
  mayEngineer,
  useController,
  useProject,
  useTelemetry,
  type Diagnostic,
  type ForcedValue,
  type ProgramEntry,
  type Variable,
  type WatchedValue,
} from './bridge';
import { readLadder, type Ladder } from './ladder';
import { LadderView, type LadderLive } from './LadderView';
import {
  checkIsCurrent,
  describeStart,
  isDirty,
  loadOf,
  runState,
  usePrograms,
  type ProgramDoc,
  type RunState,
  type WatchView,
} from './programs';
import { forceable, formatVariable, hasOneValue, parseForced, shownForced } from './values';
import { SourceEditor, type SourceEditorHandle } from './SourceEditor';

/** How often the controller's runtimes are read while an editor is open. */
export const RUNTIMES_POLL_MS = 1000;
/** How often the running program's variables are read while its editor is open. */
export const WATCH_POLL_MS = 500;

const MODE_TONE: Record<string, string> = { run: 'good', stop: 'neutral', fault: 'bad', unreachable: 'bad' };

/** What the controller's runtime of this program is doing, or that it is not known. */
/** The variables the runtime says are forced, by name as the program compares them. */
function forcedOf(state: RunState): Map<string, ForcedValue> | null {
  if (state.kind !== 'loaded' || state.stale) return null;
  const forced = state.runtime.forced;
  if (!Array.isArray(forced)) return null;
  return new Map(forced.map((f) => [f.variable.replace(/\s+/g, '').toLowerCase(), f]));
}

/** What is forced, as the controller last said: UNKNOWN when it did not say. */
function ForcedFacts({ state }: { state: RunState & { kind: 'loaded' } }) {
  const forced = state.runtime.forced;
  if (!Array.isArray(forced)) {
    return (
      <dd className="mono" data-testid="runtime-forced" data-missing="true">
        UNKNOWN
      </dd>
    );
  }
  if (forced.length === 0) {
    return (
      <dd data-testid="runtime-forced" data-count="0">
        none
      </dd>
    );
  }
  return (
    <dd data-testid="runtime-forced" data-count={forced.length}>
      {forced.map((f) => (
        <div key={f.variable} className="mono" data-testid="runtime-forced-variable">
          {f.variable} = {shownForced(f.value)}
        </div>
      ))}
    </dd>
  );
}

function RuntimePanel({ name, state, busy }: { name: string; state: RunState; busy: boolean }) {
  const command = usePrograms((s) => s.command);
  const release = usePrograms((s) => s.release);
  const unload = usePrograms((s) => s.unload);
  const project = useProject((s) => s.project);
  const active = useController((s) => s.activeProjectId);
  const may = mayEngineer();
  const elsewhere = project && active !== undefined && active !== project.id;

  return (
    <section className="logic-panel" data-testid="program-runtime" aria-label="Runtime">
      <h3 className="logic-panel__title">Runtime</h3>
      {state.kind === 'unknown' && (
        <p>
          <span className="badge badge--neutral" data-testid="runtime-mode">
            UNKNOWN
          </span>{' '}
          <span className="muted" data-testid="runtime-unknown-why">
            {state.why}
          </span>
        </p>
      )}
      {state.kind === 'not-loaded' && (
        <p>
          <span className="badge badge--neutral" data-testid="runtime-mode">
            NOT LOADED
          </span>{' '}
          <span className="muted">No runtime of the controller runs a program called {name}.</span>
        </p>
      )}
      {state.kind === 'loaded' && (
        <>
          <p>
            <span
              className={`badge badge--${MODE_TONE[state.runtime.mode] ?? 'neutral'}`}
              data-testid="runtime-mode"
            >
              {state.runtime.mode.toUpperCase()}
            </span>{' '}
            {state.stale && (
              <span className="badge badge--warn" data-testid="runtime-stale">
                LAST KNOWN
              </span>
            )}{' '}
            {Array.isArray(state.runtime.forced) && state.runtime.forced.length > 0 && (
              <span
                className="badge badge--warn"
                data-testid="runtime-forced-badge"
                title="An engineer holds variables of this program at a value: they are not what the program computes."
              >
                FORCED {state.runtime.forced.length}
              </span>
            )}{' '}
            <span className="mono" data-testid="runtime-version">
              {state.runtime.program} · version {state.runtime.version}
            </span>
          </p>
          {state.stale && (
            <p className="form__error" role="alert" data-testid="runtime-stale-why">
              The runtimes can no longer be read, so this is what was last known, not what is.
            </p>
          )}
          {state.runtime.reason && (
            <p className="form__error" data-testid="runtime-reason">
              {state.runtime.reason}
            </p>
          )}
          <dl className="logic-facts">
            <dt>Module</dt>
            <dd className="mono">{state.runtime.module}</dd>
            <dt>Interval</dt>
            <dd className="mono">{state.runtime.interval_ms} ms</dd>
            <dt>Scans</dt>
            <dd className="mono" data-testid="runtime-scans">
              {state.runtime.scans ?? 'NOT MEASURED'}
            </dd>
            <dt>Scan time</dt>
            <dd className="mono" data-testid="runtime-scan-time">
              {/* The controller sends no time before the first scan, nor while its module does
                  not answer: there is nothing measured to show. */}
              {state.runtime.last_scan_us !== null && state.runtime.max_scan_us !== null
                ? `last ${state.runtime.last_scan_us} µs · longest ${state.runtime.max_scan_us} µs`
                : 'NOT MEASURED'}
            </dd>
            <dt>Overruns</dt>
            <dd className="mono" data-testid="runtime-overruns">
              {state.runtime.overruns ?? 'NOT MEASURED'}
            </dd>
            <dt>Outputs</dt>
            <dd data-testid="runtime-outputs">
              {state.runtime.outputs.length === 0
                ? 'none'
                : state.runtime.mode === 'run'
                  ? 'written by the program'
                  : state.runtime.outputs_safe
                    ? 'at their safe values'
                    : 'NOT CONFIRMED at their safe values'}
            </dd>
            <dt title="Variables an engineer holds at a value. Stopping the program or loading it again releases every one.">
              Forced
            </dt>
            <ForcedFacts state={state} />
          </dl>
        </>
      )}
      {elsewhere && (
        <p className="muted" data-testid="runtime-elsewhere">
          The controller runs another project{active ? ` (${active})` : ''}. Loading saves the program into this
          project's runtimes and starts nothing.
        </p>
      )}
      <div className="logic-panel__actions">
        <button
          type="button"
          className="btn"
          disabled={!may || busy || state.kind !== 'loaded' || state.stale}
          title="Put the program in RUN. From STOP or FAULT that is a warm restart: its RETAIN variables keep their values, the others take their initial values."
          onClick={() => {
            // Run starts writing to the plant: it is asked, as on Settings → Runtimes.
            if (
              window.confirm(
                `Put ${name} in RUN? From STOP or FAULT that is a warm restart: its RETAIN variables keep the values they had, the others take their initial values, and the program writes its outputs.`
              )
            ) {
              void command(name, true);
            }
          }}
          data-testid="runtime-run"
        >
          <CirclePlay size={13} aria-hidden /> Run
        </button>
        <button
          type="button"
          className="btn"
          // Stop is the safe direction: neither a request still unanswered nor a state that is
          // no longer known takes it away. If the controller cannot be reached, it says so.
          disabled={!may || state.kind !== 'loaded'}
          title="Put the program in STOP. Its outputs are written their safe values."
          onClick={() => void command(name, false)}
          data-testid="runtime-stop"
        >
          <CircleStop size={13} aria-hidden /> Stop
        </button>
        <button
          type="button"
          className="btn btn--flat"
          disabled={!may || busy || state.kind === 'unknown'}
          onClick={() => void unload(name)}
          title="Take the program out of the project's runtimes. Its outputs are written their safe values."
          data-testid="runtime-unload"
        >
          <Upload size={13} aria-hidden /> Unload
        </button>
        <button
          type="button"
          className="btn btn--flat"
          disabled={
            !may ||
            busy ||
            state.kind !== 'loaded' ||
            state.stale ||
            !Array.isArray(state.runtime.forced) ||
            state.runtime.forced.length === 0
          }
          title="Release every forced variable: the program decides their values again from its next scan."
          onClick={() => {
            if (
              window.confirm(
                `Release every forced variable of ${name}? The program decides their values again from its next scan, so an output bound to one may change at once.`
              )
            ) {
              void release(name, null);
            }
          }}
          data-testid="runtime-release-all"
        >
          <LockOpen size={13} aria-hidden /> Release all
        </button>
      </div>
    </section>
  );
}

/** How the program is run: its task. */
function SettingsPanel({ doc, readOnly }: { doc: ProgramDoc; readOnly: boolean }) {
  const setSettings = usePrograms((s) => s.setSettings);
  const modules = usePrograms((s) => s.runtimes.modules);
  const { entry } = doc;
  const set = (change: Partial<ProgramEntry>) => setSettings(entry.name, change);
  // The module the program names is always offered, installed or not: it is what is saved.
  const offered = modules === null ? null : [...new Set([...modules, entry.runtime])];

  return (
    <section className="logic-panel" aria-label="Task">
      <h3 className="logic-panel__title">Task</h3>
      <div className="logic-fields">
        <label className={`field${entry.interval_ms >= 1 ? '' : ' has-error'}`}>
          <span className="field__label">Interval (ms)</span>
          <input
            type="number"
            min={1}
            aria-label="Interval in milliseconds"
            value={Number.isFinite(entry.interval_ms) ? entry.interval_ms : ''}
            disabled={readOnly}
            onChange={(event) => set({ interval_ms: event.target.valueAsNumber })}
          />
          {!(entry.interval_ms >= 1) && <span className="field__error">At least 1 ms.</span>}
        </label>
        <label className="field">
          <span className="field__label">Runtime module</span>
          {offered === null ? (
            <input
              aria-label="Runtime module"
              className="mono"
              value={entry.runtime}
              disabled={readOnly}
              onChange={(event) => set({ runtime: event.target.value })}
            />
          ) : (
            <select
              aria-label="Runtime module"
              value={entry.runtime}
              disabled={readOnly}
              onChange={(event) => set({ runtime: event.target.value })}
            >
              {offered.map((module) => (
                <option key={module} value={module}>
                  {modules?.includes(module) ? module : `${module} (not installed)`}
                </option>
              ))}
            </select>
          )}
          {offered === null && (
            <span className="field__hint">The installed runtime modules are not known yet.</span>
          )}
        </label>
        <label className="field">
          <span className="field__label">Target profile</span>
          <input
            aria-label="Target profile"
            className="mono"
            value={entry.target}
            disabled={readOnly}
            onChange={(event) => set({ target: event.target.value })}
          />
        </label>
        <label className="settings__check">
          <input
            type="checkbox"
            checked={entry.run}
            disabled={readOnly}
            onChange={(event) => set({ run: event.target.checked })}
          />
          Run when loaded
        </label>
      </div>
    </section>
  );
}

function BindingEditor({
  doc,
  row,
  variable,
  readOnly,
}: {
  doc: ProgramDoc;
  row: BindingRow;
  variable: Variable | null;
  readOnly: boolean;
}) {
  const setSettings = usePrograms((s) => s.setSettings);
  const sample = useTelemetry((s) => s.tags.get(row.tag));
  const stream = useTelemetry((s) => s.stream);
  const forcedTags = useController((s) => s.forcedTags);
  const { entry } = doc;
  const problem = problemOf(row);
  const label = `${KIND_LABEL[row.kind]} for ${row.variable}`;

  return (
    <div className="logic-binding" data-testid="binding" data-kind={row.kind}>
      <span className="logic-binding__kind">{KIND_LABEL[row.kind]}</span>
      <input
        className="mono logic-binding__tag"
        list={row.kind === 'published' ? undefined : 'logic-tags'}
        aria-label={label}
        aria-invalid={row.tag.trim() === ''}
        placeholder={row.kind === 'published' ? 'internal.Area.Program.variable' : 'tag'}
        value={row.tag}
        disabled={readOnly}
        onChange={(event) => setSettings(entry.name, setTag(entry, row, event.target.value))}
      />
      {row.kind === 'output' &&
        (variable?.type === 'BOOL' ? (
          <select
            aria-label={`Safe value of ${row.variable}`}
            aria-invalid={row.safe === null}
            value={row.safe === null || row.safe === undefined ? '' : String(row.safe)}
            disabled={readOnly}
            onChange={(event) =>
              setSettings(
                entry.name,
                setSafe(entry, row, event.target.value === '' ? null : event.target.value === 'true')
              )
            }
          >
            <option value="">safe value…</option>
            <option value="false">FALSE</option>
            <option value="true">TRUE</option>
          </select>
        ) : (
          <input
            type="number"
            className="logic-binding__safe"
            aria-label={`Safe value of ${row.variable}`}
            aria-invalid={row.safe === null}
            placeholder="safe value"
            value={typeof row.safe === 'number' ? row.safe : ''}
            disabled={readOnly}
            onChange={(event) => setSettings(entry.name, setSafe(entry, row, parseSafeNumber(event.target.value)))}
          />
        ))}
      {row.kind === 'published' && (
        <label className="settings__check" title="Whether an operator may write it: a setpoint.">
          <input
            type="checkbox"
            checked={row.writable === true}
            disabled={readOnly}
            aria-label={`${row.variable} may be written by an operator`}
            onChange={(event) => setSettings(entry.name, setWritable(entry, row, event.target.checked))}
          />
          writable
        </label>
      )}
      {row.tag.trim() !== '' && <TagValue sample={sample} stream={stream} />}
      {row.tag.trim() !== '' && <ForcedMark uri={row.tag} forcedTags={forcedTags} />}
      <button
        type="button"
        className="icon-btn"
        aria-label={`Remove: ${label}`}
        disabled={readOnly}
        onClick={() => setSettings(entry.name, removeBinding(entry, row))}
      >
        <Trash2 size={13} aria-hidden />
      </button>
      {problem && (
        <span className="field__error" data-testid="binding-problem">
          {problem}
        </span>
      )}
    </div>
  );
}

const KINDS: Kind[] = ['input', 'good', 'output', 'published'];

/**
 * A variable's value in the running program. Only a value read from a program in RUN is shown
 * as a plain value; everything else says what it is instead.
 */
function VariableValue({
  variable,
  state,
  watch,
}: {
  variable: Variable;
  state: RunState;
  watch: WatchView | undefined;
}) {
  const cell = (dataState: string, text: string, title?: string, flag?: string) => (
    <span
      className={`tagvalue mono tagvalue--${dataState === 'live' ? 'live' : dataState === 'stopped' ? 'stale' : 'none'}`}
      data-testid="variable-value"
      data-state={dataState}
      title={title}
    >
      {text}
      {flag && <span className="tagvalue__flag">{flag}</span>}
    </span>
  );
  if (!hasOneValue(variable.type)) return cell('none', '—', 'It has no single value to show.');
  if (state.kind !== 'loaded') return cell('none', '—', 'No runtime runs this program.');
  // What the runtime last said is no longer known to be so: neither are its values.
  if (state.stale) return cell('unknown', 'UNKNOWN', 'The runtimes can no longer be read.');
  if (!watch) return cell('none', '—', 'Not read yet.');
  if (watch.answer === null) return cell('unknown', 'UNKNOWN', watch.error ?? undefined);
  const found: WatchedValue | undefined = watch.answer.values.find(
    (value) => value.variable.toLowerCase() === variable.path.toLowerCase()
  );
  if (!found) return cell('none', '—', 'Not read yet.');
  if (found.error !== null || found.type === null) {
    return found.error?.startsWith('the program has no variable')
      ? cell('absent', 'not in the running program', found.error)
      : cell('none', '—', found.error ?? undefined);
  }
  const text = formatVariable(found.type, found.value);
  if (text === null) return cell('none', '—', 'The runtime sent something that is not a value of its type.');
  if (found.forced === true && watch.answer.state.mode === 'run') {
    return (
      <span
        className="tagvalue mono tagvalue--live"
        data-testid="variable-value"
        data-state="live"
        data-forced="true"
        title="An engineer holds this variable at this value: it is not what the program computes."
      >
        {text}
        <span className="tagvalue__flag" data-testid="variable-forced">
          FORCED
        </span>
      </span>
    );
  }
  return watch.answer.state.mode === 'run'
    ? cell('live', text)
    : cell('stopped', text, 'The program is not running: this is the value it stopped with.', watch.answer.state.mode);
}

/**
 * Force or release one variable of the running program: an engineer's, while the runtime is
 * known to run it. Forcing asks for the value, which is the confirmation: it holds against the
 * program and the field until released, or until the program stops or is loaded again.
 */
function ForceControl({
  name,
  variable,
  state,
  forced,
  busy,
}: {
  name: string;
  variable: Variable;
  state: RunState;
  forced: Map<string, ForcedValue> | null;
  busy: boolean;
}) {
  const force = usePrograms((s) => s.force);
  const release = usePrograms((s) => s.release);
  if (!mayEngineer() || !forceable(variable.type)) return null;
  if (state.kind !== 'loaded' || state.stale || state.runtime.mode !== 'run' || forced === null) return null;
  const held = forced.get(variable.path.replace(/\s+/g, '').toLowerCase());
  if (held) {
    return (
      <>
        {' '}
        <button
          type="button"
          className="btn btn--flat"
          disabled={busy}
          aria-label={`Release ${variable.path}`}
          title="Release the force: the program decides the value again from its next scan."
          onClick={() => {
            if (window.confirm(`Release ${variable.path} of ${name}? The program decides its value again from its next scan.`)) {
              void release(name, variable.path);
            }
          }}
          data-testid="variable-release"
        >
          <LockOpen size={12} aria-hidden /> Release
        </button>
      </>
    );
  }
  return (
    <>
      {' '}
      <button
        type="button"
        className="btn btn--flat"
        disabled={busy}
        aria-label={`Force ${variable.path}`}
        title="Hold the variable at a value, whatever the program and its inputs say."
        onClick={() => {
          const text = window.prompt(
            `Force ${variable.path} (${variable.type}) of ${name} to what value?${variable.type === 'BOOL' ? ' TRUE or FALSE.' : ''} It holds against the program and the field until it is released or the program stops: an output bound to it is written this value.`
          );
          if (text === null) return;
          const value = parseForced(variable.type, text);
          if (value === null) {
            usePrograms.setState({
              error: `${text.trim() || 'Nothing'} is not a value of ${variable.path}, a ${variable.type}: nothing was forced.`,
            });
            return;
          }
          void force(name, variable.path, value);
        }}
        data-testid="variable-force"
      >
        <Lock size={12} aria-hidden /> Force…
      </button>
    </>
  );
}

/** The program's variables, each with its bindings to tags. */
function VariablesPanel({ doc, readOnly, state }: { doc: ProgramDoc; readOnly: boolean; state: RunState }) {
  const watch = usePrograms((s) => s.watches[doc.entry.name]);
  const busy = usePrograms((s) => s.busy !== null);
  const forced = forcedOf(state);
  const setSettings = usePrograms((s) => s.setSettings);
  const configured = useController((s) => s.configuredTags);
  const io = useController((s) => s.io);
  const { entry } = doc;
  const rows = bindingsOf(entry);
  // What the Ladder Diagram front-end made for itself (`__LD_…`) is not the program's: it is
  // watched for the diagram, and not listed.
  const variables = doc.variables === null ? null : doc.variables.filter((variable) => variable.hidden !== true);
  const current = variables !== null && doc.variablesSource === doc.source;
  // What a binding may name: the tags of the configuration being edited and of what runs.
  const tags = [...new Set([...configured, ...(io?.connections.flatMap((c) => c.tags) ?? [])])].sort();
  const lost = variables === null ? rows : orphans(rows, variables);

  return (
    <section className="logic-panel" aria-label="Variables">
      <h3 className="logic-panel__title">Variables and bindings</h3>
      <datalist id="logic-tags">
        {tags.map((tag) => (
          <option key={tag} value={tag} />
        ))}
      </datalist>
      {variables === null ? (
        <p className="muted" data-testid="variables-unknown">
          The variables are not known: no text of this program has compiled yet. Check it to see them.
        </p>
      ) : (
        <>
          {!current && (
            <p className="muted" data-testid="variables-stale">
              These are the variables of the last text that compiled. The text has changed since.
            </p>
          )}
          {variables.length === 0 ? (
            <p className="muted">The program declares no variable.</p>
          ) : (
            <table className="grid grid--form" aria-label="Variables">
              <thead>
                <tr>
                  <th>Variable</th>
                  <th>Type</th>
                  <th>Section</th>
                  <th title="The variable's value in the running program, as its last scan left it">Value</th>
                  <th>Bindings</th>
                </tr>
              </thead>
              <tbody>
                {variables.map((variable) => {
                  const own = bindingsOfVariable(rows, variable.path);
                  return (
                    <tr key={variable.path} data-testid="variable-row">
                      <td className="mono">{variable.path}</td>
                      <td className="mono">{variable.type}</td>
                      <td>{variable.section}</td>
                      <td>
                        <VariableValue variable={variable} state={state} watch={watch} />
                        <ForceControl
                          name={entry.name}
                          variable={variable}
                          state={state}
                          forced={forced}
                          busy={busy}
                        />
                      </td>
                      <td className="grow">
                        {own.map((row) => (
                          <BindingEditor
                            key={`${row.kind}:${row.index}`}
                            doc={doc}
                            row={row}
                            variable={variable}
                            readOnly={readOnly}
                          />
                        ))}
                        {variable.bindable ? (
                          <select
                            className="logic-binding__add"
                            aria-label={`Bind ${variable.path}`}
                            value=""
                            disabled={readOnly}
                            onChange={(event) => {
                              const kind = event.target.value as Kind;
                              if (KINDS.includes(kind)) setSettings(entry.name, addBinding(entry, kind, variable.path));
                            }}
                          >
                            <option value="">Bind…</option>
                            {KINDS.filter((kind) => kind !== 'good' || variable.type === 'BOOL').map((kind) => (
                              <option key={kind} value={kind}>
                                {KIND_LABEL[kind]}
                              </option>
                            ))}
                          </select>
                        ) : (
                          own.length === 0 && (
                            <span
                              className="muted"
                              data-testid="not-bindable"
                              title={
                                variable.type.startsWith('STRING[')
                                  ? 'A tag carries a number, never text: a STRING cannot be bound to one.'
                                  : 'Only BOOL, integer and real variables can be bound to a tag.'
                              }
                            >
                              —
                            </span>
                          )
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </>
      )}
      {lost.length > 0 && (
        <div data-testid="bindings-without-variable">
          <p className="muted">
            {variables === null
              ? 'Bindings saved with the program:'
              : 'Bindings of variables the program does not have:'}
          </p>
          {lost.map((row) => (
            <div key={`${row.kind}:${row.index}`} className="logic-binding-orphan">
              <span className="mono">{row.variable}</span>
              <BindingEditor doc={doc} row={row} variable={null} readOnly={readOnly} />
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

/** Where a diagnostic is: a line and column of the text, or in a Ladder Diagram its element. */
export function placeOf(diagnostic: Diagnostic): string {
  const element = diagnostic.element;
  if (!element) return `${diagnostic.line}:${diagnostic.col}`;
  if (element.local_id !== null && element.local_id !== undefined) return `${element.pou} element ${element.local_id}`;
  if (element.variable) return `${element.pou} variable ${element.variable}`;
  return element.pou === '' ? `${diagnostic.line}:${diagnostic.col}` : element.pou;
}

/** What the compiler said, of which text. */
function CheckPanel({ doc, goTo }: { doc: ProgramDoc; goTo: (diagnostic: Diagnostic) => void }) {
  const current = checkIsCurrent(doc);
  if (doc.check === null) {
    return (
      <p className="logic-check muted" data-testid="check-none">
        Not checked yet.
      </p>
    );
  }
  const { check } = doc;
  return (
    <div className="logic-check" data-testid="check-result">
      {!current && (
        <p className="muted" data-testid="check-stale">
          This is the check of an earlier text. The text has changed since.
        </p>
      )}
      {check.ok ? (
        <p>
          <span className="badge badge--good" data-testid="check-ok">
            COMPILES
          </span>{' '}
          <span className="muted mono">
            {check.code_size === undefined ? 'code size UNKNOWN' : `${check.code_size} bytes of code`} ·{' '}
            {check.memory_size === undefined ? 'memory UNKNOWN' : `${check.memory_size} bytes of memory`}
          </span>
        </p>
      ) : (
        <ul className="logic-diagnostics" aria-label="Diagnostics">
          {check.diagnostics.map((diagnostic, index) => (
            <li key={index}>
              <button
                type="button"
                className="logic-diagnostic"
                data-testid="diagnostic"
                data-local-id={diagnostic.element?.local_id ?? undefined}
                onClick={() => goTo(diagnostic)}
              >
                <span className="mono logic-diagnostic__at">{placeOf(diagnostic)}</span>
                {diagnostic.message}
              </button>
            </li>
          ))}
        </ul>
      )}
      {(check.binding_problems?.length ?? 0) > 0 && (
        <ul className="logic-diagnostics" aria-label="Binding problems">
          {check.binding_problems?.map((problem) => (
            <li key={problem} className="logic-diagnostic logic-diagnostic--plain" data-testid="binding-check-problem">
              {problem}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** Removing a program is asked twice: it deletes its source. */
function RemovePanel({ name, busy, onRemoved }: { name: string; busy: boolean; onRemoved: () => void }) {
  const remove = usePrograms((s) => s.remove);
  const [asking, setAsking] = useState(false);
  return (
    <section className="logic-panel" aria-label="Remove">
      {asking ? (
        <div className="logic-panel__actions">
          <span>Remove {name} and its source from the project?</span>
          <button
            type="button"
            className="btn"
            disabled={busy}
            data-testid="program-remove-confirm"
            onClick={() => void remove(name).then((removed) => removed && onRemoved())}
          >
            Remove
          </button>
          <button type="button" className="btn btn--flat" onClick={() => setAsking(false)}>
            Keep
          </button>
        </div>
      ) : (
        <button type="button" className="btn btn--flat" disabled={busy} onClick={() => setAsking(true)}>
          <Trash2 size={13} aria-hidden /> Remove program…
        </button>
      )}
      <p className="muted">A runtime that already runs the program is not touched: unload it first.</p>
    </section>
  );
}

/** What is drawn live on a Ladder Diagram, and why, or why nothing is. */
function liveOf(doc: ProgramDoc, state: RunState, watch: WatchView | undefined): { live: LadderLive | null; why: string } {
  const check = checkIsCurrent(doc) ? doc.check : null;
  if (check === null || !check.ok) {
    return { live: null, why: 'No live state is drawn: this text has not compiled. Check it.' };
  }
  if (state.kind === 'unknown') return { live: null, why: `No live state is drawn: what the runtime does is not known (${state.why}).` };
  if (state.kind === 'not-loaded') return { live: null, why: 'No live state is drawn: no runtime runs this program.' };
  const power = check.power ?? [];
  if (state.stale) {
    return { live: { values: null, power }, why: 'The runtimes can no longer be read: every state is UNKNOWN.' };
  }
  if (check.version === undefined || check.version !== state.runtime.version) {
    return {
      live: null,
      why: `No live state is drawn: the runtime runs version ${state.runtime.version}, and this text builds version ${check.version ?? 'UNKNOWN'}. Load it to see it live.`,
    };
  }
  if (state.runtime.mode !== 'run') {
    return { live: null, why: `No live state is drawn: the program is in ${state.runtime.mode.toUpperCase()}, so no power flows.` };
  }
  if (!watch) return { live: null, why: 'No live state is drawn yet: the values have not been read.' };
  if (watch.answer === null) {
    return { live: { values: null, power }, why: `The values cannot be read: every state is UNKNOWN. ${watch.error ?? ''}`.trim() };
  }
  if (watch.answer.state.mode !== 'run' || watch.answer.state.version !== check.version) {
    return { live: null, why: 'No live state is drawn: the values read are not of this version in RUN.' };
  }
  const values = new Map(watch.answer.values.map((value) => [value.variable.toLowerCase(), value]));
  return {
    live: { values, power },
    why: 'Live, as the last scan left it: the power each element and link had when its rung ran; a rung the scan did not run is drawn as not executed.',
  };
}

/** The Ladder Diagram of the program, read-only, with its live state. */
function LadderPanel({ doc, ladder, state }: { doc: ProgramDoc; ladder: Ladder; state: RunState }) {
  const watch = usePrograms((s) => s.watches[doc.entry.name]);
  const { live, why } = liveOf(doc, state, watch);
  const check = checkIsCurrent(doc) ? doc.check : null;
  const diagnostics = check && !check.ok ? check.diagnostics : [];
  const program = check?.ok ? (check.program ?? null) : null;
  return (
    <div className="logic-panel" data-testid="ld-panel">
      <p className="muted" data-testid="ld-live" data-live={live === null ? 'none' : live.values === null ? 'unknown' : 'live'}>
        {why}
      </p>
      <p className="muted">Read-only: a Ladder Diagram is changed through save_logic_source until its editor is built.</p>
      {ladder.ok ? (
        <LadderView pous={ladder.pous} program={program} live={live} diagnostics={diagnostics} />
      ) : (
        <div className="notice notice--bad" role="alert" data-testid="ld-unreadable">
          <p>The diagram cannot be drawn: {ladder.error}.</p>
        </div>
      )}
    </div>
  );
}

/** A watched variable's path as an element writes it: a name, members and subscripts. */
const PLACE = /^[A-Za-z_]\w*(\[[^\]]+\])?(\.[A-Za-z_]\w*(\[[^\]]+\])?)*$/;

/** The variables a Ladder Diagram's contacts, coils and variable elements read or write. */
function elementVariables(ladder: Ladder | null): string[] {
  if (ladder === null || !ladder.ok) return [];
  return ladder.pous
    .filter((pou) => pou.pouType === 'program')
    .flatMap((pou) => pou.elements.map((e) => e.variable ?? ''))
    .filter((path) => PLACE.test(path));
}

/**
 * How the loaded program started, as the controller said: changed online, warm or cold, and
 * each variable that did not carry over with why.
 */
function StartFacts({ load }: { load: ReturnType<typeof loadOf> }) {
  return (
    <>
      <p data-testid="program-start">{describeStart(load)}</p>
      {load !== null && load.dropped.length > 0 && (
        <p>
          Not carried over:{' '}
          {load.dropped.map((d, i) => (
            <span key={d.variable} data-testid="program-dropped">
              {i > 0 ? '; ' : ''}
              <span className="mono">{d.variable}</span> ({d.why})
            </span>
          ))}
          .
        </p>
      )}
      {load !== null && load.initialized.length > 0 && (
        <p>
          From the initial value:{' '}
          {load.initialized.map((d, i) => (
            <span key={d.variable} data-testid="program-initialized">
              {i > 0 ? '; ' : ''}
              <span className="mono">{d.variable}</span> ({d.why})
            </span>
          ))}
          .
        </p>
      )}
    </>
  );
}

export function ProgramEditor({ name, onRemoved }: { name: string; onRemoved?: (name: string) => void }) {
  const phase = usePrograms((s) => s.phase);
  const loadError = usePrograms((s) => s.loadError);
  const doc = usePrograms((s) => s.programs.find((d) => d.entry.name === name));
  const busy = usePrograms((s) => s.busy);
  const error = usePrograms((s) => s.error);
  const runtimes = usePrograms((s) => s.runtimes);
  const open = usePrograms((s) => s.open);
  const check = usePrograms((s) => s.check);
  const save = usePrograms((s) => s.save);
  const discard = usePrograms((s) => s.discard);
  const download = usePrograms((s) => s.download);
  const setSource = usePrograms((s) => s.setSource);
  const readRuntimes = usePrograms((s) => s.readRuntimes);
  const editor = useRef<SourceEditorHandle>(null);
  const exists = doc !== undefined;
  const isLadder = doc?.entry.language === 'ld';
  const ladderSource = isLadder ? (doc?.source ?? null) : null;
  const ladder = useMemo(() => (ladderSource === null ? null : readLadder(ladderSource)), [ladderSource]);
  const sourceRead = doc?.source !== null && doc?.source !== undefined;

  // The source is read when the editor opens; the first check follows, for the variables.
  useEffect(() => {
    if (exists && phase === 'ready') void open(name);
  }, [exists, phase, name, open]);
  useEffect(() => {
    if (sourceRead && usePrograms.getState().programs.find((d) => d.entry.name === name)?.check === null) {
      void check(name);
    }
  }, [sourceRead, name, check]);

  // The runtimes are read while the editor is shown, and not after.
  useEffect(() => {
    void readRuntimes();
    const timer = setInterval(() => void readRuntimes(), RUNTIMES_POLL_MS);
    return () => clearInterval(timer);
  }, [readRuntimes]);

  // The running program's variables are read while its editor is shown and a runtime runs it.
  // Asked by name: a variable the running version does not have says so.
  const runtimeKnown = usePrograms((s) => {
    const view = runState(name, s.runtimes);
    return view.kind === 'loaded' && !view.stale;
  });
  // A Ladder Diagram also asks for what its elements name that is not a variable of its own
  // (an element of an array), each once.
  const declared = (doc?.variables ?? []).filter((variable) => hasOneValue(variable.type)).map((variable) => variable.path);
  const known = new Set(declared.map((path) => path.toLowerCase()));
  const extra = [...new Set(elementVariables(ladder))].filter((path) => {
    const fresh = !known.has(path.toLowerCase());
    known.add(path.toLowerCase());
    return fresh;
  });
  const watched = [...declared, ...extra].join('\n');
  const readWatch = usePrograms((s) => s.readWatch);
  const dropWatch = usePrograms((s) => s.dropWatch);
  useEffect(() => {
    if (!runtimeKnown || watched === '') {
      dropWatch(name);
      return undefined;
    }
    const variables = watched.split('\n');
    void readWatch(name, variables);
    const timer = setInterval(() => void readWatch(name, variables), WATCH_POLL_MS);
    return () => clearInterval(timer);
  }, [runtimeKnown, watched, name, readWatch, dropWatch]);

  if (phase === 'reading' || phase === 'idle') {
    return <p className="editor__loading muted">Reading the project's programs…</p>;
  }
  if (phase === 'unreadable') {
    return (
      <div className="notice notice--bad" role="alert" data-testid="programs-unreadable">
        <div>
          <h3>The programs could not be read</h3>
          <p>{loadError}</p>
        </div>
      </div>
    );
  }
  if (!doc) {
    return (
      <div className="notice" data-testid="program-missing">
        <p>The project has no program called {name}.</p>
      </div>
    );
  }
  if (doc.source === null) {
    return doc.readError ? (
      <div className="notice notice--bad" role="alert" data-testid="program-unreadable">
        <div>
          <h3>The source of {name} could not be read</h3>
          <p>{doc.readError}</p>
        </div>
      </div>
    ) : (
      <p className="editor__loading muted">Reading {name}…</p>
    );
  }

  const may = mayEngineer();
  const dirty = isDirty(doc);
  const working = busy !== null;
  const state = runState(name, runtimes);
  const diagnostics = checkIsCurrent(doc) && doc.check && !doc.check.ok ? doc.check.diagnostics : [];
  // What runs is another version than the last one built here: said, since the text may differ.
  const differs =
    state.kind === 'loaded' && doc.built !== null && doc.built.version !== state.runtime.version;

  return (
    <div className="editor__column" data-testid="program-editor">
      <div className="editor__tools">
        <button
          type="button"
          className="btn btn--primary"
          disabled={!may || !dirty || working}
          onClick={() => void save(name)}
        >
          <Save size={13} aria-hidden /> Save
        </button>
        <button type="button" className="btn" disabled={!may || !dirty || working} onClick={() => discard(name)}>
          <Undo2 size={13} aria-hidden /> Discard
        </button>
        <span className="toolbar__sep" aria-hidden />
        <button type="button" className="btn" disabled={working} onClick={() => void check(name)}>
          <Check size={13} aria-hidden /> Check
        </button>
        <button
          type="button"
          className="btn"
          disabled={!may || working}
          onClick={() => void download(name)}
          title="Save, check and build the program, and put it into the project's runtimes. A new version of the program while it runs is changed online: it takes over the running variables and scans on."
          data-testid="program-load"
        >
          <Download size={13} aria-hidden /> Load
        </button>
        <button
          type="button"
          className="btn"
          disabled={!may || working}
          onClick={() => {
            // A cold load throws away what the running program holds: it is asked first.
            if (
              window.confirm(
                `Load ${name} cold? Every variable starts from its initial value, RETAIN variables included, and nothing the running version holds is carried over.`
              )
            ) {
              void download(name, true);
            }
          }}
          title="Load the program with every variable from its initial value, RETAIN ones included."
          data-testid="program-load-cold"
        >
          <Download size={13} aria-hidden /> Load cold
        </button>
        {dirty && (
          <span className="badge badge--warn" data-testid="program-unsaved">
            UNSAVED
          </span>
        )}
        {!may && <span className="muted">Only an engineer changes programs.</span>}
      </div>
      {error && (
        <div className="logic-note logic-note--bad" role="alert" data-testid="program-error">
          <p>{error}</p>
        </div>
      )}
      {doc.built && (
        <div className={`logic-note${doc.built.warnings.length > 0 ? ' logic-note--warn' : ''}`} data-testid="program-built">
          <div>
            <p>
              Version <span className="mono">{doc.built.version}</span> was built
              {doc.loaded === null
                ? ' and is not loaded.'
                : doc.startFailure !== null
                  ? " and saved into the project's runtimes, but it did not start."
                  : doc.loaded.applied
                    ? ' and loaded.'
                    : " and saved into the project's runtimes. The controller runs another project, so nothing was started."}
              {differs && (
                <span data-testid="program-version-differs">
                  {' '}
                  The runtime runs another version ({state.kind === 'loaded' ? state.runtime.version : ''}).
                </span>
              )}
            </p>
            {doc.loaded?.applied && doc.startFailure === null && (
              <StartFacts load={loadOf(doc.loaded, name)} />
            )}
            {doc.built.warnings.map((warning) => (
              <p key={warning} data-testid="build-warning">
                {warning}
              </p>
            ))}
          </div>
        </div>
      )}
      <div className="logic-editor">
        <div className="logic-editor__top">
          <div className="logic-editor__main">
            <div className="logic-editor__source">
              {isLadder && ladder !== null ? (
                <LadderPanel doc={doc} ladder={ladder} state={state} />
              ) : (
                <SourceEditor
                  ref={editor}
                  source={doc.source}
                  readOnly={!may}
                  diagnostics={diagnostics}
                  onChange={(source) => setSource(name, source)}
                />
              )}
            </div>
            <CheckPanel
              doc={doc}
              goTo={(diagnostic) => {
                if (!isLadder) {
                  editor.current?.goTo(diagnostic.line, diagnostic.col);
                  return;
                }
                const id = diagnostic.element?.local_id;
                if (id === null || id === undefined) return;
                const found = document.querySelector(`[data-testid="ld-diagnostic"][data-local-id="${id}"]`);
                (found as (Element & { scrollIntoView?: (arg?: unknown) => void }) | null)?.scrollIntoView?.({ block: 'center' });
              }}
            />
          </div>
          <div className="logic-editor__side">
            <RuntimePanel name={name} state={state} busy={working} />
            <SettingsPanel doc={doc} readOnly={!may} />
            {may && <RemovePanel name={name} busy={working} onRemoved={() => onRemoved?.(name)} />}
          </div>
        </div>
        <div className="logic-editor__bottom">
          <VariablesPanel doc={doc} readOnly={!may} state={state} />
        </div>
      </div>
    </div>
  );
}
