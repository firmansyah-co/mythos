// The editor of one control program: its Structured Text, or its Ladder Diagram with its live
// state (LadderEditor for an engineer, LadderView read-only for everyone else), or its Function
// Block Diagram with its live state (FbdEditor for an engineer, FbdView read-only for everyone
// else), or its Sequential Function Chart with its live state (SfcEditor for an engineer, SfcView
// read-only for everyone else), what the compiler says of it, its task settings, its variables with their bindings to tags, and what
// the controller's runtime is doing with it.
//
// The work area holds the program and its interface (the variables and their bindings) and
// nothing else, as TIA's block editor does. The rest goes where the workbench keeps it for every
// editor: a diagram's instructions and the runtime's state and commands in the task
// cards, the program's properties (its task, its runtime, removing it) in the inspector, and what
// the compiler says in the inspector's Diagnostics tab. The workbench draws only the active
// editor, so these are set while this editor is drawn and taken away when it goes.
//
// Three things are kept apart on purpose, because an engineer acts on the difference: the text
// in the editor, the text that is saved, and the program that runs. Nothing that runs changes
// until "Load" builds the saved text and puts it into the project's runtimes.

import { useEffect, useMemo, useReducer, useRef, useState, type Ref } from 'react';
import { Check, CirclePlay, CircleStop, Download, Lock, LockOpen, Save, Trash2, Undo2, Upload } from 'lucide-react';
import type { Panel } from '@mythos/workbench-sdk';
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
  host,
  isOfThisProgram,
  mayEngineer,
  useController,
  useProject,
  useTelemetry,
  type ChartProbe,
  type CheckResult,
  type Diagnostic,
  type ForcedValue,
  type ProgramEntry,
  type Variable,
  type ValueProbe,
  type WatchedValue,
} from './bridge';
import { draw } from './draw';
import { readFbd, type Fbd } from './fbd';
import { FbdEditor } from './FbdEditor';
import { FBD_KIT } from './FbdInstructions';
import { FbdView } from './FbdView';
import { readLadder, type Ladder } from './ladder';
import { LadderEditor } from './LadderEditor';
import { LadderView, type LadderLive } from './LadderView';
import { InstructionsCard } from './LdInstructions';
import { LIBRARY_EDITOR, librarySourceOf } from './library';
import { useLibrary } from './librarystore';
import { readSfc, type Sfc } from './sfc';
import { SfcEditor } from './SfcEditor';
import { SFC_KIT } from './SfcInstructions';
import { SfcView } from './SfcView';
import {
  checkIsCurrent,
  currencyOf,
  describeStart,
  inOtherRuntime,
  isDirty,
  loadOf,
  monotonicNow,
  runState,
  staleCause,
  usePrograms,
  WATCH_POLL_MS,
  type Currency,
  type ProgramDoc,
  type RunState,
  type StaleCause,
  type WatchView,
} from './programs';
import { forceable, formatVariable, hasOneValue, parseForced, shownForced } from './values';
import { SourceEditor, type SourceEditorHandle } from './SourceEditor';

/** How often the controller's runtimes are read while an editor is open. */
export const RUNTIMES_POLL_MS = 1000;
/**
 * Whether the program's watched values are current (`currencyOf`), judged again when they stop
 * being so: an answer held while asking gets none newer ages here, with nothing else to redraw.
 * `null` while there is no answer of a loaded runtime to judge.
 */
function useCurrency(state: RunState, watch: WatchView | undefined): Currency | null {
  const [turn, judgeAgain] = useReducer((n: number) => n + 1, 0);
  const answer = watch?.answer ?? null;
  const sentAtMs = watch?.sentAtMs ?? null;
  const currency =
    state.kind === 'loaded' && answer !== null && sentAtMs !== null
      ? currencyOf(answer, sentAtMs, state.runtime, monotonicNow())
      : null;
  const untilMs = currency?.current ? currency.untilMs : null;
  useEffect(() => {
    if (untilMs === null) return undefined;
    // Past the instant, never at it: still current then, it is judged again at the next turn.
    const timer = setTimeout(judgeAgain, Math.max(0, untilMs - monotonicNow()) + 1);
    return () => clearTimeout(timer);
  }, [untilMs, turn]);
  return currency;
}

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
  // Disabled below by `state.kind`; this says why.
  const held = inOtherRuntime(state);

  return (
    <section className="logic-panel" data-testid="program-runtime" aria-label="Runtime">
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
          title={held ?? 'Put the program in RUN. From STOP or FAULT that is a warm restart: its RETAIN variables keep their values, the others take their initial values.'}
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
          title={held ?? 'Put the program in STOP. Its outputs are written their safe values.'}
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
          title={held ?? "Take the program out of the project's runtimes. Its outputs are written their safe values."}
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
          title={held ?? 'Release every forced variable: the program decides their values again from its next scan.'}
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
  currency,
}: {
  variable: Variable;
  state: RunState;
  watch: WatchView | undefined;
  currency: Currency | null;
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
  // Values that are not the program's current ones are not shown as its values (ADR-013 §2.8).
  if (currency === null || !currency.current) {
    return cell('unknown', 'UNKNOWN', `The values are not current. ${currency?.why ?? 'When they were read is not known.'}`);
  }
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

/** Whether the interface is open, as this viewer last left it. */
const INTERFACE_KEY = 'mythos.logic.interface-open';

function readOpen(): boolean {
  try {
    return window.localStorage.getItem(INTERFACE_KEY) !== 'false';
  } catch {
    return true;
  }
}

function saveOpen(open: boolean): void {
  try {
    window.localStorage.setItem(INTERFACE_KEY, open ? 'true' : 'false');
  } catch {
    // A viewer convenience: without storage it lasts as long as the page.
  }
}

/**
 * The program's interface above its body, as TIA's block interface: its variables, each with its
 * value and its bindings to tags. Folded away, it says how many there are.
 */
function VariablesPanel({ doc, readOnly, state }: { doc: ProgramDoc; readOnly: boolean; state: RunState }) {
  const [open, setOpen] = useState(readOpen);
  const watch = usePrograms((s) => s.watches[doc.entry.name]);
  const currency = useCurrency(state, watch);
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
  const sameText = doc.variablesSource === doc.source;
  // Of the text now, and of the library and the project's types as they are now.
  const current = variables !== null && sameText && doc.variablesStale === null;
  const earlier =
    doc.variablesStale === 'library'
      ? 'an earlier library: it has'
      : doc.variablesStale === 'data'
        ? "earlier types and data blocks of the project: they have"
        : 'an earlier library and earlier types and data blocks of the project: they have';
  // What a binding may name: the tags of the configuration being edited and of what runs.
  const tags = [...new Set([...configured, ...(io?.connections.flatMap((c) => c.tags) ?? [])])].sort();
  const lost = variables === null ? rows : orphans(rows, variables);

  const summary =
    variables === null
      ? 'variables not known'
      : `${variables.length} variable${variables.length === 1 ? '' : 's'} · ${rows.length} binding${rows.length === 1 ? '' : 's'}`;

  return (
    <details
      className="logic-interface"
      aria-label="Variables"
      data-testid="program-interface"
      open={open}
      onToggle={(event) => {
        const now = (event.currentTarget as HTMLDetailsElement).open;
        if (now !== open) {
          setOpen(now);
          saveOpen(now);
        }
      }}
    >
      <summary className="logic-interface__summary">
        Interface <span className="muted">{summary}</span>
      </summary>
      <div className="logic-interface__body">
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
              {doc.variablesStale === null
                ? 'These are the variables of the last text that compiled. The text has changed since.'
                : `These are the variables of the last text that compiled, with ${earlier} changed since.${sameText ? '' : ' The text has changed since too.'}`}
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
                        <VariableValue variable={variable} state={state} watch={watch} currency={currency} />
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
      </div>
    </details>
  );
}

const NOTHING_TO_DRAW: Diagnostic[] = [];
const drawn = new WeakMap<CheckResult, Diagnostic[]>();

/**
 * The diagnostics a failed check draws on the program (the text's underlines and line messages,
 * the diagrams' marks): only those of the program's own text ({@link isOfThisProgram}). One of
 * the library or the project's types is about a line of another file and is only listed. Kept
 * per check, so a view's memo sees the same list until the check changes.
 */
function drawnOf(check: CheckResult | null): Diagnostic[] {
  if (!check || check.ok) return NOTHING_TO_DRAW;
  let ours = drawn.get(check);
  if (!ours) {
    ours = check.diagnostics.filter(isOfThisProgram);
    drawn.set(check, ours);
  }
  return ours;
}

/** Where a diagnostic of another source is: its path, and its line and column when it has a line. */
function placeElsewhere(diagnostic: Diagnostic): string {
  return diagnostic.line === 0 ? `${diagnostic.source ?? ''}` : `${diagnostic.source ?? ''} ${diagnostic.line}:${diagnostic.col}`;
}

/** Where a diagnostic is: a line and column of the text, or in a Ladder Diagram its element. */
export function placeOf(diagnostic: Diagnostic): string {
  const element = diagnostic.element;
  if (!element) return `${diagnostic.line}:${diagnostic.col}`;
  // A chart's action or transition, of the POU by its name or written in the chart (its element),
  // and in a body of LD or FBD that body's own element; the line of a text of several.
  const line = element.text?.line ? ` line ${element.text.line}` : '';
  const inBody = element.local_id !== null && element.local_id !== undefined ? ` element ${element.local_id}` : '';
  if (element.inline !== null && element.inline !== undefined) return `${element.pou} element ${element.inline}, its body${inBody}${line}`;
  if (element.action) return `${element.pou} action ${element.action}${inBody}${line}`;
  if (element.transition) return `${element.pou} transition ${element.transition}${inBody}${line}`;
  if (element.local_id !== null && element.local_id !== undefined) return `${element.pou} element ${element.local_id}`;
  if (element.variable) return `${element.pou} variable ${element.variable}`;
  return element.pou === '' ? `${diagnostic.line}:${diagnostic.col}` : element.pou;
}

/** What changed since a check, other than the text: the library, the project's types, or both. */
const STALE_SAID: Record<StaleCause, string> = {
  library: 'This is the check of an earlier library. The library has changed since.',
  data: "This is a check with earlier types and data blocks of the project. They have changed since.",
  both: "This is the check of an earlier library, with earlier types and data blocks of the project. Both have changed since.",
};
/** The toolbar's word for the same. */
const STALE_BADGE: Record<StaleCause, string> = {
  library: 'LIBRARY CHANGED SINCE CHECK',
  data: 'DATA CHANGED SINCE CHECK',
  both: 'LIBRARY AND DATA CHANGED SINCE CHECK',
};
/** Why nothing is drawn live, for the same. */
const STALE_LIVE: Record<StaleCause, string> = {
  library: 'No live state is drawn: the library has changed since this text was checked.',
  data: "No live state is drawn: the project's types or data blocks have changed since this text was checked.",
  both: "No live state is drawn: the library and the project's types or data blocks have changed since this text was checked.",
};

/**
 * A diagnostic of another file. One in a library source the module listed last is a button that
 * opens that source with the caret at its place; any other (a type's file, a source not listed,
 * no list read) is only listed.
 */
function ElsewhereItem({ diagnostic }: { diagnostic: Diagnostic }) {
  const listed = useLibrary((state) => state.names);
  const source = librarySourceOf(diagnostic, listed);
  if (source === null) {
    return (
      <li className="logic-diagnostic logic-diagnostic--plain" data-testid="diagnostic-elsewhere">
        <span className="mono logic-diagnostic__at">{placeElsewhere(diagnostic)}</span>
        {diagnostic.message}
      </li>
    );
  }
  return (
    <li>
      <button
        type="button"
        className="logic-diagnostic"
        data-testid="diagnostic-library"
        onClick={() => {
          // At line 0 it is about the whole source: opened, with no place to go to.
          if (diagnostic.line > 0) useLibrary.getState().goTo(source, diagnostic.line, diagnostic.col);
          host().openEditor(LIBRARY_EDITOR, source);
        }}
      >
        <span className="mono logic-diagnostic__at">{placeElsewhere(diagnostic)}</span>
        {diagnostic.message}
      </button>
    </li>
  );
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
  const cause = staleCause(doc);
  if (cause !== null) {
    // Of an earlier library or earlier types: nothing it said is shown as said of this one.
    return (
      <div className="logic-check" data-testid="check-result">
        <p className="muted" data-testid="check-library-stale" data-cause={cause}>
          {STALE_SAID[cause]}
        </p>
      </div>
    );
  }
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
          {check.diagnostics.map((diagnostic, index) =>
            isOfThisProgram(diagnostic) ? (
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
            ) : (
              // Of the library or the project's types: a place in another file, never one that
              // would jump into this program's text.
              <ElsewhereItem key={index} diagnostic={diagnostic} />
            )
          )}
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
      {(check.project_problems?.length ?? 0) > 0 && (
        <>
          <p className="muted" data-testid="project-problems-note">
            The project's types and data blocks do not check, so this program was compiled without them.
          </p>
          <ul className="logic-diagnostics" aria-label="Problems in the project's types and data blocks">
            {check.project_problems?.map((problem, index) => (
              <li key={index} className="logic-diagnostic logic-diagnostic--plain" data-testid="project-problem">
                <span className="mono logic-diagnostic__at">{placeElsewhere(problem)}</span>
                {problem.message}
              </li>
            ))}
          </ul>
        </>
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

/**
 * What is drawn live on a diagram: the watched values and where the check says each is kept (a
 * Ladder Diagram's power or a Function Block Diagram's values in `probes`, a chart's states in
 * `chart`).
 */
interface DiagramLive {
  values: Map<string, WatchedValue> | null;
  probes: ValueProbe[];
  chart: ChartProbe[];
}

/**
 * What is drawn live on a Ladder Diagram (its power), a Function Block Diagram (its values) or a
 * Sequential Function Chart (its steps, transitions and actions), and why, or why nothing is.
 */
function liveOf(
  doc: ProgramDoc,
  state: RunState,
  watch: WatchView | undefined,
  currency: Currency | null
): { live: DiagramLive | null; why: string } {
  const check = checkIsCurrent(doc) ? doc.check : null;
  const fbd = doc.entry.language === 'fbd';
  const sfc = doc.entry.language === 'sfc';
  const cause = staleCause(doc);
  if (doc.check !== null && doc.checkedSource === doc.source && cause !== null) return { live: null, why: STALE_LIVE[cause] };
  // Not checked and does not compile are told apart: a text followed from elsewhere is being
  // checked, and saying it did not compile would be a claim nobody made.
  if (check === null) {
    return { live: null, why: 'No live state is drawn: this text has not been checked yet.' };
  }
  if (!check.ok) {
    return { live: null, why: 'No live state is drawn: this text does not compile (see Diagnostics).' };
  }
  if (state.kind === 'unknown') return { live: null, why: `No live state is drawn: what the runtime does is not known (${state.why}).` };
  if (state.kind === 'not-loaded') return { live: null, why: 'No live state is drawn: no runtime runs this program.' };
  const power: ValueProbe[] = (fbd ? check.values : check.power) ?? [];
  const chart: ChartProbe[] = check.chart ?? [];
  if (state.stale) {
    return { live: { values: null, probes: power, chart }, why: 'The runtimes can no longer be read: every state is UNKNOWN.' };
  }
  if (check.version === undefined || check.version !== state.runtime.version) {
    return {
      live: null,
      why: `No live state is drawn: the runtime runs version ${state.runtime.version}, and this text builds version ${check.version ?? 'UNKNOWN'}. Load it to see it live.`,
    };
  }
  if (state.runtime.mode !== 'run') {
    const nothing = fbd ? 'nothing is computed' : sfc ? 'the chart is not scanned' : 'no power flows';
    return { live: null, why: `No live state is drawn: the program is in ${state.runtime.mode.toUpperCase()}, so ${nothing}.` };
  }
  if (!watch) return { live: null, why: 'No live state is drawn yet: the values have not been read.' };
  if (watch.answer === null) {
    return { live: { values: null, probes: power, chart }, why: `The values cannot be read: every state is UNKNOWN. ${watch.error ?? ''}`.trim() };
  }
  if (watch.answer.state.mode !== 'run' || watch.answer.state.version !== check.version) {
    return { live: null, why: 'No live state is drawn: the values read are not of this version in RUN.' };
  }
  // Values older than the program's cycle or the route's freshness are not drawn as its state.
  if (currency === null || !currency.current) {
    const why = currency?.why ?? 'When these values were read is not known.';
    return { live: { values: null, probes: power, chart }, why: `The values are not current: every state is UNKNOWN. ${why}` };
  }
  const values = new Map(watch.answer.values.map((value) => [value.variable.toLowerCase(), value]));
  return {
    live: { values, probes: power, chart },
    why: fbd
      ? 'Live, as the last scan left it: the value each line carried when its network ran; a network the scan did not run is drawn as not executed, a function that did not run as not computed.'
      : sfc
        ? 'Live, as the last scan left it: each active step with its time; a transition’s condition where the transition was enabled and its condition evaluated, not evaluated where it was not; each action’s state.'
        : 'Live, as the last scan left it: the power each element and link had when its rung ran; a rung the scan did not run is drawn as not executed.',
  };
}

/**
 * The Sequential Function Chart of the program with its live state: the editor for an engineer
 * (L3d-3), read-only for everyone else. Its TC6 document is saved through the source route, as an
 * agent saves it over the API (`save_logic_source`), and the editor follows a source saved
 * elsewhere. The document stays reachable as text, read-only, each diagnostic underlined, for what
 * the chart does not show (a diagnostic with no element, the document as written).
 */
function SfcPanel({
  doc,
  sfc,
  state,
  may,
  editor,
  text,
}: {
  doc: ProgramDoc;
  sfc: Sfc;
  state: RunState;
  may: boolean;
  editor: Ref<SourceEditorHandle>;
  text: Ref<HTMLDetailsElement>;
}) {
  const name = doc.entry.name;
  const setSource = usePrograms((s) => s.setSource);
  const save = usePrograms((s) => s.save);
  const checkNow = usePrograms((s) => s.check);
  const current = checkIsCurrent(doc);
  // What the compiler says of an edit follows it by itself, after the editor's own check.
  useEffect(() => {
    if (!may || current || doc.source === null) return undefined;
    const timer = setTimeout(() => void checkNow(name, true), LD_CHECK_AFTER_MS);
    return () => clearTimeout(timer);
  }, [may, current, doc.source, name, checkNow]);
  const watch = usePrograms((s) => s.watches[doc.entry.name]);
  const { live, why } = liveOf(doc, state, watch, useCurrency(state, watch));
  const check = checkIsCurrent(doc) ? doc.check : null;
  const diagnostics = drawnOf(check);
  const program = check?.ok ? (check.program ?? null) : null;
  const chartLive = live === null ? null : { values: live.values, probes: live.chart };
  // The text is drawn only while it is open: a browser lays out what a closed <details> holds.
  const [textOpen, setTextOpen] = useState(!sfc.ok);
  // A document followed from elsewhere that can no longer be drawn opens its text, as one opened
  // so does; it is never folded again by itself, the engineer may be reading it.
  useEffect(() => {
    if (!sfc.ok) setTextOpen(true);
  }, [sfc.ok]);
  return (
    <div className="logic-panel" data-testid="sfc-panel">
      <p className="muted" data-testid="sfc-live" data-live={live === null ? 'none' : live.values === null ? 'unknown' : 'live'}>
        {why}
      </p>
      {!may && (
        <p className="muted" data-testid="sfc-read-only">
          Read-only: only an engineer changes a program.
        </p>
      )}
      {may && sfc.ok && doc.source !== null ? (
        <SfcEditor
          text={doc.source}
          onText={(source) => setSource(name, source)}
          onSave={() => void save(name)}
          live={chartLive}
          program={program}
          diagnostics={diagnostics}
          fallback={<SfcView pous={sfc.pous} program={program} live={chartLive} diagnostics={diagnostics} />}
        />
      ) : sfc.ok ? (
        <SfcView pous={sfc.pous} program={program} live={chartLive} diagnostics={diagnostics} />
      ) : (
        <div className="notice notice--bad" role="alert" data-testid="sfc-unreadable">
          <p>The chart cannot be drawn: {sfc.error}.</p>
        </div>
      )}
      <details
        ref={text}
        className="sfc-document"
        data-testid="sfc-document"
        open={textOpen}
        onToggle={(event) => setTextOpen(event.currentTarget.open)}
      >
        <summary>The PLCopen TC6 document, as text</summary>
        {textOpen && <SourceEditor ref={editor} source={doc.source ?? ''} readOnly diagnostics={diagnostics} onChange={() => undefined} />}
      </details>
    </div>
  );
}

/**
 * The type the compiler gives each output of the PROGRAM's boxes, by `<localId>:<FORMAL>`: the
 * variable that keeps it (`values` of the check, ADR-013 §2.16) and that variable's type. An FBD
 * editor shows these, never a type of its own (§2.16, what the FBD editor needs).
 */
export function outputTypesOf(check: CheckResult | null): Map<string, string> {
  const out = new Map<string, string>();
  if (!check?.ok) return out;
  const types = new Map((check.variables ?? []).map((v) => [v.path.toLowerCase(), v.type]));
  for (const probe of check.values ?? []) {
    if (probe.output === undefined) continue;
    const type = probe.on === true ? 'BOOL' : probe.variable !== undefined ? types.get(probe.variable.toLowerCase()) : undefined;
    if (type !== undefined) out.set(`${probe.local_id}:${probe.output.toUpperCase()}`, type);
  }
  return out;
}

/**
 * The Function Block Diagram of the program with its live state: the editor for an engineer,
 * read-only for everyone else (L3c-2). Its TC6 document is saved through the source route, as an
 * agent saves it over the API (`save_logic_source`), and the editor follows a source saved
 * elsewhere.
 */
function FbdPanel({ doc, fbd, state, may }: { doc: ProgramDoc; fbd: Fbd; state: RunState; may: boolean }) {
  const name = doc.entry.name;
  const setSource = usePrograms((s) => s.setSource);
  const save = usePrograms((s) => s.save);
  const checkNow = usePrograms((s) => s.check);
  const current = checkIsCurrent(doc);
  // What the compiler says of an edit follows it by itself, after the editor's own check.
  useEffect(() => {
    if (!may || current || doc.source === null) return undefined;
    const timer = setTimeout(() => void checkNow(name, true), LD_CHECK_AFTER_MS);
    return () => clearTimeout(timer);
  }, [may, current, doc.source, name, checkNow]);
  const watch = usePrograms((s) => s.watches[doc.entry.name]);
  const { live, why } = liveOf(doc, state, watch, useCurrency(state, watch));
  const check = current ? doc.check : null;
  const diagnostics = drawnOf(check);
  const program = check?.ok ? (check.program ?? null) : null;
  const outputTypes = useMemo(() => outputTypesOf(check ?? null), [check]);
  const view = fbd.ok ? <FbdView pous={fbd.pous} program={program} live={live} diagnostics={diagnostics} /> : null;
  return (
    <div className="logic-panel" data-testid="fbd-panel">
      <p className="muted" data-testid="fbd-live" data-live={live === null ? 'none' : live.values === null ? 'unknown' : 'live'}>
        {why}
      </p>
      {!may && <p className="muted">Read-only: only an engineer changes a program.</p>}
      {may && fbd.ok && doc.source !== null ? (
        <FbdEditor
          text={doc.source}
          onText={(text) => setSource(name, text)}
          onSave={() => void save(name)}
          live={live}
          program={program}
          diagnostics={diagnostics}
          outputTypes={outputTypes}
          fallback={view}
        />
      ) : fbd.ok ? (
        view
      ) : (
        <div className="notice notice--bad" role="alert" data-testid="fbd-unreadable">
          <p>The diagram cannot be drawn: {fbd.error}.</p>
        </div>
      )}
    </div>
  );
}

/** How long after the last edit of a diagram the editor checks it by itself. */
const LD_CHECK_AFTER_MS = 700;

/**
 * The Ladder Diagram of the program with its live state: the editor for an engineer, read-only
 * for everyone else (L3b-2b).
 */
function LadderPanel({ doc, ladder, state, may }: { doc: ProgramDoc; ladder: Ladder; state: RunState; may: boolean }) {
  const name = doc.entry.name;
  const setSource = usePrograms((s) => s.setSource);
  const save = usePrograms((s) => s.save);
  const checkNow = usePrograms((s) => s.check);
  const current = checkIsCurrent(doc);
  // What the compiler says of an edit follows it by itself, after the editor's own check.
  useEffect(() => {
    if (!may || current || doc.source === null) return undefined;
    const timer = setTimeout(() => void checkNow(name, true), LD_CHECK_AFTER_MS);
    return () => clearTimeout(timer);
  }, [may, current, doc.source, name, checkNow]);
  const watch = usePrograms((s) => s.watches[doc.entry.name]);
  const shown = liveOf(doc, state, watch, useCurrency(state, watch));
  const live: LadderLive | null = shown.live === null ? null : { values: shown.live.values, power: shown.live.probes };
  const why = shown.why;
  const check = checkIsCurrent(doc) ? doc.check : null;
  const diagnostics = drawnOf(check);
  const program = check?.ok ? (check.program ?? null) : null;
  return (
    <div className="logic-panel" data-testid="ld-panel">
      <p className="muted" data-testid="ld-live" data-live={live === null ? 'none' : live.values === null ? 'unknown' : 'live'}>
        {why}
      </p>
      {!may && <p className="muted">Read-only: only an engineer changes a program.</p>}
      {may && ladder.ok && doc.source !== null ? (
        <LadderEditor
          text={doc.source}
          onText={(text) => setSource(name, text)}
          onSave={() => void save(name)}
          live={live}
          program={program}
          diagnostics={diagnostics}
          fallback={<LadderView pous={ladder.pous} program={program} live={live} diagnostics={diagnostics} />}
        />
      ) : ladder.ok ? (
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

/** The variables a diagram's contacts, coils and variable elements read or write. */
function elementVariables(diagram: Ladder | Fbd | null): string[] {
  if (diagram === null || !diagram.ok) return [];
  const pous: Array<{ pouType: string; elements: Array<{ variable: string | null }> }> = diagram.pous;
  return pous
    .filter((pou) => pou.pouType === 'program')
    .flatMap((pou) => pou.elements.map((e) => e.variable ?? ''))
    .filter((path) => PLACE.test(path) && !/^(TRUE|FALSE)$/i.test(path));
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

/** Who sets what the program editor shows in the workbench's panes. */
export const PANES = {
  instructions: 'logic-instructions',
  runtime: 'logic-runtime',
  program: 'logic-program',
  check: 'logic-check',
} as const;

const LANGUAGE: Record<string, string> = {
  st: 'Structured Text (ST)',
  ld: 'Ladder Diagram (LD)',
  fbd: 'Function Block Diagram (FBD)',
  sfc: 'Sequential Function Chart (SFC)',
};

/** The "Runtime" task card: what the controller's runtime does with the program, and its commands. */
function RuntimeCard({ name }: { name: string }) {
  const runtimes = usePrograms((s) => s.runtimes);
  const busy = usePrograms((s) => s.busy !== null);
  return <RuntimePanel name={name} state={runState(name, runtimes)} busy={busy} />;
}

/** The program's properties in the inspector: what it is, its task, and removing it. */
function ProgramProperties({ name, onRemoved }: { name: string; onRemoved: () => void }) {
  const doc = usePrograms((s) => s.programs.find((d) => d.entry.name === name));
  const busy = usePrograms((s) => s.busy !== null);
  if (!doc) {
    return (
      <p className="muted" data-testid="program-properties">
        The project has no program called {name}.
      </p>
    );
  }
  const may = mayEngineer();
  return (
    <div className="logic-properties" data-testid="program-properties">
      <dl className="props">
        <dt>Program</dt>
        <dd className="mono">{name}</dd>
        <dt>Language</dt>
        <dd>{LANGUAGE[doc.entry.language] ?? doc.entry.language}</dd>
      </dl>
      <SettingsPanel doc={doc} readOnly={!may} />
      {may && <RemovePanel name={name} busy={busy} onRemoved={onRemoved} />}
    </div>
  );
}

/** What the compiler said of the program, in the inspector's Diagnostics tab. */
function CheckCard({ name, goTo }: { name: string; goTo: (diagnostic: Diagnostic) => void }) {
  const doc = usePrograms((s) => s.programs.find((d) => d.entry.name === name));
  if (!doc) return null;
  return <CheckPanel doc={doc} goTo={goTo} />;
}

/**
 * How many problems the check of the text now found, for the count on the Diagnostics tab:
 * `null` when the text now has not been checked, which counts as nothing known, not as none.
 */
function problemCount(doc: ProgramDoc | undefined): number | null {
  if (!doc || doc.check === null || !checkIsCurrent(doc)) return null;
  // Every diagnostic counts, those of another source too (the library, the project's types):
  // each stops this program from compiling. `project_problems` do not: the program was
  // compiled without the project's types and data blocks, and they are listed apart.
  return (doc.check.ok ? 0 : doc.check.diagnostics.length) + (doc.check.binding_problems?.length ?? 0);
}

/** The check of the text, in a word, on the editor's toolbar; it opens the Diagnostics tab. */
function CheckSummary({ doc }: { doc: ProgramDoc }) {
  const count = problemCount(doc);
  const cause = staleCause(doc);
  const [tone, text, title] =
    doc.check === null
      ? ['neutral', 'NOT CHECKED', 'The program has not been checked yet.']
      : doc.checkedSource !== doc.source
        ? ['neutral', 'CHANGED SINCE CHECK', 'The text has changed since its last check.']
        : cause !== null
          ? ['neutral', STALE_BADGE[cause], STALE_SAID[cause]]
          : !checkIsCurrent(doc)
            ? ['neutral', 'CHANGED SINCE CHECK', 'The text has changed since its last check.']
        : !doc.check.ok
          ? ['bad', `${count} ${count === 1 ? 'ERROR' : 'ERRORS'}`, 'The program does not compile.']
          : count !== null && count > 0
            ? ['warn', `COMPILES · ${count} BINDING ${count === 1 ? 'PROBLEM' : 'PROBLEMS'}`, 'The program compiles; some bindings are wrong.']
            : ['good', 'COMPILES', 'The program compiles.'];
  return (
    <button
      type="button"
      className="btn btn--flat"
      title={`${title} Show what the compiler said (Inspector → Diagnostics).`}
      onClick={() => host().showDiagnostics()}
      data-testid="check-summary"
      data-state={tone}
    >
      <span className={`badge badge--${tone}`}>{text}</span>
    </button>
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
  const isFbd = doc?.entry.language === 'fbd';
  const isSfc = doc?.entry.language === 'sfc';
  const fbdSource = isFbd ? (doc?.source ?? null) : null;
  const fbd = useMemo(() => (fbdSource === null ? null : readFbd(fbdSource)), [fbdSource]);
  const sfcSource = isSfc ? (doc?.source ?? null) : null;
  const sfc = useMemo(() => (sfcSource === null ? null : readSfc(sfcSource)), [sfcSource]);
  const sfcText = useRef<HTMLDetailsElement>(null);
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
  // A chart's states are kept where the check of the text says: each asked for by its path, the
  // enable flag of a transition with its condition.
  const chartPaths =
    isSfc && doc?.check?.ok ? (doc.check.chart ?? []).flatMap((p) => (p.ran === undefined ? [p.variable] : [p.variable, p.ran])) : [];
  const extra = [...new Set([...elementVariables(isFbd ? fbd : ladder), ...chartPaths])].filter((path) => {
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

  // What goes into the workbench's panes, while the program is there to show.
  const ready = doc !== undefined && doc.source !== null;
  const instructions = ready && ((isLadder && ladder?.ok === true) || (isFbd && fbd?.ok === true) || (isSfc && sfc?.ok === true)) && mayEngineer();
  const goToRef = useRef<(diagnostic: Diagnostic) => void>(() => undefined);
  const onRemovedRef = useRef(onRemoved);
  onRemovedRef.current = onRemoved;
  useEffect(() => {
    if (!ready) return undefined;
    const workbench = host();
    // Set in TIA's order: the instructions first, the online tools after.
    if (instructions) {
      const card = isFbd ? <InstructionsCard kit={FBD_KIT} /> : isSfc ? <InstructionsCard kit={SFC_KIT} /> : <InstructionsCard />;
      workbench.setTaskCard(PANES.instructions, { title: 'Instructions', mount: (pane) => draw(pane, card) });
    }
    workbench.setTaskCard(PANES.runtime, { title: 'Runtime', mount: (card) => draw(card, <RuntimeCard name={name} />) });
    return () => {
      workbench.setTaskCard(PANES.instructions, null);
      workbench.setTaskCard(PANES.runtime, null);
    };
  }, [ready, instructions, isFbd, isSfc, name]);
  const properties = useMemo<Panel>(
    () => ({
      title: 'Program',
      mount: (panel) => draw(panel, <ProgramProperties name={name} onRemoved={() => onRemovedRef.current?.(name)} />),
    }),
    [name]
  );
  useEffect(() => {
    if (!ready) return undefined;
    host().setInspector(PANES.program, properties);
    return () => host().setInspector(PANES.program, null);
  }, [ready, properties]);
  const checkMount = useMemo<Panel['mount']>(
    () => (panel) => draw(panel, <CheckCard name={name} goTo={(diagnostic) => goToRef.current(diagnostic)} />),
    [name]
  );
  const count = problemCount(doc);
  const countRef = useRef(count);
  countRef.current = count;
  useEffect(() => {
    if (!ready) return undefined;
    host().setDiagnostics(PANES.check, { title: `Compile: ${name}`, mount: checkMount, count: countRef.current });
    return () => host().setDiagnostics(PANES.check, null);
  }, [ready, name, checkMount]);
  // Only the count changes after a check: the same mount, so nothing is drawn again.
  useEffect(() => {
    if (ready) host().setDiagnostics(PANES.check, { title: `Compile: ${name}`, mount: checkMount, count });
  }, [ready, name, checkMount, count]);

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
  // Load replaces the instance named like the program: not offered for one that runs in a
  // runtime of another id, where it would report a load it did not make.
  const held = inOtherRuntime(state);
  const diagnostics = drawnOf(checkIsCurrent(doc) ? doc.check : null);
  // What runs is another version than the last one built here: said, since the text may differ.
  const differs =
    state.kind === 'loaded' && doc.built !== null && doc.built.version !== state.runtime.version;
  // What the compiler said is brought forward after a Check the engineer asked for, as TIA opens
  // its compile results, and after a Load only when the compiler refused it (a load that went
  // through says so in the work area). A check made by itself while editing moves no pane.
  const showChecked = () => {
    const now = usePrograms.getState().programs.find((d) => d.entry.name === name);
    if (now?.check && checkIsCurrent(now)) host().showDiagnostics();
  };
  const showIfRefused = () => {
    const now = usePrograms.getState().programs.find((d) => d.entry.name === name);
    if (now?.check && checkIsCurrent(now) && !now.check.ok) host().showDiagnostics();
  };
  goToRef.current = (diagnostic: Diagnostic) => {
    const id = diagnostic.element?.local_id;
    // In a chart, a diagnostic with no element of the drawing is shown in the document's text.
    if (isSfc && (id === null || id === undefined)) {
      // Opening it draws the text; it is gone to once drawn.
      if (sfcText.current) sfcText.current.open = true;
      setTimeout(() => editor.current?.goTo(diagnostic.line, diagnostic.col), 0);
      return;
    }
    if (!isLadder && !isFbd && !isSfc) {
      editor.current?.goTo(diagnostic.line, diagnostic.col);
      return;
    }
    if (id === null || id === undefined) return;
    const mark = isFbd ? 'fbd-diagnostic' : isSfc ? 'sfc-diagnostic' : 'ld-diagnostic';
    const found = document.querySelector(`[data-testid="${mark}"][data-local-id="${id}"]`);
    (found as (Element & { scrollIntoView?: (arg?: unknown) => void }) | null)?.scrollIntoView?.({ block: 'center' });
  };

  return (
    <div
      className="editor__column"
      data-testid="program-editor"
      // A click in the editor shows the program's properties again, as TIA shows a block's.
      onMouseDown={() => host().setInspector(PANES.program, properties)}
    >
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
        <button type="button" className="btn" disabled={working} onClick={() => void check(name).then(showChecked)}>
          <Check size={13} aria-hidden /> Check
        </button>
        <button
          type="button"
          className="btn"
          disabled={!may || working || held !== null}
          onClick={() => void download(name).then(showIfRefused)}
          title={
            held ??
            "Save, check and build the program, and put it into the project's runtimes. A new version of the program while it runs is changed online: it takes over the running variables and scans on."
          }
          data-testid="program-load"
        >
          <Download size={13} aria-hidden /> Load
        </button>
        <button
          type="button"
          className="btn"
          disabled={!may || working || held !== null}
          onClick={() => {
            // A cold load throws away what the running program holds: it is asked first.
            if (
              window.confirm(
                `Load ${name} cold? Every variable starts from its initial value, RETAIN variables included, and nothing the running version holds is carried over.`
              )
            ) {
              void download(name, true).then(showIfRefused);
            }
          }}
          title={held ?? 'Load the program with every variable from its initial value, RETAIN ones included.'}
          data-testid="program-load-cold"
        >
          <Download size={13} aria-hidden /> Load cold
        </button>
        <CheckSummary doc={doc} />
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
        <VariablesPanel doc={doc} readOnly={!may} state={state} />
        <div className="logic-editor__source">
          {isLadder && ladder !== null ? (
            <LadderPanel doc={doc} ladder={ladder} state={state} may={may} />
          ) : isFbd && fbd !== null ? (
            <FbdPanel doc={doc} fbd={fbd} state={state} may={may} />
          ) : isSfc && sfc !== null ? (
            <SfcPanel doc={doc} sfc={sfc} state={state} may={may} editor={editor} text={sfcText} />
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
      </div>
    </div>
  );
}
