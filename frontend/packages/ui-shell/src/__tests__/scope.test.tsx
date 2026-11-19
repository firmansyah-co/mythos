import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { useController } from '../data/controller';
import { runtimeScope } from '../data/scope';
import { useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';
import { ConnectionDiagnostics } from '../editors/ConnectionDiagnostics';
import { DevicesAndNetworks } from '../editors/DevicesAndNetworks';
import { TagTable } from '../editors/TagTable';
import { Inspector } from '../shell/Inspector';
import { ProjectTree } from '../shell/ProjectTree';
import { ProjectView } from '../shell/ProjectView';
import { OnlineTools } from '../shell/TaskCards';
import {
  connection,
  ioStatus,
  loadedIoConfig,
  modbusConnection,
  project,
  render,
  resetStores,
  sample,
  unmountAll,
} from './harness';

const TAG = 'external.BoilerHouse.Unit1.steam_temp';
const OWN_TAG = 'external.Intake.LT201.level';

/** The controller runs `boiler_house`, with one connection, one tag and one fault. */
function controllerRunsBoilerHouse() {
  useController.setState({
    activeProjectId: 'boiler_house',
    io: ioStatus([
      connection('boiler-plc', [TAG], {
        tags_good: 0,
        faults: [{ uri: TAG, reason: 'communication failure: no answer' }],
      }),
    ]),
  });
  useTelemetry.setState({ tags: new Map([[TAG, sample(TAG)]]), stream: 'OPEN', lastBatchAtMs: 1 });
}

function open(projectId: string) {
  act(() => useWorkbench.getState().openProject(project(projectId)));
}

beforeEach(resetStores);
afterEach(unmountAll);

describe('runtimeScope', () => {
  const io = ioStatus([]);
  it('is unknown until the controller has answered', () => {
    expect(runtimeScope('p', undefined, null, null)).toEqual({ kind: 'unknown' });
    expect(runtimeScope('p', 'p', null, null)).toEqual({ kind: 'unknown' });
  });
  it('is unreachable whenever there is an error, whatever else is cached', () => {
    expect(runtimeScope('p', 'p', io, 'boom')).toEqual({ kind: 'unreachable', error: 'boom' });
  });
  it('is not-active for any project other than the one the controller runs', () => {
    expect(runtimeScope('p', 'q', io, null)).toEqual({ kind: 'not-active', activeProjectId: 'q' });
    expect(runtimeScope('p', null, io, null)).toEqual({ kind: 'not-active', activeProjectId: null });
  });
  it('is active only for the running project', () => {
    expect(runtimeScope('p', 'p', io, null)).toEqual({ kind: 'active', io });
  });
});

describe('a project the controller is not running', () => {
  beforeEach(() => {
    controllerRunsBoilerHouse();
    open('water_intake');
  });

  it.each([
    ['the tag table', () => <TagTable />],
    ['a connection-scoped tag table', () => <TagTable connectionId="boiler-plc" />],
    ['connection diagnostics', () => <ConnectionDiagnostics connectionId="boiler-plc" />],
  ])('%s shows none of the running project’s devices or tags', (_name, element) => {
    const container = render(element());
    expect(container.querySelector('[data-testid="gate-not-active"]')).not.toBeNull();
    expect(container.querySelectorAll('[data-testid="tag-row"]').length).toBe(0);
    expect(container.textContent).not.toContain('boiler-plc');
    expect(container.textContent).not.toContain(TAG);
  });

  it('devices & networks shows its own configuration and nothing of the running project', () => {
    // water_intake has one configured connection of its own; boiler-plc is the running one.
    act(() => loadedIoConfig('water_intake', { connections: [modbusConnection('intake-plc', [OWN_TAG])] }));
    const container = render(<DevicesAndNetworks />);

    const rows = [...container.querySelectorAll('[data-testid="connection-row"]')];
    expect(rows.map((row) => row.querySelector('td')?.textContent)).toEqual(['intake-plc']);
    expect(container.querySelector('[data-testid="config-not-active"]')).not.toBeNull();
    expect(container.textContent).not.toContain('boiler-plc');
    // No runtime state is claimed for a connection the controller is not running.
    expect(container.querySelector('.badge--good, .badge--warn')).toBeNull();
  });

  it('the project tree lists its configured connections and no runtime under them', () => {
    act(() => loadedIoConfig('water_intake', { connections: [modbusConnection('intake-plc', [OWN_TAG])] }));
    const container = render(<ProjectTree />);

    const connections = [...container.querySelectorAll('[data-testid="tree-connection"]')];
    expect(connections.map((node) => node.textContent)).toEqual(['intake-plc [modbus]']);
    expect(container.querySelector('[data-testid="tree-runtime-tags"]')).toBeNull();
    expect(container.querySelector('[data-testid="tree-runtime-diagnostics"]')).toBeNull();
    expect(container.querySelector('[data-testid="tree-all-tags"]')).toBeNull();
    expect(container.querySelector('[data-testid="tree-not-active"]')).not.toBeNull();
    expect(container.textContent).not.toContain('boiler-plc');
  });

  it('a connection named like a running one is not given that connection’s state', () => {
    act(() => loadedIoConfig('water_intake', { connections: [modbusConnection('boiler-plc', [OWN_TAG])] }));
    const container = render(<DevicesAndNetworks />);
    const cells = [...container.querySelectorAll('[data-testid="connection-row"] td')].map(
      (cell) => cell.textContent
    );
    expect(cells[0]).toBe('boiler-plc');
    // The "On the controller" column: a dash, not the running connection's badge.
    expect(cells[5]).toBe('—');
    expect(container.querySelector('[data-testid="connection-row"] .badge')).toBeNull();
    expect(container.textContent).not.toContain('communication failure');
  });

  it('a connection named like a running one still gets no runtime nodes', () => {
    // Same name as the connection the controller runs for the other project.
    act(() => loadedIoConfig('water_intake', { connections: [modbusConnection('boiler-plc', [OWN_TAG])] }));
    const container = render(<ProjectTree />);
    expect(container.querySelectorAll('[data-testid="tree-connection"]').length).toBe(1);
    expect(container.querySelector('[data-testid="tree-runtime-tags"]')).toBeNull();
    expect(container.querySelector('[data-testid="tree-runtime-diagnostics"]')).toBeNull();
  });

  it('the inspector counts no problems and lists none', () => {
    act(() => useWorkbench.getState().setInspectorTab('diagnostics'));
    const container = render(<Inspector collapsed={false} />);
    expect(container.querySelector('[data-testid="problem-count"]')).toBeNull();
    expect(container.textContent).not.toContain('communication failure');
  });

  it('the online tools card reports no tag counts for it', () => {
    const container = render(<OnlineTools />);
    expect(container.querySelector('[data-testid="card-not-active"]')).not.toBeNull();
  });

  it('cannot be taken online', () => {
    act(() => useTelemetry.setState({ stream: 'IDLE', wantOnline: false }));
    const container = render(<ProjectView onAbout={() => {}} />);
    const goOnlineButton = [...container.querySelectorAll('.toolbar button')].find((button) =>
      button.textContent?.includes('Go online')
    );
    expect(goOnlineButton?.hasAttribute('disabled')).toBe(true);
  });
});

describe('the project the controller is running', () => {
  beforeEach(() => {
    controllerRunsBoilerHouse();
    open('boiler_house');
  });

  it('shows its connections, its tags and its problems', () => {
    act(() => loadedIoConfig('boiler_house', { connections: [modbusConnection('boiler-plc', [TAG])] }));
    const tree = render(<ProjectTree />);
    expect(tree.querySelectorAll('[data-testid="tree-connection"]').length).toBe(1);
    expect(tree.querySelectorAll('[data-testid="tree-runtime-tags"]').length).toBe(1);
    expect(tree.querySelectorAll('[data-testid="tree-runtime-diagnostics"]').length).toBe(1);

    const table = render(<TagTable />);
    expect(table.querySelectorAll('[data-testid="tag-row"]').length).toBe(1);

    const inspector = render(<Inspector collapsed={false} />);
    expect(inspector.querySelector('[data-testid="problem-count"]')?.textContent).toBe('1');
  });

  it('stops showing all of it the moment the controller cannot be reached', () => {
    act(() => loadedIoConfig('boiler_house', { connections: [modbusConnection('boiler-plc', [TAG])] }));
    const table = render(<TagTable />);
    const tree = render(<ProjectTree />);
    expect(table.querySelectorAll('[data-testid="tag-row"]').length).toBe(1);
    expect(tree.querySelectorAll('[data-testid="tree-runtime-tags"]').length).toBe(1);

    act(() =>
      useController.setState({
        activeProjectId: undefined,
        io: null,
        tagMeta: new Map(),
        error: 'The controller did not answer (network error)',
      })
    );

    expect(table.querySelector('[data-testid="gate-unreachable"]')).not.toBeNull();
    expect(table.querySelectorAll('[data-testid="tag-row"]').length).toBe(0);
    // The configured connection is still listed; what is gone is everything known at run time.
    expect(tree.querySelectorAll('[data-testid="tree-connection"]').length).toBe(1);
    expect(tree.querySelectorAll('[data-testid="tree-runtime-tags"]').length).toBe(0);
    expect(tree.querySelectorAll('[data-testid="tree-runtime-diagnostics"]').length).toBe(0);
  });
});
