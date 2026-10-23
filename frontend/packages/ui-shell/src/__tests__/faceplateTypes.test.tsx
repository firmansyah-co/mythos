import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import React, { act } from 'react';
import { createRoot, Root } from 'react-dom/client';
import { useHmiStore } from '../stores/useHmiStore';
import { useFaceplateStore } from '../stores/useFaceplateStore';
import { Zone2ProjectTree } from '../components/workbench/Zone2ProjectTree';
import { Zone4TaskCards } from '../components/workbench/Zone4TaskCards';
import { FaceplateTypeDesigner } from '../components/faceplates/FaceplateTypeDesigner';
import { FaceplateType, ResolvedFaceplateInstance } from '../types/faceplate';

// React 19 only flushes updates inside act() when the environment declares itself an act env.
(globalThis as any).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): void {
  act(() => {
    root.render(element);
  });
}

/** Let store updates from the mocked fetch land and re-render. */
async function settle() {
  await act(async () => {
    for (let i = 0; i < 6; i += 1) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function waitFor(check: () => boolean, label: string) {
  for (let i = 0; i < 60; i += 1) {
    if (check()) return;
    await settle();
  }
  throw new Error(`Timed out waiting for ${label}`);
}

const PROJECT_ID = 'faceplate_test_project';

const MOTOR_UDT = {
  name: 'UDT_Motor_VFD',
  kind: 'STRUCT' as const,
  members: [
    { name: 'bRunning', dataType: 'BOOL' },
    { name: 'rSpeedFeedback', dataType: 'REAL' },
  ],
};

const SUMMARY_VIEW = {
  kind: 'SUMMARY_CARD' as const,
  bindings: [
    { faceplateKey: 'running', udtProperty: 'bRunning', access: 'READ_ONLY' as const },
    { faceplateKey: 'actualSpeed', udtProperty: 'rSpeedFeedback', access: 'READ_ONLY' as const },
  ],
};

function masterType(overrides: Partial<FaceplateType> = {}): FaceplateType {
  return {
    id: 'ft_motor',
    name: 'Motor_VFD',
    version: '1.0.0',
    udt: MOTOR_UDT,
    views: [SUMMARY_VIEW],
    activeVersion: '1.0.0',
    hasDraft: false,
    instanceCount: 1,
    ...overrides,
  };
}

/** An instance is rendered entirely from what the daemon resolved against its master. */
function resolvedInstance(
  overrides: Partial<ResolvedFaceplateInstance> = {}
): ResolvedFaceplateInstance {
  return {
    id: 'fi_p101',
    name: 'P101',
    typeId: 'ft_motor',
    typeVersion: null,
    bindings: [{ faceplateKey: 'running', tagUri: 'external.plant1.P101.bRunning' }],
    resolvedVersion: '1.0.0',
    frozen: false,
    udt: MOTOR_UDT,
    views: [SUMMARY_VIEW],
    warnings: [],
    ...overrides,
  };
}

/** Route the two faceplate list calls; every other URL is a test failure. */
function mockFaceplateApi(types: FaceplateType[], instances: ResolvedFaceplateInstance[]) {
  global.fetch = vi.fn(async (input: unknown) => {
    const url = String(input);
    if (url === `/api/v1/projects/${PROJECT_ID}/faceplates/types`) {
      return { ok: true, status: 200, json: async () => ({ types }) };
    }
    if (url === `/api/v1/projects/${PROJECT_ID}/faceplates/instances`) {
      return { ok: true, status: 200, json: async () => ({ instances }) };
    }
    throw new Error(`Unexpected request: ${url}`);
  }) as unknown as typeof fetch;
}

function openProject() {
  useHmiStore.getState().setCurrentProject({
    id: PROJECT_ID,
    name: 'Faceplate_Test',
    version: '1.0.0',
  });
}

/** Expand a tree node by its visible label, the same way an engineer would. */
function clickTreeLabel(label: string) {
  const labelNode = Array.from(container.querySelectorAll('span')).find(
    (node) => node.textContent?.trim() === label
  );
  if (!labelNode) throw new Error(`Tree node "${label}" not found`);
  const clickable = labelNode.closest('div');
  if (!clickable) throw new Error(`Tree node "${label}" has no clickable ancestor`);
  act(() => {
    clickable.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
}

describe('Faceplate master types (TSK-309): real data only, no phantom library', () => {
  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    useHmiStore.getState().resetToEmptyProject();
    useFaceplateStore.getState().reset();
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
    vi.restoreAllMocks();
  });

  it('shows a clean empty state and no faceplate type name when the project has none', async () => {
    openProject();
    mockFaceplateApi([], []);

    renderClient(
      <div>
        <Zone2ProjectTree />
        <Zone4TaskCards isCollapsed={false} onToggleCollapse={() => {}} />
      </div>
    );
    await waitFor(
      () => useFaceplateStore.getState().status === 'READY',
      'faceplate library to load'
    );

    clickTreeLabel('Faceplate Types');
    await settle();

    const html = container.innerHTML;

    // Negative assertion, deliberately broad: any fabricated FP_* name trips it, not just the
    // two literals that used to be hardcoded in this tree.
    expect(html).not.toMatch(/FP_[A-Za-z]/);
    expect(html).not.toContain('v1.0.0');
    expect(html).not.toContain('P&ID, Card, Popup, Alarm');

    expect(container.querySelector('[data-testid="empty-faceplates-notice"]')).not.toBeNull();
    expect(container.querySelectorAll('[data-testid="faceplate-type-node"]')).toHaveLength(0);
    expect(useFaceplateStore.getState().types).toHaveLength(0);
  });

  it('renders exactly the types the project has, with no invented entries', async () => {
    openProject();
    mockFaceplateApi([masterType()], [resolvedInstance()]);

    renderClient(<Zone2ProjectTree />);
    await waitFor(
      () => useFaceplateStore.getState().status === 'READY',
      'faceplate library to load'
    );

    clickTreeLabel('Faceplate Types');
    await settle();

    const nodes = Array.from(container.querySelectorAll('[data-testid="faceplate-type-node"]'));
    expect(nodes).toHaveLength(1);
    expect(nodes[0].textContent).toContain('Motor_VFD');
    expect(container.innerHTML).not.toMatch(/FP_[A-Za-z]/);
    expect(container.querySelector('[data-testid="empty-faceplates-notice"]')).toBeNull();
  });

  it('never offers the sample UDT placeholders in the type creator', () => {
    openProject();
    mockFaceplateApi([], []);

    renderClient(<FaceplateTypeDesigner typeId="new" />);

    const html = container.innerHTML;
    expect(html).not.toContain('DUT_BottleSpec');
    expect(html).not.toContain('DUT_MachineState');

    // The member data-type picker only offers elementary IEC types; no project type can be
    // selected because the project has none.
    const options = Array.from(container.querySelectorAll('option')).map((option) =>
      option.textContent?.trim()
    );
    expect(options).not.toContain('DUT_BottleSpec');
    expect(options).toContain('BOOL');
  });

  it('renders an instance from its master, and follows the master when the master changes', async () => {
    openProject();
    mockFaceplateApi([masterType()], [resolvedInstance()]);

    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(
      () => container.querySelector('[data-testid="faceplate-instance-card"]') !== null,
      'instance card to render'
    );

    const firstCard = container.querySelector('[data-testid="faceplate-instance-card"]')!;
    expect(firstCard.textContent).toContain('running←bRunning');
    expect(firstCard.getAttribute('data-resolved-version')).toBe('1.0.0');

    // The master gains a binding and a version. The instance record is unchanged on the daemon,
    // so any on-screen change can only come from re-resolving the instance against the master.
    const nextView = {
      kind: 'SUMMARY_CARD' as const,
      bindings: [
        ...SUMMARY_VIEW.bindings,
        { faceplateKey: 'currentAmps', udtProperty: 'rSpeedFeedback', access: 'READ_ONLY' as const },
      ],
    };
    mockFaceplateApi(
      [masterType({ version: '2.0.0', activeVersion: '2.0.0', views: [nextView] })],
      [resolvedInstance({ resolvedVersion: '2.0.0', views: [nextView] })]
    );

    await act(async () => {
      await useFaceplateStore.getState().refresh();
    });

    const refreshed = container.querySelector('[data-testid="faceplate-instance-card"]')!;
    expect(refreshed.textContent).toContain('currentAmps←rSpeedFeedback');
    expect(refreshed.getAttribute('data-resolved-version')).toBe('2.0.0');
  });

  it('renders nothing and says so when a frozen instance has no master for its version', async () => {
    openProject();
    mockFaceplateApi(
      [masterType()],
      [
        resolvedInstance({
          frozen: true,
          typeVersion: 1,
          resolvedVersion: null,
          udt: null,
          views: null,
          warnings: ['frozen at v1, but that version is missing from history'],
        }),
      ]
    );

    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(
      () => container.querySelector('[data-testid="faceplate-instance-card"]') !== null,
      'instance card to render'
    );

    expect(container.querySelector('[data-testid="instance-frozen-badge"]')).not.toBeNull();
    expect(container.querySelector('[data-testid="instance-master-unavailable"]')).not.toBeNull();
    // The warning is the daemon's own text, not one we made up.
    expect(container.innerHTML).toContain('frozen at v1, but that version is missing from history');
    expect(container.innerHTML).not.toMatch(/FP_[A-Za-z]/);
  });
});