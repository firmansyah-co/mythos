// A project's configured runtimes and their tasks (ADR-013 §2.19, C8d): the logic module's file
// `logic/tasks.json`, through its six routes, what they answer, and the few pure rules this UI
// needs to show them. The rules the module owns (what a runtime id may be, the task rules, a
// program in one runtime) are not repeated here: the module answers them, and its words are
// shown. A runtime written here is "configured"; whether the controller runs it is the core's,
// read elsewhere, and never said from this file.

import { host, type Diagnostic, type RuntimesAnswer } from './bridge';

/** The runtime editor's kind; its target is the runtime's id. */
export const RUNTIME_EDITOR = 'logic-runtime';
/** Sent by the logic module after a runtime was saved or deleted. */
export const RUNTIMES_CHANGED_EVENT = 'LOGIC_RUNTIMES_CHANGED';
/** The program editor's kind (entry.tsx's `EDITOR`), which a program's diagnostic opens. */
export const PROGRAM_EDITOR = 'logic-program';

/** The kinds of task the module's protocol knows, in the order the form offers them. */
export const TASK_KINDS = ['cycle', 'cyclic', 'startup', 'event'] as const;
export type TaskKindName = (typeof TASK_KINDS)[number];

/**
 * When a task runs (`TaskKind` of crates/mythos-module/src/protocol.rs, tagged by `type`). As the
 * module stores it every number is there; in a draft a number not typed yet is `null`, and one
 * left out takes the module's own default (`min_cycle_ms`, `phase_ms`: 0).
 */
export type TaskKind =
  | { type: 'cycle'; min_cycle_ms?: number | null }
  | { type: 'cyclic'; interval_ms: number | null; phase_ms?: number | null }
  | { type: 'startup' }
  | { type: 'event'; tag: string };

/**
 * One task: its name, kind, priority, watchdog and programs in the order they run. As the module
 * stores it, `kind` and `priority` are always given; in a draft they are `null` until the engineer
 * gives them: no kind and no priority is right for every task, so none is assumed.
 */
export interface TaskConfig {
  name: string;
  kind: TaskKind | null;
  priority: number | null;
  /** Left out (or `null` in a draft): the runtime's default. */
  watchdog_ms?: number | null;
  programs: string[];
}

/** What `PUT /runtimes/:runtime` takes; in a draft `run` is `null` until it is chosen. */
export interface RuntimeBody {
  module: string;
  target: string;
  run: boolean | null;
  tasks: TaskConfig[];
}

/** A runtime of `logic/tasks.json`, as the module answers it (or as a draft of one). */
export interface RuntimeEntry extends RuntimeBody {
  id: string;
}

/** `GET /runtimes`: the configured runtimes in file order, the implied ones, and what a hand edit broke. */
export interface RuntimesList {
  runtimes: RuntimeEntry[];
  implied: RuntimeEntry[];
  problems: string[];
}

/** One program of a runtime's check, as the build would compile it. */
export interface ProgramCheck {
  name: string;
  ok: boolean;
  diagnostics: Diagnostic[];
  binding_problems: string[];
  version?: string;
  code_size?: number;
  memory_size?: number;
}

/** What `POST /runtimes/:runtime/check` answers. A compile error is listed, never refused. */
export interface RuntimeCheck {
  ok: boolean;
  runtime: string;
  programs: ProgramCheck[];
  /** The configuration's problems and the runtime's as a whole, in the module's words. */
  problems: string[];
  /** Only when ok. */
  version?: string;
  data?: unknown;
  project_problems?: Diagnostic[];
}

/** A program of an instance, as the core's runtimes configuration takes it. */
export interface RuntimeProgramWire {
  name: string;
  /** The project's program it was built from (logic module 0.8.0 and later). */
  built_from?: string;
  version?: string;
  [key: string]: unknown;
}

/** An instance of the core's runtimes configuration. */
export interface Instance {
  id: string;
  module?: string;
  run?: boolean;
  tasks?: unknown[];
  program?: RuntimeProgramWire;
  programs?: RuntimeProgramWire[];
  [key: string]: unknown;
}

/** What `POST /runtimes/:runtime/build` answers. */
export interface RuntimeBuild {
  version: string;
  warnings: string[];
  programs: Array<{ name: string; version: string; code_size: number; memory_size: number }>;
  /** The ids the runtime's programs ran under alone. */
  replaces: string[];
  instance: Instance;
  project_problems?: Diagnostic[];
}

/** `LOGIC_RUNTIMES_CHANGED`'s data. */
export interface RuntimesChanged {
  project: string;
  /** The request's id for a save, the stored spelling for a delete. */
  runtime: string;
  what: 'saved' | 'deleted';
}

/** `LOGIC_RUNTIMES_CHANGED`'s data, when it has the shape the module sends; otherwise nothing. */
export function runtimesEventOf(data: unknown): RuntimesChanged | null {
  if (data === null || typeof data !== 'object') return null;
  const d = data as Record<string, unknown>;
  if (typeof d.project !== 'string' || typeof d.runtime !== 'string') return null;
  if (d.what !== 'saved' && d.what !== 'deleted') return null;
  return { project: d.project, runtime: d.runtime, what: d.what };
}

export const bodyOf = (r: RuntimeEntry): RuntimeBody => ({ module: r.module, target: r.target, run: r.run, tasks: r.tasks });
/** The module compares runtime ids with eq_ignore_ascii_case. */
export const sameId = (a: string, b: string): boolean => a.toLowerCase() === b.toLowerCase();

/** Said before a runtime is deleted from the project's configuration. */
export const deleteRuntimeQuestion = (id: string) =>
  `Delete runtime ${id} from the project's configuration? Nothing that runs is touched: if the controller runs it, it runs on with all its programs until it is unloaded. Each of its programs then runs alone only once it is built and loaded alone.`;

/** A task as the form names it: by its name, or by its place when it has none yet. */
export const taskLabel = (task: TaskConfig, index: number): string =>
  task.name.trim() === '' ? `task ${index + 1}` : `task ${task.name}`;

/**
 * What a draft lacks to be a body the module takes: the decisions not made (run, a task's kind,
 * its priority) and the one number a kind has no default for (a cyclic task's interval). Each is
 * said; nothing else is judged here, the module answers every other rule in its words.
 */
export function missingOf(body: RuntimeBody): string[] {
  const missing: string[] = [];
  if (body.run === null) missing.push('whether it runs when loaded: choose "Run when loaded" or "Stay in STOP when loaded"');
  body.tasks.forEach((task, index) => {
    const label = taskLabel(task, index);
    if (task.kind === null) missing.push(`${label}: its kind`);
    else if (task.kind.type === 'cyclic' && task.kind.interval_ms === null) missing.push(`${label}: its interval`);
    if (task.priority === null) missing.push(`${label}: its priority`);
  });
  return missing;
}

/** A task's kind as sent: a number not given is left out, where the module has a default for it. */
function kindSent(kind: TaskKind): TaskKind {
  switch (kind.type) {
    case 'cycle':
      return kind.min_cycle_ms === null || kind.min_cycle_ms === undefined ? { type: 'cycle' } : { type: 'cycle', min_cycle_ms: kind.min_cycle_ms };
    case 'cyclic':
      return kind.phase_ms === null || kind.phase_ms === undefined
        ? { type: 'cyclic', interval_ms: kind.interval_ms }
        : { type: 'cyclic', interval_ms: kind.interval_ms, phase_ms: kind.phase_ms };
    case 'startup':
      return { type: 'startup' };
    case 'event':
      return { type: 'event', tag: kind.tag };
  }
}

/**
 * The body to send for `draft`: exactly the four keys the route takes, each task's keys the
 * protocol's. `null` while it is not a complete body ({@link missingOf}): nothing is sent then.
 */
export function sentOf(draft: RuntimeBody): RuntimeBody | null {
  if (missingOf(draft).length > 0) return null;
  return {
    module: draft.module,
    target: draft.target,
    run: draft.run,
    tasks: draft.tasks.map((task) => ({
      name: task.name,
      kind: task.kind === null ? null : kindSent(task.kind),
      priority: task.priority,
      ...(task.watchdog_ms === null || task.watchdog_ms === undefined ? {} : { watchdog_ms: task.watchdog_ms }),
      programs: task.programs,
    })),
  };
}

/** A new runtime's draft: nothing chosen that the engineer has not chosen. */
export const newRuntime = (id: string): RuntimeEntry => ({ id, module: '', target: '', run: null, tasks: [] });
/** A new task: no kind and no priority until they are given. */
export const newTask = (): TaskConfig => ({ name: '', kind: null, priority: null, programs: [] });
/** A kind chosen in the form: its numbers not typed yet. */
export function kindOf(type: TaskKindName): TaskKind {
  switch (type) {
    case 'cycle':
      return { type: 'cycle', min_cycle_ms: null };
    case 'cyclic':
      return { type: 'cyclic', interval_ms: null, phase_ms: null };
    case 'startup':
      return { type: 'startup' };
    case 'event':
      return { type: 'event', tag: '' };
  }
}

/**
 * The kinds of task module `module` says it runs (`profile.tasks.kinds`), `null` when its profile
 * says nothing of tasks, `undefined` when it is not among `modules` (not installed, or not known).
 */
export function kindsOf(modules: RuntimesAnswer['modules'] | null, module: string): string[] | null | undefined {
  const found = modules?.find((m) => m.provides === module);
  if (found === undefined) return undefined;
  const tasks = (found.profile as { tasks?: { kinds?: unknown } } | null | undefined)?.tasks;
  return Array.isArray(tasks?.kinds) ? tasks.kinds.filter((k): k is string => typeof k === 'string') : null;
}

/** Which configured runtime (other than `id`) names each program, by program name in lower case. */
export function heldBy(runtimes: readonly RuntimeEntry[], id: string): Map<string, string> {
  const held = new Map<string, string>();
  for (const runtime of runtimes) {
    if (sameId(runtime.id, id)) continue;
    for (const task of runtime.tasks) {
      for (const program of task.programs) if (!held.has(program.toLowerCase())) held.set(program.toLowerCase(), runtime.id);
    }
  }
  return held;
}

const runtimesPath = (projectId: string) => `/api/v1/projects/${encodeURIComponent(projectId)}/logic/runtimes`;
const json = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
});

export const api = {
  list: (projectId: string) => host().api<RuntimesList>(runtimesPath(projectId)),
  /** A configured runtime (`implied: false`), or a program's runtime alone (`implied: true`). */
  getRuntime: (projectId: string, id: string) =>
    host().api<{ runtime: RuntimeEntry; implied: boolean }>(`${runtimesPath(projectId)}/${encodeURIComponent(id)}`),
  /** Make or replace runtime `id`; the module checks every rule and answers what it stored. */
  save: (projectId: string, id: string, body: RuntimeBody) =>
    host().api<{ runtime: RuntimeEntry; created: boolean }>(`${runtimesPath(projectId)}/${encodeURIComponent(id)}`, json('PUT', body)),
  remove: (projectId: string, id: string) =>
    host().api<{ deleted: string }>(`${runtimesPath(projectId)}/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  /** Check runtime `id` with `body` put in, as a save would put it, against the stored rest. Writes nothing. */
  check: (projectId: string, id: string, body: RuntimeBody) =>
    host().api<RuntimeCheck>(`${runtimesPath(projectId)}/${encodeURIComponent(id)}/check`, json('POST', body)),
  build: (projectId: string, id: string) =>
    host().api<RuntimeBuild>(`${runtimesPath(projectId)}/${encodeURIComponent(id)}/build`, { method: 'POST' }),
  /** The core's runtimes and installed modules (`GET /api/v1/runtimes`): here, the modules. */
  installed: () => host().api<RuntimesAnswer>('/api/v1/runtimes'),
};
