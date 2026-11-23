// The logic UI under test runs against the workbench's real host (`createHost`), so what is
// proven here is the module as the workbench activates it, not against a stand-in. The
// controller is a fake `fetch` that keeps programs and runtimes the way the logic module and
// the core do, and the library's five routes literally as plugins/logic/src/lib.rs answers them
// (statuses, bodies, the stored spelling, the name rule's words), with a stand-in for the
// compiler that a test sets; it records every call. The stream's events are sent by the test
// through `emit`: the one part of the host replaced, since happy-dom has no event stream.

import { act, type ReactElement } from 'react';
import { vi } from 'vitest';
import { activate, MODULE_ID } from '../entry';
import { useLdPalette } from '../LdInstructions';
import { usePrograms } from '../programs';
import type { CheckResult, Diagnostic, ProgramEntry, RuntimesAnswer, RuntimeStatus, WatchAnswer, WatchedValue } from '../bridge';
import type { LibraryCheck } from '../library';
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
export { useController } from '../../../ui-shell/src/data/controller';
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
   * The runtimes configured in the logic module (`GET .../logic/runtimes`), each with the
   * programs its one task runs; left out, none. `'fails'` answers 500.
   */
  configured?: Array<{ id: string; programs: string[] }> | 'fails';
  /** Whether saving the runtimes applies them (the project is the active one). */
  applied?: boolean;
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
        return [{ line: 0, col: 0, end_line: 0, end_col: 0, message: `${file} cannot be read as a library source: ${text?.unreadable}`, source: file }];
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

/** A controller with `backend`'s programs. Returns the calls made to it, in order. */
export function server(backend: Backend): Call[] {
  const calls: Call[] = [];
  let builds = 0;
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
      if (/\/api\/v1\/runtimes\/[^/]+\/(run|stop)$/.test(path)) return Response.json({});
      if (/\/api\/v1\/runtimes\/[^/]+\/(force|release)$/.test(path)) {
        return Response.json({ id: path.split('/').at(-2), state: { mode: 'run', forced: [] } });
      }
      if (/\/api\/v1\/runtimes\/[^/]+\/watch$/.test(path)) {
        if (backend.watch === 'hangs') return new Promise<Response>(() => undefined);
        return typeof backend.watch === 'function'
          ? Response.json(backend.watch((body as { variables: string[] }).variables))
          : new Response('{"error":"the runtime\'s module does not answer"}', { status: 502, statusText: 'Bad Gateway' });
      }
      if (/\/projects\/[^/]+\/runtimes(\?start=cold)?$/.test(path)) {
        if (method === 'GET') return Response.json({ config: backend.config ?? null });
        backend.config = body as Backend['config'];
        const applied = backend.applied ?? true;
        return Response.json({ config: backend.config, applied, changes: applied ? { started: [] } : null });
      }

      const library = /^\/api\/v1\/projects\/[^/]+\/logic\/library(?:\/([^/]+))?$/.exec(path);
      if (library) return libraryRoute(backend, method, library[1] === undefined ? undefined : decodeURIComponent(library[1]), body);

      if (/\/projects\/[^/]+\/logic\/runtimes$/.test(path) && method === 'GET') {
        if (backend.configured === 'fails') {
          return failure(500, "logic/tasks.json cannot be read as the project's runtimes: expected value at line 1 column 1");
        }
        return Response.json({
          runtimes: (backend.configured ?? []).map(({ id, programs }) => ({
            id,
            module: 'vm',
            target: 'pc',
            run: true,
            tasks: [{ name: 't', kind: { type: 'cyclic', interval_ms: 10, phase_ms: 0 }, priority: 1, programs }],
          })),
          implied: [],
          problems: [],
        });
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
