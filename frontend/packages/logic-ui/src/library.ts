// The project's library (ADR-014 §2.8, D4c): the Structured Text sources of functions and
// function blocks that every program of the project is compiled with, under `logic/library/`.
// Its five routes, what they answer, and the few pure rules this UI needs to show them. The
// rules the logic module owns (what a name may be, which names are one) are not repeated here:
// the module answers them, and its words are shown.

import type { ApiRefusal } from '@mythos/workbench-sdk';
import { host, type Diagnostic } from './bridge';

/** The library source editor's kind; its target is the source's name. */
export const LIBRARY_EDITOR = 'logic-library';
/** Sent by the logic module after a library source was saved or deleted. */
export const LIBRARY_CHANGED_EVENT = 'LOGIC_LIBRARY_CHANGED';

/** What `POST /library/check` answers: whether the library compiles with the draft put in. */
export interface LibraryCheck {
  ok: boolean;
  diagnostics: Diagnostic[];
  /**
   * Present only when the project's user types and data blocks do not check, so the library was
   * checked without them: why, in the shape of the diagnostics.
   */
  project_problems?: Diagnostic[];
}

/** `LOGIC_LIBRARY_CHANGED`'s data. */
export interface LibraryChanged {
  project: string;
  /** The request's name for a save, the stored spelling for a delete. */
  name: string;
  what: 'saved' | 'deleted';
}

/** `LOGIC_LIBRARY_CHANGED`'s data, when it has the shape the module sends; otherwise nothing. */
export function libraryEventOf(data: unknown): LibraryChanged | null {
  if (data === null || typeof data !== 'object') return null;
  const d = data as Record<string, unknown>;
  if (typeof d.project !== 'string' || typeof d.name !== 'string') return null;
  if (d.what !== 'saved' && d.what !== 'deleted') return null;
  return { project: d.project, name: d.name, what: d.what };
}

/** Whether a diagnostic is about the text of library source `name`, so its line is a line of that text. */
export const isOfThisSource = (d: Diagnostic, name: string): boolean => d.source === `logic/library/${name}.st`;

/**
 * The library source a diagnostic is about, when it is one the module listed last (`listed`);
 * `null` for any other (a type's file, a program's, a source not listed, or no list read).
 */
export function librarySourceOf(d: Diagnostic, listed: readonly string[] | null): string | null {
  const m = /^logic\/library\/(.+)\.st$/.exec(d.source ?? '');
  if (m === null || listed === null) return null;
  return listed.find((n) => n === m[1]) ?? null;
}

/** A text with Windows line ends read as `\n`: what a text area gives back. */
export const lineEnds = (text: string): string => text.replace(/\r\n/g, '\n');
export const sameText = (a: string, b: string): boolean => lineEnds(a) === lineEnds(b);
/** The module compares names with eq_ignore_ascii_case; every name it lists is ASCII. */
export const sameName = (a: string, b: string): boolean => a.toLowerCase() === b.toLowerCase();

/** Said before a source is deleted: the module's own words for what follows. */
export const deleteQuestion = (name: string) =>
  `Delete library source ${name}? Every program is then compiled without it: one that uses what it held no longer compiles. Nothing is built or loaded.`;

/** Said before a text the library does not check with is saved anyway. */
export const saveAnywayQuestion = (name: string) =>
  `Save ${name} although the library does not check with it? Every program is compiled with every library source, and a mistake in one refuses every program.`;

/** Where a diagnostic is, as listed: its file when it is not `name`'s, and its line and column, or "about the whole". */
export function placeOf(d: Diagnostic, name: string): string {
  const at = d.line === 0 ? 'about the whole' : `${d.line}:${d.col}`;
  if (isOfThisSource(d, name) || !d.source) return at;
  return `${d.source} ${at}`;
}

/** Why the controller refused, as this UI shows it: the module's own words. */
export interface Refusal {
  /** The HTTP status; `null` when the controller did not answer. */
  status: number | null;
  /** The module's `error`, verbatim; the request's message when the answer had none. */
  error: string;
  /** The compiler's reasons, when the answer gave them; `null` when it gave none. */
  diagnostics: Diagnostic[] | null;
  /**
   * The workbench threw without the answer beside it (no `body`): whether the module gave
   * reasons is then not known, and the screen says so rather than show none.
   */
  withheld: boolean;
}

/** What `host.api` threw, as a {@link Refusal}. (A copy of data-ui's: a module UI imports only the SDK.) */
export function refusalOf(cause: unknown): Refusal {
  const thrown = cause as Partial<ApiRefusal> | null;
  const message = cause instanceof Error ? cause.message : String(cause);
  const status = typeof thrown?.status === 'number' ? thrown.status : null;
  const withheld = !(thrown !== null && typeof thrown === 'object' && 'body' in thrown);
  const body = thrown?.body;
  const record = body !== null && typeof body === 'object' ? (body as Record<string, unknown>) : null;
  const error = typeof record?.error === 'string' ? record.error : message;
  const diagnostics = Array.isArray(record?.diagnostics) ? (record.diagnostics as Diagnostic[]) : null;
  return { status, error, diagnostics, withheld };
}

/** Said beside a refusal whose answer the workbench did not hand over. */
export const WITHHELD =
  "This workbench did not hand over the module's whole answer: whether it gave reasons, and which, is not known here.";

const libraryOf = (projectId: string) => `/api/v1/projects/${encodeURIComponent(projectId)}/logic/library`;
const json = (method: string, body: unknown): RequestInit => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
});

export const libraryApi = {
  list: (projectId: string) => host().api<{ sources: Array<{ name: string }> }>(libraryOf(projectId)),
  /** The stored text, with the stored spelling of its name. */
  readSource: (projectId: string, name: string) =>
    host().api<{ name: string; source: string }>(`${libraryOf(projectId)}/${encodeURIComponent(name)}`),
  /** Make or replace a source. The module checks nothing of the text. */
  save: (projectId: string, name: string, source: string) =>
    host().api<{ name: string; created: boolean }>(`${libraryOf(projectId)}/${encodeURIComponent(name)}`, json('PUT', { source })),
  remove: (projectId: string, name: string) =>
    host().api<{ deleted: string }>(`${libraryOf(projectId)}/${encodeURIComponent(name)}`, { method: 'DELETE' }),
  /** Check the library with source `name`'s text as `source`, which need not be saved. Writes nothing. */
  check: (projectId: string, name: string, source: string) =>
    host().api<LibraryCheck>(`${libraryOf(projectId)}/check`, json('POST', { name, source })),
};
