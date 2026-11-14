// The address of what the workbench is showing, kept in the URL fragment so that a reload or a
// shared link returns to the same project and editor.
//
//   #/project/<project id>
//   #/project/<project id>/<editor kind>
//   #/project/<project id>/<editor kind>/<connection id>

import type { EditorKind } from './workbench';

/**
 * Editor kinds that can be named in the address. A draft row cannot: its key is not stable.
 * A screen is not yet: its editor needs the screen list, which loads after the project opens.
 */
type AddressableKind = Exclude<EditorKind, 'connection' | 'screen'>;

export interface WorkbenchLocation {
  projectId: string;
  editor?: { kind: AddressableKind; connectionId?: string };
}

const EDITOR_KINDS: readonly AddressableKind[] = ['overview', 'devices', 'tags', 'diagnostics'];

export function parseLocation(hash: string): WorkbenchLocation | null {
  const parts = hash.replace(/^#\/?/, '').split('/').filter(Boolean).map(decodeURIComponent);
  if (parts[0] !== 'project' || !parts[1]) return null;

  const location: WorkbenchLocation = { projectId: parts[1] };
  const kind = EDITOR_KINDS.find((candidate) => candidate === parts[2]);
  if (kind) location.editor = { kind, connectionId: parts[3] };
  return location;
}

export function formatLocation(location: WorkbenchLocation | null): string {
  if (!location) return '#/';
  const parts = ['project', location.projectId];
  if (location.editor) {
    parts.push(location.editor.kind);
    if (location.editor.connectionId) parts.push(location.editor.connectionId);
  }
  return `#/${parts.map(encodeURIComponent).join('/')}`;
}
