// The HTTP API of the MythOS server, as this workbench uses it.
//
// Every function either returns what the server said or throws. None substitutes a default for
// a failed request: a caller that cannot reach the controller must say so.

export interface ProjectConfig {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  created_at_utc: string;
  updated_at_utc: string;
}

export type ConnectionState = 'RUNNING' | 'CONFIG_ERROR' | 'STOPPED';

export interface TagFault {
  uri: string;
  reason: string;
}

export interface ConnectionStatus {
  id: string;
  driver: string;
  state: ConnectionState;
  detail: string | null;
  tags: string[];
  poll_interval_ms: number;
  cycle_timeout_ms: number;
  cycles_completed: number;
  last_cycle_unix_ms: number | null;
  last_cycle_error: string | null;
  tags_good: number | null;
  faults: TagFault[];
}

export interface IoStatus {
  source: string | null;
  note: string | null;
  connections: ConnectionStatus[];
}

/** One connection of a project's I/O configuration, as stored. */
export interface ConnectionConfig {
  id: string;
  driver: string;
  poll_interval_ms: number;
  cycle_timeout_ms?: number;
  /** Driver-specific. The workbench understands the Modbus TCP shape and shows any other as is. */
  config: unknown;
}

export interface IoConfig {
  connections: ConnectionConfig[];
}

/** One reason the server refused a configuration. */
export interface ConfigProblem {
  connection_id: string | null;
  message: string;
}

/** A refused configuration: the server's message and every problem it named. */
export class ConfigRejected extends Error {
  constructor(
    message: string,
    readonly problems: ConfigProblem[]
  ) {
    super(message);
    this.name = 'ConfigRejected';
  }
}

/** A tag as the catalog describes it. Only the fields this workbench shows are typed. */
export interface TagMetadata {
  uri: string;
  slot_id: number;
  description: string;
  engineering_units: string;
  access_mode: string;
}

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number | null
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(path, init);
  } catch (cause) {
    throw new ApiError(
      `The controller did not answer (${cause instanceof Error ? cause.message : 'network error'})`,
      null
    );
  }

  if (!response.ok) {
    let detail = '';
    try {
      const body: unknown = await response.json();
      if (body && typeof body === 'object') {
        const record = body as Record<string, unknown>;
        const text = record.error ?? record.message;
        if (typeof text === 'string') detail = `: ${text}`;
      }
    } catch {
      // The body was not JSON; the status line is all there is to report.
    }
    throw new ApiError(`${response.status} ${response.statusText}${detail}`, response.status);
  }

  return (await response.json()) as T;
}

function json(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  };
}

export const api = {
  listProjectIds: () => request<string[]>('/api/v1/projects'),
  getProject: (id: string) => request<ProjectConfig>(`/api/v1/projects/${encodeURIComponent(id)}`),
  createProject: (project: Pick<ProjectConfig, 'id' | 'name' | 'description' | 'author'>) =>
    request<ProjectConfig>('/api/v1/projects', json('POST', project)),
  activateProject: (id: string) =>
    request<{ status: string; project_id: string }>(
      `/api/v1/projects/${encodeURIComponent(id)}/activate`,
      { method: 'POST' }
    ),
  getActiveProjectId: async () =>
    (await request<{ project_id: string | null }>('/api/v1/projects/active')).project_id,
  getIoStatus: () => request<IoStatus>('/api/v1/io/status'),
  listDrivers: () => request<string[]>('/api/v1/io/drivers'),
  getIoConfig: async (projectId: string) =>
    (await request<{ config: IoConfig | null }>(`/api/v1/projects/${encodeURIComponent(projectId)}/io`))
      .config,
  /**
   * Save a project's I/O configuration. Resolves with whether the controller applied it; throws
   * `ConfigRejected` when the server refused it, with the problems it named.
   */
  saveIoConfig: async (projectId: string, config: IoConfig): Promise<{ applied: boolean }> => {
    const path = `/api/v1/projects/${encodeURIComponent(projectId)}/io`;
    let response: Response;
    try {
      response = await fetch(path, json('PUT', config));
    } catch (cause) {
      throw new ApiError(
        `The controller did not answer (${cause instanceof Error ? cause.message : 'network error'})`,
        null
      );
    }
    const body: unknown = await response.json().catch(() => null);
    const record = (body && typeof body === 'object' ? body : {}) as Record<string, unknown>;
    if (response.ok) return { applied: record.applied === true };

    const message =
      typeof record.error === 'string' ? record.error : `${response.status} ${response.statusText}`;
    if (Array.isArray(record.problems)) {
      throw new ConfigRejected(message, record.problems as ConfigProblem[]);
    }
    throw new ApiError(message, response.status);
  },
  listTags: () => request<TagMetadata[]>('/api/v1/tags'),
};

/** The server-sent event stream of tag values. */
export const TELEMETRY_STREAM_ENDPOINT = '/api/v1/telemetry/stream';
/** The event name carrying one telemetry batch. */
export const TAG_UPDATE_EVENT = 'tag_update';
