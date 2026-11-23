// The data UI under test runs against the workbench's real host (`createHost`), so what is
// proven here is the module as the workbench activates it. The controller is a fake `fetch`
// that keeps types and blocks and answers the data module's routes literally as
// plugins/data/src/lib.rs does (statuses, bodies, the stored spelling, `changed`), with a
// stand-in for the compiler that a test sets. It records every call. The stream's events are
// sent by the test through `emit`: the one part of the host replaced, since happy-dom has no
// event stream; every other part of the host is the workbench's own.

import { act, type ReactElement } from 'react';
import { vi } from 'vitest';
import type { Panel } from '@mythos/workbench-sdk';
import { activate, MODULE_ID } from '../entry';
import type { BlockAnswer, Diagnostic, MemberAnswer, RuntimeInstance } from '../bridge';
import { createHost } from '../../../ui-shell/src/modules/host';
import { render as renderShell, resetStores as resetShell, unmountAll } from '../../../ui-shell/src/__tests__/harness';
import { ModuleMount } from '../../../ui-shell/src/modules/ModuleMount';
import { useModuleUi, type Owned } from '../../../ui-shell/src/modules/registry';
import { useWorkbench } from '../../../ui-shell/src/data/workbench';

export { project, texts, unmountAll } from '../../../ui-shell/src/__tests__/harness';
export { useSession } from '../../../ui-shell/src/data/session';
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

function PaneOf({ pane, panels }: { pane: string; panels: Record<string, Owned<Panel>> }) {
  const project = useWorkbench((state) => state.project) ?? { id: 'no-project', name: 'no project' };
  return (
    <>
      {Object.entries(panels).map(([owner, panel]) => (
        <section key={owner} data-pane={pane} data-owner={owner} aria-label={panel.item.title}>
          <ModuleMount mount={panel.item.mount} context={{ project: { id: project.id, name: project.name } }} deps={[project.id, owner]} />
        </section>
      ))}
    </>
  );
}

function Panes() {
  const ui = useModuleUi();
  return (
    <>
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
export function paneOf(container: HTMLElement, pane: 'inspector' | 'diagnostics', owner: string): HTMLElement | null {
  return container.querySelector(`[data-pane="${pane}"][data-owner="${owner}"]`);
}

/** Every store as the workbench starts, and the data UI activated against it afresh. */
export function resetStores(): void {
  deactivate?.();
  resetShell();
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

/** What the stand-in compiler makes of the state: the layouts of the blocks, or why it does not check. */
export type Compiled =
  | { ok: true; layouts: Record<string, { layout: string; size: number; members: MemberAnswer[] }> }
  | { ok: false; diagnostics: Diagnostic[] };

export interface Backend {
  /** The stored types by stem: their text, or `{ unreadable }`, a file that is not UTF-8. */
  types: Record<string, string | { unreadable: string }>;
  blocks: Array<{ name: string; type: string; retain: boolean }>;
  /** The compiler: by default every state checks, each block laid out by its type's text. */
  compile?: (types: Record<string, string>, blocks: Backend['blocks']) => Compiled;
  /** The module's one-declaration rule: by default every source declares its one type. */
  declarationProblem?: (name: string, source: string) => string | null;
  /** What the file holds after a save of `source`: by default `source`. */
  stores?: (source: string) => string;
  /** The project's runtimes configuration. `'fails'` answers 500. */
  config?: { instances: RuntimeInstance[] } | null | 'fails';
  /** Answer a call before the fake does: a response, or one that comes later. */
  intercept?: (call: Call) => Response | Promise<Response> | undefined;
}

/** A layout of 16 hex digits made from `text`, as stable as the compiler's. */
export function layoutOf(text: string): string {
  let h = 0x811c9dc5;
  for (const c of text) h = Math.imul(h ^ c.charCodeAt(0), 0x01000193) >>> 0;
  return h.toString(16).padStart(8, '0').repeat(2);
}

const compilesAll = (types: Record<string, string>, blocks: Backend['blocks']): Compiled => ({
  ok: true,
  layouts: Object.fromEntries(
    blocks.map((b) => {
      const text = Object.entries(types).find(([stem]) => stem.toUpperCase() === b.type.toUpperCase())?.[1] ?? '';
      return [b.name, { layout: layoutOf(text), size: 4, members: [{ name: 'X', type: 'INT', offset: 0, size: 2 }] }];
    })
  ),
});

const error = (status: number, message: string) =>
  Response.json({ error: message }, { status, statusText: STATUS[status] ?? '' });
const refused = (status: number, message: string, diagnostics: Diagnostic[]) =>
  Response.json({ error: message, diagnostics }, { status, statusText: STATUS[status] ?? '' });
const STATUS: Record<number, string> = { 400: 'Bad Request', 404: 'Not Found', 409: 'Conflict', 500: 'Internal Server Error' };

/** The module's `type_name_problem` (plugins/data/src/lib.rs), its message word for word; device names left out. */
function typeNameProblem(name: string): string | null {
  const identifier = /^[A-Za-z_][A-Za-z0-9_]*$/.test(name) && !name.includes('__') && !name.endsWith('_') && name.length <= 64;
  return identifier
    ? null
    : `'${name}' is not a type name: a letter or '_' first, then letters, digits and single underscores, not ending in '_', at most 64`;
}

/** A diagnostic of the stand-in compiler, about `source` (`data/types/<Name>.st`) at `line`:`col`. */
export const diagnostic = (message: string, line = 1, col = 1, source: string | null = null): Diagnostic => ({
  line,
  col,
  end_line: line,
  end_col: col,
  message,
  ...(source === null ? {} : { source }),
});

/** A controller with the data module serving `backend`. Returns the calls made to it, in order. */
export function server(backend: Backend): Call[] {
  const calls: Call[] = [];
  const stems = () => Object.keys(backend.types).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  const findStem = (name: string) => stems().find((s) => s === name) ?? stems().find((s) => s.toUpperCase() === name.toUpperCase());
  const readable = (types: Backend['types']) =>
    Object.fromEntries(Object.entries(types).filter((e): e is [string, string] => typeof e[1] === 'string'));
  const compile = (types: Backend['types'], blocks: Backend['blocks']) => (backend.compile ?? compilesAll)(readable(types), blocks);
  const blocksJson = (blocks: Backend['blocks'], compiled: Compiled): BlockAnswer[] =>
    blocks.map((b) => {
      const laid = compiled.ok ? compiled.layouts[b.name] : undefined;
      return laid ? { name: b.name, type: b.type, retain: b.retain, ...laid } : { name: b.name, type: b.type, retain: b.retain };
    });
  const changed = (before: Compiled, after: Compiled) => {
    if (!before.ok || !after.ok) return null;
    return Object.entries(after.layouts)
      .filter(([name, laid]) => before.layouts[name]?.layout !== laid.layout)
      .map(([name]) => name);
  };

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

      if (/^\/api\/v1\/projects\/[^/]+\/runtimes$/.test(path) && method === 'GET') {
        if (backend.config === 'fails') return error(500, 'runtimes.json cannot be read: expected value at line 1 column 1');
        return Response.json({ config: backend.config ?? null });
      }
      const route = /^\/api\/v1\/projects\/[^/]+\/data\/(types|blocks|check)(?:\/([^/]+))?$/.exec(path);
      if (!route) return error(404, `no route ${method} ${path}`);
      const [, kind, encoded] = route;
      const param = encoded === undefined ? undefined : decodeURIComponent(encoded);

      // A type's name becomes a file name: the module refuses any other before anything.
      if (kind === 'types' && param !== undefined) {
        const problem = typeNameProblem(param);
        if (problem !== null) return error(400, problem);
      }
      if (kind === 'types' && param === undefined && method === 'GET') {
        return Response.json({ types: stems().map((name) => ({ name })) });
      }
      if (kind === 'types' && param !== undefined) {
        if (method === 'GET') {
          const stem = findStem(param);
          if (stem === undefined) return error(404, `the project has no type '${param}'`);
          const stored = backend.types[stem]!;
          if (typeof stored !== 'string') return error(500, `/projects/p/data/types/${stem}.st: ${stored.unreadable}`);
          return Response.json({ name: stem, source: stored });
        }
        if (method === 'PUT') {
          const source = (body as { source?: unknown } | undefined)?.source;
          if (typeof source !== 'string') return error(400, 'the body is not what this route takes: missing field `source`');
          const before = compile(backend.types, backend.blocks);
          const existing = findStem(param);
          if (existing !== undefined && existing !== param) {
            return error(409, `the project already has a type '${existing}'; names that differ only in case are one name`);
          }
          const next = { ...backend.types, [param]: source };
          const after = compile(next, backend.blocks);
          if (!after.ok) {
            return refused(400, "with this source the project's types and data blocks do not check, and nothing was saved", after.diagnostics);
          }
          const problem = backend.declarationProblem?.(param, source) ?? null;
          if (problem !== null) return error(400, problem);
          backend.types = { ...next, [param]: backend.stores?.(source) ?? source };
          return Response.json({ name: param, created: existing === undefined, blocks: blocksJson(backend.blocks, after), changed: changed(before, after) });
        }
        if (method === 'DELETE') {
          const stem = findStem(param);
          if (stem === undefined) return error(404, `the project has no type '${param}'`);
          const users = backend.blocks.filter((b) => b.type.toUpperCase() === stem.toUpperCase()).map((b) => `'${b.name}'`);
          if (users.length > 0) {
            return error(409, `type '${stem}' is the type of data block ${users.join(', ')}: delete those blocks or give them another type first`);
          }
          const { [stem]: _gone, ...rest } = backend.types;
          const after = compile(rest, backend.blocks);
          if (!after.ok) {
            return refused(400, `without type '${stem}' the project's types and data blocks do not check, and it was not deleted`, after.diagnostics);
          }
          backend.types = rest;
          return Response.json({ deleted: stem, blocks: blocksJson(backend.blocks, after) });
        }
      }
      if (kind === 'blocks' && param === undefined && method === 'GET') {
        const compiled = compile(backend.types, backend.blocks);
        return Response.json({
          ok: compiled.ok,
          diagnostics: compiled.ok ? [] : compiled.diagnostics,
          blocks: blocksJson(backend.blocks, compiled),
        });
      }
      if (kind === 'blocks' && param !== undefined) {
        const at = backend.blocks.findIndex((b) => b.name.toUpperCase() === param.toUpperCase());
        if (method === 'PUT') {
          const given = body as { type?: unknown; retain?: unknown } | undefined;
          const keys = Object.keys(given ?? {});
          if (typeof given?.type !== 'string' || typeof given.retain !== 'boolean' || keys.some((k) => k !== 'type' && k !== 'retain')) {
            return error(400, 'the body is not what this route takes: expected `type` and `retain`');
          }
          if (at >= 0 && backend.blocks[at]!.name !== param) {
            return error(409, `the project already has a data block '${backend.blocks[at]!.name}'; names that differ only in case are one name`);
          }
          const before = compile(backend.types, backend.blocks);
          const entry = { name: param, type: given.type, retain: given.retain };
          const next = at >= 0 ? backend.blocks.map((b, k) => (k === at ? entry : b)) : [...backend.blocks, entry];
          const after = compile(backend.types, next);
          if (!after.ok) return refused(400, `data block '${param}' does not check and was not saved`, after.diagnostics);
          backend.blocks = next;
          return Response.json({ created: at < 0, blocks: blocksJson(next, after), changed: changed(before, after) });
        }
        if (method === 'DELETE') {
          if (at < 0) return error(404, `the project has no data block '${param}'`);
          const stored = backend.blocks[at]!.name;
          const next = backend.blocks.filter((_, k) => k !== at);
          const after = compile(backend.types, next);
          if (!after.ok) {
            return refused(400, `without data block '${stored}' the project's types and data blocks do not check, and it was not deleted`, after.diagnostics);
          }
          backend.blocks = next;
          return Response.json({ deleted: stored, blocks: blocksJson(next, after) });
        }
      }
      if (kind === 'check' && param === undefined && method === 'POST') {
        const given = (body ?? {}) as { type?: string; source?: string };
        let types = backend.types;
        let extra: string | null = null;
        if (given.type !== undefined && given.source !== undefined) {
          const problem = typeNameProblem(given.type);
          if (problem !== null) return error(400, problem);
          const existing = findStem(given.type);
          if (existing !== undefined && existing !== given.type) {
            return error(409, `the project already has a type '${existing}'; names that differ only in case are one name`);
          }
          extra = backend.declarationProblem?.(given.type, given.source) ?? null;
          types = { ...types, [given.type]: given.source };
        } else if (given.type !== undefined || given.source !== undefined) {
          return error(400, 'give both type and source, or neither');
        }
        const compiled = compile(types, backend.blocks);
        // The compiler's diagnostics win; the one-declaration rule is said of a source that checks.
        const diagnostics = compiled.ok ? (extra === null ? [] : [diagnostic(extra, 0, 0)]) : compiled.diagnostics;
        const ok = compiled.ok && extra === null;
        return Response.json({ ok, diagnostics, blocks: blocksJson(backend.blocks, ok ? compiled : { ok: false, diagnostics }) });
      }
      return error(404, `the data module has no route ${method} ${path}`);
    })
  );
  return calls;
}

/** Let what was started (requests, their state changes) finish, inside `act`. */
export async function settle(): Promise<void> {
  for (let turn = 0; turn < 6; turn += 1) {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }
}

/** How much longer a wait may take before it counts as hung: see `frontend/vitest.config.ts`. */
export const timeScale = Number(process.env.MYTHOS_TEST_TIME_SCALE ?? '1');

/** Let the event loop turn until `holds` is true, and fail naming `what` when it never is. */
export async function until(what: string, holds: () => unknown, withinMs = 10_000 * timeScale): Promise<void> {
  const deadline = Date.now() + withinMs;
  while (!holds()) {
    if (Date.now() > deadline) throw new Error(`never: ${what}`);
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 5));
    });
  }
}

/** Type into the source the way a keystroke does, so React sees the change. */
export function typeSource(container: HTMLElement, value: string): void {
  const area = container.querySelector('[data-testid="type-source"]');
  if (!(area instanceof HTMLTextAreaElement)) throw new Error('no source editor');
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
  act(() => {
    setter?.call(area, value);
    area.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

/** Type into an input the way a keystroke does. */
export function typeInput(input: Element | null, value: string): void {
  if (!(input instanceof HTMLInputElement)) throw new Error('no input');
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
  act(() => {
    setter?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

export function press(element: Element | null, key: string): void {
  if (!(element instanceof HTMLElement)) throw new Error('nothing to press on');
  act(() => {
    element.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
  });
}

/** End a test: unmount what it rendered, take its controller away, and leave a `fetch` that refuses at once. */
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

export function doubleClick(element: Element | null): void {
  if (!(element instanceof HTMLElement)) throw new Error('nothing to double-click');
  act(() => {
    element.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
  });
}

export const byTestId = (container: ParentNode, id: string) => container.querySelector(`[data-testid="${id}"]`);
export const allByTestId = (container: ParentNode, id: string) => [...container.querySelectorAll(`[data-testid="${id}"]`)];

/** What changed the project: every call that is not a read. */
export const writes = (calls: Call[]) =>
  calls.filter((c) => c.method !== 'GET' && !c.path.endsWith('/check')).map((c) => `${c.method} ${c.path.replace(/^\/api\/v1\/projects\/[^/]+/, '')}`);

/** A response held until the test lets it go. */
export function held(): { response: Promise<Response>; release: (response: Response) => void } {
  let release: (response: Response) => void = () => undefined;
  const response = new Promise<Response>((resolve) => {
    release = resolve;
  });
  return { response, release };
}
