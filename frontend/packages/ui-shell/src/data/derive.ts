// Pure derivations over the controller's state, shared by the tree, the editors and the
// inspector so that they cannot disagree about what a connection or a problem is.

import { moduleEditor, type ModuleUiState } from '../modules/registry';
import type { ConnectionStatus, IoStatus } from '../api/client';
import type { EditorRef } from './workbench';

export function connectionById(io: IoStatus | null, id: string): ConnectionStatus | undefined {
  return io?.connections.find((connection) => connection.id === id);
}

export function connectionOfTag(io: IoStatus | null, uri: string): ConnectionStatus | undefined {
  return io?.connections.find((connection) => connection.tags.includes(uri));
}

/** Every tag URI the running connections provide, sorted. */
export function allTagUris(io: IoStatus | null, connectionId?: string): string[] {
  if (!io) return [];
  const uris = io.connections
    .filter((connection) => connectionId === undefined || connection.id === connectionId)
    .flatMap((connection) => connection.tags);
  return [...uris].sort((a, b) => a.localeCompare(b));
}

export interface Problem {
  id: string;
  severity: 'error' | 'warn';
  connectionId: string;
  /** The tag the problem concerns, when it concerns one. */
  uri?: string;
  text: string;
}

/** What is currently wrong with field I/O, as the controller reports it. */
export function currentProblems(io: IoStatus | null): Problem[] {
  if (!io) return [];
  const problems: Problem[] = [];
  for (const connection of io.connections) {
    if (connection.state !== 'RUNNING') {
      problems.push({
        id: `${connection.id}:state`,
        severity: 'error',
        connectionId: connection.id,
        text:
          connection.state === 'CONFIG_ERROR'
            ? `Connection not started: ${connection.detail ?? 'no reason given'}`
            : `Connection stopped: ${connection.detail ?? 'no reason given'}`,
      });
      continue;
    }
    if (connection.last_cycle_error) {
      problems.push({
        id: `${connection.id}:cycle`,
        severity: 'error',
        connectionId: connection.id,
        text: `Poll cycle failed: ${connection.last_cycle_error}`,
      });
    }
    for (const fault of connection.faults) {
      problems.push({
        id: `${connection.id}:${fault.uri}`,
        severity: 'warn',
        connectionId: connection.id,
        uri: fault.uri,
        text: fault.reason,
      });
    }
  }
  return problems;
}

/** What a draft connection is called on screen: its name, or a marker when it has none yet. */
export function connectionLabel(id: string | undefined): string {
  return id && id.trim() !== '' ? id : '(unnamed connection)';
}

/** Resolves what an editor is scoped to into the name it currently has. */
export interface EditorNames {
  /** A draft connection's name, by the key of its row. */
  connection: (key: string) => string | undefined;
  /** A screen's name, by its id. */
  screen: (id: string) => string | undefined;
}

export function editorTitle(
  editor: EditorRef,
  names: EditorNames,
  modules?: ModuleUiState
): string {
  switch (editor.kind) {
    case 'overview':
      return 'Project overview';
    case 'devices':
      return 'Devices & networks';
    case 'connection':
      return `Configuration [${connectionLabel(names.connection(editor.target ?? ''))}]`;
    case 'tags':
      return editor.target ? `Tags [${editor.target}]` : 'All tags';
    case 'diagnostics':
      return `Online & diagnostics [${editor.target ?? ''}]`;
    case 'screen':
      return `Screen [${names.screen(editor.target ?? '') ?? 'removed'}]`;
    default: {
      const registered = modules && moduleEditor(modules, editor.kind);
      if (!registered) return editor.kind;
      const title = registered.item.title(editor.target);
      return Array.isArray(title) ? title.join(' / ') : title;
    }
  }
}

/** The path shown in an editor's header, outermost first. */
export function editorPath(
  editor: EditorRef,
  projectName: string,
  names: EditorNames,
  modules?: ModuleUiState
): string[] {
  switch (editor.kind) {
    case 'overview':
      return [projectName, 'Project overview'];
    case 'devices':
      return [projectName, 'Devices & networks'];
    case 'connection':
      return [projectName, connectionLabel(names.connection(editor.target ?? '')), 'Configuration'];
    case 'tags':
      return editor.target ? [projectName, editor.target, 'Tags'] : [projectName, 'All tags'];
    case 'diagnostics':
      return [projectName, editor.target ?? '', 'Online & diagnostics'];
    case 'screen':
      return [projectName, 'HMI screens', names.screen(editor.target ?? '') ?? 'removed'];
    default: {
      const registered = modules && moduleEditor(modules, editor.kind);
      // Before the module has loaded, or with it disabled, the kind is all that is known.
      if (!registered) return [projectName, editor.kind];
      const title = registered.item.title(editor.target);
      return [projectName, ...(Array.isArray(title) ? title : [title])];
    }
  }
}
