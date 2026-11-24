// The logic UI under test runs against the workbench's real host (`createHost`), so what is
// proven here is the module as the workbench activates it, not against a stand-in. The
// controller is a fake `fetch` that keeps programs and runtimes the way the logic module and
// the core do, and the library's five routes literally as plugins/logic/src/lib.rs answers them
// (statuses, bodies, the stored spelling, the name rule's words), with a stand-in for the
// compiler that a test sets; it records every call. The stream's events are sent by the test
// through `emit`: the one part of the host replaced, since happy-dom has no event stream.

import { createHash } from 'node:crypto';
import { act, type ReactElement } from 'react';
import { vi } from 'vitest';
import { activate, MODULE_ID } from '../entry';
import { useLdPalette } from '../LdInstructions';
import { usePrograms } from '../programs';
import type { CheckResult, ConfigProblem, Diagnostic, ProgramEntry, RuntimeLoad, RuntimesAnswer, RuntimeStatus, WatchAnswer, WatchedValue } from '../bridge';
import type { LibraryCheck } from '../library';
import type { ProgramCheck, RuntimeEntry, TaskConfig } from '../runtimes';
import { createHost } from '../../../ui-shell/src/modules/host';
import { render as renderShell, resetStores as resetShell, unmountAll } from '../../../ui-shell/src/__tests__/harness';
import { ModuleMount } from '../../../ui-shell/src/modules/ModuleMount';
import { useModuleUi, type Owned } from '../../../ui-shell/src/modules/registry';
import { useWorkbench } from '../../../ui-shell/src/data/workbench';
import type { Panel } from '@mythos/workbench-sdk';

export {
  connection,
  ioStatus,
  project,
  sample,
  texts,
  typeInto,
  unmountAll,
} from '../../../ui-shell/src/__tests__/harness';
import { useController } from '../../../ui-shell/src/data/controller';
export { useController };
export { useSession } from '../../../ui-shell/src/data/session';
export { useTelemetry } from '../../../ui-shell/src/data/telemetry';
export { useWorkbench as useShell } from '../../../ui-shell/src/data/workbench';
export { useModuleUi } from '../../../ui-shell/src/modules/registry';

let deactivate: (() => void) | null = null;
const listeners = new Map<string, Set<(data: unknown) => void>>();

/** Send `name` with `data` on the stream, as the controller does after a module's write. */
export function emit(name: string, data: unknown): void {
  act(() => {
    for (const listener of listeners.get(name) ?? []) listener(data);
  });
}

/** Every panel of one pane, each under its owner, drawn as the workbench draws them. */
function PaneOf({ pane, panels }: { pane: string; panels: Record<string, Owned<Panel>> }) {
  const project = useWorkbench((state) => state.project) ?? { id: 'no-project', name: 'no project' };
  return (
    <>
      {Object.entries(panels).map(([owner, panel]) => (
        <section key={owner} data-pane={pane} data-owner={owner} aria-label={panel.item.title}>
          <ModuleMount
            mount={panel.item.mount}
            context={{ project: { id: project.id, name: project.name } }}
            deps={[project.id, owner]}
          />
        </section>
      ))}
    </>
  );
}

/**
 * What the module put into the workbench's panes, all of it at once: the task cards, the
 * inspector's panels and its diagnostics. Which of the inspector's tabs is in front is the
 * workbench's business, tested there; here every panel the module set is on the page.
 */
function Panes() {
  const ui = useModuleUi();
  return (
    <>
      <PaneOf pane="task-cards" panels={ui.taskCards} />
      <PaneOf pane="inspector" panels={ui.inspector} />
      <PaneOf pane="diagnostics" panels={ui.diagnostics} />
    </>
  );
}

/** `element` drawn as the work area's editor, with the panes the workbench draws beside it. */
export function render(element: ReactElement): HTMLElement {
  return renderShell(
    <>
      {element}
      <Panes />
    </>
  );
}

/** The panel `owner` set in a pane, or null when it has set none there. */
export function paneOf(container: HTMLElement, pane: 'task-cards' | 'inspector' | 'diagnostics', owner: string): HTMLElement | null {
  return container.querySelector(`[data-pane="${pane}"][data-owner="${owner}"]`);
}

/** Every store as the workbench starts, and the logic UI activated against it afresh. */
export function resetStores(): void {
  deactivate?.();
  resetShell();
  useLdPalette.setState({ place: null, own: [] });
  useLdPalette.getState().reread();
  listeners.clear();
  const real = createHost(MODULE_ID);
  deactivate = activate({
    ...real,
    events: {
      subscribe: (name, listener) => {
        let set = listeners.get(name);
        if (!set) listeners.set(name, (set = new Set()));
        set.add(listener);
        return () => set.delete(listener);
      },
    },
  });
}

export interface Call {
  method: string;
  path: string;
  body: unknown;
}

export interface Backend {
  programs: Array<{ entry: ProgramEntry; source: string }>;
  /** What a check of a text answers. Defaults to a program that compiles with no variable. */
  check?: (source: string) => CheckResult;
  /** `GET /api/v1/runtimes`; `'fails'` answers 500; `'hangs'` never answers. */
  runtimes?: RuntimesAnswer | 'fails' | 'hangs';
  /** The project's saved runtimes. */
  config?: { instances: Array<Record<string, unknown>> } | null;
  /**
   * The runtimes configured in the logic module (`logic/tasks.json`), in its order: each a whole
   * entry, or `{ id, programs }`, a runtime of module `vm`, target `pc`, run, whose one task `t`
   * (cyclic, 10 ms, priority 1) runs `programs`. Left out, none. `'fails'` answers every route
   * of the runtimes 500, as a file that is not the module's.
   */
  configured?: Array<{ id: string; programs: string[] } | RuntimeEntry> | 'fails';
  /**
   * Send `LOGIC_RUNTIMES_CHANGED` for every runtime save or delete answered 200, as the module
   * does, before the answer reaches the editor (the order a stream may deliver them in). Left out,
   * no event is sent by the fake: a test emits what it needs.
   */
  runtimeEvents?: boolean;
  /** A runtime's check of each of its programs; left out, every program compiles with no problem. */
  checkRuntimeProgram?: (name: string) => Omit<ProgramCheck, 'name'>;
  /**
   * Whether saving the runtimes applies them (the project is the active one). Left out: applied
   * when `active` is left out or names the project saved.
   */
  applied?: boolean;
  /**
   * The project the controller runs, as the workbench's controller poll reads it; `null`, none.
   * Given, the workbench is told so when `server` starts. The core lists the runtimes of that
   * project only: `runtimes` is then that project's list.
   */
  active?: string | null;
  /**
   * The controller's field I/O is not running: an applied save of the runtimes answers
   * `changes: null` (`reload_with` → `idle` in crates/mythos-core/src/io/manager.rs).
   */
  fieldIoOff?: boolean;
  /**
   * What the core's I/O manager finds wrong with a configuration it is asked to save
   * (`check_runtimes`); any makes the save a 400 `{error, problems}`. Left out, none.
   */
  configProblems?: (instances: Array<Record<string, unknown>>) => ConfigProblem[];
  /** How the core says each runtime of an applied save began (`changes.runtimes`); left out, none. */
  starts?: (instances: Array<Record<string, unknown>>, cold: boolean) => RuntimeLoad[];
  /** What did not start, `[connection id, why]` (`changes.failed`); left out, none. */
  failedToStart?: Array<[string, string]>;
  warnings?: string[];
  /**
   * What a watch of a runtime's variables answers; `'fails'`, or left out, answers 502;
   * `'hangs'` never answers, as a request left hanging.
   */
  watch?: ((variables: string[]) => WatchAnswer) | 'fails' | 'hangs';
  /** Answer a call before the fake does, to make one fail: a response, or one that comes later. */
  intercept?: (call: Call) => Response | Promise<Response> | undefined;
  /** The project's library by stem: each source's text, or `{ unreadable }`, a file that is not UTF-8. */
  library?: Record<string, string | { unreadable: string }>;
  /**
   * The compiler's answer for the library's sources, by file (`logic/library/<stem>.st`), once the
   * module found nothing it refuses first; left out, every library checks.
   */
  compileLibrary?: (sources: Record<string, string>) => LibraryCheck;
  /** What the file holds after a save of `source`: by default `source`. */
  libraryStores?: (source: string) => string;
}

export const entryOf = (name: string, over: Partial<ProgramEntry> = {}): ProgramEntry => ({
  name,
  language: 'st',
  interval_ms: 100,
  runtime: 'vm',
  target: 'pc',
  run: false,
  inputs: [],
  outputs: [],
  published: [],
  ...over,
});

export const runtimeOf = (id: string, over: Partial<RuntimeStatus> = {}): RuntimeStatus => ({
  id,
  module: 'vm',
  program: id,
  version: 'aaaaaaaaaaaa',
  interval_ms: 100,
  mode: 'run',
  reason: null,
  scans: 42,
  last_scan_us: 7,
  max_scan_us: 19,
  overruns: 0,
  outputs_safe: false,
  outputs: [],
  ...over,
});

export const runtimesOf = (...runtimes: RuntimeStatus[]): RuntimesAnswer => ({
  runtimes,
  modules: [{ provides: 'vm', profile: { name: 'pc' } }],
  problem: null,
});

/** How a watch answer says how old it is: what `watchOf` answers unless told otherwise. */
export interface Ages {
  age_ms?: number;
  fresh_ms?: number;
  scans?: number;
  scan_age_us?: number | null;
  max_scan_us?: number;
}

/**
 * A watch's answer: `values` by variable name, from a program in `mode`, read 12 ms ago of a
 * program whose last scan completed 30 ms before (well inside two of `runtimeOf`'s 100 ms
 * intervals), as the core gives one. `ages` changes how old it says it is; a key given as
 * `undefined` is left out of the answer, as a controller that does not say.
 */
export const watchOf =
  (values: Record<string, Partial<WatchedValue>>, mode = 'run', ages: Ages = {}) =>
  (variables: string[]): WatchAnswer => {
    const said = { age_ms: 12, fresh_ms: 1000, scans: 42, scan_age_us: 30_000, max_scan_us: 19, ...ages };
    const answer = {
      id: 'Tank',
      state: { mode, program: 'Tank', version: 'aaaaaaaaaaaa', scans: said.scans, scan_age_us: said.scan_age_us, max_scan_us: said.max_scan_us },
      values: variables.map((variable) => ({
        variable,
        type: null,
        value: null,
        error: `the program has no variable ${variable}`,
        ...(values[variable] ? { error: null, ...values[variable] } : {}),
      })),
      age_ms: said.age_ms,
      fresh_ms: said.fresh_ms,
    };
    // Through JSON, as the server sends it: what is undefined is not there.
    return JSON.parse(JSON.stringify(answer)) as WatchAnswer;
  };

/** A TC6 document holding the PROGRAM `name` with `elements` as its LD body, between the rails. */
export const ladderOf = (name: string, elements: string, interface_ = '<interface/>') =>
  `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="${name}" pouType="program">
      ${interface_}
      <body><LD>
        <leftPowerRail localId="1"><position x="0" y="0"/><connectionPointOut formalParameter=""/></leftPowerRail>
        ${elements}
        <rightPowerRail localId="2"><position x="400" y="0"/></rightPowerRail>
      </LD></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
`;

/** A TC6 project holding one PROGRAM whose body is the Function Block Diagram `elements`. */
export const fbdOf = (name: string, elements: string, interface_ = '<interface/>') =>
  `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="${name}" pouType="program">
      ${interface_}
      <body><FBD>
        ${elements}
      </FBD></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
`;

/** A new chart as the logic module makes one (its template): the PROGRAM with one initial step, Start. */
export const sfcOf = (name: string) =>
  `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="${name}" pouType="program">
      <interface/>
      <body><SFC>
        <step localId="1" name="Start" initialStep="true"><position x="0" y="0"/><connectionPointOut formalParameter=""/></step>
      </SFC></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
`;

const failure = (status: number, error: string) =>
  new Response(JSON.stringify({ error }), { status, statusText: status === 500 ? 'Internal Server Error' : 'Bad Request' });

const STATUS: Record<number, string> = { 400: 'Bad Request', 404: 'Not Found', 409: 'Conflict', 500: 'Internal Server Error' };
/** A refusal of the library's routes, with the status text the status has. */
const refusal = (status: number, error: string) => Response.json({ error }, { status, statusText: STATUS[status] ?? '' });

/**
 * The revision the core answers for a stored runtimes configuration (crates/mythos-core/src/runtimes.rs
 * `revision_of`): the lower hex SHA-256 of the stored bytes, of no bytes when there is no file. The fake
 * keeps no file: it hashes `config` as JSON with two spaces, as the core writes it, so a test that changes
 * `backend.config` (another client's save) changes the revision, as a save or a hand edit does.
 */
export const revisionOf = (config: Backend['config']): string =>
  createHash('sha256')
    .update(config === null || config === undefined ? '' : JSON.stringify(config, null, 2))
    .digest('hex');

/** The logic module's event after a runtime was saved or deleted (plugins/logic/src/lib.rs). */
const RUNTIMES_CHANGED = 'LOGIC_RUNTIMES_CHANGED';
const DEVICE_NAMES = ['CON', 'PRN', 'AUX', 'NUL', ...[...'0123456789'].flatMap((d) => [`COM${d}`, `LPT${d}`])];

/** The module's `library_name_problem`, its messages word for word. */
export function libraryNameProblem(name: string): string | null {
  if (!/^[A-Za-z][A-Za-z0-9_]*$/.test(name) || name.length > 64) {
    return `'${name}' is not a library source name: a letter, then letters, digits or _, at most 64`;
  }
  if (DEVICE_NAMES.some((d) => d.toUpperCase() === name.toUpperCase())) {
    return `'${name}' cannot be a library source name: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)`;
  }
  return null;
}

/** The logic module's library routes over `backend.library`, as `library()` in plugins/logic/src/lib.rs. */
function libraryRoute(backend: Backend, method: string, param: string | undefined, body: unknown): Response {
  const library = (backend.library ??= {});
  const bytes = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  const names = () => Object.keys(library).sort(bytes);
  // `stored_library_name`: the exact spelling, else one that differs only in ASCII case.
  const stored = (name: string) =>
    names().find((n) => n === name) ?? names().find((n) => n.toUpperCase() === name.toUpperCase());
  const otherSpelling = (existing: string) =>
    refusal(409, `the project already has a library source '${existing}'; names that differ only in case are one name`);

  // A route is matched by its method first: only a POST to `check` is the check.
  if (method === 'POST' && param === 'check') {
    const given = (body ?? {}) as { name?: string; source?: string };
    const sources = { ...library };
    if (given.name !== undefined && given.source !== undefined) {
      const problem = libraryNameProblem(given.name);
      if (problem !== null) return refusal(400, problem);
      const existing = stored(given.name);
      if (existing !== undefined && existing !== given.name) return otherSpelling(existing);
      sources[given.name] = given.source;
    } else if (given.name !== undefined || given.source !== undefined) {
      return refusal(400, 'give both name and source, or neither');
    }
    // `library_refusals` first: a source that is not text is said, and the compiler is not run.
    const unreadable: Diagnostic[] = Object.keys(sources)
      .sort(bytes)
      .flatMap((stem) => {
        const text = sources[stem];
        if (typeof text === 'string') return [];
        const file = `logic/library/${stem}.st`;
        // `read_library` (lib.rs): "it is not UTF-8 text (<the decoder's words>)".
        return [{ line: 0, col: 0, end_line: 0, end_col: 0, message: `${file} cannot be read as a library source: it is not UTF-8 text (${text?.unreadable})`, source: file }];
      });
    if (unreadable.length > 0) return Response.json({ ok: false, diagnostics: unreadable });
    const texts = Object.fromEntries(Object.entries(sources).map(([stem, text]) => [`logic/library/${stem}.st`, text as string]));
    return Response.json(backend.compileLibrary?.(texts) ?? { ok: true, diagnostics: [] });
  }
  if (param !== undefined) {
    const problem = libraryNameProblem(param);
    if (problem !== null) return refusal(400, problem);
  }
  if (param === undefined && method === 'GET') return Response.json({ sources: names().map((name) => ({ name })) });
  if (param === undefined) return refusal(404, `the logic module has no route ${method} /library`);
  if (method === 'GET') {
    const stem = stored(param);
    if (stem === undefined) return refusal(404, `the project has no library source '${param}'`);
    const text = library[stem];
    if (typeof text !== 'string') return refusal(500, `/projects/plant/logic/library/${stem}.st: ${text?.unreadable}`);
    return Response.json({ name: stem, source: text });
  }
  if (method === 'PUT') {
    const source = (body as { source?: unknown } | undefined)?.source;
    if (typeof source !== 'string') return refusal(400, 'the body is not what this route takes: missing field `source`');
    const existing = stored(param);
    if (existing !== undefined && existing !== param) return otherSpelling(existing);
    // Any text is written: the module checks nothing of it.
    library[param] = backend.libraryStores?.(source) ?? source;
    return Response.json({ name: param, created: existing === undefined });
  }
  if (method === 'DELETE') {
    const stem = stored(param);
    if (stem === undefined) return refusal(404, `the project has no library source '${param}'`);
    delete library[stem];
    return Response.json({ deleted: stem });
  }
  return refusal(404, `the logic module has no route ${method} /library/:name`);
}

/** Each configured runtime of `backend` as the module stores it. */
export function configuredOf(backend: Backend): RuntimeEntry[] {
  if (backend.configured === 'fails' || backend.configured === undefined) return [];
  return backend.configured.map((r) =>
    'tasks' in r
      ? r
      : {
          id: r.id,
          module: 'vm',
          target: 'pc',
          run: true,
          tasks: [{ name: 't', kind: { type: 'cyclic', interval_ms: 10, phase_ms: 0 }, priority: 1, programs: r.programs }],
        }
  );
}

/** The module's `id_problem` (T1), word for word. */
export const runtimeIdProblem = (id: string): string | null =>
  /^[A-Za-z0-9_-]{1,64}$/.test(id) ? null : `'${id}' is not a runtime id: use 1 to 64 letters, digits, '-' or '_'`;

/**
 * A body `PUT /runtimes/:runtime` takes, as serde reads `RuntimeBody` and the protocol's
 * `TaskConfig`/`TaskKind` (`deny_unknown_fields`, `priority: u8`, the kinds' numbers `u64`, the
 * defaults it fills), or serde's words for why it is not one, after the module's own prefix.
 * Returned as the module stores it: `min_cycle_ms` and `phase_ms` filled with 0 when left out,
 * `watchdog_ms` left out when not given (or `null`).
 */
function runtimeBodyOf(body: unknown): Omit<RuntimeEntry, 'id'> | string {
  const words = (why: string) => `the body is not what this route takes: ${why}`;
  const shape = (v: unknown): string =>
    v === null ? 'null' : typeof v === 'boolean' ? `boolean \`${v}\`` : typeof v === 'string' ? `string ${JSON.stringify(v)}` : Array.isArray(v) ? 'sequence' : typeof v === 'object' ? 'map' : String(v);
  /** An unsigned integer of `bits`, or serde's words. */
  const unsigned = (v: unknown, type: 'u8' | 'u64'): number | string => {
    if (typeof v !== 'number') return words(`invalid type: ${shape(v)}, expected ${type}`);
    if (!Number.isInteger(v)) return words(`invalid type: floating point \`${v}\`, expected ${type}`);
    if (v < 0 || (type === 'u8' && v > 255)) return words(`invalid value: integer \`${v}\`, expected ${type}`);
    return v;
  };
  const fieldsOf = (o: Record<string, unknown>, known: string[], required: string[]): string | null => {
    const unknown = Object.keys(o).find((k) => !known.includes(k));
    if (unknown !== undefined) return words(`unknown field \`${unknown}\`, expected one of ${known.map((k) => `\`${k}\``).join(', ')}`);
    const lacking = required.find((k) => !(k in o));
    return lacking === undefined ? null : words(`missing field \`${lacking}\``);
  };
  if (body === null || typeof body !== 'object' || Array.isArray(body)) return words(`invalid type: ${shape(body)}, expected struct RuntimeBody`);
  const b = body as Record<string, unknown>;
  const top = fieldsOf(b, ['module', 'target', 'run', 'tasks'], ['module', 'target', 'run', 'tasks']);
  if (top !== null) return top;
  if (typeof b.module !== 'string') return words(`invalid type: ${shape(b.module)}, expected a string`);
  if (typeof b.target !== 'string') return words(`invalid type: ${shape(b.target)}, expected a string`);
  if (typeof b.run !== 'boolean') return words(`invalid type: ${shape(b.run)}, expected a boolean`);
  if (!Array.isArray(b.tasks)) return words(`invalid type: ${shape(b.tasks)}, expected a sequence`);
  const tasks: TaskConfig[] = [];
  for (const raw of b.tasks as unknown[]) {
    if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) return words(`invalid type: ${shape(raw)}, expected struct TaskConfig`);
    const t = raw as Record<string, unknown>;
    const taskFields = fieldsOf(t, ['name', 'kind', 'priority', 'watchdog_ms', 'programs'], ['name', 'kind', 'priority', 'programs']);
    if (taskFields !== null) return taskFields;
    if (typeof t.name !== 'string') return words(`invalid type: ${shape(t.name)}, expected a string`);
    if (t.kind === null || typeof t.kind !== 'object' || Array.isArray(t.kind)) return words(`invalid type: ${shape(t.kind)}, expected internally tagged enum TaskKind`);
    const k = t.kind as Record<string, unknown>;
    if (!('type' in k)) return words('missing field `type`');
    let kind: TaskConfig['kind'];
    const variants = ['cycle', 'cyclic', 'startup', 'event'];
    if (k.type === 'cycle') {
      const f = fieldsOf(k, ['type', 'min_cycle_ms'], []);
      if (f !== null) return f;
      const min = 'min_cycle_ms' in k ? unsigned(k.min_cycle_ms, 'u64') : 0;
      if (typeof min === 'string') return min;
      kind = { type: 'cycle', min_cycle_ms: min };
    } else if (k.type === 'cyclic') {
      const f = fieldsOf(k, ['type', 'interval_ms', 'phase_ms'], ['interval_ms']);
      if (f !== null) return f;
      const interval = unsigned(k.interval_ms, 'u64');
      if (typeof interval === 'string') return interval;
      const phase = 'phase_ms' in k ? unsigned(k.phase_ms, 'u64') : 0;
      if (typeof phase === 'string') return phase;
      kind = { type: 'cyclic', interval_ms: interval, phase_ms: phase };
    } else if (k.type === 'startup') {
      const f = fieldsOf(k, ['type'], []);
      if (f !== null) return f;
      kind = { type: 'startup' };
    } else if (k.type === 'event') {
      const f = fieldsOf(k, ['type', 'tag'], ['tag']);
      if (f !== null) return f;
      if (typeof k.tag !== 'string') return words(`invalid type: ${shape(k.tag)}, expected a string`);
      kind = { type: 'event', tag: k.tag };
    } else {
      return words(`unknown variant ${typeof k.type === 'string' ? `\`${k.type}\`` : shape(k.type)}, expected one of ${variants.map((v) => `\`${v}\``).join(', ')}`);
    }
    const priority = unsigned(t.priority, 'u8');
    if (typeof priority === 'string') return priority;
    let watchdog: number | undefined;
    if (t.watchdog_ms !== undefined && t.watchdog_ms !== null) {
      const w = unsigned(t.watchdog_ms, 'u64');
      if (typeof w === 'string') return w;
      watchdog = w;
    }
    if (!Array.isArray(t.programs) || t.programs.some((p) => typeof p !== 'string')) return words(`invalid type: ${shape(t.programs)}, expected a sequence of strings`);
    tasks.push({ name: t.name, kind, priority, ...(watchdog === undefined ? {} : { watchdog_ms: watchdog }), programs: t.programs as string[] });
  }
  return { module: b.module, target: b.target, run: b.run, tasks };
}

/** `check_tasks` of the protocol (crates/mythos-module/src/protocol.rs), word for word, for the cases tests use. */
function taskProblem(tasks: TaskConfig[]): string | null {
  const owner = new Map<string, string>();
  for (const [i, t] of tasks.entries()) {
    const name = t.name;
    if (!/^[A-Za-z0-9_-]{1,64}$/.test(name)) return `'${name}' is not a task name: use 1 to 64 letters, digits, '-' or '_'`;
    if (tasks.slice(0, i).some((u) => u.name === name)) return `task name '${name}' is used more than once`;
    if ((t.priority ?? 0) > 31) return `task '${name}' has priority ${t.priority}; priorities are 0 (highest) to 31`;
    if (t.kind?.type === 'cyclic' && t.kind.interval_ms === 0) return `task '${name}' has interval_ms 0; it must be at least 1`;
    if (t.kind?.type === 'event' && t.kind.tag.trim() === '') return `event task '${name}' names no tag`;
    if (t.watchdog_ms === 0) return `task '${name}': watchdog_ms must be at least 1`;
    if (t.programs.length === 0) return `task '${name}' runs no program`;
    for (const [j, p] of t.programs.entries()) {
      if (t.programs.slice(0, j).some((q) => q.toLowerCase() === p.toLowerCase())) return `task '${name}' names '${p}' twice`;
      const other = owner.get(p.toLowerCase());
      if (other !== undefined) return `program '${p}' is in tasks '${other}' and '${name}'; a program is in one task`;
      owner.set(p.toLowerCase(), name);
    }
  }
  return null;
}

/** `problems_of` of plugins/logic/src/runtimes.rs: the rules runtime `i` breaks, in order T1 to T9, each with its status. */
function runtimeProblemsOf(backend: Backend, runtimes: RuntimeEntry[], i: number, before: boolean): Array<[number, string]> {
  const r = runtimes[i] as RuntimeEntry;
  const id = r.id;
  const compared = (j: number) => j !== i && (!before || j < i);
  const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
  const found: Array<[number, string]> = [];
  const t1 = runtimeIdProblem(id);
  if (t1 !== null) found.push([400, t1]);
  if (DEVICE_NAMES.some((d) => same(d, id))) {
    found.push([400, `'${id}' cannot be a runtime id: it is a device name on Windows (CON, PRN, AUX, NUL, COM0 to COM9, LPT0 to LPT9)`]);
  }
  const twin = runtimes.find((o, j) => compared(j) && same(o.id, id));
  if (twin) found.push([409, `the project already has a runtime '${twin.id}'; names that differ only in case are one name`]);
  if (backend.programs.some((p) => same(p.entry.name, id))) found.push([409, `'${id}' is a program of the project; a runtime is not named like a program`]);
  if (r.module.trim() === '') found.push([400, `runtime '${id}' names no module`]);
  if (!['pc', 'rp2040', 'esp32'].includes(r.target)) found.push([400, `'${r.target}' is not a target profile`]);
  if (r.tasks.length === 0) {
    found.push([400, `runtime '${id}' has no task`]);
    return found;
  }
  const named: string[] = [];
  for (const t of r.tasks) {
    for (const p of t.programs) {
      const stored = backend.programs.find((e) => same(e.entry.name, p));
      if (!stored) found.push([400, `runtime '${id}': task '${t.name}' runs '${p}': the project has no program '${p}'`]);
      if (!named.some((n) => same(n, p))) named.push(stored?.entry.name ?? p);
    }
  }
  const t7 = taskProblem(r.tasks);
  if (t7 !== null) found.push([400, `runtime '${id}': ${t7}`]);
  for (const p of named) {
    const other = runtimes.find((o, j) => compared(j) && o.tasks.some((t) => t.programs.some((q) => same(q, p))));
    if (other) found.push([409, `program '${p}' is in runtime '${other.id}' already; a program is in one runtime`]);
  }
  return found;
}

/** `members` of plugins/logic/src/runtimes.rs: the project programs a runtime names, by stored entry, each once, in task order. */
function membersOf(backend: Backend, runtime: RuntimeEntry): string[] {
  const found: string[] = [];
  for (const t of runtime.tasks) {
    for (const p of t.programs) {
      const stored = backend.programs.find((e) => e.entry.name.toLowerCase() === p.toLowerCase())?.entry.name;
      if (stored !== undefined && !found.includes(stored)) found.push(stored);
    }
  }
  return found;
}

/** The PROGRAM name a program's saved source declares (`declared_name`); `null` when it declares none. */
const declaredOf = (source: string): string | null => /^\s*PROGRAM\s+([A-Za-z_][A-Za-z0-9_]*)/m.exec(source)?.[1] ?? null;

/** `put_in`: runtime `id` of `body` in place of the one of exactly that id, or after the others, its programs respelled. */
function putIn(backend: Backend, file: RuntimeEntry[], id: string, body: Omit<RuntimeEntry, 'id'>): { at: number; created: boolean } {
  const tasks = body.tasks.map((t) => ({
    ...t,
    programs: t.programs.map((p) => backend.programs.find((e) => e.entry.name.toLowerCase() === p.toLowerCase())?.entry.name ?? p),
  }));
  const entry: RuntimeEntry = { id, module: body.module, target: body.target, run: body.run, tasks };
  const at = file.findIndex((r) => r.id === id);
  if (at >= 0) {
    file[at] = entry;
    return { at, created: false };
  }
  file.push(entry);
  return { at: file.length - 1, created: true };
}

/** The runtimes of programs no configured runtime names: each alone, in one cyclic task at its interval (`implied_task`). */
function impliedOf(backend: Backend, file: RuntimeEntry[]): RuntimeEntry[] {
  const assigned = (name: string) => file.some((r) => r.tasks.some((t) => t.programs.some((p) => p.toLowerCase() === name.toLowerCase())));
  return backend.programs
    .filter((p) => !assigned(p.entry.name))
    .map(({ entry, source }) => {
      // `listed`: the task and its program are named by the PROGRAM the saved source declares.
      const declared = declaredOf(source) ?? entry.name;
      return {
        id: entry.name,
        module: entry.runtime,
        target: entry.target,
        run: entry.run,
        tasks: [{ name: declared, kind: { type: 'cyclic', interval_ms: entry.interval_ms, phase_ms: 0 }, priority: 31, programs: [declared] }],
      };
    });
}

/** The logic module's runtime routes over `backend.configured`, as `runtimes()` in plugins/logic/src/runtimes.rs answers them. */
function runtimesRoute(backend: Backend, projectId: string, method: string, id: string | undefined, action: string | undefined, body: unknown): Response {
  if (backend.configured === 'fails') {
    return refusal(500, "logic/tasks.json cannot be read as the project's runtimes: expected value at line 1 column 1");
  }
  const file = configuredOf(backend);
  const position = (runtimes: RuntimeEntry[], wanted: string) => {
    const exact = runtimes.findIndex((r) => r.id === wanted);
    return exact >= 0 ? exact : runtimes.findIndex((r) => r.id.toLowerCase() === wanted.toLowerCase());
  };
  const noRuntime = (wanted: string) => refusal(404, `the project has no runtime '${wanted}'`);
  if (id === undefined) {
    if (method !== 'GET') return refusal(404, `the logic module has no route ${method} /runtimes`);
    // `configuration_problems`: every rule the stored file breaks, each runtime against those before it.
    const problems = file.flatMap((_, i) => runtimeProblemsOf(backend, file, i, true).map(([, m]) => m));
    return Response.json({ runtimes: file, implied: impliedOf(backend, file), problems });
  }
  // The path's id is checked before the route (lib.rs), except to delete a runtime stored under it.
  const t1 = runtimeIdProblem(id);
  if (t1 !== null && !(method === 'DELETE' && action === undefined)) return refusal(400, t1);
  if (action === undefined && method === 'GET') {
    const at = position(file, id);
    if (at >= 0) return Response.json({ runtime: file[at], implied: false });
    const alone = impliedOf(backend, file);
    const there = position(alone, id);
    return there >= 0 ? Response.json({ runtime: alone[there], implied: true }) : noRuntime(id);
  }
  if (action === undefined && method === 'PUT') {
    const parsed = runtimeBodyOf(body);
    if (typeof parsed === 'string') return refusal(400, parsed);
    const { at, created } = putIn(backend, file, id, parsed);
    const first = runtimeProblemsOf(backend, file, at, false)[0];
    if (first) return refusal(first[0], first[1]);
    backend.configured = file;
    if (backend.runtimeEvents) emit(RUNTIMES_CHANGED, { project: projectId, runtime: id, what: 'saved' });
    return Response.json({ runtime: file[at], created });
  }
  if (action === undefined && method === 'DELETE') {
    const at = position(file, id);
    if (at < 0) return t1 !== null ? refusal(400, t1) : noRuntime(id);
    const [gone] = file.splice(at, 1);
    backend.configured = file;
    if (backend.runtimeEvents) emit(RUNTIMES_CHANGED, { project: projectId, runtime: gone?.id, what: 'deleted' });
    return Response.json({ deleted: gone?.id });
  }
  const checkOf = (name: string): ProgramCheck => ({
    name,
    ...(backend.checkRuntimeProgram?.(name) ?? { ok: true, diagnostics: [], binding_problems: [], version: `v-${name}`, code_size: 12, memory_size: 4 }),
  });
  if (action === 'check' && method === 'POST') {
    const draft = body === undefined || body === null || (typeof body === 'object' && Object.keys(body).length === 0) ? null : runtimeBodyOf(body);
    if (typeof draft === 'string') return refusal(400, draft);
    const at = draft === null ? position(file, id) : putIn(backend, file, id, draft).at;
    if (at < 0) {
      const alone = impliedOf(backend, file);
      const there = alone[position(alone, id)];
      if (!there) return noRuntime(id);
      const program = checkOf(there.id);
      const ok = program.ok && program.binding_problems.length === 0;
      return Response.json({ ok, runtime: there.id, programs: [program], problems: [], ...(ok ? { version: program.version } : {}) });
    }
    const runtime = file[at] as RuntimeEntry;
    const problems = runtimeProblemsOf(backend, file, at, false).map(([, m]) => m);
    const known = ['pc', 'rp2040', 'esp32'].includes(runtime.target);
    // `check_runtime`: with a target that is not a profile no program is compiled; each says why.
    const programs = membersOf(backend, runtime).map((name) =>
      known
        ? checkOf(name)
        : {
            name,
            ok: false,
            diagnostics: [{ line: 0, col: 0, end_line: 0, end_col: 0, message: `'${runtime.target}' is not a target profile` }],
            binding_problems: [],
          }
    );
    const ok = problems.length === 0 && programs.every((p) => p.ok);
    return Response.json({ ok, runtime: id, programs, problems, ...(ok ? { version: `rt-${programs.map((p) => p.version).join('-')}` } : {}) });
  }
  if (action === 'build' && method === 'POST') {
    const at = position(file, id);
    if (at < 0) return noRuntime(id);
    const runtime = file[at] as RuntimeEntry;
    const first = runtimeProblemsOf(backend, file, at, false)[0];
    if (first) return refusal(first[0], first[1]);
    const members = membersOf(backend, runtime);
    const several = members.length > 1;
    // `runtime_program`: each program named by the PROGRAM its source declares, `built_from` its
    // name in the project; `naming_problem` refuses a declared name that differs only in a
    // runtime of several programs.
    const declared = (name: string) => {
      const source = backend.programs.find((e) => e.entry.name === name)?.source ?? '';
      return declaredOf(source) ?? name;
    };
    for (const name of members) {
      if (several && declared(name).toLowerCase() !== name.toLowerCase()) {
        return refusal(
          400,
          `runtime '${runtime.id}': program '${name}' declares PROGRAM ${declared(name)} in its source; in a runtime of several programs its variables are named ${name}.<variable>, so its PROGRAM must be named ${name}`
        );
      }
    }
    const built = members.map((name) => ({ name, version: `v-${name}`, code_size: 12, memory_size: 4 }));
    const wire = members.map((name) => {
      const entry = backend.programs.find((e) => e.entry.name === name)?.entry;
      return {
        name: declared(name),
        built_from: name,
        version: `v-${name}`,
        interval_ms: entry?.interval_ms ?? 0,
        code: '00',
        symbols: [],
        inputs: entry?.inputs ?? [],
        outputs: entry?.outputs ?? [],
        published: entry?.published ?? [],
      };
    });
    // `emitted_tasks`: each task names its programs by their declared PROGRAM.
    const tasks = runtime.tasks.map((t) => ({
      ...t,
      programs: t.programs.map((p) => {
        const member = members.find((m) => m.toLowerCase() === p.toLowerCase());
        return member === undefined ? p : declared(member);
      }),
    }));
    return Response.json({
      version: `rt-${members.join('-')}`,
      warnings: backend.warnings ?? [],
      programs: built,
      replaces: members,
      instance: {
        id: runtime.id,
        module: runtime.module,
        run: runtime.run,
        tasks,
        ...(wire.length === 1 ? { program: wire[0] } : { programs: wire }),
      },
    });
  }
  return refusal(404, `the logic module has no route ${method} /runtimes/:runtime${action ? `/${action}` : ''}`);
}

/** A controller with `backend`'s programs. Returns the calls made to it, in order. */
export function server(backend: Backend): Call[] {
  const calls: Call[] = [];
  let builds = 0;
  if (backend.active !== undefined) useController.setState({ activeProjectId: backend.active });
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      const method = init?.method ?? 'GET';
      const body = typeof init?.body === 'string' ? (JSON.parse(init.body) as unknown) : undefined;
      const call = { method, path, body };
      calls.push(call);
      const intercepted = backend.intercept?.(call);
      if (intercepted) return intercepted;

      if (path.endsWith('/api/v1/runtimes')) {
        if (backend.runtimes === 'hangs') return new Promise<Response>(() => undefined);
        return backend.runtimes === 'fails' || backend.runtimes === undefined
          ? failure(500, 'the runtimes cannot be read')
          : Response.json(backend.runtimes);
      }
      const commanded = /\/api\/v1\/runtimes\/([^/]+)\/(run|stop)$/.exec(path);
      if (commanded) {
        // `command` (crates/mythos-core/src/api/runtimes.rs): only a runtime the core lists.
        const id = decodeURIComponent(commanded[1] ?? '');
        const listed = typeof backend.runtimes !== 'object' || backend.runtimes.runtimes.some((r) => r.id === id);
        if (!listed) return refusal(404, `no runtime '${id}' is loaded on this node`);
        return Response.json({ id, asked: commanded[2] });
      }
      if (/\/api\/v1\/runtimes\/[^/]+\/(force|release)$/.test(path)) {
        return Response.json({ id: path.split('/').at(-2), state: { mode: 'run', forced: [] } });
      }
      if (/\/api\/v1\/runtimes\/[^/]+\/watch$/.test(path)) {
        if (backend.watch === 'hangs') return new Promise<Response>(() => undefined);
        return typeof backend.watch === 'function'
          ? Response.json(backend.watch((body as { variables: string[] }).variables))
          : new Response('{"error":"the runtime\'s module does not answer"}', { status: 502, statusText: 'Bad Gateway' });
      }
      const coreRuntimes = /\/projects\/[^/]+\/runtimes(?:\?(.*))?$/.exec(path);
      if (coreRuntimes) {
        if (method === 'GET') return Response.json({ config: backend.config ?? null, revision: revisionOf(backend.config) });
        // The query, as `LoadQuery::from_pairs` (crates/mythos-core/src/api/runtimes.rs) takes it,
        // word for word: a key it does not take, or one given twice, is refused, never ignored.
        const given = new Map<string, string>();
        for (const [key, value] of new URLSearchParams(coreRuntimes[1] ?? '')) {
          if (key !== 'start' && key !== 'revision') {
            return refusal(400, `'${key}' is not something this save takes, so nothing was saved: it takes start and revision`);
          }
          if (given.has(key)) return refusal(400, `'${key}' is given more than once, so nothing was saved: give it once`);
          given.set(key, value);
        }
        const start = given.get('start');
        if (start !== undefined && start !== 'cold') {
          return refusal(
            400,
            `start=${start} is not a way to start: 'cold' starts every program it loads from its initial values; leave it out to change a running program online`
          );
        }
        const sent = body as NonNullable<Backend['config']>;
        const instances = sent.instances ?? [];
        // `RuntimesConfig::validate` (crates/mythos-core/src/runtimes.rs), word for word, for the
        // rules modelled here: the id, an id used twice, the module, `program` and `programs`
        // together, one program in one runtime (only between programs that both say which project
        // program they are), and one writer and one publisher of a tag. Not modelled: the
        // instance's own task, data and binding rules (`check_tasks`, `check_data`).
        const seen = new Map<string, [string, string]>();
        const ids = new Set<string>();
        const outputs = new Map<string, string>();
        const publishers = new Map<string, string>();
        for (const instance of instances) {
          const id = String(instance.id);
          if (runtimeIdProblem(id) !== null) return refusal(400, runtimeIdProblem(id) as string);
          if (ids.has(id)) return refusal(400, `runtime id '${id}' is used more than once`);
          ids.add(id);
          if (typeof instance.module !== 'string' || instance.module.trim() === '') return refusal(400, `runtime '${id}' names no module`);
          if (instance.program && Array.isArray(instance.programs) && instance.programs.length > 0) {
            return refusal(400, `runtime '${id}': give program or programs, not both`);
          }
          const programs = (instance.program ? [instance.program] : ((instance.programs as unknown[] | undefined) ?? [])) as Array<{
            name?: string;
            built_from?: string;
            outputs?: Array<{ tag: string }>;
            published?: Array<{ tag: string }>;
          }>;
          for (const program of programs) {
            const owner = programs.length > 1 ? `${id}/${String(program.name)}` : id;
            for (const o of program.outputs ?? []) {
              const other = outputs.get(o.tag);
              if (other !== undefined) return refusal(400, `${o.tag} is an output of both '${other}' and '${owner}': one tag has one writer`);
              outputs.set(o.tag, owner);
            }
            for (const b of program.published ?? []) {
              const other = publishers.get(b.tag);
              if (other !== undefined) return refusal(400, `${b.tag} is published by both '${other}' and '${owner}'`);
              publishers.set(b.tag, owner);
            }
          }
          for (const program of programs) {
            if (typeof program.built_from !== 'string') continue;
            const first = seen.get(program.built_from.toLowerCase());
            if (first) {
              return refusal(
                400,
                `program '${first[0]}' of the project is in runtimes '${first[1]}' and '${String(instance.id)}'; a program runs in one runtime: take it out of one of them in the same save`
              );
            }
            seen.set(program.built_from.toLowerCase(), [program.built_from, String(instance.id)]);
          }
        }
        // `put_config`'s precondition, in its words: a save with a revision the configuration no
        // longer has is refused before the module checks, and nothing is saved.
        const expected = given.get('revision');
        const now = revisionOf(backend.config);
        if (expected !== undefined && expected !== now) {
          return refusal(
            409,
            `the project's runtimes have changed since they were read (revision ${expected}, now ${now}), so nothing was saved: read them again`
          );
        }
        const problems = backend.configProblems?.(instances) ?? [];
        if (problems.length > 0) {
          return Response.json(
            { error: 'the runtimes configuration has problems and was not saved', problems },
            { status: 400, statusText: 'Bad Request' }
          );
        }
        backend.config = sent;
        const saving = decodeURIComponent(/\/projects\/([^/]+)\/runtimes/.exec(path)?.[1] ?? '');
        const applied = backend.applied ?? (backend.active === undefined || backend.active === saving);
        const cold = start === 'cold';
        return Response.json({
          config: backend.config,
          revision: revisionOf(backend.config),
          applied,
          changes:
            applied && !backend.fieldIoOff
              ? { started: [], failed: backend.failedToStart ?? [], runtimes: backend.starts?.(instances, cold) ?? [] }
              : null,
        });
      }

      const library = /^\/api\/v1\/projects\/[^/]+\/logic\/library(?:\/([^/]+))?$/.exec(path);
      if (library) {
        const param = library[1] === undefined ? undefined : decodeURIComponent(library[1]);
        // The core decodes the path before it routes (crates/mythos-core/src/services.rs
        // `forward`): an encoded separator is a separator there, and such a name never reaches
        // the module. These are the core's words, not the module's name rule.
        if (param !== undefined && param.split('/').some((segment) => segment === '.' || segment === '..' || segment.includes('\\'))) {
          return refusal(400, "a path may not contain '.', '..' or '\\' segments");
        }
        if (param?.includes('/')) return refusal(404, `module 'logic' serves no ${method} /library/${param}`);
        return libraryRoute(backend, method, param, body);
      }

      const runtimes = /^\/api\/v1\/projects\/([^/]+)\/logic\/runtimes(?:\/([^/]+))?(?:\/(check|build))?$/.exec(path);
      if (runtimes) {
        const id = runtimes[2] === undefined ? undefined : decodeURIComponent(runtimes[2]);
        return runtimesRoute(backend, decodeURIComponent(runtimes[1] ?? ''), method, id, runtimes[3], body);
      }
      const logic = /\/projects\/[^/]+\/logic\/programs(?:\/([^/]+))?(?:\/(source|settings|check|build))?$/.exec(path);
      if (!logic) return failure(404, `no route ${method} ${path}`);
      const [, name, action] = logic;
      if (!name) {
        if (method === 'GET') return Response.json({ programs: backend.programs.map((p) => p.entry) });
        const asked = body as { name: string; language?: string };
        const made =
          asked.language === 'ld'
            ? { entry: entryOf(asked.name, { language: 'ld' }), source: ladderOf(asked.name, '') }
            : asked.language === 'fbd'
              ? { entry: entryOf(asked.name, { language: 'fbd' }), source: fbdOf(asked.name, '') }
              : asked.language === 'sfc'
                ? { entry: entryOf(asked.name, { language: 'sfc' }), source: sfcOf(asked.name) }
                : { entry: entryOf(asked.name), source: `PROGRAM ${asked.name}\nEND_PROGRAM\n` };
        backend.programs.push(made);
        return Response.json({ program: made.entry, source: made.source }, { status: 201 });
      }
      const program = backend.programs.find((p) => p.entry.name === name);
      if (!program) return failure(404, `the project has no program '${name}'`);
      if (!action) {
        if (method === 'DELETE') {
          backend.programs = backend.programs.filter((p) => p !== program);
          return Response.json({ deleted: name });
        }
        return Response.json({ program: program.entry, source: program.source });
      }
      if (action === 'source') {
        program.source = (body as { source: string }).source;
        return Response.json({ saved: true });
      }
      if (action === 'settings') {
        program.entry = { ...program.entry, ...(body as Partial<ProgramEntry>) };
        return Response.json({ program: program.entry });
      }
      if (action === 'check') {
        const source = (body as { source?: string } | undefined)?.source ?? program.source;
        return Response.json(
          backend.check?.(source) ?? { ok: true, diagnostics: [], variables: [], code_size: 12, memory_size: 4, binding_problems: [] }
        );
      }
      builds += 1;
      return Response.json({
        version: `build${builds}`,
        warnings: backend.warnings ?? [],
        code_size: 12,
        memory_size: 4,
        instance: { id: name, module: program.entry.runtime, run: program.entry.run, program: { name, source: program.source } },
      });
    })
  );
  return calls;
}

/** Let what was started (requests, their state changes) finish, inside `act`. */
export async function settle(): Promise<void> {
  for (let turn = 0; turn < 4; turn += 1) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

/** How much longer a wait may take before it counts as hung: see `frontend/vitest.config.ts`. */
export const timeScale = Number(process.env.MYTHOS_TEST_TIME_SCALE ?? '1');

/**
 * Let the event loop turn until `holds` is true, and fail naming `what` when it never is.
 * `settle` turns it a fixed four times, which is enough on a quiet machine; a response body is
 * read through a stream, and on a loaded CI runner that takes more turns than timers do.
 */
export async function until(what: string, holds: () => unknown, withinMs = 10_000 * timeScale): Promise<void> {
  const deadline = Date.now() + withinMs;
  while (!holds()) {
    if (Date.now() > deadline) throw new Error(`never: ${what}`);
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 5));
    });
  }
}

/** Run something that changes state, and let it finish. */
export async function doing(action: () => unknown): Promise<void> {
  await act(async () => {
    await action();
  });
  await settle();
}

/** Type into the source the way a keystroke does, so React sees the change. */
export function typeSource(container: HTMLElement, value: string): void {
  const area = container.querySelector('textarea');
  if (!(area instanceof HTMLTextAreaElement)) throw new Error('no source editor');
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
  act(() => {
    setter?.call(area, value);
    area.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

/**
 * End a test: unmount what it rendered, take its controller away, and leave a `fetch` that
 * refuses at once. A test that timed out goes on running after it is reported; without this it
 * would reach the real network (happy-dom's localhost:3000), where a connection can hang for
 * minutes on a CI runner, and every test after it in the file would fail with it.
 */
export function endTest(): void {
  unmountAll();
  vi.unstubAllGlobals();
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => {
      throw new Error('no controller: the test that asked has ended');
    })
  );
}

export function click(element: Element | null): void {
  if (!(element instanceof HTMLElement)) throw new Error('nothing to click');
  act(() => element.click());
}

export const byTestId = (container: HTMLElement, id: string) => container.querySelector(`[data-testid="${id}"]`);
export const byLabel = (container: HTMLElement, label: string) => container.querySelector(`[aria-label="${label}"]`);
export const button = (container: HTMLElement, text: string) =>
  [...container.querySelectorAll('button')].find((b) => (b.textContent ?? '').trim() === text) ?? null;

/**
 * Watch answers each view must not draw as current, beside the ones just inside each bound
 * (ADR-013 §2.8), in an order a view goes through: `runtimeOf`'s interval is 100 ms and
 * `watchOf`'s longest scan 19 µs, so a scan is on time within 200.019 ms; an answer is current
 * within its `fresh_ms` (1000) and one interval of asking (500 ms).
 */
export const CURRENCY_CASES: Array<{ label: string; ages: Ages; current: boolean; why?: string }> = [
  { label: 'a scan just inside two intervals and the longest scan', ages: { scan_age_us: 200_019 }, current: true },
  {
    label: 'the scan counter held and its age just past that: a stalled scan thread',
    ages: { scan_age_us: 200_020 },
    current: false,
    why: 'completed no scan for 200.02 ms, past the 200.019 ms',
  },
  { label: 'the counter on again, a scan just completed', ages: { scans: 43, scan_age_us: 1_000 }, current: true },
  { label: 'a read just past its freshness and one interval of asking', ages: { age_ms: 1501 }, current: false, why: 'no newer answer has come' },
  { label: 'a read of a second ago', ages: { age_ms: 1000 }, current: true },
  { label: 'an answer that does not say its age', ages: { age_ms: undefined }, current: false, why: 'does not say how old these values are' },
  { label: 'nor for how long it is current', ages: { fresh_ms: undefined }, current: false, why: 'does not say for how long' },
  {
    label: 'nor when the program last completed a scan',
    ages: { scan_age_us: undefined },
    current: false,
    why: 'does not say when the program last completed a scan',
  },
  { label: 'a scan age of null, as nothing known', ages: { scan_age_us: null }, current: false, why: 'does not say when the program last completed a scan' },
  { label: 'nor how long its longest scan took', ages: { max_scan_us: undefined }, current: false, why: 'longest scan' },
];

/** Change what a watch of `name` answers, and read it now with what was asked last. */
export async function answerNext(backend: Backend, calls: Call[], name: string, watch: Backend['watch']): Promise<void> {
  backend.watch = watch;
  const asked = calls.filter((call) => call.path.endsWith(`/runtimes/${name}/watch`)).at(-1);
  if (!asked) throw new Error(`${name} was never watched`);
  const { variables } = asked.body as { variables: string[] };
  await act(async () => {
    await usePrograms.getState().readWatch(name, variables);
  });
  await settle();
}
