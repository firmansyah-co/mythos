// The logic UI under test runs against the workbench's real host (`createHost`), so what is
// proven here is the module as the workbench activates it, not against a stand-in. The
// controller is a fake `fetch` that keeps programs and runtimes the way the logic module and
// the core do, and records every call.

import { act } from 'react';
import { vi } from 'vitest';
import { activate, MODULE_ID } from '../entry';
import type { CheckResult, ProgramEntry, RuntimesAnswer, RuntimeStatus, WatchAnswer, WatchedValue } from '../bridge';
import { createHost } from '../../../ui-shell/src/modules/host';
import { resetStores as resetShell } from '../../../ui-shell/src/__tests__/harness';

export {
  connection,
  ioStatus,
  project,
  render,
  sample,
  texts,
  typeInto,
  unmountAll,
} from '../../../ui-shell/src/__tests__/harness';
export { useController } from '../../../ui-shell/src/data/controller';
export { useSession } from '../../../ui-shell/src/data/session';
export { useTelemetry } from '../../../ui-shell/src/data/telemetry';
export { useWorkbench as useShell } from '../../../ui-shell/src/data/workbench';

let deactivate: (() => void) | null = null;

/** Every store as the workbench starts, and the logic UI activated against it afresh. */
export function resetStores(): void {
  deactivate?.();
  resetShell();
  deactivate = activate(createHost(MODULE_ID));
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
  /** `GET /api/v1/runtimes`; `'fails'` answers 500. */
  runtimes?: RuntimesAnswer | 'fails';
  /** The project's saved runtimes. */
  config?: { instances: Array<Record<string, unknown>> } | null;
  /** Whether saving the runtimes applies them (the project is the active one). */
  applied?: boolean;
  warnings?: string[];
  /** What a watch of a runtime's variables answers; `'fails'`, or left out, answers 502. */
  watch?: ((variables: string[]) => WatchAnswer) | 'fails';
  /** Answer a call before the fake does, to make one fail. */
  intercept?: (call: Call) => Response | undefined;
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

/** A watch's answer: `values` by variable name, from a program in `mode`. */
export const watchOf =
  (values: Record<string, Partial<WatchedValue>>, mode = 'run') =>
  (variables: string[]): WatchAnswer => ({
    id: 'Tank',
    state: { mode, program: 'Tank', version: 'aaaaaaaaaaaa', scans: 42 },
    values: variables.map((variable) => ({
      variable,
      type: null,
      value: null,
      error: `the program has no variable ${variable}`,
      ...(values[variable] ? { error: null, ...values[variable] } : {}),
    })),
    age_ms: 12,
  });

const failure = (status: number, error: string) =>
  new Response(JSON.stringify({ error }), { status, statusText: status === 500 ? 'Internal Server Error' : 'Bad Request' });

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
        return backend.runtimes === 'fails' || backend.runtimes === undefined
          ? failure(500, 'the runtimes cannot be read')
          : Response.json(backend.runtimes);
      }
      if (/\/api\/v1\/runtimes\/[^/]+\/(run|stop)$/.test(path)) return Response.json({});
      if (/\/api\/v1\/runtimes\/[^/]+\/(force|release)$/.test(path)) {
        return Response.json({ id: path.split('/').at(-2), state: { mode: 'run', forced: [] } });
      }
      if (/\/api\/v1\/runtimes\/[^/]+\/watch$/.test(path)) {
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

      const logic = /\/projects\/[^/]+\/logic\/programs(?:\/([^/]+))?(?:\/(source|settings|check|build))?$/.exec(path);
      if (!logic) return failure(404, `no route ${method} ${path}`);
      const [, name, action] = logic;
      if (!name) {
        if (method === 'GET') return Response.json({ programs: backend.programs.map((p) => p.entry) });
        const made = { entry: entryOf((body as { name: string }).name), source: `PROGRAM ${(body as { name: string }).name}\nEND_PROGRAM\n` };
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

export function click(element: Element | null): void {
  if (!(element instanceof HTMLElement)) throw new Error('nothing to click');
  act(() => element.click());
}

export const byTestId = (container: HTMLElement, id: string) => container.querySelector(`[data-testid="${id}"]`);
export const byLabel = (container: HTMLElement, label: string) => container.querySelector(`[aria-label="${label}"]`);
export const button = (container: HTMLElement, text: string) =>
  [...container.querySelectorAll('button')].find((b) => (b.textContent ?? '').trim() === text) ?? null;
