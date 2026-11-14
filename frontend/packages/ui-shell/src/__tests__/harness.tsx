// Shared test plumbing: mount a component into happy-dom, and put every store back to a known
// state between tests.

import { act } from 'react';
import type { ReactElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import type { ConnectionStatus, IoStatus, ProjectConfig } from '../api/client';
import { useController } from '../data/controller';
import { useLog } from '../data/log';
import type { TagSample } from '../data/quality';
import { goOffline, useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

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
  useTelemetry.setState({ tags: new Map(), stream: 'IDLE', lastBatchAtMs: null, wantOnline: false });
  useController.setState({ activeProjectId: undefined, io: null, tagMeta: new Map(), error: null });
  useLog.setState({ entries: [] });
  useWorkbench.setState({
    view: 'portal',
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
