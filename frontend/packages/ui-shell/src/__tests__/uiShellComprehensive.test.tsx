import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { useHmiStore } from '../stores/useHmiStore';
import { NonActuatingPreviewBanner } from '../components/banner/NonActuatingPreviewBanner';
import { ExitKioskDialog } from '../components/kiosk/ExitKioskDialog';
import { Zone1TopNav } from '../components/workbench/Zone1TopNav';
import { Zone2ProjectTree } from '../components/workbench/Zone2ProjectTree';
import { Zone4TaskCards } from '../components/workbench/Zone4TaskCards';
import { Zone5BottomConsole } from '../components/workbench/Zone5BottomConsole';
import { DockviewWorkspace } from '../components/workbench/DockviewWorkspace';
import { App } from '../App';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('UI Shell & HMI Lifecycle Comprehensive Verification (TSK-308 / SYS-REQ-097)', () => {
  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);

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

    // Reset Zustand store state to sample project for UI shell baseline tests
    useHmiStore.getState().loadSampleProject();
    useHmiStore.setState({
      lifecycleMode: 'EDIT',
      isRuntimeFullscreen: false,
      isHmiDirty: false,
      workspaceMode: 'PROJECT',
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    flushSync(() => {
      root.unmount();
    });
    container.remove();
  });

  describe('1. 3-Stage Lifecycle Transitions and State Management', () => {
    it('executes EDIT -> PREVIEW -> DEPLOY -> RUNTIME transitions deterministically', () => {
      const store = useHmiStore.getState();
      expect(useHmiStore.getState().lifecycleMode).toBe('EDIT');

      store.enterPreviewMode();
      expect(useHmiStore.getState().lifecycleMode).toBe('PREVIEW');

      store.enterDeployMode();
      expect(useHmiStore.getState().lifecycleMode).toBe('DEPLOY');

      store.enterRuntimeMode();
      expect(useHmiStore.getState().lifecycleMode).toBe('RUNTIME');

      store.enterEditMode();
      expect(useHmiStore.getState().lifecycleMode).toBe('EDIT');
    });

    it('NonActuatingPreviewBanner renders visible blue banner strictly in PREVIEW mode', () => {
      // In EDIT mode, banner is null (empty render)
      useHmiStore.setState({ lifecycleMode: 'EDIT' });
      let html = renderClient(<NonActuatingPreviewBanner />);
      expect(html).toBe('');

      // In PREVIEW mode, banner renders with warning text and blue background
      useHmiStore.setState({ lifecycleMode: 'PREVIEW' });
      html = renderClient(<NonActuatingPreviewBanner />);
      expect(html).toContain('NON-ACTUATING PREVIEW MODE');
      expect(html).toContain('CONTROL COMMANDS INHIBITED');
      expect(html).toContain('Return to Edit Mode');

      // In DEPLOY mode, banner is suppressed
      useHmiStore.setState({ lifecycleMode: 'DEPLOY' });
      html = renderClient(<NonActuatingPreviewBanner />);
      expect(html).toBe('');

      // In RUNTIME mode, banner is suppressed
      useHmiStore.setState({ lifecycleMode: 'RUNTIME' });
      html = renderClient(<NonActuatingPreviewBanner />);
      expect(html).toBe('');
    });
  });

  describe('2. Fullscreen Industrial Kiosk Mode & Security Gate', () => {
    it('enters kiosk mode and tracks isRuntimeFullscreen state', async () => {
      expect(useHmiStore.getState().isRuntimeFullscreen).toBe(false);

      await useHmiStore.getState().requestEnterRuntime();
      expect(useHmiStore.getState().isRuntimeFullscreen).toBe(true);
      expect(useHmiStore.getState().lifecycleMode).toBe('RUNTIME');
    });

    it('enforces IEC 62443 authorization challenge to exit kiosk', async () => {
      await useHmiStore.getState().requestEnterRuntime();
      expect(useHmiStore.getState().isRuntimeFullscreen).toBe(true);

      // Operator role cannot exit kiosk
      const denied = await useHmiStore.getState().requestExitRuntime({
        role: 'Operator',
        password: 'any',
      });
      expect(denied).toBe(false);
      expect(useHmiStore.getState().isRuntimeFullscreen).toBe(true);

      // Engineer with invalid password cannot exit
      const badPass = await useHmiStore.getState().requestExitRuntime({
        role: 'Engineer',
        password: 'wrong_password',
      });
      expect(badPass).toBe(false);
      expect(useHmiStore.getState().isRuntimeFullscreen).toBe(true);

      // Engineer with correct credentials exits kiosk back to EDIT
      const granted = await useHmiStore.getState().requestExitRuntime({
        role: 'Engineer',
        password: 'admin123',
      });
      expect(granted).toBe(true);
      expect(useHmiStore.getState().isRuntimeFullscreen).toBe(false);
      expect(useHmiStore.getState().lifecycleMode).toBe('EDIT');
    });

    it('renders ExitKioskDialog when isOpen is true', () => {
      const closedHtml = renderClient(<ExitKioskDialog isOpen={false} onClose={() => {}} />);
      expect(closedHtml).toBe('');

      const openHtml = renderClient(<ExitKioskDialog isOpen={true} onClose={() => {}} />);
      expect(openHtml).toContain('IEC 62443 KIOSK EXIT CHALLENGE');
      expect(openHtml).toContain('Engineer');
      expect(openHtml).toContain('Admin');
      expect(openHtml).toContain('Authenticate &amp; Exit Kiosk');
    });
  });

  describe('3. Hierarchical Screen Navigation & Tab Operations', () => {
    it('manages hierarchical screen navigation and multi-document tab state', () => {
      const store = useHmiStore.getState();
      expect(store.screens.length).toBeGreaterThanOrEqual(2);

      // Open screen tab
      store.openScreenTab('screen_02_boiler_area');
      expect(useHmiStore.getState().activeScreenId).toBe('screen_02_boiler_area');
      expect(useHmiStore.getState().openTabs.length).toBe(2);

      // Open sub-screen tab
      store.openScreenTab('screen_02_boiler_area', 'sub_boiler_tuning');
      expect(useHmiStore.getState().activeTabId).toBe('screen_02_boiler_area::sub_boiler_tuning');
      expect(useHmiStore.getState().activeSubscreenId).toBe('sub_boiler_tuning');

      // Switch active tab
      store.setActiveTab('screen_01_plant_overview');
      expect(useHmiStore.getState().activeTabId).toBe('screen_01_plant_overview');
      expect(useHmiStore.getState().activeScreenId).toBe('screen_01_plant_overview');
    });

    it('creates new screens and sub-screens via store actions', async () => {
      const store = useHmiStore.getState();
      const newScreenId = await store.addNewScreen('Screen_04_Distillation');
      expect(newScreenId).toBeTruthy();

      const stateAfterScreen = useHmiStore.getState();
      const screen = stateAfterScreen.screens.find((s) => s.id === newScreenId);
      expect(screen).toBeDefined();
      expect(screen?.name).toBe('Screen_04_Distillation');

      const subId = await store.addNewSubScreen(newScreenId, 'Distillation Column Temperature Popup');
      expect(subId).toBeTruthy();

      const stateAfterSub = useHmiStore.getState();
      const updatedScreen = stateAfterSub.screens.find((s) => s.id === newScreenId);
      expect(updatedScreen?.subscreens.some((sb) => sb.id === subId)).toBe(true);
    });

    it('handles dual-buffer screen persistence (draft vs committed)', async () => {
      const store = useHmiStore.getState();
      const screenId = 'screen_01_plant_overview';

      store.updateDraftCanvas(screenId, {
        title: 'New Infeed Stage',
        components: [{ id: 'cv-101', type: 'Conveyor' }],
      });

      expect(useHmiStore.getState().isHmiDirty).toBe(true);
      expect(useHmiStore.getState().draftScreens.get(screenId)?.title).toBe('New Infeed Stage');

      // Discard returns to active state
      store.discardHmiDraft();
      expect(useHmiStore.getState().isHmiDirty).toBe(false);

      // Update and commit
      store.updateDraftCanvas(screenId, {
        title: 'Committed Plant Canvas',
      });
      const committed = await store.commitHmiDraft();
      expect(committed).toBe(true);
      expect(useHmiStore.getState().isHmiDirty).toBe(false);
      expect(useHmiStore.getState().lifecycleMode).toBe('RUNTIME');
    });
  });

  describe('4. 5-Zone Workbench Shell Rendering (SYS-REQ-097 Parity)', () => {
    it('renders Zone 1 Top Navigation with title and mode controls', () => {
      const html = renderClient(<Zone1TopNav onOpenExitDialog={() => {}} />);
      expect(html).toContain('MythOS');
      expect(html).toContain('EDIT');
      expect(html).toContain('PREVIEW');
      expect(html).toContain('DEPLOY');
      expect(html).toContain('KIOSK RUNTIME');
    });

    it('renders Zone 2 Project Tree with industrial engineering taxonomy', () => {
      const html = renderClient(<Zone2ProjectTree />);
      expect(html).toContain('Project Tree');
      expect(html).toContain('SYS-REQ-097');
      expect(html).toContain('Devices &amp; Networks');
      expect(html).toContain('Controller [STruC++]');
      expect(html).toContain('Program Blocks');
      expect(html).toContain('HMI Screens');
      expect(html).toContain('Screen_01_PlantOverview');
      expect(html).toContain('Faceplate Types');
    });

    it('renders Zone 3 Dockview Workspace host container', () => {
      const html = renderClient(<DockviewWorkspace />);
      expect(html).toContain('data-testid="dockview-workspace-host"');
      expect(html).toMatch(/height:\s*100%/);
    });

    it('renders Zone 4 Task Cards and Inspector Drawer with tabs', () => {
      const html = renderClient(
        <Zone4TaskCards isCollapsed={false} onToggleCollapse={() => {}} />
      );
      expect(html).toContain('data-testid="zone4-task-cards"');
      expect(html).toContain('Inspector');
      expect(html).toContain('Instructions');
      expect(html).toContain('Catalog');
      expect(html).toContain('Libraries');
      expect(html).toContain('Interlock Matrix Checklist');
    });

    it('renders Zone 5 Bottom Console with ISA-18.2 Alarm Banner and Diagnostics', () => {
      const html = renderClient(
        <Zone5BottomConsole isCollapsed={false} onToggleCollapse={() => {}} />
      );
      expect(html).toContain('data-testid="zone5-bottom-console"');
      expect(html).toContain('ISA-18.2 Alarms');
      expect(html).toContain('Compiler &amp; Hot-Reload Console');
      expect(html).toContain('Diagnostic Buffer');
      expect(html).toContain('CYCLE:');
    });

    it('renders full App assembling all 5 zones in normal mode', () => {
      useHmiStore.setState({ isRuntimeFullscreen: false });
      const html = renderClient(<App />);
      expect(html).toContain('MythOS'); // Zone 1
      expect(html).toContain('Project Tree'); // Zone 2
      expect(html).toContain('data-testid="dockview-workspace-host"'); // Zone 3
      expect(html).toContain('data-testid="zone4-task-cards"'); // Zone 4
      expect(html).toContain('data-testid="zone5-bottom-console"'); // Zone 5
    });

    it('hides Zone 1, 2, and 5 chrome in fullscreen kiosk mode', () => {
      useHmiStore.setState({ isRuntimeFullscreen: true });
      const html = renderClient(<App />);
      // Zone 1, 2, 5 chrome should be hidden
      expect(html).not.toContain('Project Tree');
      expect(html).not.toContain('data-testid="zone5-bottom-console"');
      // Main workspace and exit trigger should be present
      expect(html).toContain('data-testid="dockview-workspace-host"');
      expect(html).toContain('kiosk-exit-trigger');
      expect(html).toContain('EXIT KIOSK (IEC 62443)');
    });
  });
});
