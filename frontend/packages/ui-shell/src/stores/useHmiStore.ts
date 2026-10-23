import { create } from 'zustand';
import {
  ScreenDescriptor,
  SubScreenDescriptor,
  HmiLifecycleMode,
  ScreenTab,
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
  errorMessage: string | null;
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
  /** Last canvas on disk per screen id: the baseline `discardHmiDraft` reverts to. */
  savedScreens: Map<string, any>;  // Persisted draft revision (screens/draft/screens.json)
  isHmiDirty: boolean;
  /** Operator-facing state of the last Save / Commit attempt, shown by the canvas toolbar. */
  screenSaveStatus: 'IDLE' | 'SAVING' | 'SAVED' | 'COMMITTING' | 'COMMITTED' | 'ERROR';

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
  ) => Promise<ProjectMetadata | null>;
  clearError: () => void;
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
  addNewScreen: (name: string, isRoot?: boolean) => Promise<string | null>;
  addNewSubScreen: (parentScreenId: string, name: string) => Promise<string | null>;

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
  saveScreenDraft: (screenId: string) => Promise<boolean>;
  commitHmiDraft: () => Promise<boolean>;
  discardHmiDraft: () => void;

  // Actions: Fullscreen Kiosk Handshake
  requestEnterRuntime: () => Promise<void>;
  /** Leaves runtime kiosk mode. Unauthenticated: no backend authorization endpoint exists yet. */
  requestExitRuntime: () => Promise<boolean>;
}

/**
 * Structural comparison for canvas payloads, used to tell an edited draft from the revision
 * last written to disk. Key order is irrelevant; `undefined` members are ignored so a payload
 * that merely gained explicit undefined fields does not read as an unsaved change.
 */
export function isCanvasEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a === null || b === null || a === undefined || b === undefined) {
    return a === undefined && b === undefined ? true : a === null && b === null;
  }
  if (typeof a !== 'object' || typeof b !== 'object') return false;

  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((item, index) => isCanvasEqual(item, b[index]));
  }

  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = Object.keys(left).filter((key) => left[key] !== undefined);
  const rightKeys = Object.keys(right).filter((key) => right[key] !== undefined);
  if (keys.length !== rightKeys.length) return false;
  return keys.every((key) => isCanvasEqual(left[key], right[key]));
}

/**
 * Turn a failed API call into a message an operator can act on. `fetch` rejects with a
 * TypeError when the daemon is unreachable, whose bare "Failed to fetch" says nothing
 * about the backend being down, so that case is named explicitly.
 */
function apiFailure(error: unknown, fallback: string): string {
  if (error instanceof TypeError) {
    return `${fallback}: backend unreachable (${error.message})`;
  }
  return error instanceof Error ? error.message : fallback;
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
  errorMessage: null,
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
  savedScreens: new Map(),
  isHmiDirty: false,
  screenSaveStatus: 'IDLE',
  isRuntimeFullscreen: false,
  runtimeAuthLock: true,

  setWorkspaceMode: (mode) => set({ workspaceMode: mode }),

  setCurrentProject: (project) => set({ currentProject: project }),

  clearError: () => set({ errorMessage: null }),

  createNewProject: async (name, author = 'MythOS Engineer', description = '', path = '/projects') => {
    const projId = `proj_${name.toLowerCase().replace(/[^a-z0-9_-]/g, '_')}_${Date.now().toString().slice(-4)}`;
    set({ errorMessage: null });

    try {
      const res = await fetch('/api/v1/projects', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id: projId, name, description, author }),
      });
      if (!res.ok) {
        throw new Error(`Project creation failed (HTTP ${res.status})`);
      }

      const data: ProjectMetadata = await res.json();
      if (!data?.id || !data?.name) {
        throw new Error('Project creation returned incomplete project data');
      }

      const metadata: ProjectMetadata = {
        ...data,
        author: data.author ?? author,
        description: data.description ?? description,
        path: data.path ?? path,
      };
      get().resetToEmptyProject();
      set({
        errorMessage: null,
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
    } catch (error) {
      const message = apiFailure(error, 'Project creation failed');
      set({ errorMessage: message });
      return null;
    }
  },

  openExistingProject: async (projectId: string) => {
    set({ errorMessage: null });
    try {
      const res = await fetch(`/api/v1/projects/${encodeURIComponent(projectId)}`);
      if (!res.ok) {
        throw new Error(`Project could not be opened (HTTP ${res.status})`);
      }
      const data: ProjectMetadata = await res.json();
      if (!data?.id || !data?.name) {
        throw new Error('Project response was incomplete');
      }
      const screensResponse = await fetch(`/api/v1/projects/${encodeURIComponent(projectId)}/hmi/screens`);
      if (!screensResponse.ok) throw new Error(`Project screens unavailable (HTTP ${screensResponse.status})`);
      const screens: ScreenDescriptor[] = await screensResponse.json();
      if (!Array.isArray(screens)) throw new Error('Project screens response was invalid');
      get().resetToEmptyProject();
      const savedScreens = new Map<string, any>();
      screens.forEach((screen) => {
        if (screen.canvasData) savedScreens.set(screen.id, screen.canvasData);
      });
      set({
        errorMessage: null,
        screens,
        savedScreens,
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
    } catch (error) {
      const message = apiFailure(error, 'Project could not be opened');
      set({ errorMessage: message });
      return false;
    }
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
      savedScreens: new Map(),
      activeScreens: new Map(),
      isHmiDirty: false,
      screenSaveStatus: 'IDLE',
      errorMessage: null,
    });
  },

  loadSampleProject: () => {
    const savedScreens = new Map<string, any>();
    SAMPLE_SCREENS.forEach((screen) => {
      if (screen.canvasData) savedScreens.set(screen.id, screen.canvasData);
    });
    set({
      errorMessage: null,
      currentProject: SAMPLE_PROJECT_METADATA,
      projectBlocks: SAMPLE_PROJECT_BLOCKS,
      screens: SAMPLE_SCREENS,
      savedScreens,
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
    set({ errorMessage: null });
    try {
      const project = get().currentProject;
      if (!project) throw new Error('Open a project before creating a screen');
      const res = await fetch(`/api/v1/projects/${encodeURIComponent(project.id)}/hmi/screens`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, is_root: isRoot }),
      });
      if (!res.ok) {
        throw new Error(`Screen creation failed (HTTP ${res.status})`);
      }
      const newScreen: ScreenDescriptor = await res.json();
      if (!newScreen?.id || !newScreen?.name || !Array.isArray(newScreen.subscreens)) {
        throw new Error('Screen creation returned incomplete screen data');
      }

      set((s) => {
        const savedScreens = new Map(s.savedScreens);
        if (newScreen.canvasData) savedScreens.set(newScreen.id, newScreen.canvasData);
        return { screens: [...s.screens, newScreen], savedScreens, errorMessage: null };
      });
      get().openScreenTab(newScreen.id);
      get().setSelectedContextItem({
        id: newScreen.id,
        name: newScreen.name,
        type: 'HMI_SCREEN',
        metadata: { is_root: newScreen.is_root },
      });
      return newScreen.id;
    } catch (error) {
      const message = apiFailure(error, 'Screen creation failed');
      set({ errorMessage: message });
      return null;
    }
  },

  addNewSubScreen: async (parentScreenId, name) => {
    set({ errorMessage: null });
    try {
      const project = get().currentProject;
      if (!project) throw new Error('Open a project before creating a sub-screen');
      const res = await fetch(`/api/v1/projects/${encodeURIComponent(project.id)}/hmi/screens/${encodeURIComponent(parentScreenId)}/subscreens`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, type: 'POPUP' }),
      });
      if (!res.ok) {
        throw new Error(`Sub-screen creation failed (HTTP ${res.status})`);
      }
      const newSub: SubScreenDescriptor = await res.json();
      if (!newSub?.id || !newSub?.name) {
        throw new Error('Sub-screen creation returned incomplete data');
      }

      set((s) => ({
        screens: s.screens.map((sc) =>
          sc.id === parentScreenId ? { ...sc, subscreens: [...sc.subscreens, newSub] } : sc
        ),
        errorMessage: null,
      }));
      get().openScreenTab(parentScreenId, newSub.id);
      return newSub.id;
    } catch (error) {
      const message = apiFailure(error, 'Sub-screen creation failed');
      set({ errorMessage: message });
      return null;
    }
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
    if (!canvasData) return true;
    set({ errorMessage: null, screenSaveStatus: 'SAVING' });

    try {
      const project = get().currentProject;
      if (!project) throw new Error('Open a project before saving a screen');
      const res = await fetch(`/api/v1/projects/${encodeURIComponent(project.id)}/hmi/screens/${encodeURIComponent(screenId)}/draft`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(canvasData),
      });
      if (!res.ok) {
        throw new Error(`Screen draft save failed (HTTP ${res.status})`);
      }
      // The draft buffer keeps reading from the in-editor copy; savedScreens becomes the
      // discard baseline and screens can now be re-seeded from a reload or a fresh panel.
      set((state) => {
        const savedScreens = new Map(state.savedScreens);
        savedScreens.set(screenId, JSON.parse(JSON.stringify(canvasData)));
        return {
          screens: state.screens.map((screen) => screen.id === screenId ? { ...screen, canvasData } : screen),
          savedScreens,
          screenSaveStatus: 'SAVED' as const,
        };
      });
      return true;
    } catch (error) {
      set({
        errorMessage: apiFailure(error, 'Screen draft save failed'),
        screenSaveStatus: 'ERROR',
      });
      return false;
    }
  },

  commitHmiDraft: async () => {
    set({ errorMessage: null, screenSaveStatus: 'COMMITTING' });
    try {
      const project = get().currentProject;
      if (!project) throw new Error('Open a project before committing HMI drafts');
      // Only persist screens whose draft drifted from the saved revision. Re-saving clean
      // screens would write identical payloads and fail on screens the server never created.
      const dirtyScreenIds = [...get().draftScreens.entries()]
        .filter(([screenId, draft]) => !isCanvasEqual(draft, get().savedScreens.get(screenId)))
        .map(([screenId]) => screenId);
      for (const screenId of dirtyScreenIds) {
        if (!await get().saveScreenDraft(screenId)) {
          set({ screenSaveStatus: 'ERROR' });
          return false;
        }
      }
      const res = await fetch(`/api/v1/projects/${encodeURIComponent(project.id)}/hmi/commit`, { method: 'POST' });
      if (!res.ok) {
        throw new Error(`HMI draft commit failed (HTTP ${res.status})`);
      }
      const activeData = await res.json();
      if (!activeData || typeof activeData !== 'object' || Array.isArray(activeData)) {
        throw new Error('HMI draft commit returned invalid data');
      }
      // The committed payload now equals the draft, so the draft buffer can be dropped and
      // the canvas renders from the active buffer instead of a stale editing copy.
      const activeScreens = new Map<string, any>(Object.entries(activeData));
      set({
        activeScreens,
        savedScreens: new Map(activeScreens),
        draftScreens: new Map(),
        isHmiDirty: false,
        lifecycleMode: 'RUNTIME',
        screenSaveStatus: 'COMMITTED',
        errorMessage: null,
      });
      return true;
    } catch (error) {
      set({
        errorMessage: apiFailure(error, 'HMI draft commit failed'),
        screenSaveStatus: 'ERROR',
      });
      return false;
    }
  },

  discardHmiDraft: () => {
    // Revert to the last revision written to disk, not to the committed runtime buffer:
    // discarding unsaved edits must not silently roll a screen back past its last save.
    // Only screens that were actually edited revert; untouched saved screens keep no draft entry.
    const { savedScreens, draftScreens } = get();
    const reverted = new Map<string, any>();
    draftScreens.forEach((_draft, screenId) => {
      const saved = savedScreens.get(screenId);
      if (saved !== undefined) reverted.set(screenId, JSON.parse(JSON.stringify(saved)));
    });
    set({ draftScreens: reverted, isHmiDirty: false, screenSaveStatus: 'IDLE' });
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

  requestExitRuntime: async () => {
    // IEC 62443 credential verification is not implemented on the backend, so this is an
    // explicit local exit rather than a role check. ExitKioskDialog states this to the operator.
    set({ errorMessage: null });
    if (typeof document !== 'undefined' && document.fullscreenElement) {
      try {
        await document.exitFullscreen();
      } catch (error) {
        console.warn('[HMI Kiosk] Fullscreen exit suppressed or unsupported:', error);
      }
    }
    set({ isRuntimeFullscreen: false, runtimeAuthLock: false, lifecycleMode: 'EDIT' });
    return true;
  },
}));
