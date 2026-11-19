// The logic UI under test runs against the workbench's real host (`createHost`), so what is
// proven here is the module as the workbench activates it, not against a stand-in. The
// controller is a fake `fetch` that keeps programs and runtimes the way the logic module and
// the core do, and records every call.

import { act, type ReactElement } from 'react';
import { vi } from 'vitest';
import { activate, MODULE_ID } from '../entry';
import { useLdPalette } from '../LdInstructions';
import type { CheckResult, ProgramEntry, RuntimesAnswer, RuntimeStatus, WatchAnswer, WatchedValue } from '../bridge';
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

let deactivate: (() => void) | null = null;

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
        const asked = body as { name: string; language?: string };
        const made =
          asked.language === 'ld'
            ? { entry: entryOf(asked.name, { language: 'ld' }), source: ladderOf(asked.name, '') }
            : asked.language === 'fbd'
              ? { entry: entryOf(asked.name, { language: 'fbd' }), source: fbdOf(asked.name, '') }
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
