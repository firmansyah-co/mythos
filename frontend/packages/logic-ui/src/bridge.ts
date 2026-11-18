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

/** A program's settings, as the logic module stores them. */
export interface ProgramEntry {
  name: string;
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
}

export interface Variable {
  path: string;
  type: string;
  /** `input`, `output`, `local`, … as the compiler names the section. */
  section: string;
  bindable: boolean;
}

/** The answer of a check: the diagnostics, or what compiled. */
export interface CheckResult {
  ok: boolean;
  diagnostics: Diagnostic[];
  variables?: Variable[];
  code_size?: number;
  memory_size?: number;
  binding_problems?: string[];
}

export interface BuildResult {
  version: string;
  warnings: string[];
  code_size: number;
  memory_size: number;
  /** What the project's runtimes take: `{ id, module, run, program }`. */
  instance: { id: string } & Record<string, unknown>;
}

export interface OutputStatus {
  tag: string;
  [key: string]: unknown;
}

/** One runtime of the controller, as `GET /api/v1/runtimes` reports it. */
export interface RuntimeStatus {
  id: string;
  module: string;
  program: string;
  version: string;
  interval_ms: number;
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
  modules: Array<{ provides: string; name?: string; profile?: { name?: string } | null }>;
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
  /** Which program and version answered, and whether it is in `run`. */
  state: { mode: string; program: string; version: string; scans: number };
  values: WatchedValue[];
  age_ms: number;
}

interface RuntimesConfig {
  instances: Array<{ id: string } & Record<string, unknown>>;
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
const json = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
});
const name = encodeURIComponent;

export const api = {
  list: (projectId: string) => host().api<{ programs: ProgramEntry[] }>(logicOf(projectId)),
  create: (projectId: string, program: string) =>
    host().api<{ program: ProgramEntry; source: string }>(
      logicOf(projectId),
      json('POST', { name: program })
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
   * read. With `cold`, what this starts starts from its initial values, RETAIN ones included;
   * otherwise a new version of a running program is changed online.
   */
  setInstance: async (
    projectId: string,
    id: string,
    instance: ({ id: string } & Record<string, unknown>) | null,
    cold = false
  ): Promise<ApplyAnswer> => {
    const saved = await host().api<{ config: RuntimesConfig | null }>(runtimesOf(projectId));
    const others = (saved.config?.instances ?? []).filter((other) => other.id !== id);
    return host().api<ApplyAnswer>(
      `${runtimesOf(projectId)}${cold ? '?start=cold' : ''}`,
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
