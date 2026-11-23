// The editor of one configured runtime of the project (ADR-013 §2.19, C8d-1): which runtime module
// runs it, the target its programs are compiled for, whether it runs when loaded, and its tasks,
// each with its kind, priority, watchdog and programs in order. Checked and saved through the logic
// module, which answers every rule in its own words; what is shown as saved is what it read back.
// The form is the runtime as configured; whether the controller runs it is said only on the
// "Runtime" task card, from what the core lists.

import { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowDown, ArrowUp, Check, CirclePlay, CircleStop, Download, Plus, Save, Trash2, Undo2, Upload } from 'lucide-react';
import type { DiagnosticsPanel } from '@mythos/workbench-sdk';
import { host, mayEngineer, type Diagnostic, type NotCarried, type RuntimeLoad, type RuntimeStatus } from './bridge';
import { draw } from './draw';
import { WITHHELD } from './library';
import { describeStart, usePrograms } from './programs';
import { RUNTIMES_POLL_MS } from './ProgramEditor';
import {
  deleteRuntimeQuestion,
  describeLoad,
  heldBy,
  loadedIds,
  kindOf,
  kindsOf,
  missingOf,
  newTask,
  PROGRAM_EDITOR,
  TASK_KINDS,
  taskLabel,
  type LoadAnswer,
  type ProgramCheck,
  type RuntimeEntry,
  type TaskConfig,
  type TaskKind,
} from './runtimes';
import { draftKey, hasChanges, useRuntimes, type RuntimeDoc, type Stopped, type VersionsHere } from './runtimestore';

/** The owner of this editor's panel in the Diagnostics tab. */
export const RUNTIME_DIAGNOSTICS_OWNER = 'logic-runtime-check';
/** The owner of this editor's "Runtime" task card: what the controller says of the runtime. */
export const RUNTIME_STATUS_OWNER = 'logic-runtime-status';

const MODE_TONE: Record<string, string> = { run: 'good', stop: 'neutral', fault: 'bad', unreachable: 'bad' };

/** The question asked before a cold load starts anything. */
export const loadColdQuestion = (id: string) =>
  `Load runtime ${id} cold? Every runtime this load starts or replaces starts from its initial values, RETAIN variables included, and nothing they hold is carried over. The draft is saved first if it has changes, and what the load will change is asked before anything is loaded.`;

/** Said beside an empty watchdog: the protocol's default (crates/mythos-module/src/protocol.rs, `TaskConfig::watchdog_ms`). */
export const WATCHDOG_DEFAULT =
  "Empty: the runtime's default, ten intervals and never less than 2 s for a cyclic or cycle task, 2 s for the others.";

/**
 * How many problems the Diagnostics tab lists for `doc`'s last check: the runtime's problems and
 * every program's diagnostics and binding problems; `null` before anything was checked.
 */
export function problemCount(doc: RuntimeDoc | undefined): number | null {
  if (!doc || doc.check === null) return null;
  const { answer } = doc.check;
  return answer.problems.length + answer.programs.reduce((n, p) => n + p.diagnostics.length + p.binding_problems.length, 0);
}

/** Where a diagnostic is: its source when it names one, and its line and column, or "about the whole". */
const placeOf = (d: Diagnostic): string => {
  const at = d.line === 0 ? 'about the whole' : `${d.line}:${d.col}`;
  return d.source ? `${d.source} ${at}` : at;
};

/**
 * A number typed into a field: `null` while nothing is typed; `NaN` when what is typed is not a
 * finite number (text a browser keeps but cannot read, `badInput`, comes with an empty value; text
 * such as `1e400` reads as Infinity, which JSON would send as `null`). `NaN` is never sent: the form
 * says the field is not a number ({@link missingOf}).
 */
export function numberOf(text: string, badInput = false): number | null {
  if (badInput) return Number.NaN;
  if (text.trim() === '') return null;
  const n = Number(text);
  return Number.isFinite(n) ? n : Number.NaN;
}
const shown = (n: number | null | undefined): string => (n === null || n === undefined || Number.isNaN(n) ? '' : String(n));

/**
 * A number field of the form. It shows what was typed, and gives the draft what that text stands
 * for: `empty` when nothing is typed, which is what the module stores when the key is left out
 * (`undefined`: the key left out, as the module stores a watchdog not given; `0`: the module's
 * default it fills in; `null`: nothing, a value that must be given). So typing a value and clearing
 * it again leaves the draft as stored. A value that comes from elsewhere (a read, a save's read
 * back, a discard) is shown as it is.
 */
function NumberField({
  value,
  empty,
  onValue,
  ...input
}: {
  value: number | null | undefined;
  empty: number | null | undefined;
  onValue: (value: number | null | undefined) => void;
  'aria-label': string;
  'data-testid': string;
  disabled: boolean;
}) {
  const [text, setText] = useState(() => shown(value));
  // The value this field last gave the draft: when the draft comes back with it, the text typed stays.
  const given = useRef<{ value: number | null | undefined } | null>(null);
  useEffect(() => {
    if (given.current !== null && Object.is(given.current.value, value)) return;
    given.current = null;
    setText(shown(value));
  }, [value]);
  return (
    <input
      type="number"
      {...input}
      value={text}
      onChange={(event) => {
        const typed = event.target.value;
        const n = numberOf(typed, event.target.validity?.badInput === true);
        const next = n === null ? empty : n;
        given.current = { value: next };
        setText(typed);
        onValue(next);
      }}
    />
  );
}

/** One program of a check: whether it compiles, its diagnostics (each opens the program) and binding problems. */
function ProgramResult({ program }: { program: ProgramCheck }) {
  return (
    <li data-testid="runtime-check-program" data-program={program.name}>
      <p>
        <strong>{program.name}</strong>
        {program.ok ? ': compiles' : ': does not compile as configured'}
        {program.version !== undefined && <span className="muted mono"> · version {program.version}</span>}
      </p>
      <ul className="logic-diagnostics" aria-label={`Diagnostics of ${program.name}`}>
        {program.diagnostics.map((d, index) => (
          <li key={index}>
            <button
              type="button"
              className="logic-diagnostic"
              data-testid="runtime-diagnostic"
              title={`Open program ${program.name}`}
              onClick={() => host().openEditor(PROGRAM_EDITOR, program.name)}
            >
              <span className="mono logic-diagnostic__at">{placeOf(d)}</span>
              {d.message}
            </button>
          </li>
        ))}
        {program.binding_problems.map((problem, index) => (
          <li key={`b${index}`} className="logic-diagnostic logic-diagnostic--plain" data-testid="runtime-binding-problem">
            {problem}
          </li>
        ))}
      </ul>
    </li>
  );
}

/** The Diagnostics tab's panel: the last check of this runtime's draft, as the module answered it. */
function CheckCard({ id }: { id: string }) {
  const doc = useRuntimes((state) => state.docs[id]);
  if (!doc) return null;
  const { check } = doc;
  if (check === null) {
    return (
      <p className="logic-check muted" data-testid="runtime-check-none">
        Not checked yet.
      </p>
    );
  }
  const { answer } = check;
  const projectProblems = answer.project_problems ?? [];
  return (
    <div className="logic-check" data-testid="runtime-check-result">
      {check.body !== draftKey(doc.draft) && (
        <p className="muted" data-testid="runtime-check-stale">
          This is the check of an earlier draft. The draft has changed since.
        </p>
      )}
      {answer.ok && (
        <p data-testid="runtime-check-ok">
          Runtime {answer.runtime} checks with this draft{answer.version !== undefined ? `, as version ${answer.version}` : ''}. Nothing
          was saved, built or loaded.
        </p>
      )}
      {answer.problems.length > 0 && (
        <ul className="logic-diagnostics" aria-label="Problems of the runtime">
          {answer.problems.map((problem, index) => (
            <li key={index} className="logic-diagnostic logic-diagnostic--plain" data-testid="runtime-problem">
              {problem}
            </li>
          ))}
        </ul>
      )}
      <ul className="logic-diagnostics" aria-label="Programs">
        {answer.programs.map((program) => (
          <ProgramResult key={program.name} program={program} />
        ))}
      </ul>
      {projectProblems.length > 0 && (
        <>
          <p className="muted" data-testid="runtime-project-problems-note">
            The project's types and data blocks do not check, so the programs were checked without them.
          </p>
          <ul className="logic-diagnostics" aria-label="Problems in the project's types and data blocks">
            {projectProblems.map((d, index) => (
              <li key={index} className="logic-diagnostic logic-diagnostic--plain" data-testid="runtime-project-problem">
                <span className="mono logic-diagnostic__at">{placeOf(d)}</span>
                {d.message}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function CheckState({ doc }: { doc: RuntimeDoc }) {
  if (doc.checking || doc.check === null) {
    return (
      <span className="badge badge--neutral" data-testid="runtime-check-state">
        {doc.checking ? 'CHECKING' : 'NOT CHECKED'}
      </span>
    );
  }
  const earlier = doc.check.body !== draftKey(doc.draft);
  const tone = earlier ? 'neutral' : doc.check.answer.ok ? 'good' : 'bad';
  return (
    <button type="button" className="btn btn--flat" onClick={() => host().showDiagnostics()} title="Show the diagnostics">
      <span className={`badge badge--${tone}`} data-testid="runtime-check-state">
        {doc.check.answer.ok ? 'CHECKS' : 'DOES NOT CHECK'}
        {earlier ? ' (EARLIER DRAFT)' : ''}
      </span>
    </button>
  );
}

/** One task of the draft: its fields and its programs in the order they run. */
function TaskRow({
  task,
  index,
  count,
  draft,
  readOnly,
  supported,
  moduleName,
  onChange,
  onMove,
  onRemove,
}: {
  task: TaskConfig;
  index: number;
  count: number;
  draft: RuntimeEntry;
  readOnly: boolean;
  /** The kinds the selected module says it runs; `null`/`undefined` when it does not say. */
  supported: string[] | null | undefined;
  moduleName: string;
  onChange: (task: TaskConfig) => void;
  onMove: (by: -1 | 1) => void;
  onRemove: () => void;
}) {
  const projectPrograms = usePrograms((state) => state.programs);
  const programsPhase = usePrograms((state) => state.phase);
  const list = useRuntimes((state) => state.list);
  const label = taskLabel(task, index);
  const kind = task.kind;
  const setKind = (change: Partial<TaskKind>) => kind !== null && onChange({ ...task, kind: { ...kind, ...change } as TaskKind });
  const inDraft = new Set(draft.tasks.flatMap((t) => t.programs.map((p) => p.toLowerCase())));
  const held = list === null ? null : heldBy(list.runtimes, draft.id);
  const offered = programsPhase === 'ready' ? projectPrograms.map((doc) => doc.entry.name).filter((name) => !inDraft.has(name.toLowerCase())) : [];
  const moveProgram = (at: number, by: -1 | 1) => {
    const programs = [...task.programs];
    const [moved] = programs.splice(at, 1);
    if (moved === undefined) return;
    programs.splice(at + by, 0, moved);
    onChange({ ...task, programs });
  };

  return (
    <section className="logic-panel" data-testid="runtime-task" aria-label={label}>
      <div className="logic-fields">
        <label className="field">
          <span className="field__label">Task name</span>
          <input
            aria-label="Task name"
            className="mono"
            data-testid="task-name"
            value={task.name}
            disabled={readOnly}
            onChange={(event) => onChange({ ...task, name: event.target.value })}
          />
        </label>
        <label className="field">
          <span className="field__label">Kind</span>
          <select
            aria-label="Task kind"
            data-testid="task-kind"
            value={kind?.type ?? ''}
            disabled={readOnly}
            onChange={(event) => {
              const chosen = TASK_KINDS.find((k) => k === event.target.value);
              onChange({ ...task, kind: chosen === undefined ? null : kindOf(chosen) });
            }}
          >
            {kind === null && <option value="">Choose a kind</option>}
            {TASK_KINDS.map((k) => (
              <option key={k} value={k} data-testid="task-kind-option">
                {Array.isArray(supported) && !supported.includes(k) ? `${k} (not run by ${moduleName})` : k}
              </option>
            ))}
          </select>
        </label>
        {kind?.type === 'cyclic' && (
          <>
            <label className="field">
              <span className="field__label">Interval (ms)</span>
              <NumberField
                aria-label="Interval in milliseconds"
                data-testid="task-interval"
                value={kind.interval_ms}
                empty={null}
                disabled={readOnly}
                onValue={(n) => setKind({ interval_ms: n ?? null })}
              />
            </label>
            <label className="field">
              <span className="field__label">Phase (ms)</span>
              <NumberField
                aria-label="Phase in milliseconds"
                data-testid="task-phase"
                value={kind.phase_ms}
                empty={0}
                disabled={readOnly}
                onValue={(n) => setKind({ phase_ms: n })}
              />
              <span className="field__hint">Empty: 0, the module's default.</span>
            </label>
          </>
        )}
        {kind?.type === 'cycle' && (
          <label className="field">
            <span className="field__label">Shortest cycle (ms)</span>
            <NumberField
              aria-label="Shortest cycle in milliseconds"
              data-testid="task-min-cycle"
              value={kind.min_cycle_ms}
              empty={0}
              disabled={readOnly}
              onValue={(n) => setKind({ min_cycle_ms: n })}
            />
            <span className="field__hint">Empty: 0, the module's default.</span>
          </label>
        )}
        {kind?.type === 'event' && (
          <label className="field">
            <span className="field__label">Tag</span>
            <input
              aria-label="Event tag"
              className="mono"
              data-testid="task-tag"
              value={kind.tag}
              disabled={readOnly}
              onChange={(event) => setKind({ tag: event.target.value })}
            />
            <span className="field__hint">The task runs at each rising edge of this BOOL tag.</span>
          </label>
        )}
        <label className="field">
          <span className="field__label">Priority</span>
          <NumberField
            aria-label="Task priority"
            data-testid="task-priority"
            value={task.priority}
            empty={null}
            disabled={readOnly}
            onValue={(n) => onChange({ ...task, priority: n ?? null })}
          />
          <span className="field__hint">0 is the highest.</span>
        </label>
        <label className="field">
          <span className="field__label">Watchdog (ms)</span>
          <NumberField
            aria-label="Watchdog in milliseconds"
            data-testid="task-watchdog"
            value={task.watchdog_ms}
            empty={undefined}
            disabled={readOnly}
            onValue={(n) => onChange({ ...task, watchdog_ms: n })}
          />
          <span className="field__hint">{WATCHDOG_DEFAULT}</span>
        </label>
      </div>
      <p className="field__label">Programs, in the order they run</p>
      <ol className="logic-runtime__programs">
        {task.programs.map((program, at) => (
          <li key={`${program}:${at}`} className="logic-binding" data-testid="task-program">
            <span className="mono logic-binding__tag" data-testid="task-program-name">
              {program}
            </span>
            {!readOnly && (
              <>
                <button type="button" className="icon-btn" aria-label={`Move ${program} up`} disabled={at === 0} onClick={() => moveProgram(at, -1)}>
                  <ArrowUp size={13} />
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={`Move ${program} down`}
                  disabled={at === task.programs.length - 1}
                  onClick={() => moveProgram(at, 1)}
                >
                  <ArrowDown size={13} />
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={`Take ${program} out of ${label}`}
                  onClick={() => onChange({ ...task, programs: task.programs.filter((_, i) => i !== at) })}
                >
                  <Trash2 size={13} />
                </button>
              </>
            )}
          </li>
        ))}
      </ol>
      {!readOnly && programsPhase === 'ready' && (
        <label className="field">
          <span className="field__label">Add program</span>
          <select
            aria-label={`Add a program to ${label}`}
            data-testid="task-add-program"
            value=""
            onChange={(event) => {
              const name = event.target.value;
              if (name !== '') onChange({ ...task, programs: [...task.programs, name] });
            }}
          >
            <option value="">Choose a program</option>
            {offered.map((name) => {
              const other = held?.get(name.toLowerCase());
              return (
                <option key={name} value={name} disabled={other !== undefined} data-testid="task-add-program-option">
                  {other !== undefined ? `${name} (in runtime ${other})` : name}
                </option>
              );
            })}
          </select>
          {held === null && <span className="field__hint">Which runtime holds each program is not known: the runtimes could not be read.</span>}
        </label>
      )}
      {!readOnly && programsPhase === 'unreadable' && (
        <p className="muted" data-testid="task-programs-unknown">
          The project's programs could not be read: none can be added.
        </p>
      )}
      {!readOnly && (
        <div className="logic-panel__actions">
          <button type="button" className="btn" aria-label={`Move ${label} up`} disabled={index === 0} onClick={() => onMove(-1)}>
            <ArrowUp size={13} aria-hidden /> Up
          </button>
          <button type="button" className="btn" aria-label={`Move ${label} down`} disabled={index === count - 1} onClick={() => onMove(1)}>
            <ArrowDown size={13} aria-hidden /> Down
          </button>
          <button type="button" className="btn" data-testid="task-remove" onClick={onRemove}>
            <Trash2 size={13} aria-hidden /> Remove task
          </button>
        </div>
      )}
    </section>
  );
}

/** The form: module, target, run choice and tasks. */
function RuntimeForm({ doc, readOnly }: { doc: RuntimeDoc; readOnly: boolean }) {
  const edit = useRuntimes((state) => state.edit);
  const modules = useRuntimes((state) => state.modules);
  const modulesError = useRuntimes((state) => state.modulesError);
  const { draft } = doc;
  const set = (change: Partial<RuntimeEntry>) => edit(doc.id, { ...draft, ...change });
  const setTasks = (tasks: TaskConfig[]) => set({ tasks });
  const provides = modules?.map((m) => m.provides) ?? null;
  const supported = kindsOf(modules, draft.module);

  return (
    <div className="logic-runtime__form">
      <section className="logic-panel" aria-label="Runtime">
        <div className="logic-fields">
          <label className="field">
            <span className="field__label">Runtime module</span>
            {provides === null ? (
              <input
                aria-label="Runtime module"
                className="mono"
                data-testid="runtime-module"
                value={draft.module}
                disabled={readOnly}
                onChange={(event) => set({ module: event.target.value })}
              />
            ) : (
              <select
                aria-label="Runtime module"
                data-testid="runtime-module"
                value={draft.module}
                disabled={readOnly}
                onChange={(event) => set({ module: event.target.value })}
              >
                {draft.module === '' && <option value="">Choose a module</option>}
                {draft.module !== '' && !provides.includes(draft.module) && (
                  <option value={draft.module}>{`${draft.module} (not installed)`}</option>
                )}
                {provides.map((module) => (
                  <option key={module} value={module}>
                    {module}
                  </option>
                ))}
              </select>
            )}
            {provides === null && (
              <span className="field__hint" data-testid="runtime-modules-unknown">
                {modulesError === null
                  ? 'The installed runtime modules are not known yet.'
                  : `The installed runtime modules could not be read: ${modulesError}`}
              </span>
            )}
          </label>
          <label className="field">
            <span className="field__label">Target profile</span>
            <input
              aria-label="Target profile"
              className="mono"
              data-testid="runtime-target"
              value={draft.target}
              disabled={readOnly}
              onChange={(event) => set({ target: event.target.value })}
            />
            <span className="field__hint">What its programs are compiled for.</span>
          </label>
          <fieldset className="field" data-testid="runtime-run-choice" disabled={readOnly}>
            <legend className="field__label">When loaded</legend>
            <label className="settings__check">
              <input
                type="radio"
                name={`run-${doc.id}`}
                data-testid="runtime-run"
                checked={draft.run === true}
                onChange={() => set({ run: true })}
              />
              Run when loaded
            </label>
            <label className="settings__check">
              <input
                type="radio"
                name={`run-${doc.id}`}
                data-testid="runtime-stop"
                checked={draft.run === false}
                onChange={() => set({ run: false })}
              />
              Stay in STOP when loaded
            </label>
          </fieldset>
        </div>
        {draft.module !== '' && supported === null && (
          <p className="muted" data-testid="runtime-kinds-note">
            {draft.module} does not say it runs tasks: the core refuses a runtime with tasks for it.
          </p>
        )}
      </section>
      {draft.tasks.map((task, index) => (
        <TaskRow
          key={index}
          task={task}
          index={index}
          count={draft.tasks.length}
          draft={draft}
          readOnly={readOnly}
          supported={supported}
          moduleName={draft.module}
          onChange={(changed) => setTasks(draft.tasks.map((t, i) => (i === index ? changed : t)))}
          onMove={(by) => {
            const tasks = [...draft.tasks];
            const [moved] = tasks.splice(index, 1);
            if (moved === undefined) return;
            tasks.splice(index + by, 0, moved);
            setTasks(tasks);
          }}
          onRemove={() => setTasks(draft.tasks.filter((_, i) => i !== index))}
        />
      ))}
      {draft.tasks.length === 0 && (
        <p className="logic-panel muted" data-testid="runtime-no-task">
          No task yet.
        </p>
      )}
      {!readOnly && (
        <div className="logic-panel">
          <button type="button" className="btn" data-testid="runtime-add-task" onClick={() => setTasks([...draft.tasks, newTask()])}>
            <Plus size={13} aria-hidden /> Add task
          </button>
        </div>
      )}
    </div>
  );
}

/** The programs a listed runtime runs, by name: its `programs`, else its joined `program`. */
const runsOf = (runtime: RuntimeStatus): string[] =>
  Array.isArray(runtime.programs) ? runtime.programs.map((p) => p.name) : runtime.program.split(', ').filter((n) => n !== '');

/** One program of a listed runtime: the version that runs beside the one last built or checked here. */
function ProgramVersion({ name, version, here }: { name: string; version: string; here: VersionsHere | null }) {
  const mine = here?.byName[name.toLowerCase()];
  const compared =
    mine === undefined
      ? 'not known: nothing of it was built or checked here'
      : mine === version
        ? `the same as the one ${here?.of === 'build' ? 'built' : 'checked'} here`
        : `another version than the one ${here?.of === 'build' ? 'built' : 'checked'} here (${mine})`;
  return (
    <li data-testid="runtime-program" data-program={name}>
      <span className="mono">{name}</span> · version <span className="mono">{version}</span>:{' '}
      <span data-testid="runtime-program-compare">{compared}</span>
    </li>
  );
}

/** Why a command, an unload or a load stopped: these words, then the refuser's own. */
function StoppedNotice({ stopped, testid }: { stopped: Stopped; testid: string }) {
  const { refusal, problems } = stopped;
  return (
    <div className="notice notice--bad" role="alert" data-testid={testid}>
      <div>
        <p>{stopped.words}</p>
        {refusal !== null && <p data-testid={`${testid}-error`}>{refusal.error}</p>}
        {(refusal?.diagnostics ?? []).map((d, index) => (
          <p key={index} className="mono" data-testid={`${testid}-diagnostic`}>
            {placeOf(d)} {d.message}
          </p>
        ))}
        {(problems ?? []).map((problem, index) => (
          <p key={`p${index}`} data-testid={`${testid}-problem`}>
            {problem.connection_id !== '' && <span className="mono">{problem.connection_id}: </span>}
            {problem.message}
          </p>
        ))}
        {refusal?.withheld && <p>{WITHHELD}</p>}
      </div>
    </div>
  );
}

/** Each variable of a list, as `variable (why)`. */
function Carried({ label, list, testid }: { label: string; list: NotCarried[]; testid: string }) {
  if (list.length === 0) return null;
  return (
    <p>
      {label}{' '}
      {list.map((d, i) => (
        <span key={d.variable} data-testid={testid}>
          {i > 0 ? '; ' : ''}
          <span className="mono">{d.variable}</span> ({d.why})
        </span>
      ))}
      .
    </p>
  );
}

/** How one runtime of a load started, as the core said it. */
function StartOf({ id, load }: { id: string; load: RuntimeLoad | null }) {
  return (
    <div data-testid="runtime-start" data-runtime={id}>
      <p>
        Runtime <span className="mono">{id}</span>: <span data-testid="runtime-start-said">{describeStart(load)}</span>
      </p>
      {load !== null && (
        <>
          <Carried label="Not carried over:" list={load.dropped} testid="runtime-dropped" />
          <Carried label="From the initial value:" list={load.initialized} testid="runtime-initialized" />
          <Carried label="Not given the value kept for it:" list={load.not_given ?? []} testid="runtime-not-given" />
        </>
      )}
    </div>
  );
}

/** What the core answered the last load made here: exactly what it said, nothing more. */
function LoadResult({ loaded }: { loaded: LoadAnswer }) {
  const { plan, answer } = loaded;
  const ids = loadedIds(plan);
  const failed = (answer.changes?.failed ?? []).filter(([failedId]) => ids.some((r) => failedId === `runtime:${r}`));
  const builds = [plan.build, ...plan.rebuilt.map((r) => r.build)];
  const projectProblems = builds.flatMap((b) => b.project_problems ?? []);
  return (
    <div className={`notice${failed.length > 0 ? ' notice--bad' : ''}`} data-testid="runtime-loaded">
      <div>
        <p data-testid="runtime-loaded-said">{describeLoad(loaded)}</p>
        {failed.map(([failedId, why]) => (
          <p key={failedId} data-testid="load-failed">
            <span className="mono">{failedId}</span>: {why}
          </p>
        ))}
        {answer.applied &&
          ids
            .filter((r) => !failed.some(([failedId]) => failedId === `runtime:${r}`))
            .map((r) => <StartOf key={r} id={r} load={answer.changes?.runtimes?.find((l) => l.runtime === r) ?? null} />)}
        {plan.takenOut.map((i) => (
          <p key={i.id} data-testid="load-taken-out">
            Taken out of the project's runtimes: <span className="mono">{i.id}</span>.
          </p>
        ))}
        {builds.flatMap((b) => b.warnings).map((warning, index) => (
          <p key={index} data-testid="build-warning">
            {warning}
          </p>
        ))}
        {projectProblems.map((d, index) => (
          <p key={`pp${index}`} data-testid="load-project-problem">
            <span className="mono">{placeOf(d)}</span> {d.message}
          </p>
        ))}
      </div>
    </div>
  );
}

/**
 * The "Runtime" task card: what the controller says of this runtime, from the same reading of
 * its runtimes the program editor makes. NOT LOADED only after a reading that succeeded; what was
 * read before a reading failed is shown as last known; no task figure is shown here.
 */
function StatusCard({ id }: { id: string }) {
  const view = usePrograms((s) => s.runtimes);
  const doc = useRuntimes((s) => s.docs[id]);
  const { command, unload } = useRuntimes.getState();
  const may = mayEngineer();
  const known = view.runtimes !== null && view.error === null;
  const runtime = view.runtimes?.find((r) => r.id === id);
  const busy = doc?.busy !== null && doc?.busy !== undefined;
  const runs = runtime === undefined ? [] : runsOf(runtime);
  const listed = runs.length === 0 ? 'no program it names' : runs.join(', ');
  return (
    <section className="logic-panel" data-testid="runtime-status" aria-label="Runtime">
      <p>
        {known || runtime !== undefined ? (
          <span className={`badge badge--${runtime === undefined ? 'neutral' : (MODE_TONE[runtime.mode] ?? 'neutral')}`} data-testid="runtime-status-mode">
            {runtime === undefined ? 'NOT LOADED' : runtime.mode.toUpperCase()}
          </span>
        ) : (
          <span className="badge badge--neutral" data-testid="runtime-status-mode">
            UNKNOWN
          </span>
        )}{' '}
        {!known && runtime !== undefined && (
          <span className="badge badge--warn" data-testid="runtime-status-stale">
            LAST KNOWN
          </span>
        )}{' '}
        {runtime !== undefined && (
          <span className="mono" data-testid="runtime-status-version">
            {runtime.program} · version {runtime.version}
          </span>
        )}
      </p>
      <p className="muted" data-testid="runtime-status-why">
        {runtime === undefined
          ? known
            ? `The controller lists no runtime ${id}: it is configured, not loaded.`
            : view.runtimes === null
              ? `Whether the controller runs runtime ${id} is not known: ${view.error ?? 'its runtimes have not been read yet'}.`
              : `Whether the controller runs runtime ${id} is not known: its runtimes can no longer be read (${view.error}).`
          : known
            ? `As the controller lists it.`
            : `The controller's runtimes can no longer be read (${view.error}), so this is what was last known, not what is.`}
      </p>
      {runtime?.reason && (
        <p className="form__error" data-testid="runtime-status-reason">
          {runtime.reason}
        </p>
      )}
      {runtime !== undefined && Array.isArray(runtime.programs) && (
        <ul className="logic-runtime__programs" aria-label="Programs it runs">
          {runtime.programs.map((p) => (
            <ProgramVersion key={p.name} name={p.name} version={p.version} here={doc?.versions ?? null} />
          ))}
        </ul>
      )}
      {may && (
        <div className="logic-panel__actions">
          <button
            type="button"
            className="btn"
            disabled={busy || !known || runtime === undefined}
            data-testid="runtime-card-run"
            title="Put the runtime in RUN, every program it runs. From STOP or FAULT that is a warm restart."
            onClick={() => {
              if (
                window.confirm(
                  `Put runtime ${id} in RUN? It runs ${listed}. From STOP or FAULT that is a warm restart: their RETAIN variables keep the values they had, the others take their initial values, and the programs write their outputs.`
                )
              ) {
                void command(id, true);
              }
            }}
          >
            <CirclePlay size={13} aria-hidden /> Run
          </button>
          <button
            type="button"
            className="btn"
            // Stop is the safe direction: neither a request unanswered nor a reading that failed
            // takes it away, and it is not asked about.
            disabled={runtime === undefined}
            data-testid="runtime-card-stop"
            title="Put the runtime in STOP, every program it runs. Their outputs are written their safe values."
            onClick={() => void command(id, false)}
          >
            <CircleStop size={13} aria-hidden /> Stop
          </button>
          <button
            type="button"
            className="btn btn--flat"
            disabled={busy || !known || runtime === undefined}
            data-testid="runtime-card-unload"
            title="Take the runtime out of the project's runtimes. The outputs of its programs are written their safe values."
            onClick={() => {
              if (
                window.confirm(
                  `Take runtime ${id} out of the project's runtimes? It stops running ${listed}, and their outputs go to their safe values. Its configuration in the logic module is kept.`
                )
              ) {
                void unload(id);
              }
            }}
          >
            <Upload size={13} aria-hidden /> Unload
          </button>
        </div>
      )}
      {doc?.commanded && doc.commanded.stopped === null && (
        <p data-testid="runtime-commanded">{doc.commanded.words}</p>
      )}
      {doc?.commanded?.stopped && <StoppedNotice stopped={doc.commanded.stopped} testid="command-refused" />}
    </section>
  );
}

export function RuntimeEditor({ id, onClose }: { id: string; onClose: (id: string) => void }) {
  const doc = useRuntimes((state) => state.docs[id]);
  const projectId = useRuntimes((state) => state.projectId);
  const { open, discard, check, save, remove, readModules, loadRuntime } = useRuntimes.getState();
  const readRuntimes = usePrograms((s) => s.readRuntimes);
  // Deleted or discarded from here: its editor is closing, and the runtime is not read again meanwhile.
  const closing = useRef(false);
  const missing = doc === undefined;

  useEffect(() => {
    if (missing && projectId !== null && !closing.current) void open(id);
  }, [missing, projectId, id, open]);

  // Which runtime modules are installed, for the module field and the kinds each says it runs.
  useEffect(() => {
    if (projectId !== null) void readModules();
  }, [projectId, readModules]);

  // What the controller says of the runtime: read while the editor is shown, and not after.
  useEffect(() => {
    void readRuntimes();
    const timer = setInterval(() => void readRuntimes(), RUNTIMES_POLL_MS);
    return () => clearInterval(timer);
  }, [readRuntimes]);

  const configuredHere = doc !== undefined && doc.phase === 'ready';
  useEffect(() => {
    if (!configuredHere) return undefined;
    host().setTaskCard(RUNTIME_STATUS_OWNER, { title: 'Runtime', mount: (card) => draw(card, <StatusCard id={id} />) });
    return () => host().setTaskCard(RUNTIME_STATUS_OWNER, null);
  }, [configuredHere, id]);

  const mount = useMemo<DiagnosticsPanel['mount']>(() => (panel) => draw(panel, <CheckCard id={id} />), [id]);
  const count = problemCount(doc);
  const countRef = useRef(count);
  countRef.current = count;
  useEffect(() => {
    host().setDiagnostics(RUNTIME_DIAGNOSTICS_OWNER, { title: `Check: ${id}`, mount, count: countRef.current });
    return () => host().setDiagnostics(RUNTIME_DIAGNOSTICS_OWNER, null);
  }, [id, mount]);
  useEffect(() => {
    host().setDiagnostics(RUNTIME_DIAGNOSTICS_OWNER, { title: `Check: ${id}`, mount, count });
  }, [id, mount, count]);

  if (!doc && closing.current) return null;
  if (!doc || doc.phase === 'reading') {
    return <p className="editor__loading muted">Reading runtime {id}…</p>;
  }
  if (doc.phase === 'unreadable') {
    return (
      <div className="notice notice--bad" role="alert" data-testid="runtime-read-error">
        <div>
          <h3>The runtime {id} could not be read</h3>
          <p>{doc.readError?.error}</p>
          {doc.readError?.withheld && <p data-testid="read-error-withheld">{WITHHELD}</p>}
        </div>
      </div>
    );
  }
  if (doc.phase === 'implied') {
    return (
      <div className="notice" data-testid="runtime-implied">
        <p>{id} is a program running alone, not a configured runtime.</p>
      </div>
    );
  }

  const may = mayEngineer();
  const dirty = hasChanges(doc);
  const working = doc.busy !== null;
  const refusal = doc.refusal;
  const lacking = missingOf(doc.draft);
  const complete = lacking.length === 0;

  return (
    <div className="editor__column" data-testid="runtime-editor">
      <div className="editor__tools">
        {may && (
          <button
            type="button"
            className="btn btn--primary"
            disabled={!dirty || working || !complete}
            onClick={() => void save(id)}
            data-testid="runtime-save"
            title="Save the runtime into the project's configuration. Nothing is built or loaded."
          >
            <Save size={13} aria-hidden /> Save
          </button>
        )}
        {may && (
          <button
            type="button"
            className="btn"
            disabled={!dirty || working}
            data-testid="runtime-discard"
            onClick={() => {
              if (!discard(id)) return;
              closing.current = true;
              onClose(id);
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
            data-testid="runtime-delete"
            onClick={() => {
              if (!window.confirm(deleteRuntimeQuestion(id))) return;
              void remove(id).then((gone) => {
                if (!gone) return;
                closing.current = true;
                onClose(id);
                useRuntimes.getState().forget(id);
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
          disabled={working || !complete}
          data-testid="runtime-check"
          onClick={() => void check(id)}
        >
          <Check size={13} aria-hidden /> Check
        </button>
        {may && (
          <button
            type="button"
            className="btn"
            disabled={working || !complete}
            data-testid="runtime-load"
            title="Load saves the draft first if it has changes, then checks it, builds it and, after asking, puts it into the project's runtimes, taking out or rebuilding what holds its programs."
            onClick={() => void loadRuntime(id, false)}
          >
            <Download size={13} aria-hidden /> Load
          </button>
        )}
        {may && (
          <button
            type="button"
            className="btn"
            disabled={working || !complete}
            data-testid="runtime-load-cold"
            title="Load cold saves the draft first if it has changes, and loads with every variable from its initial value, RETAIN ones included."
            onClick={() => {
              if (window.confirm(loadColdQuestion(id))) void loadRuntime(id, true);
            }}
          >
            <Download size={13} aria-hidden /> Load cold
          </button>
        )}
        <CheckState doc={doc} />
        {doc.isNew && (
          <span className="badge badge--warn" data-testid="runtime-new">
            NOT SAVED YET
          </span>
        )}
        {dirty && !doc.isNew && (
          <span className="badge badge--warn" data-testid="runtime-unsaved">
            UNSAVED
          </span>
        )}
        {!may && (
          <span className="muted" data-testid="runtime-read-only">
            Only an engineer changes runtimes.
          </span>
        )}
      </div>
      <p className="logic-note" data-testid="runtime-configured">
        Runtime {id} as configured in the project's logic module. Whether the controller runs it is on the Runtime card, as the
        controller lists it.
      </p>
      {!complete && (
        <div className="logic-note logic-note--warn" data-testid="runtime-missing">
          <p>Not checked or saved until these are given:</p>
          <ul>
            {lacking.map((field) => (
              <li key={field} data-testid="runtime-missing-field">
                {field}
              </li>
            ))}
          </ul>
        </div>
      )}
      {doc.readBack !== null && (
        <div className="notice notice--warn" role="alert" data-testid="runtime-read-back">
          <p>Saved; the stored runtime could not be read back: {doc.readBack}. The form shows what the save answered.</p>
        </div>
      )}
      {doc.elsewhere !== null && (
        <div className="notice notice--warn" role="alert" data-testid="runtime-elsewhere">
          <p>
            {doc.elsewhereAs !== null
              ? `A runtime ${doc.elsewhereAs} was ${doc.elsewhere === 'deleted' ? 'deleted' : 'saved'} elsewhere since ${id} was opened.`
              : doc.elsewhere === 'deleted'
                ? `${id} was deleted elsewhere since it was opened; saving makes it again.`
                : `${id} was changed elsewhere since it was opened; saving replaces that change.`}
          </p>
        </div>
      )}
      {refusal && (
        <div className="notice notice--bad" role="alert" data-testid={refusal.of === 'save' ? 'save-refused' : 'delete-refused'}>
          <p>{refusal.refusal.error}</p>
          {refusal.refusal.withheld && <p data-testid="refusal-withheld">{WITHHELD}</p>}
        </div>
      )}
      {doc.saveAnswer && (
        <div className="notice" data-testid="runtime-saved">
          <p>{doc.saveAnswer.created ? 'Saved; the module made the runtime. Nothing was built or loaded.' : 'Saved. Nothing was built or loaded.'}</p>
        </div>
      )}
      {doc.loadStopped !== null && <StoppedNotice stopped={doc.loadStopped} testid="load-refused" />}
      {doc.loaded !== null && <LoadResult loaded={doc.loaded} />}
      {doc.checkRefusal !== null && (
        <div className="notice notice--bad" role="alert" data-testid="check-refused">
          <p>{doc.checkRefusal.error}</p>
          {doc.checkRefusal.withheld && <p data-testid="check-refused-withheld">{WITHHELD}</p>}
        </div>
      )}
      <RuntimeForm doc={doc} readOnly={!may || working} />
    </div>
  );
}
