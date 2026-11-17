// Shared test plumbing: mount a component into happy-dom, and put every store back to a known
// state between tests.

import { act } from 'react';
import type { ReactElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import type {
  ConnectionStatus,
  DriverInfo,
  IoConfig,
  IoStatus,
  ModuleList,
  ModuleManifest,
  ProjectConfig,
} from '../api/client';
import { useController } from '../data/controller';
import { useIoConfig } from '../data/ioConfig';
import { useProgramTags } from '../data/programTags';
import { toForm, type DriverSchemas } from '../data/ioForm';
import type { JsonSchema } from '../data/schemaForm';
import modbusManifest from './fixtures/modbus-manifest.json';
import { useLog } from '../data/log';
import type { TagSample } from '../data/quality';
import { useSession } from '../data/session';
import { goOffline, useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';
import { unloadModuleUis } from '../modules/loader';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/**
 * The Modbus module's manifest, as `mythos-module-modbus --manifest` prints it. A Rust test
 * (`the_workbench_fixture_is_this_manifest`) fails when the module's manifest changes and this
 * copy does not.
 */
export const MODBUS_MANIFEST = modbusManifest as ModuleManifest;
export const MODBUS_SCHEMA = MODBUS_MANIFEST.config_schema as JsonSchema;
export const SCHEMAS: DriverSchemas = { modbus: MODBUS_SCHEMA };
export const MODBUS_DRIVER: DriverInfo = {
  name: 'modbus',
  source: 'module',
  module: { path: '/opt/mythos/mythos-module-modbus', manifest: MODBUS_MANIFEST },
};
/** What `GET /api/v1/modules` answers on a controller with the Modbus module. */
export const MODULES: ModuleList = { drivers: [MODBUS_DRIVER], problems: [] };

const mounted: Array<{ root: Root; container: HTMLElement }> = [];

export function render(element: ReactElement): HTMLElement {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const root = createRoot(container);
  act(() => root.render(element));
  mounted.push({ root, container });
  return container;
}

export function unmountAll(): void {
  for (const { root, container } of mounted.splice(0)) {
    act(() => root.unmount());
    container.remove();
  }
}

export function resetStores(): void {
  goOffline();
  unloadModuleUis();
  useTelemetry.setState({ tags: new Map(), stream: 'IDLE', lastBatchAtMs: null, wantOnline: false });
  useController.setState({ activeProjectId: undefined, io: null, tagMeta: new Map(), error: null });
  useLog.setState({ entries: [] });
  useIoConfig.getState().reset();
  useProgramTags.getState().reset();
  useIoConfig.setState({ drivers: [], driverInfo: [], schemas: {} });
  useSession.setState({ phase: 'loading', user: null, error: null, notice: null });
  useWorkbench.setState({
    view: 'portal',
    settingsSection: 'account',
    settingsReturn: 'portal',
    moduleView: null,
    project: null,
    editors: [],
    activeEditorId: null,
    selection: null,
    inspectorTab: 'properties',
    panes: { tree: true, inspector: true, taskCards: true },
  });
}

export function project(id: string, name = id): ProjectConfig {
  return {
    id,
    name,
    version: '1.0.0',
    description: '',
    author: '',
    created_at_utc: '',
    updated_at_utc: '',
  };
}

export function connection(id: string, tags: string[], over: Partial<ConnectionStatus> = {}): ConnectionStatus {
  return {
    id,
    driver: 'modbus',
    state: 'RUNNING',
    detail: null,
    tags,
    poll_interval_ms: 250,
    cycle_timeout_ms: 2000,
    cycles_completed: 10,
    last_cycle_unix_ms: 1_700_000_000_000,
    last_cycle_error: null,
    tags_good: tags.length,
    faults: [],
    ...over,
  };
}

export function ioStatus(connections: ConnectionStatus[]): IoStatus {
  return { source: '/projects/p/io.json', note: null, connections };
}

export function sample(uri: string, over: Partial<TagSample> = {}): TagSample {
  return {
    uri,
    type: 'UINT',
    value: 1234,
    qualityName: 'GOOD',
    qualityCode: 0x00c0,
    timestampNs: 1_700_000_000_000_000_000,
    ...over,
  };
}

/** Text of every element with the given test id, in document order. */
export function texts(container: HTMLElement, testId: string): string[] {
  return [...container.querySelectorAll(`[data-testid="${testId}"]`)].map(
    (element) => element.textContent ?? ''
  );
}

/** A stored Modbus TCP connection with one holding-register tag per given URI. */
export function modbusConnection(id: string, uris: string[], port = 502) {
  return {
    id,
    driver: 'modbus',
    poll_interval_ms: 250,
    config: {
      transport: 'tcp',
      host: '192.0.2.10',
      port,
      slave_id: 1,
      timeout_ms: 1000,
      mappings: uris.map((uri, index) => ({
        uri,
        address: index,
        function_code: 3,
        data_type: 'u16',
        word_order: 'high_first',
        scale: 1,
      })),
    },
  };
}

/** Put the I/O configuration store in the state it has after loading `config` for a project. */
export function loadedIoConfig(projectId: string, config: IoConfig | null): void {
  const draft = toForm(config, SCHEMAS);
  useIoConfig.setState({
    projectId,
    phase: 'ready',
    loadError: null,
    saved: config,
    draft,
    baseline: JSON.stringify(draft, (key, value: unknown) => (key === 'key' ? undefined : value)),
    drivers: ['modbus'],
    driverInfo: [MODBUS_DRIVER],
    schemas: SCHEMAS,
    fieldErrors: [],
    checking: false,
    problems: [],
    saveError: null,
  });
}

/** Type into a field the way a keystroke does, so React sees the change. */
export function typeInto(element: Element | null, value: string): void {
  if (!(element instanceof HTMLInputElement) && !(element instanceof HTMLSelectElement)) {
    throw new Error('not a field');
  }
  const proto = element instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
  const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set;
  act(() => {
    setter?.call(element, value);
    element.dispatchEvent(
      new Event(element instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true })
    );
  });
}
