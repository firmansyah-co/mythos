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

export interface Diagnostic {
  line: number;
  col: number;
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
  scans: number;
  last_scan_us: number;
  max_scan_us: number;
  overruns: number;
  outputs_safe: boolean;
  outputs: OutputStatus[];
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

/** What saving a project's runtimes answers. */
export interface ApplyAnswer {
  applied: boolean;
  /** `failed`: what did not start, as `[connection id, why]`; a runtime's id is `runtime:<id>`. */
  changes?: {
    started?: string[];
    stopped?: string[];
    kept?: string[];
    failed?: Array<[string, string]>;
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
  /**
   * Put `instance` into the project's runtimes in place of the one with its id, or take the
   * one called `id` out when `instance` is `null`. Every other instance is sent back as it was
   * read.
   */
  setInstance: async (
    projectId: string,
    id: string,
    instance: ({ id: string } & Record<string, unknown>) | null
  ): Promise<ApplyAnswer> => {
    const saved = await host().api<{ config: RuntimesConfig | null }>(runtimesOf(projectId));
    const others = (saved.config?.instances ?? []).filter((other) => other.id !== id);
    return host().api<ApplyAnswer>(
      runtimesOf(projectId),
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
