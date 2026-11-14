// Pure derivations over the controller's state, shared by the tree, the editors and the
// inspector so that they cannot disagree about what a connection or a problem is.

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

export function editorTitle(editor: EditorRef): string {
  switch (editor.kind) {
    case 'overview':
      return 'Project overview';
    case 'devices':
      return 'Devices & networks';
    case 'tags':
      return editor.connectionId ? `Tags [${editor.connectionId}]` : 'All tags';
    case 'diagnostics':
      return `Online & diagnostics [${editor.connectionId ?? ''}]`;
  }
}

/** The path shown in an editor's header, outermost first. */
export function editorPath(editor: EditorRef, projectName: string): string[] {
  switch (editor.kind) {
    case 'overview':
      return [projectName, 'Project overview'];
    case 'devices':
      return [projectName, 'Devices & networks'];
    case 'tags':
      return editor.connectionId
        ? [projectName, editor.connectionId, 'Tags']
        : [projectName, 'All tags'];
    case 'diagnostics':
      return [projectName, editor.connectionId ?? '', 'Online & diagnostics'];
  }
}
