// The address of what the workbench is showing, kept in the URL fragment so that a reload or a
// shared link returns to the same project and editor.
//
//   #/project/<project id>
//   #/project/<project id>/<editor kind>
//   #/project/<project id>/<editor kind>/<connection id>
//   #/<module view id>/<project id>[/<target>]   a module's full-window view (ADR-011 §2.3),
//                                                 e.g. the HMI's operator runtime, #/runtime/<id>

import type { CoreEditorKind } from './workbench';

/** Editor kinds that can be named in the address. A draft row cannot: its key is not stable. */
type AddressableKind = Exclude<CoreEditorKind, 'connection'>;

export interface WorkbenchLocation {
  projectId: string;
  editor?: { kind: AddressableKind; connectionId?: string };
  /** Present when the address is a module's full-window view. */
  view?: { id: string; target?: string };
}

/** First words of an address that are the workbench's own, never a module view's. */
const OWN_WORDS = ['project'];

const EDITOR_KINDS: readonly AddressableKind[] = ['overview', 'plant', 'devices', 'tags', 'diagnostics'];

export function parseLocation(hash: string): WorkbenchLocation | null {
  const parts = hash.replace(/^#\/?/, '').split('/').filter(Boolean).map(decodeURIComponent);
  if (parts[0] && parts[1] && !OWN_WORDS.includes(parts[0])) {
    return { projectId: parts[1], view: parts[2] ? { id: parts[0], target: parts[2] } : { id: parts[0] } };
  }
  if (parts[0] !== 'project' || !parts[1]) return null;

  const location: WorkbenchLocation = { projectId: parts[1] };
  const kind = EDITOR_KINDS.find((candidate) => candidate === parts[2]);
  if (kind) location.editor = { kind, connectionId: parts[3] };
  return location;
}

export function formatLocation(location: WorkbenchLocation | null): string {
  if (!location) return '#/';
  if (location.view) {
    const parts = [location.view.id, location.projectId];
    if (location.view.target) parts.push(location.view.target);
    return `#/${parts.map(encodeURIComponent).join('/')}`;
  }
  const parts = ['project', location.projectId];
  if (location.editor) {
    parts.push(location.editor.kind);
    if (location.editor.connectionId) parts.push(location.editor.connectionId);
  }
  return `#/${parts.map(encodeURIComponent).join('/')}`;
}
