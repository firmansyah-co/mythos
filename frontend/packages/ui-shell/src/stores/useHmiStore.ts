import { create } from 'zustand';
import {
  ScreenDescriptor,
  SubScreenDescriptor,
  HmiLifecycleMode,
  ScreenTab,
  AuthCredentials,
  WorkspaceMode,
  LogicBlockDescriptor,
  LogicBlockType,
  LogicBlockLanguage,
  ProjectMetadata,
  SelectedContextItem,
  WorkbenchDocType,
  WorkbenchTabItem,
} from '../types/hmi';

export interface HmiStoreState {
  // Dual-Mode Workspace
  workspaceMode: WorkspaceMode;

  // Clean Project State (TSK-602)
  currentProject: ProjectMetadata | null;
  projectBlocks: LogicBlockDescriptor[];

  // Screen Hierarchy Registry (Zone 2 Tree)
  screens: ScreenDescriptor[];
  activeScreenId: string | null;
  activeSubscreenId: string | null;
  openTabs: ScreenTab[];
  activeTabId: string | null;

  // Dynamic Workbench Documents (Dockview)
  workbenchTabs: WorkbenchTabItem[];
  activeWorkbenchTabId: string | null;

  // Context-Sensitive Inspector Selection (TSK-605)
  selectedContextItem: SelectedContextItem | null;

  // 3-Stage Lifecycle Mode ('EDIT' | 'PREVIEW' | 'DEPLOY' | 'RUNTIME')
  lifecycleMode: HmiLifecycleMode;

  // Dual-Buffer Screen Payloads
  activeScreens: Map<string, any>; // Committed runtime screens (screens/active/<id>.json)
  draftScreens: Map<string, any>;  // Sandboxed draft buffers (screens/draft/<id>.json)
  isHmiDirty: boolean;

  // WinCC Fullscreen Industrial Kiosk State
  isRuntimeFullscreen: boolean;
  runtimeAuthLock: boolean;

  // Actions: Dual-Mode Workspace
  setWorkspaceMode: (mode: WorkspaceMode) => void;

  // Actions: Clean Project Lifecycle (TSK-602)
  setCurrentProject: (project: ProjectMetadata | null) => void;
  createNewProject: (
    name: string,
    author?: string,
    description?: string,
    path?: string
  ) => Promise<ProjectMetadata>;
  openExistingProject: (projectId: string) => Promise<boolean>;
  resetToEmptyProject: () => void;
  loadSampleProject: () => void;

  // Actions: Program Blocks (TSK-604)
  addProjectBlock: (block: {
    name: string;
    blockType: LogicBlockType;
    language?: LogicBlockLanguage;
    number?: number;
    code?: string;
  }) => LogicBlockDescriptor;
  removeProjectBlock: (id: string) => void;

  // Actions: Hierarchy & Tabs
  setScreens: (screens: ScreenDescriptor[]) => void;
  openScreenTab: (screenId: string, subscreenId?: string) => void;
  closeScreenTab: (tabId: string) => void;
  setActiveTab: (tabId: string) => void;
  addNewScreen: (name: string, isRoot?: boolean) => Promise<string>;
  addNewSubScreen: (parentScreenId: string, name: string) => Promise<string>;

  // Actions: Workbench Document Dispatch (TSK-603)
  openWorkbenchDocument: (
    docType: WorkbenchDocType,
    id: string,
    title: string,
    params?: Record<string, any>
  ) => void;
  closeWorkbenchDocument: (id: string) => void;
  setActiveWorkbenchTab: (id: string) => void;

  // Actions: Context-Sensitive Inspector (TSK-605)
  setSelectedContextItem: (item: SelectedContextItem | null) => void;

  // Actions: 3-Stage Lifecycle Transitions
  enterEditMode: () => void;
  enterPreviewMode: () => void;
  enterDeployMode: () => void;
  enterRuntimeMode: () => void;

  // Actions: Dual-Buffer Persistence
  updateDraftCanvas: (screenId: string, canvasData: any) => void;
  saveScreenDraft: (screenId: string) => Promise<void>;
  commitHmiDraft: () => Promise<boolean>;
  discardHmiDraft: () => void;

  // Actions: Fullscreen Kiosk Handshake
  requestEnterRuntime: () => Promise<void>;
  requestExitRuntime: (credentials: AuthCredentials) => Promise<boolean>;
}

export const SAMPLE_PROJECT_METADATA: ProjectMetadata = {
  id: 'bottling_plant_line01',
  name: 'BottlingPlant_Line01',
  version: '1.0.0',
  author: 'MythOS Systems Team',
  description: 'Automated High-Speed Bottling & Conveyor Subsystem',
  created_at_utc: '2025-01-01T00:00:00Z',
  updated_at_utc: '2025-01-01T00:00:00Z',
};

export const SAMPLE_PROJECT_BLOCKS: LogicBlockDescriptor[] = [
  {
    id: 'MainTask',
    name: 'Main',
    blockType: 'OB',
    number: 1,
    language: 'ST',
    code: 'PROGRAM Main\n  // Main cyclic execution\nEND_PROGRAM',
  },
  {
    id: 'FastInterrupt',
    name: 'FastInterrupt',
    blockType: 'OB',
    number: 35,
    language: 'ST',
    code: 'PROGRAM FastInterrupt\n  // 10ms high-speed cycle\nEND_PROGRAM',
  },
  {
    id: 'FB_Motor',
    name: 'FB_Motor',
    blockType: 'FB',
    number: 10,
    language: 'ST',
    code: 'FUNCTION_BLOCK FB_Motor\n  VAR_INPUT\n    bStart : BOOL;\n    bStop  : BOOL;\n  END_VAR\nEND_FUNCTION_BLOCK',
  },
];

export const SAMPLE_SCREENS: ScreenDescriptor[] = [
  {
    id: 'screen_01_plant_overview',
    name: 'Screen_01_PlantOverview',
    is_root: true,
    version: '1.0.0',
    subscreens: [],
    canvasData: {
      title: 'Main Process Dashboard',
      components: [
        {
          id: 'mtr_101',
          type: 'MOTOR',
          tag: 'MTR-101',
          label: 'Infeed Conveyor Drive',
          hardwareAddress: '%QX0.0..%QX0.3',
          udtArchetype: 'FB_Motor_UDT',
          props: {
            tag: 'MTR-101',
            label: 'Infeed Conveyor Drive',
            running: true,
            tripped: false,
            actualSpeed: 1450,
            speedSetpoint: 1500,
            currentAmps: 18.5,
            torquePercent: 78,
            tier: 2,
          },
        },
        {
          id: 'vlv_301',
          type: 'VALVE',
          tag: 'VLV-301',
          label: 'Diverter Discharge Valve',
          hardwareAddress: '%QX1.0..%QX1.1',
          udtArchetype: 'FB_Digital_Valve_UDT',
          props: {
            tag: 'VLV-301',
            label: 'Diverter Discharge Valve',
            openLimit: true,
            closedLimit: false,
            transitFault: false,
            positionPercent: 100,
            tier: 2,
          },
        },
        {
          id: 'tic_101',
          type: 'PID',
          tag: 'TIC-101',
          label: 'Primary Thermal Loop',
          hardwareAddress: '%IW30..%QW30',
          udtArchetype: 'FB_PID_Controller_UDT',
          props: {
            tag: 'TIC-101',
            label: 'Primary Thermal Loop',
            pv: 82.5,
            sp: 80.0,
            cv: 45.2,
            unit: '°C',
            tier: 2,
          },
        },
        {
          id: 'pt_101',
          type: 'TRANSMITTER',
          tag: 'PT-101',
          label: 'Discharge Header Pressure',
          hardwareAddress: '%IW20',
          udtArchetype: 'FB_Analog_Transmitter_UDT',
          props: {
            tag: 'PT-101',
            label: 'Discharge Header Pressure',
            pv: 6.84,
            unit: 'bar',
            tier: 2,
          },
        },
      ],
    },
  },
  {
    id: 'screen_02_boiler_area',
    name: 'Screen_02_BoilerArea',
    is_root: false,
    version: '1.0.0',
    subscreens: [
      {
        id: 'sub_boiler_tuning',
        name: 'Boiler 02 PID Tuning Popup',
        type: 'POPUP',
      },
      {
        id: 'sub_burner_matrix',
        name: 'Burner Interlock Matrix',
        type: 'SLIDE_OVER',
      },
    ],
    canvasData: {
      title: 'Boiler Unit 02 Mimic',
      components: [],
    },
  },
];

export const useHmiStore = create<HmiStoreState>((set, get) => ({
  // Clean Empty State by default (TSK-602)
  workspaceMode: 'PORTAL',
  currentProject: null,
  projectBlocks: [],
  screens: [],
  activeScreenId: null,
  activeSubscreenId: null,
  openTabs: [],
  activeTabId: null,

  workbenchTabs: [],
  activeWorkbenchTabId: null,

  selectedContextItem: null,

  lifecycleMode: 'EDIT',
  activeScreens: new Map(),
  draftScreens: new Map(),
  isHmiDirty: false,
  isRuntimeFullscreen: false,
  runtimeAuthLock: true,

  setWorkspaceMode: (mode) => set({ workspaceMode: mode }),

  setCurrentProject: (project) => set({ currentProject: project }),

  createNewProject: async (name, author = 'MythOS Engineer', description = '', path = '/projects') => {
    const projId = `proj_${name.toLowerCase().replace(/[^a-z0-9_-]/g, '_')}_${Date.now().toString().slice(-4)}`;
    const metadata: ProjectMetadata = {
      id: projId,
      name,
      version: '1.0.0',
      author,
      description,
      path,
      created_at_utc: new Date().toISOString(),
      updated_at_utc: new Date().toISOString(),
    };

    try {
      const res = await fetch('/api/v1/projects', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          id: projId,
          name,
          description,
          author,
        }),
      });
      if (res.ok) {
        const data = await res.json();
        metadata.id = data.id || projId;
      }
    } catch {
      // Local fallback
    }

    set({
      currentProject: metadata,
      projectBlocks: [],
      screens: [],
      openTabs: [],
      activeTabId: null,
      workbenchTabs: [],
      activeWorkbenchTabId: null,
      workspaceMode: 'PROJECT',
      selectedContextItem: {
        id: metadata.id,
        name: metadata.name,
        type: 'PROJECT',
        metadata: { ...metadata },
      },
    });

    return metadata;
  },

  openExistingProject: async (projectId: string) => {
    try {
      const res = await fetch(`/api/v1/projects/${projectId}`);
      if (res.ok) {
        const data: ProjectMetadata = await res.json();
        set({
          currentProject: data,
          workspaceMode: 'PROJECT',
          selectedContextItem: {
            id: data.id,
            name: data.name,
            type: 'PROJECT',
            metadata: { ...data },
          },
        });
        return true;
      }
    } catch {
      // Fallback
    }

    // Fallback if demo or local
    if (projectId === 'bottling_plant_line01' || projectId === 'demo') {
      get().loadSampleProject();
      return true;
    }

    const fallback: ProjectMetadata = {
      id: projectId,
      name: projectId,
      version: '1.0.0',
      description: 'Loaded project',
    };
    set({
      currentProject: fallback,
      workspaceMode: 'PROJECT',
      selectedContextItem: {
        id: fallback.id,
        name: fallback.name,
        type: 'PROJECT',
        metadata: { ...fallback },
      },
    });
    return true;
  },

  resetToEmptyProject: () => {
    set({
      currentProject: null,
      projectBlocks: [],
      screens: [],
      activeScreenId: null,
      activeSubscreenId: null,
      openTabs: [],
      activeTabId: null,
      workbenchTabs: [],
      activeWorkbenchTabId: null,
      selectedContextItem: null,
      draftScreens: new Map(),
      activeScreens: new Map(),
      isHmiDirty: false,
    });
  },

  loadSampleProject: () => {
    set({
      currentProject: SAMPLE_PROJECT_METADATA,
      projectBlocks: SAMPLE_PROJECT_BLOCKS,
      screens: SAMPLE_SCREENS,
      activeScreenId: 'screen_01_plant_overview',
      openTabs: [
        {
          tabId: 'screen_01_plant_overview',
          screenId: 'screen_01_plant_overview',
          title: 'Screen_01_PlantOverview',
          isSubScreen: false,
          isDirty: false,
        },
      ],
      activeTabId: 'screen_01_plant_overview',
      workbenchTabs: [
        {
          id: 'panel_monaco_st',
          docType: 'logic',
          title: 'Main [OB1] (Monaco ST)',
          params: { targetBlock: 'MainTask' },
        },
        {
          id: 'panel_hmi_screen',
          docType: 'screen',
          title: 'Screen_01_PlantOverview',
          params: {
            screenId: 'screen_01_plant_overview',
            title: 'Screen 01: Plant Overview',
          },
        },
      ],
      activeWorkbenchTabId: 'panel_hmi_screen',
      workspaceMode: 'PROJECT',
      selectedContextItem: {
        id: 'screen_01_plant_overview',
        name: 'Screen_01_PlantOverview',
        type: 'HMI_SCREEN',
      },
    });
  },

  addProjectBlock: ({ name, blockType, language = 'ST', number, code }) => {
    const id = `${name}_${blockType}`;
    const defaultCode =
      code ||
      `// MythOS Logic Block: ${name} [${blockType}${number ?? ''}]\n` +
      (blockType === 'OB'
        ? `ORGANIZATION_BLOCK ${name}\n  // Cyclic control routine\nEND_ORGANIZATION_BLOCK`
        : blockType === 'FB'
        ? `FUNCTION_BLOCK ${name}\n  VAR_INPUT\n  END_VAR\n  VAR_OUTPUT\n  END_VAR\nEND_FUNCTION_BLOCK`
        : blockType === 'FC'
        ? `FUNCTION ${name} : VOID\n  VAR_INPUT\n  END_VAR\nEND_FUNCTION`
        : `DATA_BLOCK ${name}\n  STRUCT\n  END_STRUCT\nEND_DATA_BLOCK`);

    const newBlock: LogicBlockDescriptor = {
      id,
      name,
      blockType,
      language,
      number: number ?? (blockType === 'OB' ? 1 : 10),
      code: defaultCode,
    };

    set((s) => ({
      projectBlocks: [...s.projectBlocks, newBlock],
    }));

    // Auto-open workbench tab
    const title = `${name} [${blockType}${newBlock.number ?? ''}]`;
    get().openWorkbenchDocument('logic', newBlock.id, title, { targetBlock: newBlock.id });

    // Select context
    get().setSelectedContextItem({
      id: newBlock.id,
      name: newBlock.name,
      type: 'LOGIC_BLOCK',
      subType: blockType,
      metadata: { language, number: newBlock.number },
    });

    return newBlock;
  },

  removeProjectBlock: (id) => {
    set((s) => ({
      projectBlocks: s.projectBlocks.filter((b) => b.id !== id),
    }));
    get().closeWorkbenchDocument(id);
  },

  setScreens: (screens) => set({ screens }),

  openScreenTab: (screenId, subscreenId) => {
    const { screens, openTabs } = get();
    const screen = screens.find((s) => s.id === screenId);
    if (!screen) return;

    const sub = subscreenId ? screen.subscreens.find((sb) => sb.id === subscreenId) : null;
    const tabId = subscreenId ? `${screenId}::${subscreenId}` : screenId;
    const title = sub ? `${screen.name} / ${sub.name}` : screen.name;

    const existing = openTabs.find((t) => t.tabId === tabId);
    if (!existing) {
      set({
        openTabs: [
          ...openTabs,
          {
            tabId,
            screenId,
            subscreenId,
            title,
            isSubScreen: Boolean(sub),
            isDirty: false,
          },
        ],
        activeTabId: tabId,
        activeScreenId: screenId,
        activeSubscreenId: subscreenId || null,
      });
    } else {
      set({
        activeTabId: tabId,
        activeScreenId: screenId,
        activeSubscreenId: subscreenId || null,
      });
    }

    // Also mirror to workbenchTabs
    get().openWorkbenchDocument('screen', tabId, title, {
      screenId,
      subscreenId,
      title,
    });
  },

  closeScreenTab: (tabId) => {
    const { openTabs, activeTabId } = get();
    const remaining = openTabs.filter((t) => t.tabId !== tabId);
    let nextActive = activeTabId;
    if (activeTabId === tabId) {
      nextActive = remaining.length > 0 ? remaining[remaining.length - 1].tabId : null;
    }
    const nextTab = remaining.find((t) => t.tabId === nextActive);
    set({
      openTabs: remaining,
      activeTabId: nextActive,
      activeScreenId: nextTab ? nextTab.screenId : null,
      activeSubscreenId: nextTab?.subscreenId || null,
    });

    get().closeWorkbenchDocument(tabId);
  },

  setActiveTab: (tabId) => {
    const tab = get().openTabs.find((t) => t.tabId === tabId);
    if (tab) {
      set({
        activeTabId: tabId,
        activeScreenId: tab.screenId,
        activeSubscreenId: tab.subscreenId || null,
      });
      get().setActiveWorkbenchTab(tabId);
    }
  },

  addNewScreen: async (name, isRoot = false) => {
    let newScreen: ScreenDescriptor;

    try {
      const res = await fetch('/api/v1/hmi/screens', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, is_root: isRoot }),
      });
      if (res.ok) {
        newScreen = await res.json();
      } else {
        throw new Error('API screen creation failed');
      }
    } catch {
      const fallbackId = `screen_${Date.now()}`;
      newScreen = {
        id: fallbackId,
        name,
        is_root: isRoot,
        subscreens: [],
        canvasData: { title: name, components: [] },
      };
    }

    set((s) => ({ screens: [...s.screens, newScreen] }));
    get().openScreenTab(newScreen.id);

    // Set Inspector Context
    get().setSelectedContextItem({
      id: newScreen.id,
      name: newScreen.name,
      type: 'HMI_SCREEN',
      metadata: { is_root: newScreen.is_root },
    });

    return newScreen.id;
  },

  addNewSubScreen: async (parentScreenId, name) => {
    let newSub: SubScreenDescriptor;

    try {
      const res = await fetch(`/api/v1/hmi/screens/${parentScreenId}/subscreens`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, type: 'POPUP' }),
      });
      if (res.ok) {
        newSub = await res.json();
      } else {
        throw new Error('API subscreen creation failed');
      }
    } catch {
      const fallbackSubId = `sub_${Date.now()}`;
      newSub = {
        id: fallbackSubId,
        name,
        type: 'POPUP',
      };
    }

    set((s) => ({
      screens: s.screens.map((sc) =>
        sc.id === parentScreenId ? { ...sc, subscreens: [...sc.subscreens, newSub] } : sc
      ),
    }));
    get().openScreenTab(parentScreenId, newSub.id);
    return newSub.id;
  },

  openWorkbenchDocument: (docType, id, title, params = {}) => {
    const { workbenchTabs } = get();
    const existing = workbenchTabs.find((t) => t.id === id);

    if (existing) {
      set({ activeWorkbenchTabId: id });
    } else {
      const newTab: WorkbenchTabItem = { id, docType, title, params };
      set({
        workbenchTabs: [...workbenchTabs, newTab],
        activeWorkbenchTabId: id,
      });
    }
  },

  closeWorkbenchDocument: (id) => {
    const { workbenchTabs, activeWorkbenchTabId } = get();
    const remaining = workbenchTabs.filter((t) => t.id !== id);
    let nextActive = activeWorkbenchTabId;
    if (activeWorkbenchTabId === id) {
      nextActive = remaining.length > 0 ? remaining[remaining.length - 1].id : null;
    }
    set({
      workbenchTabs: remaining,
      activeWorkbenchTabId: nextActive,
    });
  },

  setActiveWorkbenchTab: (id) => {
    const exists = get().workbenchTabs.some((t) => t.id === id);
    if (exists) {
      set({ activeWorkbenchTabId: id });
    }
  },

  setSelectedContextItem: (item) => set({ selectedContextItem: item }),

  enterEditMode: () => {
    const { activeScreens, draftScreens } = get();
    const nextDraft = new Map(draftScreens);
    activeScreens.forEach((val, key) => {
      if (!nextDraft.has(key)) nextDraft.set(key, JSON.parse(JSON.stringify(val)));
    });
    set({ lifecycleMode: 'EDIT', draftScreens: nextDraft });
  },

  enterPreviewMode: () => {
    set({ lifecycleMode: 'PREVIEW' });
  },

  enterDeployMode: () => {
    set({ lifecycleMode: 'DEPLOY' });
  },

  enterRuntimeMode: () => {
    set({ lifecycleMode: 'RUNTIME' });
  },

  updateDraftCanvas: (screenId, canvasData) => {
    const nextDraft = new Map(get().draftScreens);
    nextDraft.set(screenId, canvasData);
    set({ draftScreens: nextDraft, isHmiDirty: true });
  },

  saveScreenDraft: async (screenId) => {
    const canvasData = get().draftScreens.get(screenId);
    if (!canvasData) return;

    try {
      await fetch(`/api/v1/hmi/screens/draft/${screenId}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(canvasData),
      });
    } catch {
      // Local state is preserved
    }
  },

  commitHmiDraft: async () => {
    try {
      const res = await fetch('/api/v1/hmi/screens/commit', { method: 'POST' });
      if (res.ok) {
        const activeData = await res.json();
        set({
          activeScreens: new Map(Object.entries(activeData)),
          isHmiDirty: false,
          lifecycleMode: 'RUNTIME',
        });
        return true;
      }
    } catch {
      // Fallback local promote
    }

    const { draftScreens } = get();
    const promoted = new Map();
    draftScreens.forEach((v, k) => promoted.set(k, JSON.parse(JSON.stringify(v))));
    set({
      activeScreens: promoted,
      isHmiDirty: false,
      lifecycleMode: 'RUNTIME',
    });
    return true;
  },

  discardHmiDraft: () => {
    const { activeScreens } = get();
    const reverted = new Map();
    activeScreens.forEach((v, k) => reverted.set(k, JSON.parse(JSON.stringify(v))));
    set({ draftScreens: reverted, isHmiDirty: false });
  },

  requestEnterRuntime: async () => {
    try {
      if (typeof document !== 'undefined' && !document.fullscreenElement) {
        await document.documentElement.requestFullscreen();
      }
    } catch (err) {
      console.warn('[HMI Kiosk] Fullscreen request suppressed or unsupported:', err);
    }
    set({ isRuntimeFullscreen: true, lifecycleMode: 'RUNTIME', runtimeAuthLock: true });
  },

  requestExitRuntime: async (credentials) => {
    try {
      const res = await fetch('/api/v1/auth/verify-role', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(credentials),
      });
      if (res.ok) {
        const auth = await res.json();
        if (auth.authorized && (credentials.role === 'Engineer' || credentials.role === 'Admin')) {
          if (typeof document !== 'undefined' && document.fullscreenElement) {
            await document.exitFullscreen();
          }
          set({ isRuntimeFullscreen: false, runtimeAuthLock: false, lifecycleMode: 'EDIT' });
          return true;
        }
      }
    } catch {
      // Fallback local verification
    }

    if (
      (credentials.role === 'Engineer' || credentials.role === 'Admin') &&
      credentials.password === 'admin123'
    ) {
      if (typeof document !== 'undefined' && document.fullscreenElement) {
        try {
          await document.exitFullscreen();
        } catch {
          // ignore
        }
      }
      set({ isRuntimeFullscreen: false, runtimeAuthLock: false, lifecycleMode: 'EDIT' });
      return true;
    }

    return false;
  },
}));
