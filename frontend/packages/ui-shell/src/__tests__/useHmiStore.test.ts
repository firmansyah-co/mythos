import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { useHmiStore } from '../stores/useHmiStore';

describe('useHmiStore (TSK-308: 3-Stage Lifecycle & Screen Hierarchy)', () => {
  beforeEach(() => {
    // Mock global.fetch to cleanly handle HTTP endpoints without network errors
    global.fetch = vi.fn().mockImplementation(async (url: string) => {
      if (url.includes('/api/v1/auth/verify-role')) {
        return {
          ok: false,
          status: 401,
          json: async () => ({ authorized: false }),
        };
      }
      return {
        ok: false,
        status: 404,
        json: async () => ({}),
      };
    });

    // Reset store state with sample project for legacy screen hierarchy tests
    const state = useHmiStore.getState();
    state.loadSampleProject();
    state.enterEditMode();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('initializes with default screens and EDIT mode', () => {
    const state = useHmiStore.getState();
    expect(state.screens.length).toBeGreaterThan(0);
    expect(state.screens[0].name).toBe('Screen_01_PlantOverview');
    expect(state.lifecycleMode).toBe('EDIT');
  });

  it('transitions cleanly between 3-stage lifecycle modes', () => {
    const { enterEditMode, enterPreviewMode, enterDeployMode, enterRuntimeMode } = useHmiStore.getState();

    enterPreviewMode();
    expect(useHmiStore.getState().lifecycleMode).toBe('PREVIEW');

    enterDeployMode();
    expect(useHmiStore.getState().lifecycleMode).toBe('DEPLOY');

    enterRuntimeMode();
    expect(useHmiStore.getState().lifecycleMode).toBe('RUNTIME');

    enterEditMode();
    expect(useHmiStore.getState().lifecycleMode).toBe('EDIT');
  });

  it('opens and closes screen tabs dynamically', () => {
    const { openScreenTab, closeScreenTab } = useHmiStore.getState();

    // Open Screen 02
    openScreenTab('screen_02_boiler_area');
    let state = useHmiStore.getState();
    expect(state.openTabs.some((t) => t.tabId === 'screen_02_boiler_area')).toBe(true);
    expect(state.activeTabId).toBe('screen_02_boiler_area');

    // Open Subscreen
    openScreenTab('screen_02_boiler_area', 'sub_boiler_tuning');
    state = useHmiStore.getState();
    expect(state.openTabs.some((t) => t.tabId === 'screen_02_boiler_area::sub_boiler_tuning')).toBe(true);
    expect(state.activeTabId).toBe('screen_02_boiler_area::sub_boiler_tuning');

    // Close Subscreen Tab
    closeScreenTab('screen_02_boiler_area::sub_boiler_tuning');
    state = useHmiStore.getState();
    expect(state.openTabs.some((t) => t.tabId === 'screen_02_boiler_area::sub_boiler_tuning')).toBe(false);
  });

  it('adds new screens and nested sub-screens', async () => {
    const { addNewScreen, addNewSubScreen } = useHmiStore.getState();

    const newId = await addNewScreen('Screen_03_PackagingArea');
    expect(newId).toBeDefined();

    let state = useHmiStore.getState();
    const createdScreen = state.screens.find((s) => s.id === newId);
    expect(createdScreen).toBeDefined();
    expect(createdScreen?.name).toBe('Screen_03_PackagingArea');

    const newSubId = await addNewSubScreen(newId, 'Packaging Palletizer Diagnostics');
    state = useHmiStore.getState();
    const updatedScreen = state.screens.find((s) => s.id === newId);
    expect(updatedScreen?.subscreens.some((sb) => sb.id === newSubId)).toBe(true);
  });

  it('manages draft updates and draft commit / discard cycles', async () => {
    const { updateDraftCanvas, discardHmiDraft, commitHmiDraft } = useHmiStore.getState();

    updateDraftCanvas('screen_01_plant_overview', {
      title: 'Modified Canvas',
      components: [{ id: 'pump-1', type: 'Pump' }],
    });

    let state = useHmiStore.getState();
    expect(state.isHmiDirty).toBe(true);
    expect(state.draftScreens.get('screen_01_plant_overview')?.title).toBe('Modified Canvas');

    // Discard draft
    discardHmiDraft();
    state = useHmiStore.getState();
    expect(state.isHmiDirty).toBe(false);

    // Update again and commit
    updateDraftCanvas('screen_01_plant_overview', {
      title: 'Final Canvas',
    });
    const success = await commitHmiDraft();
    expect(success).toBe(true);
    state = useHmiStore.getState();
    expect(state.isHmiDirty).toBe(false);
    expect(state.lifecycleMode).toBe('RUNTIME');
  });

  it('enforces IEC 62443 password challenge gate on kiosk exit', async () => {
    const { requestEnterRuntime, requestExitRuntime } = useHmiStore.getState();

    await requestEnterRuntime();
    let state = useHmiStore.getState();
    expect(state.isRuntimeFullscreen).toBe(true);
    expect(state.lifecycleMode).toBe('RUNTIME');

    // Wrong password / role rejected
    const failedExit = await requestExitRuntime({
      role: 'Operator',
      password: 'wrong',
    });
    expect(failedExit).toBe(false);
    expect(useHmiStore.getState().isRuntimeFullscreen).toBe(true);

    // Correct Engineer password accepted
    const successExit = await requestExitRuntime({
      role: 'Engineer',
      password: 'admin123',
    });
    expect(successExit).toBe(true);
    state = useHmiStore.getState();
    expect(state.isRuntimeFullscreen).toBe(false);
    expect(state.lifecycleMode).toBe('EDIT');
  });
});
