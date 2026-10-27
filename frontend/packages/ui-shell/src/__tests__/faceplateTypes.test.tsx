import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import React, { act } from 'react';
import { createRoot, Root } from 'react-dom/client';
import { useHmiStore } from '../stores/useHmiStore';
import { useFaceplateStore } from '../stores/useFaceplateStore';
import { Zone2ProjectTree } from '../components/workbench/Zone2ProjectTree';
import { Zone4TaskCards } from '../components/workbench/Zone4TaskCards';
import { FaceplateTypeDesigner } from '../components/faceplates/FaceplateTypeDesigner';
import { FaceplateType, FaceplateTypeSummary, ResolvedInstance } from '../types/faceplate';

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
  enumConstants: [] as string[],
  baseType: null,
};

const SUMMARY_VIEW = {
  kind: 'SUMMARY_CARD' as const,
  bindings: [
    { faceplateKey: 'running', udtProperty: 'bRunning', access: 'READ_ONLY' as const, unit: null },
    { faceplateKey: 'actualSpeed', udtProperty: 'rSpeedFeedback', access: 'READ_ONLY' as const, unit: 'RPM' },
  ],
};

function summary(overrides: Partial<FaceplateTypeSummary> = {}): FaceplateTypeSummary {
  return {
    id: 'ft_motor',
    name: 'Motor_VFD',
    activeVersion: 1,
    hasDraft: false,
    udtName: MOTOR_UDT.name,
    instanceCount: 1,
    ...overrides,
  };
}

function masterType(overrides: Partial<FaceplateType> = {}): FaceplateType {
  return {
    id: 'ft_motor',
    name: 'Motor_VFD',
    version: 1,
    udt: MOTOR_UDT,
    views: [SUMMARY_VIEW],
    ...overrides,
  };
}

/** An instance is rendered entirely from what the daemon resolved against its master. */
function resolvedInstance(overrides: Partial<ResolvedInstance> = {}): ResolvedInstance {
  return {
    id: 'fi_p101',
    name: 'P101',
    typeId: 'ft_motor',
    typeVersion: null,
    resolvedVersion: 1,
    frozen: false,
    udt: MOTOR_UDT,
    views: [SUMMARY_VIEW],
    bindings: [{ faceplateKey: 'running', tagUri: 'external.plant1.P101.bRunning' }],
    warnings: [],
    ...overrides,
  };
}

/** Route the faceplate routes; every other URL is a test failure. */
function mockFaceplateApi(
  types: FaceplateTypeSummary[],
  instances: ResolvedInstance[],
  fullType: FaceplateType | null = null
) {
  global.fetch = vi.fn(async (input: unknown) => {
    const url = String(input);
    if (url === `/api/v1/projects/${PROJECT_ID}/faceplates/types`) {
      return { ok: true, status: 200, json: async () => ({ types }) };
    }
    if (url === `/api/v1/projects/${PROJECT_ID}/faceplates/instances`) {
      return { ok: true, status: 200, json: async () => ({ instances }) };
    }
    if (url === `/api/v1/projects/${PROJECT_ID}/faceplates/types/ft_motor/release`) {
      return {
        ok: true,
        status: 200,
        json: async () => ({
          id: 'ft_motor',
          version: 2,
          instancesAffected: instances.length,
          strandedInstances: [],
        }),
      };
    }
    if (url.startsWith(`/api/v1/projects/${PROJECT_ID}/faceplates/types/`)) {
      if (!fullType) {
        return { ok: false, status: 404, json: async () => ({ error: 'Faceplate type not found' }) };
      }
      return { ok: true, status: 200, json: async () => fullType };
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
    mockFaceplateApi([summary()], [resolvedInstance()], masterType());

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

  it('never offers the sample UDT placeholders in the type creator or the type editor', async () => {
    openProject();
    mockFaceplateApi([summary()], [resolvedInstance()], masterType());

    renderClient(<FaceplateTypeDesigner typeId="new" />);
    await settle();
    expect(container.innerHTML).not.toContain('DUT_BottleSpec');
    expect(container.innerHTML).not.toContain('DUT_MachineState');

    // The member data-type picker only offers elementary IEC types.
    const creatorOptions = Array.from(container.querySelectorAll('option')).map((option) =>
      option.textContent?.trim()
    );
    expect(creatorOptions).not.toContain('DUT_BottleSpec');
    expect(creatorOptions).toContain('BOOL');

    // The existing-type editor is the other place a type picker could appear.
    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(
      () => container.querySelector('[data-testid="udt-binding-section"]') !== null,
      'UDT binding section to render'
    );
    const editorOptions = Array.from(container.querySelectorAll('option')).map((option) =>
      option.textContent?.trim()
    );
    expect(editorOptions).not.toContain('DUT_BottleSpec');
    expect(editorOptions).not.toContain('DUT_MachineState');
    // Bindings offer only the UDT members this master declares.
    expect(editorOptions).toContain('bRunning');
    expect(container.innerHTML).not.toContain('DUT_BottleSpec');
  });

  it('renders an instance from its master, and follows the master when the master changes', async () => {
    openProject();
    mockFaceplateApi([summary()], [resolvedInstance()], masterType());

    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(
      () => container.querySelector('[data-testid="faceplate-instance-card"]') !== null,
      'instance card to render'
    );

    const firstCard = container.querySelector('[data-testid="faceplate-instance-card"]')!;
    expect(firstCard.textContent).toContain('running←bRunning');
    expect(firstCard.getAttribute('data-resolved-version')).toBe('1');

    // The master gains a binding and a version. The instance record is unchanged on the daemon,
    // so any on-screen change can only come from re-resolving the instance against the master.
    const nextView = {
      kind: 'SUMMARY_CARD' as const,
      bindings: [
        ...SUMMARY_VIEW.bindings,
        {
          faceplateKey: 'currentAmps',
          udtProperty: 'rSpeedFeedback',
          access: 'READ_ONLY' as const,
          unit: null,
        },
      ],
    };
    mockFaceplateApi(
      [summary({ activeVersion: 2 })],
      [resolvedInstance({ resolvedVersion: 2, views: [nextView] })],
      masterType({ version: 2, views: [nextView] })
    );

    await act(async () => {
      await useFaceplateStore.getState().release('ft_motor');
    });
    await settle();

    const refreshed = container.querySelector('[data-testid="faceplate-instance-card"]')!;
    expect(refreshed.textContent).toContain('currentAmps←rSpeedFeedback');
    expect(refreshed.getAttribute('data-resolved-version')).toBe('2');
  });

  it('renders nothing and says so when a frozen instance has no master for its version', async () => {
    openProject();
    mockFaceplateApi(
      [summary()],
      [
        resolvedInstance({
          frozen: true,
          typeVersion: 1,
          resolvedVersion: null,
          udt: null,
          views: null,
          warnings: ['frozen at v1, but that version is missing from history'],
        }),
      ],
      masterType()
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

  it('names the frozen instances a release left behind instead of reporting a silent success', async () => {
    openProject();
    mockFaceplateApi([summary()], [resolvedInstance()], masterType());

    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(
      () => container.querySelector('[data-testid="faceplate-instance-card"]') !== null,
      'instance card to render'
    );

    const fetchMock = vi.fn(async (input: unknown) => {
      const url = String(input);
      if (url === `/api/v1/projects/${PROJECT_ID}/faceplates/types/ft_motor/release`) {
        return {
          ok: true,
          status: 200,
          json: async () => ({
            id: 'ft_motor',
            version: 2,
            instancesAffected: 0,
            strandedInstances: [{ id: 'fi_p101', name: 'P101', typeVersion: 1 }],
          }),
        };
      }
      if (url === `/api/v1/projects/${PROJECT_ID}/faceplates/types`) {
        return { ok: true, status: 200, json: async () => ({ types: [summary({ activeVersion: 2 })] }) };
      }
      if (url === `/api/v1/projects/${PROJECT_ID}/faceplates/instances`) {
        return {
          ok: true,
          status: 200,
          json: async () => ({
            instances: [resolvedInstance({ frozen: true, typeVersion: 1 })],
          }),
        };
      }
      if (url.startsWith(`/api/v1/projects/${PROJECT_ID}/faceplates/types/`)) {
        return { ok: true, status: 200, json: async () => masterType({ version: 2 }) };
      }
      throw new Error(`Unexpected request: ${url}`);
    });
    global.fetch = fetchMock as unknown as typeof fetch;

    await act(async () => {
      await useFaceplateStore.getState().release('ft_motor');
    });
    await settle();

    const stranded = container.querySelector('[data-testid="stranded-instances"]');
    expect(stranded).not.toBeNull();
    expect(stranded!.textContent).toContain('P101');
    expect(stranded!.textContent).toContain('v1');
    // It is a warning, not a failure: the release still reported its version.
    expect(container.querySelector('[data-testid="faceplate-release-result"]')!.textContent).toContain(
      'Released v2'
    );
  });
});
/**
 * Review round 2 — the two blocking defects.
 *
 * B1: `release()` publishes the server draft while the editor holds unsaved edits; the refresh
 *     then remounted the editor (the key embedded `views`) and the edits vanished with nothing
 *     said. B1 is fixed by blocking release while dirty, and by not letting a background refresh
 *     replace a dirty draft.
 * B2: `saveDraft` / `release` / `createType` reported failure only through `errorMessage`, which
 *     no editor read — a failed click looked like a no-op.
 */
describe('Faceplate designer review round 2: unsaved work and visible failures', () => {
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

  function typeNameInput(): HTMLInputElement {
    const input = container.querySelector(
      'input[data-testid="faceplate-type-name"]'
    ) as HTMLInputElement | null;
    if (!input) throw new Error('type name input not found');
    return input;
  }

  /** Type into a controlled input the way the browser does, so React sees the change. */
  function typeInto(input: HTMLInputElement, value: string) {
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')!.set!;
    setter.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  }

  it('blocks Release while the draft has unsaved edits, and says why', async () => {
    openProject();
    mockFaceplateApi([summary()], [resolvedInstance()], masterType());
    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(
      () => useFaceplateStore.getState().openTypeStatus === 'READY',
      'type to load'
    );

    act(() => {
      typeInto(typeNameInput(), 'Motor_VFD_RENAMED');
    });
    await settle();

    const releaseBtn = container.querySelector(
      '[data-testid="release-faceplate-type"]'
    ) as HTMLButtonElement;
    expect(releaseBtn).not.toBeNull();
    expect(releaseBtn.disabled).toBe(true);
    expect(container.querySelector('[data-testid="release-blocked-unsaved"]')).not.toBeNull();

    // Clicking anyway must not release, must not claim a version, and must not drop the edit.
    act(() => {
      releaseBtn.click();
    });
    await settle();
    expect(container.querySelector('[data-testid="faceplate-release-result"]')).toBeNull();
    expect(typeNameInput().value).toBe('Motor_VFD_RENAMED');
  });

  it('keeps a dirty draft when the store refreshes underneath it', async () => {
    openProject();
    mockFaceplateApi([summary()], [resolvedInstance()], masterType());
    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(
      () => useFaceplateStore.getState().openTypeStatus === 'READY',
      'type to load'
    );

    act(() => {
      typeInto(typeNameInput(), 'Motor_VFD_RENAMED');
    });
    await settle();

    // Another station releases: the store hands the editor a fresh copy of the server type.
    act(() => {
      useFaceplateStore.setState({ openType: masterType({ version: 2 }) });
    });
    await settle();

    expect(typeNameInput().value).toBe('Motor_VFD_RENAMED');
    expect(container.querySelector('[data-testid="release-blocked-unsaved"]')).not.toBeNull();
  });

  it('shows a failed Save draft instead of looking like a no-op', async () => {
    openProject();
    mockFaceplateApi([summary()], [resolvedInstance()], masterType());
    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(
      () => useFaceplateStore.getState().openTypeStatus === 'READY',
      'type to load'
    );

    global.fetch = vi.fn(async (input: unknown, init?: RequestInit) => {
      if (init?.method === 'PUT') {
        return { ok: false, status: 500, json: async () => ({ error: 'daemon exploded' }) };
      }
      return { ok: true, status: 200, json: async () => masterType() };
    }) as unknown as typeof fetch;

    act(() => {
      typeInto(typeNameInput(), 'Motor_VFD_RENAMED');
    });
    await settle();

    const saveBtn = container.querySelector(
      '[data-testid="save-faceplate-draft"]'
    ) as HTMLButtonElement;
    act(() => {
      saveBtn.click();
    });
    await settle();

    const alert = container.querySelector('[data-testid="faceplate-write-error"]');
    expect(alert, 'a failed save must be visible in the editor').not.toBeNull();
    expect(alert!.textContent).toContain('daemon exploded');
  });

  it('shows a failed type creation instead of looking like a no-op', async () => {
    openProject();
    global.fetch = vi.fn(async (input: unknown, init?: RequestInit) => {
      const url = String(input);
      if (url.endsWith('/faceplates/types') && init?.method === 'POST') {
        return { ok: false, status: 500, json: async () => ({ error: 'daemon exploded' }) };
      }
      if (url.endsWith('/faceplates/instances')) {
        return { ok: true, status: 200, json: async () => ({ instances: [] }) };
      }
      if (url.endsWith('/faceplates/types')) {
        return { ok: true, status: 200, json: async () => ({ types: [] }) };
      }
      return { ok: false, status: 404, json: async () => ({}) };
    }) as unknown as typeof fetch;

    renderClient(<FaceplateTypeDesigner typeId="new" />);
    await settle();

    const nameInput = container.querySelector(
      '[data-testid="new-faceplate-type-name"]'
    ) as HTMLInputElement;
    const udtInput = container.querySelector(
      '[data-testid="new-faceplate-udt-name"]'
    ) as HTMLInputElement;
    expect(nameInput).not.toBeNull();
    expect(udtInput).not.toBeNull();
    act(() => {
      typeInto(nameInput, 'New_Type');
      typeInto(udtInput, 'UDT_New_Type');
    });
    await settle();

    const createBtn = container.querySelector(
      '[data-testid="create-faceplate-type"]'
    ) as HTMLButtonElement;
    expect(createBtn, 'create button not found').toBeDefined();
    expect(createBtn.disabled).toBe(false);
    act(() => {
      createBtn!.click();
    });
    await settle();

    const alert = container.querySelector('[data-testid="faceplate-write-error"]');
    expect(alert, 'a failed create must be visible').not.toBeNull();
    expect(alert!.textContent).toContain('daemon exploded');
  });
});

/**
 * The dirty gate is a two-direction property, and the first four tests only exercised one of them.
 *
 * Every one of those four passes against a STICKY gate — one that latches dirty and never clears —
 * which is the opposite failure of the same mechanism B1 was about. A gate that blocks Release
 * forever breaks the feature just as thoroughly as one that discards edits, and it was invisible to
 * the suite.
 *
 * So these assert the OUTCOME (the release happened, the blocked notice is gone), not that a button
 * became clickable: a gate that re-enables the control but still refuses to release would satisfy a
 * button-state assertion.
 */
describe('Faceplate designer: the dirty gate can come back', () => {
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

  /** A store-backed faceplate API: a saved draft is what a later read returns. */
  function mockWritableApi(initial: FaceplateType) {
    let stored = initial;
    let releaseCalls = 0;
    const savedBodies: FaceplateType[] = [];
    global.fetch = vi.fn(async (input: unknown, init?: RequestInit) => {
      const url = String(input);
      const method = init?.method ?? 'GET';
      if (url.endsWith('/faceplates/types') && method === 'GET') {
        return { ok: true, status: 200, json: async () => ({ types: [summary({ name: stored.name })] }) };
      }
      if (url.endsWith('/faceplates/instances')) {
        return { ok: true, status: 200, json: async () => ({ instances: [] }) };
      }
      if (url.endsWith('/faceplates/types/ft_motor/draft') && method === 'PUT') {
        stored = { ...JSON.parse(String(init!.body)), id: 'ft_motor' } as FaceplateType;
        savedBodies.push(stored);
        return { ok: true, status: 200, json: async () => stored };
      }
      if (url.endsWith('/faceplates/types/ft_motor/release')) {
        releaseCalls += 1;
        return {
          ok: true,
          status: 200,
          json: async () => ({ id: 'ft_motor', version: 2, instancesAffected: 0, strandedInstances: [] }),
        };
      }
      if (url.includes('/faceplates/types/')) {
        return { ok: true, status: 200, json: async () => stored };
      }
      throw new Error(`Unexpected request: ${method} ${url}`);
    }) as unknown as typeof fetch;
    return { saved: () => savedBodies, releases: () => releaseCalls };
  }

  function typeInto(input: HTMLInputElement, value: string) {
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')!.set!;
    setter.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  }

  function typeName(): HTMLInputElement {
    return container.querySelector('input[data-testid="faceplate-type-name"]') as HTMLInputElement;
  }

  function releaseButton(): HTMLButtonElement {
    return container.querySelector('[data-testid="release-faceplate-type"]') as HTMLButtonElement;
  }

  it('clears the gate after a save, and the release then actually happens', async () => {
    openProject();
    const api = mockWritableApi(masterType());
    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(() => useFaceplateStore.getState().openTypeStatus === 'READY', 'type to load');

    act(() => {
      typeInto(typeName(), 'Motor_VFD_RENAMED');
    });
    await settle();
    expect(container.querySelector('[data-testid="release-blocked-unsaved"]')).not.toBeNull();

    const save = container.querySelector('[data-testid="save-faceplate-draft"]') as HTMLButtonElement;
    act(() => {
      save.click();
    });
    await waitFor(() => api.saved().length === 1, 'the draft to be saved');
    await settle();

    // The gate cleared: not merely re-enabled, but reporting clean.
    expect(
      container.querySelector('[data-testid="release-blocked-unsaved"]'),
      'the dirty gate stayed latched after a successful save'
    ).toBeNull();
    expect(releaseButton().disabled).toBe(false);
    // The saved value is what is on screen.
    expect(typeName().value).toBe('Motor_VFD_RENAMED');

    // And the release actually succeeds — the outcome, not the control's appearance.
    act(() => {
      releaseButton().click();
    });
    await waitFor(() => api.releases() === 1, 'the release to be sent');
    await settle();
    const banner = container.querySelector('[data-testid="faceplate-release-result"]');
    expect(banner, 'release did not report a result').not.toBeNull();
    expect(banner!.textContent).toContain('Released');
    expect(container.querySelector('[data-testid="faceplate-write-error"]')).toBeNull();
  });

  it('stays clean when a save changes nothing', async () => {
    openProject();
    mockWritableApi(masterType());
    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(() => useFaceplateStore.getState().openTypeStatus === 'READY', 'type to load');

    // Untouched: already clean, and there is nothing to block.
    expect(container.querySelector('[data-testid="release-blocked-unsaved"]')).toBeNull();
    expect(releaseButton().disabled).toBe(false);
  });

  it('stays dirty when the save fails, so a lost edit is never presented as saved', async () => {
    openProject();
    mockWritableApi(masterType());
    renderClient(<FaceplateTypeDesigner typeId="ft_motor" />);
    await waitFor(() => useFaceplateStore.getState().openTypeStatus === 'READY', 'type to load');

    global.fetch = vi.fn(async (input: unknown, init?: RequestInit) => {
      const url = String(input);
      if (init?.method === 'PUT') {
        return { ok: false, status: 500, json: async () => ({ error: 'daemon exploded' }) };
      }
      if (url.endsWith('/instances')) return { ok: true, status: 200, json: async () => ({ instances: [] }) };
      if (url.endsWith('/types')) return { ok: true, status: 200, json: async () => ({ types: [summary()] }) };
      return { ok: true, status: 200, json: async () => masterType() };
    }) as unknown as typeof fetch;

    act(() => {
      typeInto(typeName(), 'Motor_VFD_RENAMED');
    });
    await settle();

    act(() => {
      (container.querySelector('[data-testid="save-faceplate-draft"]') as HTMLButtonElement).click();
    });
    await settle();

    expect(container.querySelector('[data-testid="faceplate-write-error"]')).not.toBeNull();
    expect(
      container.querySelector('[data-testid="release-blocked-unsaved"]'),
      'a failed save must leave the draft dirty and Release blocked'
    ).not.toBeNull();
    expect(releaseButton().disabled).toBe(true);
    expect(typeName().value).toBe('Motor_VFD_RENAMED');
  });
});
