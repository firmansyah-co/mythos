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
  /** Driver-specific. Its shape is the JSON Schema in the driver module's manifest. */
  config: unknown;
}

/** What a module says about itself (`--manifest`). */
export interface ModuleManifest {
  protocol: number;
  id: string;
  name: string;
  version: string;
  kind: string;
  provides: string;
  description: string;
  capabilities: string[];
  config_schema: unknown;
}

/** One driver a connection may use, and the module executable behind it when there is one. */
export interface DriverInfo {
  name: string;
  source: 'module' | 'in_process';
  module: { path: string; manifest: ModuleManifest } | null;
}

/** One module the controller found, used or switched off (Settings → Modules). */
export interface ModuleRow {
  id: string;
  name: string;
  version: string;
  kind: 'driver' | 'service';
  provides: string;
  description: string;
  path: string;
  enabled: boolean;
  /** Installed from a package, so it can be removed; otherwise it belongs to the installation. */
  removable: boolean;
  /** Where the workbench imports this module's UI from, when it is enabled and has one. */
  ui?: string | null;
}

/** A running service module (ADR-011). */
export interface ServiceStatus {
  provides: string;
  id: string;
  version: string;
  path: string;
  pid: number | null;
  starts: number;
  last_error: string | null;
}

export interface ModuleList {
  /** Absent from servers older than Settings → Modules. */
  modules?: ModuleRow[];
  drivers: DriverInfo[];
  services?: ServiceStatus[];
  /** Executables named like modules that could not be used, and why. */
  problems: { path: string; message: string }[];
  install_dir?: string | null;
  /** The platform the server runs on, which picks a package's executable. */
  target?: string;
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

/** Accounts are ordered: each role may do everything the one before it may. */
export type Role = 'viewer' | 'operator' | 'engineer' | 'admin';
export const ROLES: readonly Role[] = ['viewer', 'operator', 'engineer', 'admin'];

/** The account a request acts as. */
export interface Identity {
  username: string;
  display_name: string;
  role: Role;
  /** `session`, or `token:<name>`. */
  via: string;
}

export interface AuthStatus {
  /** Whether this server has accounts at all. */
  auth: boolean;
  /** Whether the first administrator is still to be created. */
  setup_required: boolean;
  user: Identity | null;
}

export interface UserAccount {
  username: string;
  display_name: string;
  role: Role;
  disabled: boolean;
  created_unix_s: number;
}

export interface AccessToken {
  id: string;
  name: string;
  created_unix_s: number;
  last_used_unix_s: number | null;
}

/** What an installation follows: releases only, release candidates too, or everything. */
export type ReleaseChannel = 'release' | 'rc' | 'dev';
export const CHANNELS: readonly ReleaseChannel[] = ['release', 'rc', 'dev'];

export type NodeRole = 'workstation' | 'edge' | 'server';
export const NODE_ROLES: readonly NodeRole[] = ['workstation', 'edge', 'server'];

/** This installation as a node (ADR-012 §2.1). */
export interface NodeInfo {
  /** Never changes: what pairing and plant projects refer to. */
  id: string;
  name: string;
  role: NodeRole;
  site: string;
  version: string;
  module_protocol: number;
  /** Where this run of the server listens; `null` when it did not say. */
  listening: string | null;
  /** Where the role (or MYTHOS_BIND) says to listen. */
  listen_address: string | null;
  /** The role's listening address differs from this run's: it applies after a restart. */
  restart_required: boolean;
}

export interface UpdateStatus {
  running: { version: string; installed_version: string | null; target: string };
  /** `null` when the server runs from no installation it can update. */
  installation: {
    home: string;
    current_version: string | null;
    previous_version: string | null;
    installed: string[];
    channel: ReleaseChannel;
    pinned_version: string | null;
    feed: string;
  } | null;
  /** Why there is no installation, when there is none. */
  note: string | null;
}

export interface PublishedRelease {
  version: string;
  tag: string;
  channel: ReleaseChannel;
  name: string | null;
  /** Release notes, as published (Markdown). */
  notes: string | null;
  published_at: string | null;
  url: string | null;
  /** Whether it publishes a bundle for the platform the server runs on. */
  has_bundle: boolean;
  installed: boolean;
  current: boolean;
}

export interface ReleaseList {
  feed: string;
  fetched_unix_s: number;
  /** What the followed channel, or the pin, would install. */
  target_version: string | null;
  releases: PublishedRelease[];
}

export interface Installed {
  version: string;
  reported: string | null;
  restart_required: boolean;
  status: UpdateStatus;
}

let onUnauthorized: (() => void) | null = null;

/** Called whenever the server answers 401 to a request: the session has ended. */
export function whenUnauthorized(handler: (() => void) | null) {
  onUnauthorized = handler;
}

/** `fetch`, reporting an unanswered request as an `ApiError` and a 401 to `whenUnauthorized`. */
async function call(path: string, init?: RequestInit): Promise<Response> {
  let response: Response;
  try {
    response = await fetch(path, init);
  } catch (cause) {
    throw new ApiError(
      `The controller did not answer (${cause instanceof Error ? cause.message : 'network error'})`,
      null
    );
  }
  if (response.status === 401 && !path.startsWith('/api/v1/auth/')) onUnauthorized?.();
  return response;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await call(path, init);
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

/**
 * A request on behalf of a module's UI: `request`, except that an empty body (204, or a module
 * that answers nothing) resolves to `null` rather than failing to parse.
 */
export async function apiRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await call(path, init);
  if (!response.ok) {
    let detail = '';
    try {
      const body: unknown = await response.json();
      if (body && typeof body === 'object') {
        const text = (body as Record<string, unknown>).error;
        if (typeof text === 'string') detail = `: ${text}`;
      }
    } catch {
      // The body was not JSON; the status line is all there is to report.
    }
    throw new ApiError(`${response.status} ${response.statusText}${detail}`, response.status);
  }
  const text = await response.text();
  return (text === '' ? null : JSON.parse(text)) as T;
}

/** A request whose success carries no body. Throws exactly as `request` does. */
async function send(path: string, init: RequestInit): Promise<void> {
  const response = await call(path, init);
  if (response.ok) return;
  let detail = '';
  try {
    const body: unknown = await response.json();
    if (body && typeof body === 'object') {
      const text = (body as Record<string, unknown>).error;
      if (typeof text === 'string') detail = `: ${text}`;
    }
  } catch {
    // The body was not JSON; the status line is all there is to report.
  }
  throw new ApiError(`${response.status} ${response.statusText}${detail}`, response.status);
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
  listModules: () => request<ModuleList>('/api/v1/modules'),
  rescanModules: () => request<unknown>('/api/v1/modules/rescan', { method: 'POST' }),
  setModuleEnabled: (id: string, enabled: boolean) =>
    request<{ modules: ModuleRow[] }>(
      `/api/v1/modules/${encodeURIComponent(id)}/enabled`,
      json('PUT', { enabled })
    ),
  /** Install a module package: the file itself is the request body. */
  installModule: (pkg: Blob) =>
    request<{ installed: string; version: string; enabled: boolean }>('/api/v1/modules/install', {
      method: 'POST',
      headers: { 'Content-Type': 'application/zip' },
      body: pkg,
    }),
  installModuleFromUrl: (url: string, sha256: string) =>
    request<{ installed: string; version: string; enabled: boolean }>(
      '/api/v1/modules/install-url',
      json('POST', { url, sha256 })
    ),
  removeModule: (id: string) =>
    request<{ removed: string }>(`/api/v1/modules/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  getIoConfig: async (projectId: string) =>
    (await request<{ config: IoConfig | null }>(`/api/v1/projects/${encodeURIComponent(projectId)}/io`))
      .config,
  /**
   * Save a project's I/O configuration. Resolves with whether the controller applied it; throws
   * `ConfigRejected` when the server refused it, with the problems it named.
   */
  saveIoConfig: async (projectId: string, config: IoConfig): Promise<{ applied: boolean }> => {
    const path = `/api/v1/projects/${encodeURIComponent(projectId)}/io`;
    const response = await call(path, json('PUT', config));
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

  // Updates
  getNode: () => request<NodeInfo>('/api/v1/node'),
  updateNode: (change: Partial<Pick<NodeInfo, 'name' | 'site' | 'role'>>) =>
    request<NodeInfo>('/api/v1/node', json('PUT', change)),
  getUpdates: () => request<UpdateStatus>('/api/v1/updates'),
  listReleases: (refresh = false) =>
    request<ReleaseList>(`/api/v1/updates/releases${refresh ? '?refresh=true' : ''}`),
  setFollow: (channel: ReleaseChannel, pinnedVersion: string | null) =>
    request<UpdateStatus>('/api/v1/updates/follow', json('PUT', { channel, pinned_version: pinnedVersion })),
  installUpdate: (version?: string) =>
    request<Installed>('/api/v1/updates/install', json('POST', version ? { version } : {})),
  rollbackUpdate: () => request<Installed>('/api/v1/updates/rollback', { method: 'POST' }),

  // Accounts
  authStatus: () => request<AuthStatus>('/api/v1/auth/status'),
  setUp: (setupCode: string, username: string, displayName: string, password: string) =>
    request<Identity>(
      '/api/v1/auth/setup',
      json('POST', { setup_code: setupCode, username, display_name: displayName, password })
    ),
  signIn: (username: string, password: string) =>
    request<Identity>('/api/v1/auth/login', json('POST', { username, password })),
  signOut: () => send('/api/v1/auth/logout', { method: 'POST' }),
  changePassword: (currentPassword: string, newPassword: string) =>
    send(
      '/api/v1/auth/me/password',
      json('PUT', { current_password: currentPassword, new_password: newPassword })
    ),
  listTokens: () => request<AccessToken[]>('/api/v1/auth/me/tokens'),
  createToken: (name: string) =>
    request<{ token: AccessToken; secret: string }>('/api/v1/auth/me/tokens', json('POST', { name })),
  revokeToken: (id: string) =>
    send(`/api/v1/auth/me/tokens/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  listUsers: () => request<{ users: UserAccount[]; roles: Role[] }>('/api/v1/users'),
  createUser: (user: { username: string; display_name: string; role: Role; password: string }) =>
    request<UserAccount>('/api/v1/users', json('POST', user)),
  updateUser: (
    username: string,
    change: { display_name?: string; role?: Role; disabled?: boolean; password?: string }
  ) => request<UserAccount>(`/api/v1/users/${encodeURIComponent(username)}`, json('PUT', change)),
  deleteUser: (username: string) =>
    send(`/api/v1/users/${encodeURIComponent(username)}`, { method: 'DELETE' }),
};

/** Whether `role` may do what `needed` may. */
export function hasRole(role: Role | undefined, needed: Role): boolean {
  return role !== undefined && ROLES.indexOf(role) >= ROLES.indexOf(needed);
}

/** The server-sent event stream of tag values. */
export const TELEMETRY_STREAM_ENDPOINT = '/api/v1/telemetry/stream';
/** The event name carrying one telemetry batch. */
export const TAG_UPDATE_EVENT = 'tag_update';
