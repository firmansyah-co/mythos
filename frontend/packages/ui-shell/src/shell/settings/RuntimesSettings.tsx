import { useCallback, useEffect, useRef, useState } from 'react';
import { Play, Square } from 'lucide-react';
import {
  api,
  hasRole,
  type RuntimeCommandAnswer,
  type RuntimeList,
  type RuntimeModule,
  type RuntimeOutput,
  type RuntimeStatus,
} from '../../api/client';
import { useSession } from '../../data/session';
import { describeError } from './format';

/** How often the list is read while the page is shown. */
export const RUNTIMES_EVERY_MS = 1000;
/**
 * An answer older than this is not the current state, whatever the reason no newer one came: a
 * read that failed, or one that was never answered.
 */
export const RUNTIMES_STALE_AFTER_MS = 3000;
/** A read not answered for this long is given up, so that the next one can be made. */
export const RUNTIMES_READ_GIVEN_UP_AFTER_MS = 5000;

// What stands where the controller sent no value. Never a number, a name or a mode of our own.
const UNKNOWN = 'UNKNOWN';
const NOT_MEASURED = 'NOT MEASURED';

const isNumber = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);
const isText = (value: unknown): value is string => typeof value === 'string' && value !== '';

/** A value as the controller sent it. Only called with a value that was sent. */
function shown(value: unknown): string {
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  if (Array.isArray(value)) return value.length === 0 ? 'none' : value.map(shown).join(', ');
  return JSON.stringify(value);
}

const sent = (value: unknown) => value !== null && value !== undefined;

function Missing({ word = UNKNOWN, testId }: { word?: string; testId?: string }) {
  return (
    <span className="muted settings__whole" data-missing="true" data-testid={testId}>
      {word}
    </span>
  );
}

/** A name the controller sent, or UNKNOWN. */
function Name({ value, testId, mono = false }: { value: unknown; testId: string; mono?: boolean }) {
  if (!isText(value)) return <Missing testId={testId} />;
  return (
    <span className={mono ? 'mono' : undefined} data-testid={testId}>
      {value}
    </span>
  );
}

/** The badge class of a mode. A mode this page does not know is never shown as a healthy one. */
function modeClass(mode: unknown): string {
  if (mode === 'run') return 'badge--good';
  if (mode === 'stop') return 'badge--neutral';
  if (mode === 'fault' || mode === 'unreachable') return 'badge--bad';
  return 'badge--warn';
}

/** The mode exactly as reported, and why, when the controller said why. */
function Mode({ runtime, stale }: { runtime: RuntimeStatus; stale: boolean }) {
  const mode: unknown = runtime.mode;
  const reason: unknown = runtime.reason;
  return (
    <>
      {/* A colour says what a runtime is doing now. A last known state makes no such claim. */}
      <span
        className={`badge ${stale ? 'badge--neutral' : modeClass(mode)}`}
        data-testid="runtime-mode"
        data-mode={isText(mode) ? mode : 'unknown'}
      >
        {isText(mode) ? mode : UNKNOWN}
      </span>
      {isText(reason) ? (
        <div className="settings__said" data-testid="runtime-reason">
          {reason}
        </div>
      ) : (
        // A program that runs needs no reason, and a stop need not have one: the controller gave
        // none, and that is what is said. A fault, a module that does not answer, or a mode this
        // page does not know has a cause, and it is not known here.
        mode !== 'run' && (
          <div className="muted" data-testid="runtime-reason" data-missing="true">
            {mode === 'stop' ? 'no reason given' : `reason ${UNKNOWN}`}
          </div>
        )
      )}
    </>
  );
}

/**
 * The scan figures. The controller sends zeros when it holds no state of the program (its module
 * does not answer, or no exchange happened yet), so a figure is shown only when it was measured:
 * nothing while `unreachable`, and no scan time or overrun count before the first scan.
 */
function Scan({ runtime }: { runtime: RuntimeStatus }) {
  const answered = runtime.mode !== 'unreachable';
  const scans: unknown = runtime.scans;
  const scanned = answered && isNumber(scans) && scans > 0;
  const micros = (value: unknown, testId: string) =>
    scanned && isNumber(value) ? (
      <span className="settings__whole" data-testid={testId}>
        <span className="mono">{value}</span> µs
      </span>
    ) : (
      <Missing word={NOT_MEASURED} testId={testId} />
    );
  return (
    <>
      <div>
        every{' '}
        {isNumber(runtime.interval_ms) ? (
          <span className="settings__whole" data-testid="runtime-interval">
            <span className="mono">{runtime.interval_ms}</span> ms
          </span>
        ) : (
          <Missing testId="runtime-interval" />
        )}
      </div>
      <div className="muted">
        scans{' '}
        {answered && isNumber(scans) ? (
          <span className="mono" data-testid="runtime-scans">
            {scans}
          </span>
        ) : (
          <Missing word={NOT_MEASURED} testId="runtime-scans" />
        )}
        {' · '}overruns{' '}
        {scanned && isNumber(runtime.overruns) ? (
          <span className="mono" data-testid="runtime-overruns">
            {runtime.overruns}
          </span>
        ) : (
          <Missing word={NOT_MEASURED} testId="runtime-overruns" />
        )}
      </div>
      <div className="muted">
        last {micros(runtime.last_scan_us, 'runtime-last-scan')}
        {' · '}max {micros(runtime.max_scan_us, 'runtime-max-scan')}
      </div>
    </>
  );
}

/** Whether the outputs are at their safe values, as the controller says, and only that. */
function OutputsSafe({ runtime, stale }: { runtime: RuntimeStatus; stale: boolean }) {
  const safe: unknown = runtime.outputs_safe;
  if (typeof safe !== 'boolean') return <Missing testId="runtime-outputs-safe" />;
  if (safe) {
    return (
      <span className="badge badge--neutral settings__whole" data-testid="runtime-outputs-safe" data-safe="true">
        AT SAFE VALUES
      </span>
    );
  }
  // Not at their safe values is what a running program is for. For any other mode it means the
  // device has not confirmed them, which is what an operator must see.
  if (runtime.mode === 'run') {
    return (
      <span className="muted" data-testid="runtime-outputs-safe" data-safe="false">
        written by the program
      </span>
    );
  }
  return (
    <span className={`badge ${stale ? 'badge--neutral' : 'badge--bad'}`} data-testid="runtime-outputs-safe" data-safe="false">
      NOT CONFIRMED SAFE
    </span>
  );
}

function Output({ output }: { output: RuntimeOutput }) {
  return (
    <div data-testid="runtime-output">
      <Name value={output.tag} testId="runtime-output-tag" mono />{' '}
      <span className="muted">
        from <Name value={output.variable} testId="runtime-output-variable" mono />
      </span>
      <div className="muted">
        safe value{' '}
        {sent(output.safe) ? (
          <span className="mono" data-testid="runtime-output-safe">
            {shown(output.safe)}
          </span>
        ) : (
          <Missing testId="runtime-output-safe" />
        )}
        {' · '}last confirmed{' '}
        {sent(output.written) ? (
          <span className="mono" data-testid="runtime-output-written">
            {shown(output.written)}
          </span>
        ) : (
          // Nothing the device confirmed: not the safe value, not zero, not the last request.
          <>
            <span data-testid="runtime-output-written" data-missing="true">
              {UNKNOWN}
            </span>
            {output.written === null && ' (the device has confirmed no write yet)'}
          </>
        )}
      </div>
      {isText(output.error) && (
        <div className="settings__said" data-testid="runtime-output-error">
          <span className="tagvalue__flag">NOT CONFIRMED</span> {output.error}
        </div>
      )}
    </div>
  );
}

function Outputs({ runtime, stale }: { runtime: RuntimeStatus; stale: boolean }) {
  const outputs: unknown = runtime.outputs;
  if (!Array.isArray(outputs)) return <Missing testId="runtime-outputs" />;
  if (outputs.length === 0) {
    return (
      <span className="muted" data-testid="runtime-outputs">
        writes no field tag
      </span>
    );
  }
  return (
    <div data-testid="runtime-outputs">
      <OutputsSafe runtime={runtime} stale={stale} />
      {(outputs as unknown[]).map((output, index) =>
        // An entry that is not an output is not skipped: something is there, and it is not known.
        typeof output === 'object' && output !== null ? (
          <Output key={index} output={output as RuntimeOutput} />
        ) : (
          <div key={index} data-testid="runtime-output">
            <Missing />
          </div>
        )
      )}
    </div>
  );
}

/** A module's profile: data the module defines, shown as it declared it. */
function Profile({ profile }: { profile: unknown }) {
  if (!sent(profile)) {
    return (
      <span data-testid="runtime-module-profile" data-missing="true">
        <span className="muted">{UNKNOWN}</span> <span className="muted">(the module declares no profile)</span>
      </span>
    );
  }
  if (typeof profile !== 'object' || Array.isArray(profile)) {
    return (
      <span className="mono" data-testid="runtime-module-profile">
        {shown(profile)}
      </span>
    );
  }
  return (
    <div data-testid="runtime-module-profile">
      {Object.entries(profile as Record<string, unknown>).map(([key, value]) => (
        <div key={key}>
          <span className="muted mono">{key}</span> <span className="mono">{sent(value) ? shown(value) : UNKNOWN}</span>
        </div>
      ))}
    </div>
  );
}

function Modules({ modules }: { modules: RuntimeModule[] }) {
  return (
    <section className="settings__block" aria-label="Runtime modules">
      <h3>Runtime modules</h3>
      {modules.length === 0 ? (
        <p className="muted" data-testid="runtime-modules-empty">
          No runtime module is installed.
        </p>
      ) : (
        <table className="grid settings__users settings__tall" aria-label="Runtime modules">
          <thead>
            <tr>
              <th>Module</th>
              <th>Runs</th>
              <th>Version</th>
              <th>Profile</th>
            </tr>
          </thead>
          <tbody>
            {modules.map((module, index) => (
              <tr key={isText(module.id) ? module.id : index} data-testid="runtime-module">
                <td>
                  <strong>
                    <Name value={module.name} testId="runtime-module-name" />
                  </strong>
                  <div className="muted">
                    <Name value={module.id} testId="runtime-module-id" mono />
                  </div>
                </td>
                <td>
                  <Name value={module.provides} testId="runtime-module-provides" mono />
                </td>
                <td>
                  <Name value={module.version} testId="runtime-module-version" mono />
                </td>
                <td>
                  <Profile profile={module.profile} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}

/** The last answer of the controller, and when it arrived. */
interface Reading {
  list: RuntimeList;
  atMs: number;
}

type Verb = 'run' | 'stop';
type Commanded = { id: string; verb: Verb } & ({ answer: RuntimeCommandAnswer } | { error: string });

/**
 * Every runtime of this node (ADR-013 §2.5): the program each executes, what it is doing, its scan
 * figures and its outputs; run and stop for an engineer; and the runtime modules that could run one.
 *
 * The list is read about once a second. What is shown is the controller's last answer and nothing
 * else: a value it did not send is UNKNOWN or NOT MEASURED, and an answer that is no longer fresh
 * (the read failed, or was never answered) is marked as the last known state with its age.
 */
export function RuntimesSettings() {
  const user = useSession((state) => state.user);
  // Without accounts (a server built without them) everything is open. The controller enforces
  // the same on its routes; this only saves a refused request.
  const mayCommand = !user || hasRole(user.role, 'engineer');

  const [reading, setReading] = useState<Reading | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [asking, setAsking] = useState<{ id: string; verb: Verb } | null>(null);
  const [commanded, setCommanded] = useState<Commanded | null>(null);

  const alive = useRef(false);
  const pending = useRef<{ controller: AbortController; startedMs: number } | null>(null);
  const issued = useRef(0);
  const applied = useRef(0);
  const commands = useRef(0);

  const read = useCallback(async () => {
    const sequence = ++issued.current;
    const controller = new AbortController();
    pending.current = { controller, startedMs: Date.now() };
    try {
      const list = await api.listRuntimes(controller.signal);
      // An answer that arrives after a later one is not the newer state.
      if (!alive.current || sequence < applied.current) return;
      applied.current = sequence;
      setReading({ list, atMs: Date.now() });
      setReadError(null);
    } catch (cause) {
      if (!alive.current || sequence < applied.current) return;
      applied.current = sequence;
      // The last answer stays, to be shown as the last known state: it is not replaced, and it
      // is not presented as current.
      setReadError(describeError(cause));
    } finally {
      if (pending.current?.controller === controller) pending.current = null;
      if (alive.current) setNowMs(Date.now());
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void read();
    const timer = setInterval(() => {
      const now = Date.now();
      // The clock moves whether or not the controller answers: an answer ages by itself.
      setNowMs(now);
      const waiting = pending.current;
      if (waiting) {
        if (now - waiting.startedMs < RUNTIMES_READ_GIVEN_UP_AFTER_MS) return;
        waiting.controller.abort(
          new Error(`no answer within ${Math.round(RUNTIMES_READ_GIVEN_UP_AFTER_MS / 1000)} s`)
        );
      }
      void read();
    }, RUNTIMES_EVERY_MS);
    return () => {
      alive.current = false;
      clearInterval(timer);
      pending.current?.controller.abort();
      pending.current = null;
    };
  }, [read]);

  const command = async (id: string, verb: Verb) => {
    const sequence = ++commands.current;
    setAsking({ id, verb });
    setCommanded(null);
    let result: Commanded;
    try {
      result = { id, verb, answer: await (verb === 'run' ? api.runRuntime(id) : api.stopRuntime(id)) };
    } catch (cause) {
      result = { id, verb, error: describeError(cause) };
    }
    if (!alive.current) return;
    setCommanded(result);
    // A later command may still be waiting for its answer.
    if (sequence === commands.current) setAsking(null);
    // What the runtime does now is the controller's to say, not the command's.
    await read();
  };

  const ageMs = reading ? Math.max(0, nowMs - reading.atMs) : 0;
  const stale = reading !== null && (readError !== null || ageMs > RUNTIMES_STALE_AFTER_MS);
  const age = `${Math.floor(ageMs / 1000)} s`;
  const list = reading?.list ?? null;
  const problem: unknown = list?.problem;

  return (
    <>
      <div className="settings__heading">
        <h2>Runtimes</h2>
      </div>
      <p className="muted">
        The control programs this node executes, each in a runtime module process. Whenever a program is not running
        (stopped, faulted, its module not answering) the controller writes every output its safe value until the
        device confirms it. This is not a safety system: if the controller itself stops, nothing writes anything.
      </p>
      {!mayCommand && (
        <p className="muted" data-testid="runtimes-who-may">
          Only an engineer or an administrator runs or stops a program.
        </p>
      )}

      {readError !== null && (
        <p className="form__error" role="alert" data-testid="runtimes-read-error">
          The runtimes could not be read: {readError}
        </p>
      )}
      {stale && (
        <p className="form__error" role="alert" data-testid="runtimes-stale">
          What is shown below is the last known state, read {age} ago. It is not what the runtimes are doing now.
        </p>
      )}

      {asking && (
        <p className="muted" data-testid="runtime-command-asking">
          Asked the controller to {asking.verb} <span className="mono">{asking.id}</span>; it has not answered yet.
        </p>
      )}
      {commanded && 'answer' in commanded && (
        <div className="notice" data-testid="runtime-command-answer">
          <p>
            The controller answered for <span className="mono">{isText(commanded.answer?.id) ? commanded.answer.id : commanded.id}</span>
            : asked <strong>{isText(commanded.answer?.asked) ? commanded.answer.asked : UNKNOWN}</strong>
            {isText(commanded.answer?.note) && <>; {commanded.answer.note}</>}
          </p>
        </div>
      )}
      {commanded && 'error' in commanded && (
        <p className="form__error" role="alert" data-testid="runtime-command-error">
          {commanded.verb === 'run' ? 'Run' : 'Stop'} was not taken for <span className="mono">{commanded.id}</span>:{' '}
          {commanded.error}
        </p>
      )}

      {list === null ? (
        readError === null && <p className="muted">Reading the runtimes…</p>
      ) : (
        <div data-testid="runtimes" data-stale={stale ? 'true' : 'false'}>
          {isText(problem) && (
            <div className="notice notice--bad" data-testid="runtimes-problem">
              <p>The runtimes are not as the project configures them: {problem}</p>
            </div>
          )}
          {list.runtimes.length === 0 ? (
            list.modules.length === 0 ? (
              <p className="muted" data-testid="runtimes-no-module">
                {stale ? 'When last read, no runtime was loaded and' : 'No runtime is loaded, and'} no runtime module
                {stale ? ' was' : ' is'} installed, so this node can run no program.
              </p>
            ) : (
              <p className="muted" data-testid="runtimes-empty">
                {stale ? 'When last read, no runtime was loaded.' : 'No runtime is loaded.'} A program can be loaded
                into: <span className="mono">{list.modules.map((m) => (isText(m.provides) ? m.provides : UNKNOWN)).join(', ')}</span>{' '}
                (a project's <span className="mono">runtimes.json</span>, over the API).
              </p>
            )
          ) : (
            <table className="grid settings__users settings__tall" aria-label="Runtimes">
              <thead>
                <tr>
                  <th>Runtime</th>
                  <th>Program</th>
                  <th>State</th>
                  <th>Scan</th>
                  <th>Outputs</th>
                  <th>Command</th>
                </tr>
              </thead>
              <tbody>
                {list.runtimes.map((runtime, index) => {
                  const id: unknown = runtime.id;
                  const named = isText(id);
                  return (
                    <tr key={`${index}:${named ? id : ''}`} data-testid="runtime-row" data-stale={stale ? 'true' : 'false'}>
                      <td>
                        <strong>
                          <Name value={id} testId="runtime-id" mono />
                        </strong>
                        {stale && (
                          <span className="tagvalue__flag" data-testid="runtime-stale">
                            LAST KNOWN · {age} ago
                          </span>
                        )}
                        <div className="muted">
                          in <Name value={runtime.module} testId="runtime-module-of" mono />
                        </div>
                        <div className="muted">
                          process{' '}
                          {isNumber(runtime.pid) ? (
                            <span className="mono" data-testid="runtime-pid">
                              {runtime.pid}
                            </span>
                          ) : runtime.pid === null ? (
                            // The controller says its module has no process now.
                            <span data-testid="runtime-pid">no process</span>
                          ) : (
                            <Missing testId="runtime-pid" />
                          )}
                          {' · '}restarts{' '}
                          {isNumber(runtime.restarts) ? (
                            <span className="mono" data-testid="runtime-restarts">
                              {runtime.restarts}
                            </span>
                          ) : (
                            <Missing testId="runtime-restarts" />
                          )}
                        </div>
                      </td>
                      <td>
                        <Name value={runtime.program} testId="runtime-program" />
                        <div className="muted">
                          version <Name value={runtime.version} testId="runtime-version" mono />
                        </div>
                      </td>
                      <td>
                        <Mode runtime={runtime} stale={stale} />
                      </td>
                      <td>
                        <Scan runtime={runtime} />
                      </td>
                      <td>
                        <Outputs runtime={runtime} stale={stale} />
                      </td>
                      <td>
                        {/* A runtime the controller did not name cannot be commanded by name. Stop is
                            never held back by a command still unanswered: it is the safe direction. */}
                        <div className="settings__row-actions settings__whole">
                          <button
                            type="button"
                            className="btn"
                            disabled={!mayCommand || !named || asking?.id === id}
                            aria-label={`Run ${named ? id : UNKNOWN}`}
                            title="Put the program in RUN. From STOP or FAULT that is a cold start: its variables take their initial values."
                            onClick={() => {
                              if (
                                named &&
                                window.confirm(
                                  `Put ${id} in RUN? From STOP or FAULT that is a cold start: its variables take their initial values, and the program writes its outputs.`
                                )
                              ) {
                                void command(id, 'run');
                              }
                            }}
                          >
                            <Play size={13} aria-hidden /> Run
                          </button>
                          <button
                            type="button"
                            className="btn"
                            disabled={!mayCommand || !named}
                            aria-label={`Stop ${named ? id : UNKNOWN}`}
                            title="Put the program in STOP. Its outputs are written their safe values."
                            onClick={() => named && void command(id, 'stop')}
                          >
                            <Square size={13} aria-hidden /> Stop
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
          <Modules modules={list.modules} />
        </div>
      )}
    </>
  );
}
