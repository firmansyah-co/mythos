/**
 * Project-scoped faceplate master-type API client (TSK-309).
 *
 * Every call goes to the daemon; there is no local fallback and no cached
 * fiction. Failures surface as an Error with a readable message so the designer
 * can show what actually went wrong.
 */

import {
  FaceplateInstance,
  FaceplateReleaseResult,
  FaceplateType,
  ResolvedFaceplateInstance,
  UdtDefinition,
} from '../types/faceplate';

const base = (projectId: string) =>
  `/api/v1/projects/${encodeURIComponent(projectId)}/faceplates`;

async function responseError(res: Response): Promise<string> {
  try {
    const body = await res.json();
    if (body && typeof body.error === 'string') return body.error;
  } catch {
    // Non-JSON error body; the status is all we have.
  }
  return `HTTP ${res.status}`;
}

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`backend unreachable (${detail})`);
  }
  if (!res.ok) {
    throw new Error(await responseError(res));
  }
  if (res.status === 204) {
    return undefined as T;
  }
  return (await res.json()) as T;
}

/** The list routes answer with an envelope; tolerate a bare array for safety. */
function unwrap<T>(body: unknown, key: string): T[] {
  if (Array.isArray(body)) return body as T[];
  const value = (body as Record<string, unknown> | null)?.[key];
  return Array.isArray(value) ? (value as T[]) : [];
}

export async function listFaceplateTypes(projectId: string): Promise<FaceplateType[]> {
  const body = await call<unknown>(`${base(projectId)}/types`);
  return unwrap<FaceplateType>(body, 'types');
}

export async function createFaceplateType(
  projectId: string,
  input: { name: string; udt: UdtDefinition }
): Promise<FaceplateType> {
  return call<FaceplateType>(`${base(projectId)}/types`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
}

export async function saveFaceplateTypeDraft(
  projectId: string,
  typeId: string,
  type: FaceplateType
): Promise<void> {
  await call<void>(
    `${base(projectId)}/types/${encodeURIComponent(typeId)}/draft`,
    {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(type),
    }
  );
}

export async function releaseFaceplateType(
  projectId: string,
  typeId: string
): Promise<FaceplateReleaseResult> {
  return call<FaceplateReleaseResult>(
    `${base(projectId)}/types/${encodeURIComponent(typeId)}/release`,
    { method: 'POST' }
  );
}

export async function listFaceplateInstances(
  projectId: string
): Promise<ResolvedFaceplateInstance[]> {
  const body = await call<unknown>(`${base(projectId)}/instances`);
  return unwrap<ResolvedFaceplateInstance>(body, 'instances');
}

export async function createFaceplateInstance(
  projectId: string,
  input: {
    name: string;
    typeId: string;
    typeVersion?: number | null;
    bindings?: Array<{ faceplateKey: string; tagUri: string }>;
  }
): Promise<FaceplateInstance> {
  return call<FaceplateInstance>(`${base(projectId)}/instances`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(input),
  });
}

/** `typeVersion: null` unfreezes the instance so it follows the active master. */
export async function updateFaceplateInstance(
  projectId: string,
  instanceId: string,
  patch: { typeVersion?: number | null; bindings?: Array<{ faceplateKey: string; tagUri: string }> }
): Promise<FaceplateInstance> {
  return call<FaceplateInstance>(
    `${base(projectId)}/instances/${encodeURIComponent(instanceId)}`,
    {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(patch),
    }
  );
}