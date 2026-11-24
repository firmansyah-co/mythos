// The logic UI's view of the workbench, through the `WorkbenchHost` it was activated with
// (ADR-011 §2.3), and the routes it calls: the logic module's own (ADR-013 §2.6) and the
// core's runtimes (§2.5). A module cannot call the core, so it is this UI that takes what the
// logic module built and saves it into the project's runtimes.

import { create } from 'zustand';
import type {
  ControllerView,
  Dispose,
  Project,
  TelemetrySnapshot,
  WorkbenchHost,
} from '@mythos/workbench-sdk';

let current: WorkbenchHost | null = null;

/** The host the module was activated with. */
export function host(): WorkbenchHost {
  if (!current) throw new Error('the logic UI is not activated');
  return current;
}

const EMPTY_TELEMETRY: TelemetrySnapshot = { tags: new Map(), stream: 'IDLE', lastBatchAtMs: null };
const EMPTY_CONTROLLER: ControllerView = {
  activeProjectId: undefined,
  io: null,
  error: null,
  configuredTags: [],
  forcedTags: null,
};

/** The live tag cache and the stream's state, as the workbench holds them. */
export const useTelemetry = create<TelemetrySnapshot>(() => EMPTY_TELEMETRY);
/** The controller, as the workbench polls it. */
export const useController = create<ControllerView>(() => EMPTY_CONTROLLER);
/** The open project. */
export const useProject = create<{ project: Project | null }>(() => ({ project: null }));

export const log = {
  info: (text: string) => current?.log.info(text),
  error: (text: string) => current?.log.error(text),
};

/** Whether the signed-in user may change programs and runtimes. Without accounts, anyone may. */
export function mayEngineer(): boolean {
  const user = current?.user() ?? null;
  return user === null || user.role === 'engineer' || user.role === 'admin';
}

// ---------------------------------------------------------------------------------------------
// What the controller sends
// ---------------------------------------------------------------------------------------------

export interface InputBinding {
  variable: string;
  tag: string;
  /** `value` (the default), or `good`: a BOOL that says whether the tag is current. */
  of?: 'value' | 'good';
}

export interface OutputBinding {
  variable: string;
  tag: string;
  /**
   * What the tag is written whenever the program is not running. There is no default: `null`
   * is a binding whose safe value has not been given yet, which the module refuses to save.
   */
  safe: number | boolean | null;
}

export interface PublishedBinding {
  variable: string;
  tag: string;
  writable?: boolean;
}

/**
 * The languages a program is written in: Structured Text, or a Ladder Diagram, a Function
 * Block Diagram or a Sequential Function Chart, each stored as a PLCopen TC6 XML document.
 */
export type Language = 'st' | 'ld' | 'fbd' | 'sfc';

/** A program's settings, as the logic module stores them. */
export interface ProgramEntry {
  name: string;
  /** `st`, `ld`, `fbd` or `sfc`; set when the program is created. */
  language: string;
  interval_ms: number;
  runtime: string;
  target: string;
  run: boolean;
  inputs: InputBinding[];
  outputs: OutputBinding[];
  published: PublishedBinding[];
}

/**
 * A message of the compiler and the stretch of the text it is about: from `line`:`col` up to,
 * not including, `end_line`:`end_col`, all counted from 1, a column in characters. The end
 * equals the start when the message is about a point; all four are 0 when it is about the
 * whole program.
 */
export interface Diagnostic {
  line: number;
  col: number;
  end_line: number;
  end_col: number;
  message: string;
  /**
   * In a Ladder Diagram, the element it is about (ADR-013 §2.15): its POU, its `localId` and
   * position as the document gives them, or the interface's variable, and the characters of
   * its text the message is about. Absent for Structured Text.
   */
  element?: DiagnosticElement | null;
  /**
   * The source it is in, when a program is compiled from more than one: the program's own file
   * by its name (`Tank.st`), any other source (the project's library, its user types) by its path
   * under the project (`logic/library/Motors.st`, `data/types/Shared.st`). See
   * {@link isOfThisProgram}.
   */
  source?: string | null;
}

/**
 * Whether a diagnostic is about the program's own text, so its line and column, or its element,
 * are places in that program: true when it names no source, or a source with no `/`. The logic
 * module names the program's own source by its file name, which has no `/`, and every other
 * source it compiles the program with (the library, the project's types) by its path under the
 * project, which always has one. A diagnostic of another source must not be drawn on the
 * program: its line 3 is line 3 of that other file.
 */
export function isOfThisProgram(diagnostic: Diagnostic): boolean {
  const source = diagnostic.source;
  return source === undefined || source === null || source === '' || !source.includes('/');
}

export interface DiagnosticElement {
  pou: string;
  local_id?: number | null;
  x?: number | null;
  y?: number | null;
  variable?: string | null;
  /** A Sequential Function Chart's action or transition of the POU, by its name (ADR-013 §2.17). */
  action?: string | null;
  transition?: string | null;
  /**
   * The action of an action block or the transition, by its `localId`, whose inline body in LD
   * or FBD the message is about; `local_id` is then an element of that body, not of the chart.
   */
  inline?: number | null;
  /** `line`: the line of a text of several (an action's body), from 1. */
  text?: { field: string; col: number; end_col: number; line?: number | null } | null;
}

export interface Variable {
  path: string;
  type: string;
  /** `input`, `output`, `local`, … as the compiler names the section. */
  section: string;
  bindable: boolean;
  /** Made by the diagram front-end for itself (`__LD_…`, `__FBD_…`, `__EDGE_…`), not declared. */
  hidden?: boolean;
}

/**
 * Where the power of one point of a Ladder Diagram is kept (ADR-013 §2.15): of an output of an
 * element (`output`), of a junction of links (`input`), each a variable assigned when it is
 * evaluated in the scan, or ON whenever its rung runs; or whether a rung ran in the last scan
 * (`network`). A point with none carries no power.
 */
export interface PowerProbe {
  local_id: number;
  /** An output: a block's formal parameter in upper case; empty for the one output of any other element. */
  output?: string;
  /** An input where links join: a block's formal parameter in upper case; empty for the one input of any other element. */
  input?: string;
  /** A rung: its elements by `localId`, in the order they run. */
  network?: number[];
  on?: boolean;
  variable?: string;
  negated?: boolean;
}

/**
 * Where the value of one output of a Function Block Diagram is kept (ADR-013 §2.16), in the shape
 * of a {@link PowerProbe}: the hidden variable assigned when the element is evaluated in the scan,
 * of the output's own type (or `on`: an ENO always TRUE); `ran`, where given, the BOOL that is TRUE
 * where the value was computed in the scan, which is not this scan's where it is FALSE. A network's
 * flag as in LD. A line from an output with none carries a literal, written on its element.
 */
export interface ValueProbe extends PowerProbe {
  ran?: string;
}

/** The answer of a check: the diagnostics, or what compiled. */
export interface CheckResult {
  ok: boolean;
  diagnostics: Diagnostic[];
  /** The PROGRAM's name. */
  program?: string;
  /** The version a build of the checked text with the saved settings would have. */
  version?: string;
  variables?: Variable[];
  code_size?: number;
  memory_size?: number;
  binding_problems?: string[];
  /** A Ladder Diagram's: where the power of each output, junction and rung is kept. */
  power?: PowerProbe[];
  /** A Function Block Diagram's: where the value of each output and each network's flag is kept. */
  values?: ValueProbe[];
  /**
   * A Sequential Function Chart's: where each step's flag and time, each transition's condition
   * and each action's state is kept (ADR-013 §2.17), which the chart's view draws (L3d-2).
   */
  chart?: ChartProbe[];
  /**
   * A chart's actions and conditions written in LD or FBD (ADR-013 §2.17, L3d-1b): each entry in
   * LD's or FBD's shape with the body it is in. The chart's view names their language and does
   * not draw them yet.
   */
  bodies?: Array<ValueProbe & { body: { action?: string; transition?: string; inline?: number; wired?: number; language: 'ld' | 'fbd' } }>;
  /**
   * Present only when the project's stored user types and data blocks do not check, so the
   * program was compiled without them: why, in the shape of the diagnostics. An item names its
   * source (`data/types/Shared.st`) with a line, or no source and line 0 when the problem is of
   * the whole (the data blocks' file cannot be read). Never places in the program's text.
   */
  project_problems?: Diagnostic[];
}

/** Where one state of a chart is kept: a step's (`step`: `X` or `T`), a transition's, an action's. */
export interface ChartProbe {
  local_id: number;
  step?: 'X' | 'T';
  transition?: boolean;
  action?: 'Q';
  variable: string;
  negated: boolean;
  ran?: string;
}

export interface BuildResult {
  version: string;
  warnings: string[];
  code_size: number;
  memory_size: number;
  /** What the project's runtimes take: `{ id, module, run, program }`. */
  instance: { id: string } & Record<string, unknown>;
  /** As a check's {@link CheckResult.project_problems}: why the project's types and data blocks were left out. */
  project_problems?: Diagnostic[];
}

export interface OutputStatus {
  tag: string;
  [key: string]: unknown;
}

/** One runtime of the controller, as `GET /api/v1/runtimes` reports it. */
export interface RuntimeStatus {
  id: string;
  module: string;
  /** The PROGRAM names it runs, joined by `, `. */
  program: string;
  /** Their versions, joined by `, `. */
  version: string;
  /** Its smallest cyclic interval; `null` when it has no cyclic task. */
  interval_ms: number | null;
  /** `run`, `stop`, `fault`, or `unreachable`. */
  mode: string;
  reason: string | null;
  /** `null`: nothing is known (the module does not answer). */
  scans: number | null;
  /** `null` while nothing is known, and until a scan has run. */
  last_scan_us: number | null;
  max_scan_us: number | null;
  overruns: number | null;
  outputs_safe: boolean;
  outputs: OutputStatus[];
  /**
   * The variables an engineer holds at a value. `null`: not known (the module does not
   * answer); absent from a server without forcing, which is not known either.
   */
  forced?: ForcedValue[] | null;
  /** Every tag bound to a forced variable. */
  forced_tags?: string[];
  /** Only for a runtime with its tasks written out (ADR-013 §2.19): each program and its version. */
  programs?: Array<{ name: string; version: string }>;
  /** Only for a runtime with its tasks written out: what it says of each task. Not shown yet (C8d-2). */
  tasks?: TaskState[];
  data?: unknown;
}

/**
 * What a runtime says of one of its tasks (`TaskState` of crates/mythos-module/src/protocol.rs).
 * A time not measured is absent, never 0. Not shown in C8d-1.
 */
export interface TaskState {
  name: string;
  kind: unknown;
  priority: number;
  programs: string[];
  watchdog_ms: number;
  runs: number;
  overruns: number;
  preemptions: number;
  last_run_us?: number | null;
  longest_run_us?: number | null;
  run_age_us?: number | null;
}

/** A variable held at a value by an engineer (`POST /api/v1/runtimes/<id>/force`). */
export interface ForcedValue {
  variable: string;
  value: unknown;
}

/** What forcing or releasing answers: the program's state, with what is forced now. */
export interface ForceAnswer {
  id: string;
  state: { mode: string; forced?: ForcedValue[] };
}

export interface RuntimesAnswer {
  runtimes: RuntimeStatus[];
  modules: Array<{ provides: string; name?: string; profile?: { name?: string; tasks?: { kinds?: string[] } | null } | null }>;
  problem: string | null;
}

/** One variable of a watch: its value, or why it has none. */
export interface WatchedValue {
  variable: string;
  type: string | null;
  value: unknown;
  error: string | null;
  /** Whether an engineer holds it at this value, rather than the program. */
  forced?: boolean;
}

/** A program's variables as its last scan left them (`POST /api/v1/runtimes/<id>/watch`). */
export interface WatchAnswer {
  id: string;
  /**
   * Which program and version answered, and whether it is in `run`; `scan_age_us`, how long
   * before the read the program last completed a scan (absent before its first scan and from a
   * module that does not say), and `max_scan_us` its longest scan.
   */
  state: {
    mode: string;
    program: string;
    version: string;
    scans: number;
    max_scan_us?: number;
    scan_age_us?: number | null;
    /** A runtime with its tasks written out: its programs and tasks. Not read yet (C8d-2). */
    programs?: Array<{ name: string; version: string }>;
    tasks?: TaskState[];
  };
  values: WatchedValue[];
  /** How long ago the runtime was read. */
  age_ms: number;
  /** How old an answer may be and still be given at once (absent from an older server). */
  fresh_ms?: number;
}

interface RuntimesConfig {
  instances: Array<{ id: string } & Record<string, unknown>>;
}

/** What the core's read of the project's runtimes configuration answers (`GET …/runtimes`). */
export interface StoredRuntimes {
  config: RuntimesConfig | null;
  /** The revision of what is stored: passed back with a save, which is refused if it changed. */
  revision: string;
}

/** A variable that did not carry over from the running version, and why. */
export interface NotCarried {
  variable: string;
  why: string;
}

/**
 * How a runtime a load started began (ADR-013 §2.5): `online`, it took over the running
 * version's variables and scans on; `warm`, its RETAIN variables took the values kept for them
 * and every other its initial value; `cold`, every variable its initial value; `unknown`, its
 * module took the running version's values and does not say which.
 */
export interface RuntimeLoad {
  runtime: string;
  start: 'online' | 'warm' | 'cold' | 'unknown';
  /** Why it was not changed online, when it was not. */
  why: string | null;
  /** Changed online: how many variables carried over. */
  carried: number | null;
  /** Changed online: the running version's variables it did not take. */
  dropped: NotCarried[];
  /** Changed online: its variables that took no value of the running version. */
  initialized: NotCarried[];
  /** The RETAIN values kept under its id that it was not given, and why. */
  not_given?: NotCarried[];
}

/** One problem of a runtimes configuration the core refused to save: which connection, and why. */
export interface ConfigProblem {
  connection_id: string;
  message: string;
}

/**
 * The `problems` the core gave with a refusal to save the project's runtimes
 * (`{error, problems: [{connection_id, message}]}`), verbatim; `null` when it gave none, or when
 * the workbench did not hand the answer over.
 */
export function problemsOf(cause: unknown): ConfigProblem[] | null {
  const body = (cause as { body?: unknown } | null)?.body;
  const problems = body !== null && typeof body === 'object' ? (body as Record<string, unknown>).problems : undefined;
  if (!Array.isArray(problems)) return null;
  return problems.flatMap((p: unknown) => {
    if (p === null || typeof p !== 'object') return [];
    const { connection_id, message } = p as Record<string, unknown>;
    return typeof message === 'string' ? [{ connection_id: typeof connection_id === 'string' ? connection_id : '', message }] : [];
  });
}

/** What saving a project's runtimes answers. */
export interface ApplyAnswer {
  applied: boolean;
  /**
   * `failed`: what did not start, as `[connection id, why]`; a runtime's id is `runtime:<id>`.
   * `runtimes`: how each runtime it started began.
   */
  changes?: {
    started?: string[];
    stopped?: string[];
    kept?: string[];
    failed?: Array<[string, string]>;
    runtimes?: RuntimeLoad[];
  } | null;
  [key: string]: unknown;
}

const logicOf = (projectId: string) =>
  `/api/v1/projects/${encodeURIComponent(projectId)}/logic/programs`;
const runtimesOf = (projectId: string) =>
  `/api/v1/projects/${encodeURIComponent(projectId)}/runtimes`;

/**
 * The query of a save of the project's runtimes configuration (`PUT …/runtimes`): `start=cold`
 * with `cold`, and the `revision` the read answered, so that the core refuses the save (409)
 * when the configuration has changed since that read instead of overwriting the change.
 */
export function saveQuery(cold: boolean, revision: string | undefined): string {
  const keys = [...(cold ? ['start=cold'] : []), ...(revision === undefined ? [] : [`revision=${encodeURIComponent(revision)}`])];
  return keys.length === 0 ? '' : `?${keys.join('&')}`;
}
const json = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
});
const name = encodeURIComponent;

export const api = {
  list: (projectId: string) => host().api<{ programs: ProgramEntry[] }>(logicOf(projectId)),
  create: (projectId: string, program: string, language: Language = 'st') =>
    host().api<{ program: ProgramEntry; source: string }>(
      logicOf(projectId),
      json('POST', language === 'st' ? { name: program } : { name: program, language })
    ),
  read: (projectId: string, program: string) =>
    host().api<{ program: ProgramEntry; source: string }>(`${logicOf(projectId)}/${name(program)}`),
  saveSource: (projectId: string, program: string, source: string) =>
    host().api(`${logicOf(projectId)}/${name(program)}/source`, json('PUT', { source })),
  saveSettings: (projectId: string, program: string, entry: ProgramEntry) => {
    const { name: _name, language: _language, ...settings } = entry;
    return host().api<{ program: ProgramEntry }>(
      `${logicOf(projectId)}/${name(program)}/settings`,
      json('PUT', settings)
    );
  },
  remove: (projectId: string, program: string) =>
    host().api(`${logicOf(projectId)}/${name(program)}`, { method: 'DELETE' }),
  /** Check `source`, which need not be saved. */
  check: (projectId: string, program: string, source: string) =>
    host().api<CheckResult>(`${logicOf(projectId)}/${name(program)}/check`, json('POST', { source })),
  build: (projectId: string, program: string) =>
    host().api<BuildResult>(`${logicOf(projectId)}/${name(program)}/build`, { method: 'POST' }),

  runtimes: () => host().api<RuntimesAnswer>('/api/v1/runtimes'),
  run: (id: string) => host().api(`/api/v1/runtimes/${name(id)}/run`, { method: 'POST' }),
  stop: (id: string) => host().api(`/api/v1/runtimes/${name(id)}/stop`, { method: 'POST' }),
  watch: (id: string, variables: string[]) =>
    host().api<WatchAnswer>(`/api/v1/runtimes/${name(id)}/watch`, json('POST', { variables })),
  force: (id: string, variable: string, value: number | boolean) =>
    host().api<ForceAnswer>(`/api/v1/runtimes/${name(id)}/force`, json('POST', { variable, value })),
  /** Release one forced variable, or every one with `null`. */
  release: (id: string, variable: string | null) =>
    host().api<ForceAnswer>(
      `/api/v1/runtimes/${name(id)}/release`,
      json('POST', variable === null ? {} : { variable })
    ),
  /**
   * Put `instance` into the project's runtimes in place of the one with its id, or take the
   * one called `id` out when `instance` is `null`. Every other instance is sent back as it was
   * read, with the revision of that read: if another client saved since, the core refuses the
   * save (409, its words in the error) and nothing is changed. With `cold`, what this starts
   * starts from its initial values, RETAIN ones included; otherwise a new version of a running
   * program is changed online.
   */
  setInstance: async (
    projectId: string,
    id: string,
    instance: ({ id: string } & Record<string, unknown>) | null,
    cold = false
  ): Promise<ApplyAnswer> => {
    const saved = await host().api<StoredRuntimes>(runtimesOf(projectId));
    const others = (saved.config?.instances ?? []).filter((other) => other.id !== id);
    return host().api<ApplyAnswer>(
      `${runtimesOf(projectId)}${saveQuery(cold, saved.revision)}`,
      json('PUT', { instances: instance ? [...others, instance] : others })
    );
  },
};

/**
 * Follow `next` from now on: mirror its project, telemetry and controller into the stores
 * above. Returns what stops it. Called by `activate`, and by tests with a host of their own.
 */
export function bind(next: WorkbenchHost): Dispose {
  current = next;
  useTelemetry.setState(next.telemetry.snapshot(), true);
  useController.setState(next.controller.get(), true);
  useProject.setState({ project: next.project() });
  const stops = [
    next.telemetry.subscribe(() => useTelemetry.setState(next.telemetry.snapshot(), true)),
    next.controller.subscribe(() => useController.setState(next.controller.get(), true)),
    next.onProject((project) => useProject.setState({ project })),
  ];
  return () => {
    for (const stop of stops) stop();
    if (current === next) current = null;
  };
}
