// The data UI's view of the workbench, through the `WorkbenchHost` it was activated with
// (ADR-011 §2.3), and the routes it calls: the data module's own (ADR-014 §2.4), which keep the
// project's user types and data blocks, and the core's runtimes configuration of the project.

import { create } from 'zustand';
import type { ApiRefusal, Dispose, Project, WorkbenchHost } from '@mythos/workbench-sdk';

let current: WorkbenchHost | null = null;

/** The host the module was activated with. */
export function host(): WorkbenchHost {
  if (!current) throw new Error('the data UI is not activated');
  return current;
}

/** The open project. */
export const useProject = create<{ project: Project | null }>(() => ({ project: null }));

/**
 * Whether the signed-in user may change the project's types and data blocks: what the data
 * module's writing routes ask (engineer). Without accounts, anyone may.
 */
export function mayEngineer(): boolean {
  const user = current?.user() ?? null;
  return user === null || user.role === 'engineer' || user.role === 'admin';
}

// ---------------------------------------------------------------------------------------------
// What the data module answers (plugins/data/src/lib.rs)
// ---------------------------------------------------------------------------------------------

/**
 * A message of the compiler about the project's types and data blocks: from `line`:`col` up to,
 * not including, `end_line`:`end_col`, counted from 1, in the file `source`
 * (`data/types/<Name>.st`). All four positions are 0, and there is no source, when it is about
 * the whole (the one-declaration rule, a `blocks.json` that cannot be read).
 */
export interface Diagnostic {
  line: number;
  col: number;
  end_line: number;
  end_col: number;
  message: string;
  source?: string | null;
}

/** A member of a data block, as the compiler laid it out. */
export interface MemberAnswer {
  name: string;
  type: string;
  offset: number;
  size: number;
}

/**
 * A data block as the module answers it: its name, type and retain always; its layout (16 hex
 * digits), size and members only when the whole state checks, absent otherwise.
 */
export interface BlockAnswer {
  name: string;
  type: string;
  retain: boolean;
  layout?: string;
  size?: number;
  members?: MemberAnswer[];
}

/** `GET /blocks` and `POST /check`: whether the state checks, why not, and the blocks. */
export interface CheckAnswer {
  ok: boolean;
  diagnostics: Diagnostic[];
  blocks: BlockAnswer[];
}

/**
 * `PUT /types/:type`. No source: what was stored is read again. `changed`: the blocks whose
 * layout changed or that are new, or `null` when the state before did not check, so what it
 * was is not known.
 */
export interface TypeSaveAnswer {
  name: string;
  created: boolean;
  blocks: BlockAnswer[];
  changed: string[] | null;
}

/** `DELETE /types/:type` and `DELETE /blocks/:block`: the stored name, and the blocks after. */
export interface DeleteAnswer {
  deleted: string;
  blocks: BlockAnswer[];
}

/** `PUT /blocks/:block`. */
export interface BlockSaveAnswer {
  created: boolean;
  blocks: BlockAnswer[];
  changed: string[] | null;
}

/**
 * A data block as a runtime of the project's runtimes configuration carries it
 * (`GET /api/v1/projects/:id/runtimes`, each instance's `data`). `retain` is absent when false.
 */
export interface RuntimeBlock {
  name: string;
  type: string;
  layout: string;
  size: number;
  members?: unknown;
  retain?: boolean;
}

/** One instance of the project's runtimes configuration, as far as this UI reads it. */
export interface RuntimeInstance {
  id: string;
  data?: RuntimeBlock[];
  [key: string]: unknown;
}

/** `DATA_CHANGED`: after each write the module made, never after a refused one. */
export interface DataChanged {
  project: string;
  what: 'type' | 'block';
  name: string;
  change: 'saved' | 'deleted';
}

/** Sent by the data module after each write. */
export const DATA_CHANGED_EVENT = 'DATA_CHANGED';

/** Why the controller refused, as this UI shows it: the module's own words and reasons. */
export interface Refusal {
  /** The HTTP status; `null` when the controller did not answer. */
  status: number | null;
  /** The module's `error`, verbatim; the request's message when the answer had none. */
  error: string;
  /** The compiler's reasons, when the answer gave them; `null` when it gave none. */
  diagnostics: Diagnostic[] | null;
}

/** What `host.api` threw, as a {@link Refusal}. */
export function refusalOf(cause: unknown): Refusal {
  const thrown = cause as Partial<ApiRefusal> | null;
  const message = cause instanceof Error ? cause.message : String(cause);
  const status = typeof thrown?.status === 'number' ? thrown.status : null;
  const body = thrown?.body;
  const record = body !== null && typeof body === 'object' ? (body as Record<string, unknown>) : null;
  const error = typeof record?.error === 'string' ? record.error : message;
  const diagnostics = Array.isArray(record?.diagnostics) ? (record.diagnostics as Diagnostic[]) : null;
  return { status, error, diagnostics };
}

const dataOf = (projectId: string) => `/api/v1/projects/${encodeURIComponent(projectId)}/data`;
const json = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
});
const name = encodeURIComponent;

export const api = {
  types: (projectId: string) => host().api<{ types: Array<{ name: string }> }>(`${dataOf(projectId)}/types`),
  readType: (projectId: string, type: string) =>
    host().api<{ name: string; source: string }>(`${dataOf(projectId)}/types/${name(type)}`),
  saveType: (projectId: string, type: string, source: string) =>
    host().api<TypeSaveAnswer>(`${dataOf(projectId)}/types/${name(type)}`, json('PUT', { source })),
  deleteType: (projectId: string, type: string) =>
    host().api<DeleteAnswer>(`${dataOf(projectId)}/types/${name(type)}`, { method: 'DELETE' }),
  blocks: (projectId: string) => host().api<CheckAnswer>(`${dataOf(projectId)}/blocks`),
  saveBlock: (projectId: string, block: string, type: string, retain: boolean) =>
    host().api<BlockSaveAnswer>(`${dataOf(projectId)}/blocks/${name(block)}`, json('PUT', { type, retain })),
  deleteBlock: (projectId: string, block: string) =>
    host().api<DeleteAnswer>(`${dataOf(projectId)}/blocks/${name(block)}`, { method: 'DELETE' }),
  /** Check the stored state with type `type`'s source as `source`, which need not be saved. Writes nothing. */
  check: (projectId: string, type: string, source: string) =>
    host().api<CheckAnswer>(`${dataOf(projectId)}/check`, json('POST', { type, source })),
  /** The project's runtimes configuration, as saved. */
  runtimes: (projectId: string) =>
    host().api<{ config: { instances: RuntimeInstance[] } | null }>(
      `/api/v1/projects/${encodeURIComponent(projectId)}/runtimes`
    ),
};

/**
 * Follow `next` from now on: mirror its project into the store above. Returns what stops it.
 * Called by `activate`, and by tests with a host of their own.
 */
export function bind(next: WorkbenchHost): Dispose {
  current = next;
  useProject.setState({ project: next.project() });
  const stop = next.onProject((project) => useProject.setState({ project }));
  return () => {
    stop();
    if (current === next) current = null;
  };
}
