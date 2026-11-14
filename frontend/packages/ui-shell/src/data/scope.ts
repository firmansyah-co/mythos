// Whose runtime the workbench is looking at.
//
// The controller runs exactly one project, and its connections and tags belong to that project.
// The workbench can have any project open. Every surface that shows runtime state goes through
// this scope, so a project that is not running can never be shown another project's devices.

import type { IoStatus } from '../api/client';
import { useController } from './controller';
import { useWorkbench } from './workbench';

export type RuntimeScope =
  /** The controller has not answered yet. */
  | { kind: 'unknown' }
  /** The controller cannot be reached; nothing about its runtime is known. */
  | { kind: 'unreachable'; error: string }
  /** The open project is not the one the controller runs. */
  | { kind: 'not-active'; activeProjectId: string | null }
  /** The open project is running, and this is its field I/O. */
  | { kind: 'active'; io: IoStatus };

export function runtimeScope(
  projectId: string | null,
  activeProjectId: string | null | undefined,
  io: IoStatus | null,
  error: string | null
): RuntimeScope {
  if (error) return { kind: 'unreachable', error };
  if (projectId === null || activeProjectId === undefined || io === null) return { kind: 'unknown' };
  if (activeProjectId !== projectId) return { kind: 'not-active', activeProjectId };
  return { kind: 'active', io };
}

export function useRuntimeScope(): RuntimeScope {
  const projectId = useWorkbench((state) => state.project?.id ?? null);
  const activeProjectId = useController((state) => state.activeProjectId);
  const io = useController((state) => state.io);
  const error = useController((state) => state.error);
  return runtimeScope(projectId, activeProjectId, io, error);
}

/** The open project's field I/O, or `null` when the workbench is not looking at a running project. */
export function useProjectIo(): IoStatus | null {
  const scope = useRuntimeScope();
  return scope.kind === 'active' ? scope.io : null;
}
