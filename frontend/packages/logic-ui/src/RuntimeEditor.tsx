// The editor of one configured runtime of the project (ADR-013 §2.19, C8d-1): which runtime module
// runs it, the target its programs are compiled for, whether it runs when loaded, and its tasks,
// each with its kind, priority, watchdog and programs in order. Checked and saved through the logic
// module, which answers every rule in its own words; what is shown as saved is what it read back.
// Nothing here says whether the controller runs the runtime: it is configured, nothing more.

import { useEffect, useMemo, useRef } from 'react';
import { ArrowDown, ArrowUp, Check, Plus, Save, Trash2, Undo2 } from 'lucide-react';
import type { DiagnosticsPanel } from '@mythos/workbench-sdk';
import { host, mayEngineer, type Diagnostic } from './bridge';
import { draw } from './draw';
import { WITHHELD } from './library';
import { usePrograms } from './programs';
import {
  deleteRuntimeQuestion,
  heldBy,
  kindOf,
  kindsOf,
  missingOf,
  newTask,
  PROGRAM_EDITOR,
  TASK_KINDS,
  taskLabel,
  type ProgramCheck,
  type RuntimeEntry,
  type TaskConfig,
  type TaskKind,
} from './runtimes';
import { draftKey, hasChanges, useRuntimes, type RuntimeDoc } from './runtimestore';

/** The owner of this editor's panel in the Diagnostics tab. */
export const RUNTIME_DIAGNOSTICS_OWNER = 'logic-runtime-check';

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

/** A number typed into a field: `null` while nothing (or nothing that is a number) is typed. */
function numberOf(text: string): number | null {
  if (text.trim() === '') return null;
  const n = Number(text);
  return Number.isNaN(n) ? null : n;
}
const shown = (n: number | null | undefined): string => (n === null || n === undefined ? '' : String(n));

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
              <input
                type="number"
                aria-label="Interval in milliseconds"
                data-testid="task-interval"
                value={shown(kind.interval_ms)}
                disabled={readOnly}
                onChange={(event) => setKind({ interval_ms: numberOf(event.target.value) })}
              />
            </label>
            <label className="field">
              <span className="field__label">Phase (ms)</span>
              <input
                type="number"
                aria-label="Phase in milliseconds"
                data-testid="task-phase"
                value={shown(kind.phase_ms)}
                disabled={readOnly}
                onChange={(event) => setKind({ phase_ms: numberOf(event.target.value) })}
              />
              <span className="field__hint">Empty: 0, the module's default.</span>
            </label>
          </>
        )}
        {kind?.type === 'cycle' && (
          <label className="field">
            <span className="field__label">Shortest cycle (ms)</span>
            <input
              type="number"
              aria-label="Shortest cycle in milliseconds"
              data-testid="task-min-cycle"
              value={shown(kind.min_cycle_ms)}
              disabled={readOnly}
              onChange={(event) => setKind({ min_cycle_ms: numberOf(event.target.value) })}
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
          <input
            type="number"
            aria-label="Task priority"
            data-testid="task-priority"
            value={shown(task.priority)}
            disabled={readOnly}
            onChange={(event) => onChange({ ...task, priority: numberOf(event.target.value) })}
          />
          <span className="field__hint">0 is the highest.</span>
        </label>
        <label className="field">
          <span className="field__label">Watchdog (ms)</span>
          <input
            type="number"
            aria-label="Watchdog in milliseconds"
            data-testid="task-watchdog"
            value={shown(task.watchdog_ms)}
            disabled={readOnly}
            onChange={(event) => onChange({ ...task, watchdog_ms: numberOf(event.target.value) })}
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

export function RuntimeEditor({ id, onClose }: { id: string; onClose: (id: string) => void }) {
  const doc = useRuntimes((state) => state.docs[id]);
  const projectId = useRuntimes((state) => state.projectId);
  const { open, discard, check, save, remove, readModules } = useRuntimes.getState();
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
        Runtime {id} as configured in the project's logic module. Whether the controller runs it is not shown here.
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
      {doc.checkRefusal !== null && (
        <div className="notice notice--bad" role="alert" data-testid="check-refused">
          <p>{doc.checkRefusal.error}</p>
        </div>
      )}
      <RuntimeForm doc={doc} readOnly={!may || working} />
    </div>
  );
}
