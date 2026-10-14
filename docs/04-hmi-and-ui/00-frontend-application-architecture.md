# MythOS HMI & UI Specification: Frontend Application Architecture

- **Document Identifier**: MYTHOS-HMI-000
- **Version**: 3.0.0
- **Author**: Neo (VP Software & Infrastructure), Iris (VP Design & Visual Architecture) & Adrian (Board of Commissioners Directive)
- **Status**: APPROVED / SPECIFICATION
- **Standard Reference**: Siemens TIA Portal Parity (SYS-REQ-097), ANSI/ISA-101.01, ISO 9241-110, IEC 61131-3, W3C DTCG
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification defines the frontend software architecture, component hierarchy, client state management, real-time telemetry streaming, screen lifecycle management, and multi-user collaborative editing subsystems for the MythOS web workbench and Supervisory Control and Data Acquisition (SCADA) Human-Machine Interface (HMI).

The frontend application executes as a high-performance Single Page Application (SPA) built on **React 19**, **Vite 6**, and **TypeScript 5.6+**. It renders inside standard Chromium-based industrial touch panels, multi-monitor control room workstations, and field engineering laptops.

The frontend architecture enforces eight core engineering principles:
1. **Pure Dumb Presentation Layer**: The frontend performs zero mathematical process modeling, zero business logic calculations, and zero raw hardware register manipulation. The Rust microkernel and OpenPLC runtime act as the absolute Single Source of Truth (SSOT). The UI strictly renders schemas and dispatches structured user intent.
2. **Dockview Multi-Zone Industrial Shell**: The entire workspace utilizes `@hexact/dockview-react` to host a deterministic 5-Zone layout accommodating multi-screen workflows, docking panels, floating diagnostic drawers, and tabbed document editors.
3. **Throttled Real-Time Telemetry (100ms / 10 Hz Sliding Window)**: Server-Sent Events (SSE) stream process values into a dedicated Zustand store. Tag updates are buffered and applied at 100 ms intervals, preventing browser DOM thrashing while preserving smooth visual indicators.
4. **Integrated In-Canvas PLC Logic Editor**: There is no file upload form. IEC 61131-3 Structured Text (Monaco) and Ladder Diagram canvases reside directly inside the workbench. Clicking **[Deploy / Hot-Reload to Controller]** dispatches source code directly to the backend compilation pipeline (`POST /api/v1/control/deploy`), triggering zero-downtime hot-reloading in under 50 ms.
5. **Multi-User Real-Time Collaboration (CRDT / Yjs)**: Multiple engineers can work on the same project simultaneously across a local network. Using Conflict-Free Replicated Data Types (CRDT) via `yjs`, `y-monaco`, and `y-websocket`, the workbench delivers live peer presence avatars, multi-cursor collaborative code editing, and instant canvas object movement synchronization.
6. **Decoupled 3-Stage HMI Lifecycle (`[ EDIT ] ──► [ PREVIEW ] ──► [ DEPLOY ]`)**: Canvas HMI layouts and task logic programs remain strictly decoupled between sandboxed staging buffers (`project.draft.json`, `screens/draft/<screen_id>.json`) and live active runtime buffers (`project.active.json`, `screens/active/<screen_id>.json`). Engineer or operator modifications never disrupt production screens until explicit validation and atomic promotion.
7. **Immersive Industrial Runtime Fullscreen (WinCC / InTouch Kiosk Paradigm)**: When entering Runtime Mode, the application expands to fullscreen via `requestFullscreen()`, completely suppressing all engineering IDE chrome (Zone 1 menus, 48px Activity Bar, Zone 2 Project Explorer, and Zone 5 Console). The kiosk surface presents solely the plant graphic canvas, an ISA-18.2 global alarm banner, and contextual slide-over faceplates. Exiting runtime is secured by an IEC 62443 credential challenge gate.
8. **Faceplate Master Library & Global Propagation (TIA Portal Library Pattern)**: Reusable industrial component types are centrally authored in Zone 2 Faceplate Library against formal UDT interface properties. Updating a master faceplate type and bumping its version (`v1.0.0` to `v1.1.0`) atomically propagates graphics, layouts, and property bindings across all instantiated screens in the draft buffer.

---

## 2. Directory and Module Structure

The frontend application resides in `frontend/` within the MythOS repository:

```
frontend/
├── index.html                         -- HTML5 entrypoint with preloaded industrial fonts
├── package.json                       -- React 19, Vite 6, Dockview, Zustand, Yjs dependencies
├── tsconfig.json                      -- Strict TypeScript configuration
├── vite.config.ts                     -- Vite 6 bundler setup with proxy and SVG optimization
└── src/
    ├── main.tsx                       -- React root mounting point with strict error boundaries
    ├── App.tsx                        -- Root application shell hosting Dockview workspace
    │
    ├── assets/                        -- Static design tokens and base SVGs
    │   ├── icons/                     -- ISO 1219 / ISA-5.1 industrial symbol library
    │   └── tokens/                    -- DTCG W3C design tokens (design-tokens-dtcg.json)
    │
    ├── components/                    -- Reusable dumb presentation components
    │   ├── ui/                        -- Atomic primitives (Button, Dropdown, Input, Modal, Badge)
    │   ├── faceplates/                -- UDT-bound industrial faceplates (Motor, Valve, Analog)
    │   ├── canvas/                    -- Bounded grid canvas primitives, zoom/pan transforms
    │   └── presence/                  -- Collaborator avatar stack, peer cursor badges
    │
    ├── features/                      -- Domain-specific operational feature modules
    │   ├── workbench/                 -- 5-Zone layout shell and dockview panel containers
    │   │   ├── Zone1TopHud.tsx        -- Plant status HUD, active project switcher, avatars
    │   │   ├── Zone1StatusBar.tsx     -- Bottom 24px operational status bar, cycle time, mode badge
    │   │   ├── Zone2Explorer.tsx      -- Hierarchical TIA Portal project tree, devices, screens
    │   │   ├── Zone3MainCanvas.tsx    -- Tabbed document host (3 Hardware views, Logic, HMI)
    │   │   ├── Zone4TaskCardsDock.tsx -- Far-Right Collapsible Task Cards dock (384px)
    │   │   └── Zone5BottomInspector.tsx -- Context-sensitive bottom properties & inspector
    │   ├── devices-networks/          -- Siemens TIA Portal 3 Canonical Hardware Views
    │   │   ├── DevicesNetworksCanvas.tsx -- Zone 3 host for Network, Device, and Topology views
    │   │   ├── NetworkView.tsx        -- Interconnected fieldbus topology (PROFINET, Modbus)
    │   │   ├── DeviceView.tsx         -- Slot-by-slot modular chassis rack inspector (Rack 0)
    │   │   ├── TopologyView.tsx       -- Port-to-port physical cabling & LLDP neighborhood
    │   │   └── TerminalPinoutStrip.tsx -- Module terminal pin inspection and %I/%Q mapping
    │   ├── task-cards/                -- Zone 4 Far-Right Collapsible Task Cards (384px)
    │   │   ├── InstructionsCard.tsx   -- Standard IEC 61131-3 logic operations palette
    │   │   ├── HardwareCatalogCard.tsx -- Searchable tree of OEM modules, sensors, and drives
    │   │   ├── LibrariesCard.tsx      -- Project Library (Master Types) and Global Libraries
    │   │   └── LayoutWidgetsCard.tsx  -- iOS-style 2-column modular widgets & P&ID symbols
    │   ├── bottom-inspector/          -- Zone 5 Context-Sensitive Bottom Properties & Inspector
    │   │   ├── PropertiesGeneralTab.tsx -- Element metadata, author, version, and comments
    │   │   ├── PropertiesIoAddressesTab.tsx -- %I/%Q bounds, process image, tag channel table
    │   │   ├── PropertiesDiagnosticsTab.tsx -- Live hardware status, CRC packet counters, log
    │   │   ├── PropertiesAlarmsTab.tsx -- ISA-18.2 active alarm banner and setpoint thresholds
    │   │   └── CompilerConsoleTab.tsx  -- STruC++ / OpenPLC build logs and hot-reload status
    │   ├── hmi-screens/               -- TIA Portal style HMI screens & sub-screens
    │   │   ├── ScreenHierarchyTree.tsx -- Zone 2 nested tree for screens and subscreens
    │   │   ├── ScreenCanvasHost.tsx   -- Zone 3 tab host for FHD/4K screen drafting & preview
    │   │   ├── SubScreenModal.tsx     -- Contextual popup sub-screen floating wrapper
    │   │   ├── ScreenLifecycleBar.tsx -- Staging control bar (Edit, Preview, Deploy buttons)
    │   │   └── CommitDiffModal.tsx    -- Visual diff summary and pre-flight validation modal    │   ├── faceplate-designer/        -- TIA Portal style Faceplate Master Type authoring
    │   │   ├── FaceplateMasterDesigner.tsx -- Master Zone 3 canvas for component type design
    │   │   ├── UdtInterfaceEditor.tsx -- Formal UDT property binding matrix (no concrete tags)
    │   │   ├── views/                 -- 4 persona authoring canvases for master faceplate
    │   │   │   ├── PidSymbolView.tsx      -- ISA-101 vector symbol layout
    │   │   │   ├── InteractiveCardView.tsx -- Actuation mimic card layout
    │   │   │   ├── PopupModalView.tsx     -- Diagnostic deep-dive popup layout
    │   │   │   └── AlarmRowView.tsx       -- High-density annunciator row layout
    │   │   └── PropagationDialog.tsx  -- Global version bump (`v1.0.0` -> `v1.1.0`) impact analyzer
    │   ├── runtime-kiosk/             -- WinCC-style fullscreen industrial runtime mode
    │   │   ├── RuntimeKioskShell.tsx  -- Fullscreen viewport container (document.requestFullscreen)
    │   │   ├── KioskAlarmBanner.tsx   -- ISA-18.2 top/bottom global operational alarm ticker
    │   │   └── ExitRuntimeDialog.tsx  -- IEC 62443 credential challenge gate to return to IDE
    │   ├── logic-editor/              -- Integrated in-canvas PLC programming environment
    │   │   ├── MonacoStEditor.tsx     -- Monaco editor with IEC 61131-3 ST syntax & y-monaco
    │   │   ├── LadderCanvas.tsx       -- Visual rung editor with contact/coil drag-and-drop
    │   │   └── DeployControlBar.tsx   -- Deploy / Hot-Reload action bar with compiler diagnostics
    │   ├── topology/                  -- Network graph viewer (React Flow / SVG node canvas)
    │   ├── alarms/                    -- ISA-18.2 alarm annunciator, summary grid, shelve dialog
    │   ├── recipes/                   -- ISA-88 recipe execution dashboard, phase sequence view
    │   └── historian/                 -- High-speed WebGL/Canvas trend sparklines & query deck
    │
    ├── stores/                        -- Global client state managed via Zustand
    │   ├── useTagStore.ts             -- Real-time process tag cache (100ms throttled)
    │   ├── useTopologyStore.ts        -- Network device graph, links, discovery scan state
    │   ├── useAlarmStore.ts           -- Active alarm records, unack badges, alarm filter states
    │   ├── useInspectorStore.ts       -- Active selection, inspector drawer layout, display mode
    │   ├── useProjectStore.ts         -- Multi-project lifecycle (active vs draft staging buffers)
    │   ├── useHmiStore.ts             -- Screens hierarchy, tabs, 3-stage lifecycle, kiosk fullscreen
    │   ├── useFaceplateStore.ts       -- Faceplate Master Library, UDT interfaces, version propagation
    │   └── useCollabStore.ts          -- Active collaboration session, presence, awareness
    │
    ├── hooks/                         -- Custom reusable React hooks
    │   ├── useSseTelemetry.ts         -- SSE connection with auto-reconnect & 100ms throttle
    │   ├── useCollabRoom.ts           -- Yjs document & awareness room binder
    │   ├── useDebouncedAutosave.ts    -- 1000ms idle autosave trigger to backend staging WAL
    │   ├── useFullscreenKiosk.ts      -- Browser Fullscreen API wrapper with error recovery
    │   └── useHotkeys.ts              -- Standard industrial keyboard shortcuts (Deploy, Ack, Kiosk)
    │
    ├── services/                      -- Typed HTTP and WebSocket clients
    │   ├── apiClient.ts               -- Fetch wrapper for /api/v1 REST endpoints
    │   ├── sseService.ts              -- EventSource lifecycle manager with exponential backoff
    │   └── wsCollabService.ts         -- y-websocket provider factory
    │
    └── types/                         -- TypeScript interfaces generated from Rust Specta schemas
        ├── tags.ts                    -- TagDto, TagQuality, AccessMode
        ├── topology.ts                -- DeviceDescriptor, DriverConfig, PortLink
        ├── alarms.ts                  -- AlarmRecord, AlarmState, AlarmSeverity
        ├── project.ts                 -- ProjectDescriptor, ProjectConfig, ProjectBufferState
        ├── hmi.ts                     -- ScreenDescriptor, SubScreenDescriptor, HmiLifecycleMode
        ├── faceplate.ts               -- FaceplateMasterType, FormalUdtProperty, PersonaLayoutMap
        └── logic.ts                   -- DeployRequest, DeployResponse, CompilerDiagnostic
```

---

## 3. Global State Architecture (Zustand Stores)

MythOS utilizes **Zustand** for global application state. State slices are strictly decoupled by domain:

```
+───────────────────────────────────────────────────────────────────────────────────+
|                                  ZUSTAND STORES                                   |
+──────────────────────────┬──────────────────────────┬─────────────────────────────+
|       useTagStore        |      useTopologyStore    |        useAlarmStore        |
| - 100ms Throttled Values | - Field Devices & Racks  | - ANSI/ISA-18.2 State Matrix|
| - OPC UA Quality Status  | - Physical Bus Links     | - Unacknowledged Counters   |
| - Forced Value Overrides | - Discovery Scan State   | - Shelved Alarms Registry   |
+──────────────────────────┼──────────────────────────┼─────────────────────────────+
|    useInspectorStore     |      useProjectStore     |        useCollabStore       |
| - Active Canvas Selection| - Active vs Draft WAL    | - Connected Users Presence  |
| - 4 Display Archetypes   | - Project Tree Hierarchy | - Multi-Cursor Remote State |
| - Reorderable Card Stack | - Mode: EDIT | RUNTIME   | - Yjs Document Provider     |
+──────────────────────────┼──────────────────────────┼─────────────────────────────+
|        useHmiStore       |     useFaceplateStore    |                             |
| - Screen/SubScreen Tree  | - Master Library Types   |                             |
| - 3-Stage Lifecycle State| - Formal UDT Interfaces  |                             |
| - Fullscreen Kiosk Mode  | - Global Version Bump    |                             |
+──────────────────────────┴──────────────────────────┴─────────────────────────────+
```

### 3.1 `useTagStore`: Real-Time Telemetry Cache
Handles high-throughput tag updates without triggering excessive component re-renders:

```typescript
// frontend/src/stores/useTagStore.ts
import { create } from 'zustand';
import { TagDto, TagQuality, AccessMode } from '../types/tags';

interface TagStoreState {
  tags: Map<string, TagDto>;
  pendingBatch: Map<string, TagDto>;
  lastBatchTimestamp: number;
  
  // Actions
  ingestSseUpdate: (updates: TagDto[]) => void;
  flushPendingBatch: () => void;
  setTagForce: (uri: string, value: unknown) => Promise<void>;
  clearTagForce: (uri: string) => Promise<void>;
}

export const useTagStore = create<TagStoreState>((set, get) => ({
  tags: new Map(),
  pendingBatch: new Map(),
  lastBatchTimestamp: 0,

  ingestSseUpdate: (updates) => {
    // Accumulate into pending batch; flushed every 100ms by RAF timer
    const pending = get().pendingBatch;
    for (const update of updates) {
      pending.set(update.uri, update);
    }
  },

  flushPendingBatch: () => {
    const { pendingBatch, tags } = get();
    if (pendingBatch.size === 0) return;

    const nextTags = new Map(tags);
    pendingBatch.forEach((val, key) => nextTags.set(key, val));

    pendingBatch.clear();
    set({ tags: nextTags, lastBatchTimestamp: Date.now() });
  },

  setTagForce: async (uri, value) => {
    // Optimistic UI update + REST dispatch
    await fetch(`/api/v1/tags/${encodeURIComponent(uri)}/force`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ forced_value: value }),
    });
  },

  clearTagForce: async (uri) => {
    await fetch(`/api/v1/tags/${encodeURIComponent(uri)}/force`, {
      method: 'DELETE',
    });
  },
}));
```

### 3.2 `useProjectStore`: Multi-Project Lifecycle & Dual-Buffer State
Coordinates active runtime execution against isolated draft staging buffers. The store decouples mutable drafting from immutable production runtime states:

```typescript
// frontend/src/stores/useProjectStore.ts
import { create } from 'zustand';
import { ProjectDescriptor, ProjectConfig } from '../types/project';

export type ProjectMode = 'EDIT' | 'RUNTIME';

interface ProjectStoreState {
  // Dual-Buffer Core State
  activeState: ProjectConfig | null;       // Immutable mirror of project.active.json
  draftState: ProjectConfig | null;        // Mutable local staging sandbox (project.draft.json)
  isDirty: boolean;                        // True if draftState diverges from activeState
  mode: ProjectMode;                       // Global Project view mode
  
  // Multi-Project Registry
  activeProjectId: string | null;
  loadedDraftProjectId: string | null;
  projects: ProjectDescriptor[];
  isAutosaving: boolean;
  lastAutosavedAt: number | null;
  dirtyDocuments: Set<string>;

  // Actions
  fetchProjects: () => Promise<void>;
  enterEditMode: () => void;
  enterRuntimeMode: () => Promise<boolean>;
  saveDraft: () => Promise<void>;
  commitDraft: (preflightNotes?: string) => Promise<boolean>;
  discardDraft: () => Promise<void>;
  markDocumentDirty: (docPath: string) => void;
  markDocumentClean: (docPath: string) => void;
}

export const useProjectStore = create<ProjectStoreState>((set, get) => ({
  activeState: null,
  draftState: null,
  isDirty: false,
  mode: 'RUNTIME',
  activeProjectId: null,
  loadedDraftProjectId: null,
  projects: [],
  isAutosaving: false,
  lastAutosavedAt: null,
  dirtyDocuments: new Set(),

  fetchProjects: async () => {
    const res = await fetch('/api/v1/projects');
    const projects: ProjectDescriptor[] = await res.json();
    const active = projects.find((p) => p.status === 'ACTIVE');
    const loaded = projects.find((p) => p.status === 'LOADED_DRAFT');

    let activeConfig: ProjectConfig | null = null;
    let draftConfig: ProjectConfig | null = null;

    if (active) {
      const activeRes = await fetch(`/api/v1/projects/${active.project_id}/active-config`);
      if (activeRes.ok) activeConfig = await activeRes.json();
    }
    if (loaded) {
      const draftRes = await fetch(`/api/v1/projects/${loaded.project_id}/draft-config`);
      if (draftRes.ok) draftConfig = await draftRes.json();
    }

    set({
      projects,
      activeProjectId: active ? active.project_id : null,
      loadedDraftProjectId: loaded ? loaded.project_id : null,
      activeState: activeConfig,
      draftState: draftConfig || activeConfig,
      isDirty: false,
    });
  },

  enterEditMode: () => {
    // Clone active state into draft state sandbox if draft is null
    const { activeState, draftState } = get();
    set({
      mode: 'EDIT',
      draftState: draftState || (activeState ? JSON.parse(JSON.stringify(activeState)) : null),
    });
  },

  enterRuntimeMode: async () => {
    const { isDirty } = get();
    if (isDirty) {
      // Caller must handle confirmation dialog if uncommitted changes exist
      return false;
    }
    set({ mode: 'RUNTIME' });
    return true;
  },

  saveDraft: async () => {
    const { activeProjectId, draftState } = get();
    if (!activeProjectId || !draftState) return;

    set({ isAutosaving: true });
    try {
      await fetch(`/api/v1/projects/${activeProjectId}/draft`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(draftState),
      });
      set({ isDirty: true, lastAutosavedAt: Date.now() });
    } finally {
      set({ isAutosaving: false });
    }
  },

  commitDraft: async (preflightNotes) => {
    const { activeProjectId, draftState } = get();
    if (!activeProjectId || !draftState) return false;

    const res = await fetch(`/api/v1/projects/${activeProjectId}/commit`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        preflight_notes: preflightNotes || 'Manual commit to runtime',
        target_version: 'next',
      }),
    });

    if (res.ok) {
      const updatedActive: ProjectConfig = await res.json();
      set({
        activeState: updatedActive,
        draftState: JSON.parse(JSON.stringify(updatedActive)),
        isDirty: false,
        dirtyDocuments: new Set(),
      });
      return true;
    }
    return false;
  },

  discardDraft: async () => {
    const { activeState } = get();
    // Revert draft buffer back to exact active runtime mirror
    set({
      draftState: activeState ? JSON.parse(JSON.stringify(activeState)) : null,
      isDirty: false,
      dirtyDocuments: new Set(),
    });
  },

  markDocumentDirty: (docPath) => {
    const dirty = new Set(get().dirtyDocuments);
    dirty.add(docPath);
    set({ dirtyDocuments: dirty, isDirty: true });
  },

  markDocumentClean: (docPath) => {
    const dirty = new Set(get().dirtyDocuments);
    dirty.delete(docPath);
    set({ dirtyDocuments: dirty, isDirty: dirty.size > 0 });
  },
}));
```

### 3.3 `useHmiStore`: Screen & Sub-Screen Hierarchy, 3-Stage Lifecycle, and Fullscreen Kiosk
Manages the complete TIA Portal style HMI screen tree, multi-document tab registry, 3-stage lifecycle (`EDIT` | `PREVIEW` | `RUNTIME`), and WinCC-style fullscreen industrial runtime mode:

```typescript
// frontend/src/stores/useHmiStore.ts
import { create } from 'zustand';
import { ScreenDescriptor, SubScreenDescriptor, HmiLifecycleMode } from '../types/hmi';

export interface ScreenTab {
  tabId: string;
  screenId: string;
  subscreenId?: string;
  title: string;
  isSubScreen: boolean;
  isDirty: boolean;
}

interface HmiStoreState {
  // Screen Hierarchy Registry (Zone 2 Tree)
  screens: ScreenDescriptor[];
  activeScreenId: string | null;
  activeSubscreenId: string | null;
  openTabs: ScreenTab[];
  activeTabId: string | null;

  // 3-Stage Lifecycle Mode
  lifecycleMode: HmiLifecycleMode; // 'EDIT' | 'PREVIEW' | 'RUNTIME'
  
  // Dual-Buffer Screen Payloads
  activeScreens: Map<string, any>;   // Immutable runtime mirror (screens/active/<id>.json)
  draftScreens: Map<string, any>;    // Staging sandbox (screens/draft/<id>.json)
  isHmiDirty: boolean;

  // WinCC Fullscreen Industrial Kiosk State
  isRuntimeFullscreen: boolean;
  runtimeAuthLock: boolean;

  // Actions: Hierarchy & Tabs
  setScreens: (screens: ScreenDescriptor[]) => void;
  openScreenTab: (screenId: string, subscreenId?: string) => void;
  closeScreenTab: (tabId: string) => void;
  setActiveTab: (tabId: string) => void;
  addNewScreen: (name: string, isRoot?: boolean) => Promise<string>;
  addNewSubScreen: (parentScreenId: string, name: string) => Promise<string>;

  // Actions: 3-Stage Lifecycle Transitions
  enterEditMode: () => void;
  enterPreviewMode: () => void;
  enterRuntimeMode: () => void;

  // Actions: Dual-Buffer Persistence
  updateDraftCanvas: (screenId: string, canvasData: any) => void;
  saveScreenDraft: (screenId: string) => Promise<void>;
  commitHmiDraft: () => Promise<boolean>;
  discardHmiDraft: () => void;

  // Actions: Fullscreen Kiosk Handshake
  requestEnterRuntime: () => Promise<void>;
  requestExitRuntime: (credentials: { role: string; token: string }) => Promise<boolean>;
}

export const useHmiStore = create<HmiStoreState>((set, get) => ({
  screens: [],
  activeScreenId: null,
  activeSubscreenId: null,
  openTabs: [],
  activeTabId: null,
  lifecycleMode: 'RUNTIME',
  activeScreens: new Map(),
  draftScreens: new Map(),
  isHmiDirty: false,
  isRuntimeFullscreen: false,
  runtimeAuthLock: true,

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
        openTabs: [...openTabs, { tabId, screenId, subscreenId, title, isSubScreen: Boolean(sub), isDirty: false }],
        activeTabId: tabId,
        activeScreenId: screenId,
        activeSubscreenId: subscreenId || null,
      });
    } else {
      set({ activeTabId: tabId, activeScreenId: screenId, activeSubscreenId: subscreenId || null });
    }
  },

  closeScreenTab: (tabId) => {
    const { openTabs, activeTabId } = get();
    const remaining = openTabs.filter((t) => t.tabId !== tabId);
    let nextActive = activeTabId;
    if (activeTabId === tabId) {
      nextActive = remaining.length > 0 ? remaining[remaining.length - 1].tabId : null;
    }
    set({ openTabs: remaining, activeTabId: nextActive });
  },

  setActiveTab: (tabId) => {
    const tab = get().openTabs.find((t) => t.tabId === tabId);
    if (tab) {
      set({ activeTabId: tabId, activeScreenId: tab.screenId, activeSubscreenId: tab.subscreenId || null });
    }
  },

  addNewScreen: async (name, isRoot = false) => {
    const res = await fetch('/api/v1/hmi/screens', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, is_root: isRoot }),
    });
    const newScreen: ScreenDescriptor = await res.json();
    set((s) => ({ screens: [...s.screens, newScreen] }));
    get().openScreenTab(newScreen.id);
    return newScreen.id;
  },

  addNewSubScreen: async (parentScreenId, name) => {
    const res = await fetch(`/api/v1/hmi/screens/${parentScreenId}/subscreens`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, type: 'POPUP' }),
    });
    const newSub: SubScreenDescriptor = await res.json();
    set((s) => ({
      screens: s.screens.map((sc) =>
        sc.id === parentScreenId ? { ...sc, subscreens: [...sc.subscreens, newSub] } : sc
      ),
    }));
    get().openScreenTab(parentScreenId, newSub.id);
    return newSub.id;
  },

  enterEditMode: () => {
    // Switch to staging canvas; copy active buffers to draft if absent
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

    await fetch(`/api/v1/hmi/screens/draft/${screenId}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(canvasData),
    });
  },

  commitHmiDraft: async () => {
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
    return false;
  },

  discardHmiDraft: () => {
    const { activeScreens } = get();
    const reverted = new Map();
    activeScreens.forEach((v, k) => reverted.set(k, JSON.parse(JSON.stringify(v))));
    set({ draftScreens: reverted, isHmiDirty: false });
  },

  requestEnterRuntime: async () => {
    // Expand to WinCC fullscreen kiosk
    try {
      if (!document.fullscreenElement) {
        await document.documentElement.requestFullscreen();
      }
      set({ isRuntimeFullscreen: true, lifecycleMode: 'RUNTIME', runtimeAuthLock: true });
    } catch (err) {
      console.warn('[HMI Kiosk] Fullscreen request suppressed or unsupported:', err);
      set({ isRuntimeFullscreen: true, lifecycleMode: 'RUNTIME', runtimeAuthLock: true });
    }
  },

  requestExitRuntime: async (credentials) => {
    // Verify IEC 62443 authorization before restoring IDE chrome
    const res = await fetch('/api/v1/auth/verify-role', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(credentials),
    });
    const auth = await res.json();
    if (auth.authorized && (credentials.role === 'Engineer' || credentials.role === 'Admin')) {
      if (document.fullscreenElement) {
        await document.exitFullscreen();
      }
      set({ isRuntimeFullscreen: false, runtimeAuthLock: false, lifecycleMode: 'EDIT' });
      return true;
    }
    return false;
  },
}));
```

### 3.4 `useFaceplateStore`: Master Component Types & Global Version Propagation Engine
Provides central state for authoring reusable Faceplate types against formal UDT interfaces in Zone 3 Master Designer and propagating version updates across all plant screens:

```typescript
// frontend/src/stores/useFaceplateStore.ts
import { create } from 'zustand';
import { FaceplateMasterType, FormalUdtProperty, PersonaLayoutMap } from '../types/faceplate';

export type FaceplatePersonaTab = 'PID' | 'INTERACTIVE' | 'POPUP' | 'ALARM_ROW';

interface FaceplateStoreState {
  // Master Library Registry (Zone 2 Faceplate Library)
  masterTypes: Map<string, FaceplateMasterType>;
  activeTypeId: string | null;
  activePersonaTab: FaceplatePersonaTab;
  isDesignerDirty: boolean;

  // Actions: Library Management
  fetchMasterTypes: () => Promise<void>;
  createMasterType: (name: string, targetUdt: string) => Promise<string>;
  setActiveMasterType: (typeId: string) => void;
  setActivePersonaTab: (tab: FaceplatePersonaTab) => void;

  // Actions: Master Type Designer Canvas
  updateFormalUdtProperties: (typeId: string, properties: FormalUdtProperty[]) => void;
  updatePersonaLayout: (typeId: string, persona: FaceplatePersonaTab, layoutData: any) => void;

  // Actions: Global Propagation Engine (TIA Portal Library Pattern)
  bumpVersionAndPropagate: (typeId: string, newVersion: string) => Promise<{ updatedScreensCount: number }>;
}

export const useFaceplateStore = create<FaceplateStoreState>((set, get) => ({
  masterTypes: new Map(),
  activeTypeId: null,
  activePersonaTab: 'PID',
  isDesignerDirty: false,

  fetchMasterTypes: async () => {
    const res = await fetch('/api/v1/hmi/faceplates/library');
    const list: FaceplateMasterType[] = await res.json();
    const map = new Map(list.map((item) => [item.id, item]));
    set({ masterTypes: map });
  },

  createMasterType: async (name, targetUdt) => {
    const res = await fetch('/api/v1/hmi/faceplates/library', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, target_udt: targetUdt, version: '1.0.0' }),
    });
    const created: FaceplateMasterType = await res.json();
    set((s) => {
      const next = new Map(s.masterTypes);
      next.set(created.id, created);
      return { masterTypes: next, activeTypeId: created.id };
    });
    return created.id;
  },

  setActiveMasterType: (typeId) => set({ activeTypeId: typeId }),
  setActivePersonaTab: (tab) => set({ activePersonaTab: tab }),

  updateFormalUdtProperties: (typeId, properties) => {
    const item = get().masterTypes.get(typeId);
    if (!item) return;
    const updated = { ...item, formalProperties: properties };
    set((s) => {
      const next = new Map(s.masterTypes);
      next.set(typeId, updated);
      return { masterTypes: next, isDesignerDirty: true };
    });
  },

  updatePersonaLayout: (typeId, persona, layoutData) => {
    const item = get().masterTypes.get(typeId);
    if (!item) return;
    const layouts = { ...item.personaLayouts, [persona]: layoutData };
    const updated = { ...item, personaLayouts: layouts };
    set((s) => {
      const next = new Map(s.masterTypes);
      next.set(typeId, updated);
      return { masterTypes: next, isDesignerDirty: true };
    });
  },

  bumpVersionAndPropagate: async (typeId, newVersion) => {
    // Dispatches atomic propagation request to backend WAL engine
    const res = await fetch(`/api/v1/hmi/faceplates/library/${typeId}/propagate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ new_version: newVersion }),
    });
    const result = await res.json();
    await get().fetchMasterTypes();
    set({ isDesignerDirty: false });
    return { updatedScreensCount: result.propagated_screens_count };
  },
}));
```

---

## 4. Real-Time Telemetry SSE Listener Hook (`useSseTelemetry`)

To ensure robust field operation over unreliable wireless links or noisy industrial Ethernet switches, the SSE client implements exponential backoff reconnection with an internal 100 ms flush loop. It also consumes layout change notifications for live operator synchronization:

```typescript
// frontend/src/hooks/useSseTelemetry.ts
import { useEffect, useRef } from 'react';
import { useTagStore } from '../stores/useTagStore';
import { useAlarmStore } from '../stores/useAlarmStore';
import { useHmiStore } from '../stores/useHmiStore';

interface SseTelemetryOptions {
  endpoint?: string;
  minReconnectDelayMs?: number;
  maxReconnectDelayMs?: number;
}

export function useSseTelemetry({
  endpoint = '/api/v1/telemetry/stream',
  minReconnectDelayMs = 100,
  maxReconnectDelayMs = 5000,
}: SseTelemetryOptions = {}) {
  const ingestTagUpdate = useTagStore((s) => s.ingestSseUpdate);
  const flushTagBatch = useTagStore((s) => s.flushPendingBatch);
  const ingestAlarmTransition = useAlarmStore((s) => s.ingestAlarmTransition);
  const lifecycleMode = useHmiStore((s) => s.lifecycleMode);

  const reconnectDelayRef = useRef(minReconnectDelayMs);
  const eventSourceRef = useRef<EventSource | null>(null);

  useEffect(() => {
    let isSubscribed = true;
    let flushIntervalId: number;

    // 100ms batch flush loop (10 Hz DOM update rate)
    flushIntervalId = window.setInterval(() => {
      flushTagBatch();
    }, 100);

    function connect() {
      if (!isSubscribed) return;

      const es = new EventSource(endpoint);
      eventSourceRef.current = es;

      es.onopen = () => {
        // Reset reconnect delay on successful connection
        reconnectDelayRef.current = minReconnectDelayMs;
      };

      es.addEventListener('tag_update', (event) => {
        try {
          const payload = JSON.parse(event.data);
          ingestTagUpdate(payload.updates);
        } catch (err) {
          console.error('[SSE] Failed to parse tag_update payload:', err);
        }
      });

      es.addEventListener('alarm_transition', (event) => {
        try {
          const alarm = JSON.parse(event.data);
          // Alarms bypass 100ms batching for immediate audible annunciator
          ingestAlarmTransition(alarm);
        } catch (err) {
          console.error('[SSE] Failed to parse alarm_transition payload:', err);
        }
      });

      // Hot-reload committed HMI layout updates across active operator panels
      es.addEventListener('HMI_LAYOUT_UPDATED', async (event) => {
        try {
          const updateNotification = JSON.parse(event.data);
          console.log('[SSE] Hot-reloading active HMI layout version:', updateNotification.version);
          if (useHmiStore.getState().lifecycleMode === 'RUNTIME') {
            const res = await fetch('/api/v1/hmi/screens/active');
            if (res.ok) {
              const activeScreens = await res.json();
              useHmiStore.setState({ activeScreens: new Map(Object.entries(activeScreens)) });
            }
          }
        } catch (err) {
          console.error('[SSE] Failed to process HMI_LAYOUT_UPDATED event:', err);
        }
      });

      es.onerror = () => {
        es.close();
        if (!isSubscribed) return;

        // Exponential backoff reconnect
        const delay = reconnectDelayRef.current;
        reconnectDelayRef.current = Math.min(delay * 1.5, maxReconnectDelayMs);
        setTimeout(connect, delay);
      };
    }

    connect();

    return () => {
      isSubscribed = false;
      window.clearInterval(flushIntervalId);
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
      }
    };
  }, [endpoint, ingestTagUpdate, flushTagBatch, ingestAlarmTransition]);
}
```

---

## 5. Multi-User Real-Time Collaboration (CRDT / Yjs)

The collaborative editing engine enables simultaneous engineering without locking files:

```
+───────────────────────────────────────────────────────────────────────────────────+
|                                CLIENT A (Engineer 1)                              |
|  - Monaco ST Editor (y-monaco) ──► Y.Text("logic/main.st")                         |
|  - Bounded Canvas Node Drag   ──► Y.Map("screens/draft/screen_01.json")           |
|  - Mouse Cursor Tracking      ──► Awareness.setLocalStateField("cursor")          |
+─────────────────────────────────────────┬─────────────────────────────────────────+
                                          │
                                          │ WebSocket: /api/v1/collab/ws
                                          ▼
+───────────────────────────────────────────────────────────────────────────────────+
|                     RUST BACKEND COLLABORATIVE ACTOR (yrs 0.21)                   |
|  - Maintains Shared In-Memory yrs::Doc Per Project Room                           |
|  - Broadcasts Binary Update Vectors to All Connected Sockets                      |
|  - Appends Changes to project.wal via Debounced Autosave Pipeline                 |
+─────────────────────────────────────────┬─────────────────────────────────────────+
                                          │
                                          │ WebSocket: /api/v1/collab/ws
                                          ▼
+───────────────────────────────────────────────────────────────────────────────────+
|                                CLIENT B (Engineer 2)                              |
|  - Peer cursor badge "Engineer 1" appears in Monaco line 14 with blue highlight   |
|  - Moving motor symbol on Canvas updates position instantly without page reload   |
|  - Live Avatar in Zone 1 Top HUD displays active user list with online badges     |
+───────────────────────────────────────────────────────────────────────────────────+
```

### 5.1 Collaboration Room Binder Hook (`useCollabRoom`)

```typescript
// frontend/src/hooks/useCollabRoom.ts
import { useEffect, useState } from 'react';
import * as Y from 'yjs';
import { WebsocketProvider } from 'y-websocket';
import { useCollabStore } from '../stores/useCollabStore';

interface CollabUser {
  id: string;
  name: string;
  color: string;
  avatarUrl?: string;
}

export function useCollabRoom(projectId: string, currentUser: CollabUser) {
  const [doc] = useState(() => new Y.Doc());
  const [provider, setProvider] = useState<WebsocketProvider | null>(null);
  const setCollaborators = useCollabStore((s) => s.setCollaborators);

  useEffect(() => {
    const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${wsProtocol}//${window.location.host}/api/v1/collab/ws`;

    const wsProvider = new WebsocketProvider(wsUrl, projectId, doc, {
      params: {
        user_id: currentUser.id,
        user_name: currentUser.name,
        color: currentUser.color,
      },
    });

    // Set local awareness state
    wsProvider.awareness.setLocalStateField('user', currentUser);

    // Listen to remote awareness changes (presence avatars & cursors)
    wsProvider.awareness.on('change', () => {
      const states = Array.from(wsProvider.awareness.getStates().values());
      const users = states
        .map((s: any) => s.user)
        .filter((u): u is CollabUser => Boolean(u));
      setCollaborators(users);
    });

    setProvider(wsProvider);

    return () => {
      wsProvider.disconnect();
      doc.destroy();
    };
  }, [projectId, currentUser, doc, setCollaborators]);

  return { doc, provider };
}
```

### 5.2 Monaco Collaborative Editor (`MonacoStEditor.tsx`)
Binds the Monaco editor instance to the shared `Y.Text` CRDT type:

```typescript
// frontend/src/features/logic-editor/MonacoStEditor.tsx
import React, { useEffect, useRef } from 'react';
import * as monaco from 'monaco-editor';
import { MonacoBinding } from 'y-monaco';
import * as Y from 'yjs';
import { WebsocketProvider } from 'y-websocket';

interface MonacoStEditorProps {
  doc: Y.Doc;
  provider: WebsocketProvider;
  documentPath?: string;
}

export const MonacoStEditor: React.FC<MonacoStEditorProps> = ({
  doc,
  provider,
  documentPath = 'logic/main.st',
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    // Initialize Monaco instance
    const editor = monaco.editor.create(containerRef.current, {
      language: 'structured-text',
      theme: 'vs-dark',
      automaticLayout: true,
      minimap: { enabled: true },
      fontSize: 13,
      lineNumbers: 'on',
    });
    editorRef.current = editor;

    // Obtain or create CRDT text document for this path
    const yText = doc.getText(documentPath);

    // Bind Monaco editor buffer to Yjs CRDT with remote cursor display
    const binding = new MonacoBinding(
      yText,
      editor.getModel()!,
      new Set([editor]),
      provider.awareness
    );

    return () => {
      binding.destroy();
      editor.dispose();
    };
  }, [doc, provider, documentPath]);

  return <div ref={containerRef} className="w-full h-full" />;
};
```

---

## 6. Integrated In-Canvas PLC Logic Editor

The MythOS logic editor is **100% integrated into Zone 3 of the workbench**. It eliminates external compilation tools or manual `.st` file uploads.

```
+───────────────────────────────────────────────────────────────────────────────────+
| Zone 3 Tab: [main.st (Integrated Monaco Editor)]                   [Deploy to PLC] |
+───────────────────────────────────────────────────────────────────────────────────+
| 01 | PROGRAM MainBottlingControl                                                  |
| 02 |   VAR                                                                        |
| 03 |     Conveyor_Motor AT %QX0.0 : BOOL;                                         |
| 04 |     Bottle_Detect_PE AT %IX0.0 : BOOL;                                       |
| 05 |     Capping_Cylinder AT %QX0.1 : BOOL;                                       |
| 06 |   END_VAR                                                                    |
| 07 |                                                                              |
| 08 |   IF Bottle_Detect_PE THEN                                                   |
| 09 |     Capping_Cylinder := TRUE;    ◄── Remote peer (Neo) cursor active here    |
| 10 |   ELSE                                                                       |
| 11 |     Capping_Cylinder := FALSE;                                               |
| 12 |   END_IF;                                                                    |
| 13 | END_PROGRAM                                                                  |
+───────────────────────────────────────────────────────────────────────────────────+
| Zone 5 Output Console:                                                            |
| [14:22:01] POST /api/v1/control/deploy dispatched.                                |
| [14:22:01] STruC++ compiler: generated.cpp produced (0 warnings).                 |
| [14:22:02] libopenplc_program_v5.so hot-swapped during 410us scan dead-time.      |
| [14:22:02] Hot-reload SUCCESSFUL. Production control loop unaffected.             |
+───────────────────────────────────────────────────────────────────────────────────+
```

### 6.1 Deploy Action Bar Component (`DeployControlBar.tsx`)

```typescript
// frontend/src/features/logic-editor/DeployControlBar.tsx
import React, { useState } from 'react';
import { useProjectStore } from '../../stores/useProjectStore';

interface DeployControlBarProps {
  getStSource: () => string;
}

export const DeployControlBar: React.FC<DeployControlBarProps> = ({ getStSource }) => {
  const [isDeploying, setIsDeploying] = useState(false);
  const [deployResult, setDeployResult] = useState<string | null>(null);

  const handleDeploy = async () => {
    setIsDeploying(true);
    setDeployResult(null);

    try {
      const sourceCode = getStSource();
      const response = await fetch('/api/v1/control/deploy', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          st_source: sourceCode,
          target_program: 'MainProgram',
          dry_run: false,
        }),
      });

      const data = await response.json();
      if (response.ok && data.success) {
        setDeployResult(`Hot-Reload Successful! Version ${data.library_version} loaded in ${data.hot_swap_deadtime_us}µs dead-time.`);
      } else {
        const errorMsg = data.errors?.[0]?.message || 'Compilation failed';
        setDeployResult(`Deploy Failed: ${errorMsg}`);
      }
    } catch (err: any) {
      setDeployResult(`Network Error: ${err.message}`);
    } finally {
      setIsDeploying(false);
    }
  };

  return (
    <div className="flex items-center justify-between px-4 py-2 bg-neutral-900 border-b border-neutral-700">
      <div className="flex items-center space-x-2">
        <span className="text-xs font-semibold text-neutral-400">RUNTIME TARGET:</span>
        <span className="text-xs font-mono text-emerald-400">OpenPLC v4 (SCHED_FIFO Core 1)</span>
      </div>

      <div className="flex items-center space-x-4">
        {deployResult && (
          <span className="text-xs font-mono text-neutral-300">{deployResult}</span>
        )}
        <button
          onClick={handleDeploy}
          disabled={isDeploying}
          className="px-3 py-1.5 text-xs font-semibold text-white bg-blue-600 hover:bg-blue-500 disabled:bg-neutral-700 rounded transition-colors flex items-center space-x-1.5"
        >
          {isDeploying ? (
            <span>Compiling STruC++...</span>
          ) : (
            <span>Deploy / Hot-Reload to Controller</span>
          )}
        </button>
      </div>
    </div>
  );
};
```

---

## 7. Atomic Debounced Autosave Hook (`useDebouncedAutosave`)

Every modification made in the Monaco editor, canvas widget layout, or faceplate designer triggers a debounced timer. If no additional input occurs within **1000 ms**, the delta is committed to the backend Write-Ahead Log under the sandboxed draft buffer (`project.draft.json`, `screens/draft/<screen_id>.json`):

```typescript
// frontend/src/hooks/useDebouncedAutosave.ts
import { useEffect, useRef } from 'react';
import { useProjectStore } from '../stores/useProjectStore';

export function useDebouncedAutosave(
  projectId: string,
  documentPath: string,
  content: string,
  delayMs: number = 1000
) {
  const markDirty = useProjectStore((s) => s.markDocumentDirty);
  const markClean = useProjectStore((s) => s.markDocumentClean);
  const timerRef = useRef<number | null>(null);
  const lastSavedContentRef = useRef<string>(content);

  useEffect(() => {
    // If content has not changed from last saved state, do nothing
    if (content === lastSavedContentRef.current) return;

    markDirty(documentPath);

    if (timerRef.current) {
      window.clearTimeout(timerRef.current);
    }

    timerRef.current = window.setTimeout(async () => {
      try {
        const response = await fetch(`/api/v1/projects/${projectId}/autosave`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            client_id: 'client_session',
            document_path: documentPath, // Points to draft path: screens/draft/<id>.json
            payload: content,
          }),
        });

        if (response.ok) {
          lastSavedContentRef.current = content;
          markClean(documentPath);
        }
      } catch (err) {
        console.error('[Autosave] Failed to commit staging WAL delta:', err);
      }
    }, delayMs);

    return () => {
      if (timerRef.current) {
        window.clearTimeout(timerRef.current);
      }
    };
  }, [projectId, documentPath, content, delayMs, markDirty, markClean]);
}
```

---

## 8. Dockview 5-Zone Workbench Shell Integration (Siemens TIA Portal Parity)

The root workbench shell (`WorkbenchShell.tsx`) coordinates the 5 display zones using `@hexact/dockview-react`, strictly enforcing Siemens TIA Portal workspace conventions:
- **Zone 1 (Top HUD Banner - 38px)**: Real-time system health, controller cycle time, active user presence, and global ISA-18.2 alarm counters.
- **Zone 1.1 (Industrial Status Bar - 24px)**: Microkernel engine status, communication gateway telemetry, and clock.
- **Zone 2 (Primary Project Explorer - 240px to 360px)**: Canonical project tree taxonomy containing Devices & Networks, Controller Soft-Logic, and Visualization.
- **Zone 3 (Main Engineering Canvas - 100% Flex)**: Multi-document tab host running the 3 Canonical Hardware Views (Network View, Device View, Topology View), ST/Ladder logic editors, and HMI screen designers.
- **Zone 4 (Far-Right Collapsible Task Cards - Fixed 384px)**: Vertical collapsible task card tabs (`Instructions`, `Hardware Catalog`, `Libraries`, `Layout & Widgets`).
- **Zone 5 (Context-Sensitive Bottom Properties & Inspector - 180px to 320px)**: Contextual tab deck (`General`, `IO Addresses / Variables`, `Diagnostics`, `Alarms`) dynamically synchronized to the active element selected in Zone 3.

```typescript
// frontend/src/features/workbench/WorkbenchShell.tsx
import React, { useRef } from 'react';
import { DockviewReact, DockviewReadyEvent } from 'dockview-react';
import 'dockview-react/dist/styles/dockview.css';

import { Zone1TopHud } from './Zone1TopHud';
import { Zone1StatusBar } from './Zone1StatusBar';
import { Zone2Explorer } from './Zone2Explorer';
import { Zone3MainCanvas } from './Zone3MainCanvas';
import { Zone4TaskCardsDock } from './Zone4TaskCardsDock';
import { Zone5BottomInspector } from './Zone5BottomInspector';
import { useSseTelemetry } from '../../hooks/useSseTelemetry';
import { useHmiStore } from '../../stores/useHmiStore';
import { RuntimeKioskShell } from '../runtime-kiosk/RuntimeKioskShell';

export const WorkbenchShell: React.FC = () => {
  // Activate 100ms throttled SSE telemetry listener
  useSseTelemetry();

  const isRuntimeFullscreen = useHmiStore((s) => s.isRuntimeFullscreen);

  // If in WinCC-style fullscreen industrial kiosk mode, suppress all IDE chrome
  if (isRuntimeFullscreen) {
    return <RuntimeKioskShell />;
  }

  const onDockviewReady = (event: DockviewReadyEvent) => {
    const api = event.api;

    // Register dockview panel components
    api.registerPanel('explorer', Zone2Explorer);
    api.registerPanel('main_canvas', Zone3MainCanvas);
    api.registerPanel('task_cards', Zone4TaskCardsDock);
    api.registerPanel('bottom_inspector', Zone5BottomInspector);

    // Build canonical TIA Portal industrial layout
    api.addPanel({ id: 'zone2_explorer', component: 'explorer', position: { direction: 'left' } });
    api.addPanel({ id: 'zone3_canvas', component: 'main_canvas', position: { direction: 'right' } });
    api.addPanel({ id: 'zone4_task_cards', component: 'task_cards', position: { direction: 'right' } });
    api.addPanel({ id: 'zone5_inspector', component: 'bottom_inspector', position: { direction: 'below' } });
  };

  return (
    <div className="flex flex-col w-screen h-screen bg-neutral-950 text-neutral-100 overflow-hidden select-none">
      {/* Zone 1: Top Industrial HUD Banner (38px) */}
      <Zone1TopHud />

      {/* Main Workspace Host (Zones 2, 3, 4, 5 managed by Dockview) */}
      <div className="flex-1 relative w-full h-full min-h-0">
        <DockviewReact onReady={onDockviewReady} className="dockview-theme-dark w-full h-full" />
      </div>

      {/* Zone 1.1: Industrial Status Bar (24px) */}
      <Zone1StatusBar />
    </div>
  );
};
```

---

## 9. Siemens TIA Portal Canonical Project Tree Taxonomy (Zone 2 Primary Explorer)

Zone 2 Primary Explorer hosts the standardized hierarchical project tree taxonomy defined in **SYS-REQ-097**, mirroring Siemens TIA Portal structure:

```
+───────────────────────────────────────────────────────────────────────────+
| ZONE 2: PRIMARY PROJECT EXPLORER (240px - 360px Resizable)                |
+───────────────────────────────────────────────────────────────────────────+
| ▼ Project: BottlingPlant_Line01                                           |
|   ▼ Devices & Networks                                                    |
|     ├── 🌐 Network View (Fieldbus overview & subnet mapping)              |
|     ├── 🎛️ Device View (Chassis rack & slot configuration)                |
|     └── 🔌 Topology View (Physical port-to-port cable wiring)             |
|   ▼ Controller [MythOS Soft-Logic / STruC++]                              |
|     ├── ⚙️ Device configuration & Network interfaces                      |
|     ├── 🩺 Online & Diagnostics (Live CPU cycle time, buffer usage)       |
|     ▼ 📜 Program Blocks                                                   |
|       ├── [➕ Add new block: OB / FB / FC / DB]                           |
|       ├── Main [OB1] (Cyclic main task execution)                         |
|       ├── FastInterrupt [OB35] (10ms periodic interrupt task)             |
|       └── Function Blocks & Data Blocks (FB_Motor, DB_PlantParameters)    |
|     ▼ 🏷️ PLC Tags                                                         |
|       ├── Show all tags (Flat search table)                               |
|       └── Tag Tables (external.* physical vs internal.* logic variables)  |
|     ▼ 🧱 PLC Data Types (UDTs - User Defined Data Types)                  |
|     └── 👁️ Watch and Force Tables (Online variable forcing deck)          |
|   ▼ Visualization [MythOS Web HMI / WinCC Kiosk]                          |
|     ▼ 🖥️ HMI Screens (Plant Operational Screens)                          |
|       ├── [➕ Add new screen]                                             |
|       ├── Screen_01_PlantOverview (Main process dashboard)               |
|       └── Screen_02_BoilerArea (with nested Sub-Screens)                  |
|     ▼ 📑 Screen Management (Templates, Pop-up screens, Slide-in drawers)  |
|     ▼ 📦 Faceplate Types (Project Library Master Component Templates)     |
|     ├── 🚨 HMI Tags & Alarm Management (ISA-18.2 Annunciator Matrix)      |
|     └── 🧪 Recipes (ISA-88 Batch Execution Engine)                        |
+───────────────────────────────────────────────────────────────────────────+
```

### 9.1 Multi-Tab Workspace Navigation in Zone 3
- **Double-Click on Network View, Device View, or Topology View**: Opens the dedicated **Devices & Networks** multi-mode canvas in Zone 3 (`DevicesNetworksCanvas.tsx`).
- **Double-Click on Program Blocks (`OB1`, `FB_Motor`)**: Opens the **Monaco IEC 61131-3 Logic Editor** tab in Zone 3.
- **Double-Click on HMI Screen or Sub-Screen**: Opens the **HMI Screen Designer / Preview Canvas** tab in Zone 3 (`ScreenCanvasHost.tsx`).
- **Double-Click on Faceplate Library Type**: Opens the dedicated **Faceplate Master Designer** tab (`FaceplateMasterDesigner.tsx`) in Zone 3.

### 9.2 Zone 4 Far-Right Task Cards Integration
When working in Zone 3, Zone 4 presents contextually relevant task cards:
- In **Logic Editor**: The `Instructions` task card displays standard IEC 61131-3 operators (Bit logic, Timers, Counters, Math).
- In **Devices & Networks**: The `Hardware Catalog` task card displays modular racks, I/O slices, communication adapters, and drives for direct drag-and-drop into chassis racks.
- In **HMI Screen Designer**: The `Layout & Widgets` and `Libraries` task cards display P&ID symbols, modular widgets, and master faceplate types.

### 9.3 Zone 5 Context-Sensitive Bottom Properties Synchronization
Selecting any element in Zone 3 drives Zone 5 with zero latency:
- Selecting an I/O module in **Device View** reveals its hardware start/end addresses (`%I10.0..%I11.7`), channel inversion bits, and diagnostics buffer.
- Selecting a logic block in **Program Blocks** reveals its author, version, cycle allocation, and local tag declarations.
- Selecting an HMI graphic object reveals its W3C DTCG styling tokens, dynamic alarm bindings, and coordinate geometry.
The Faceplate Master Designer decouples graphic component engineering from specific field device tags.

```
+───────────────────────────────────────────────────────────────────────────────────────────────+
| Zone 3 Tab: [📦 FP_Motor_VFD (Master Designer v1.0.0)]           [💾 Save Type] [🚀 Bump v1.1.0]|
+───────────────────────────────────────────────────────────────────────────────────────────────+
| Persona Views: [ (•) P&ID Symbol View | ( ) Interactive Card | ( ) Popup Modal | ( ) Alarm Row] |
+───────────────────────────────────────────────────────────────────────────────────────────────+
| FORMAL UDT INTERFACE MATRIX (Zero Concrete Tags):                                              |
| - UDT.bRunning       : BOOL   (Read-Only Telemetry)                                           |
| - UDT.bFault         : BOOL   (Read-Only Telemetry)                                           |
| - UDT.rSpeedActual   : REAL   (Read-Only Telemetry - Unit: RPM)                               |
| - UDT.rSpeedSetpoint : REAL   (Read-Write Setpoint  - Unit: RPM)                              |
| - UDT.bStartCmd      : BOOL   (Read-Write Pulse Command)                                      |
| - UDT.bStopCmd       : BOOL   (Read-Write Pulse Command)                                      |
+───────────────────────────────────────────────────────────────────────────────────────────────+
| PERSONA CANVAS: (P&ID Vector Symbol Editor - 16px Grid Snap)                                  |
|                                                                                               |
|       [ SVG Motor Circle: dynamic fill = UDT.bFault ? Red : UDT.bRunning ? Cyan : Slate ]     |
|       [ Text Label: {UDT.rSpeedActual} RPM ]                                                  |
|                                                                                               |
+───────────────────────────────────────────────────────────────────────────────────────────────+
```

### 10.1 Persona Canvas Tabs
When authoring a Master Faceplate Type, the engineer designs four standardized presentation archetypes:
1. **P&ID Symbol View**: Muted 2D SVG vector symbol for plant overview schematics.
2. **Interactive Card View**: Modular actuation widget with Start/Stop buttons and Setpoint jog sliders.
3. **Popup Modal View**: Floating diagnostic deep-dive with PID tuning graphs and interlock diagnostics.
4. **Alarm Row View**: High-density table annunciator row rendering status and alarm priorities.

### 10.2 Instantiation & Screen Drag-and-Drop
1. An engineer drags `FP_Motor_VFD` from the Faceplate Library toolbox onto `Screen_02_BoilerSystem`.
2. The UI displays an atomic binding prompt:
   `[ Assign Target PLC UDT Instance: Line1.Boiler.FeedWater_Pump_01 (FB_Motor_VFD) ]`
3. All internal properties (`UDT.bRunning`, `UDT.rSpeedActual`, etc.) resolve immediately to concrete physical tags (`Line1.Boiler.FeedWater_Pump_01.bRunning`). Zero manual wire binding is required.

### 10.3 Global Version Propagation Engine
When an engineer alters a master Faceplate Type in the library (e.g. adding an emergency interlock icon or altering badge padding) and clicks **[🚀 Bump v1.1.0 & Propagate]**:
1. The backend WAL engine checks all screen schemas located in `screens/draft/`.
2. It locates every component referencing archetype `FP_Motor_VFD`.
3. It updates the layout definition and binding map atomically while preserving individual screen coordinates and tag instance assignments.
4. The workbench reports an atomic propagation summary:
   `[ Propagation Complete: 14 instances updated across 3 screens in draft staging buffer ]`.

---

## 11. 3-Stage HMI Lifecycle Architecture: Edit View vs Preview Simulation vs Runtime View

To guarantee 100% operational safety on active industrial lines, the frontend strictly isolates staging buffers from production execution:

```
+───────────────────────────────────────────────────────────────────────────────────────────────+
|                                3-STAGE HMI LIFECYCLE PIPELINE                                 |
+───────────────────────────────┬───────────────────────────────┬───────────────────────────────+
| 1. EDIT VIEW                  | 2. PREVIEW MODE               | 3. RUNTIME VIEW               |
| (Drafting & Layout Staging)   | (Non-Actuating Simulation)    | (Production Live Committed)   |
+───────────────────────────────┼───────────────────────────────┼───────────────────────────────+
| Badge: Amber                  | Badge: Blue                   | Badge: Green                  |
| [✏️ EDIT MODE - UNCOMMITTED]  | [👁️ PREVIEW - NON-ACTUATING]   | [🟢 RUNTIME MODE - COMMITTED] |
| Storage: screens/draft/*.json | Storage: In-Memory Sandbox    | Storage: screens/active/*.json|
| Canvas is 100% editable       | Canvas is locked              | Canvas is locked              |
| Drag/drop, resize, reorder    | Full navigation & animations  | Full operational control      |
| Telemetry values stream live  | Live or simulated values      | Live telemetry streams 100ms  |
| Actuator buttons DISABLED     | Actuator outputs BLOCKED      | Actuator outputs LIVE         |
+───────────────────────────────┴───────────────────────────────┴───────────────────────────────+
```

### 11.1 Stage 1: Edit View (Draft / Staging Buffer)
- **Visual Badge**: Amber badge in Zone 1 Top HUD: `[✏️ EDIT MODE — UNCOMMITTED CHANGES]`.
- **Behavior**:
  * Engineers freely drag and drop pumps, valves, piping nodes, and widgets across the 16px grid.
  * Property Inspector in Zone 4 allows editing visual transforms, tag bindings, and color thresholds.
  * Live telemetry continues to stream to indicator values to assist visual calibration, but all actuation controls (Start/Stop/Write Setpoint) are disabled.
  * Every mutation autosaves cleanly to `screens/draft/<screen_id>.json` via the Write-Ahead Log.
  * Active production screens running in control room panels are completely unaffected.

### 11.2 Stage 2: Preview Mode (Staging Simulation / Runtime Preview)
- **Visual Badge**: Blue badge in Zone 1 Top HUD: `[👁️ PREVIEW SIMULATION — NON-ACTUATING]`.
- **Behavior**:
  * Simulates exactly how the operator will experience the screen without deploying to production.
  * Screen-to-screen navigation buttons, subscreen popups, and drawer expansions execute realistically.
  * Widget animations (e.g. spinning agitators, color transitions on simulated sensor trips) are active.
  * **Strict Isolation Contract**: Actuator write commands are intercepted by the client sandbox. Zero hardware register write requests are dispatched to OpenPLC or fieldbus drivers.

### 11.3 Stage 3: Runtime View (Live Production Release)
- **Visual Badge**: Green badge in Zone 1 Top HUD: `[🟢 RUNTIME MODE — LIVE COMMITTED]`.
- **Behavior**:
  * Canvas graphics, component positions, and bindings are locked. Zero drafting controls appear.
  * Reflects committed schemas located in `screens/active/<screen_id>.json`.
  * Operator interactions (clicking Start, ramping Setpoint sliders, acknowledging alarms) send commands directly to the backend runtime.
  * Listens to SSE event `HMI_LAYOUT_UPDATED` to apply new committed layouts instantly across all active panels without requiring page refresh.

### 11.4 Mode Switcher Toolbar & Staging Actions
The workbench status bar and top navigation deck feature dedicated staging controls:

```
[ ✏️ Edit Mode ]  [ 👁️ Preview Simulation ]  [ 💾 Commit & Deploy to Runtime ]  [ ↺ Discard Draft ]
```

- **Commit Action (`[💾 Commit & Deploy to Runtime]`)**:
  1. Triggers pre-flight validation (verifies zero unbound UDT properties, validates grid bounds).
  2. Opens `CommitDiffModal.tsx` showing visual diff summary:
     `"Screen_02_BoilerSystem: 2 widgets added, 1 tag rebinding. FP_Motor_VFD bumped to v1.1.0."`
  3. Upon operator confirmation, atomic promotion promotes `screens/draft/` to `screens/active/`.
  4. Backend broadcasts `HMI_LAYOUT_UPDATED` via SSE.
- **Discard Action (`[↺ Discard Changes]`)**:
  1. Prompts for confirmation: `"Discard all uncommitted draft changes and revert to active runtime layout?"`.
  2. Overwrites `screens/draft/` with current `screens/active/` content.
  3. Clears dirty flags across `useProjectStore` and `useHmiStore`.

---

## 12. Immersive Industrial Runtime Fullscreen (WinCC Kiosk Paradigm)

In accordance with Adrian's mandate, when an operator enters Runtime Mode, the workbench shifts from an engineering IDE into an **Immersive Fullscreen Industrial Kiosk** identical to Siemens WinCC and AVEVA InTouch.

```
+───────────────────────────────────────────────────────────────────────────────────────────────+
| [🔴 2 CRITICAL ALARMS] Boiler 02 High Pressure Trip (48.2 bar)           [ 14:32:05 | OP-01 ] |
+───────────────────────────────────────────────────────────────────────────────────────────────+
|                                                                               [ 🚪 Exit ]     |
|                                                                                               |
|                       100% IMMERSIVE INDUSTRIAL PLANT GRAPHIC                                 |
|                                                                                               |
|           (P&ID Canvas / Unit Overview / Interactive Mimic Controls)                          |
|                                                                                               |
|                                                                                               |
|   - Zero IDE chrome (No Menu bar, No 48px Activity bar, No Explorer tree, No Console)         |
|   - Bounded Canvas scales to 100vw x 100vh touch target                                       |
|   - Docked 384px Sidebar drawer slides over upon faceplate selection                          |
|                                                                                               |
+───────────────────────────────────────────────────────────────────────────────────────────────+
| [ACK ALL] [SILENCE HORN] [SCREEN 01: OVERVIEW] [SCREEN 02: BOILER]           CYCLE TIME: 410µs|
+───────────────────────────────────────────────────────────────────────────────────────────────+
```

### 12.1 Kiosk Fullscreen Presentation Invariants
1. **Fullscreen Expansion**: Clicking `[▶️ Run / Enter Runtime]` calls `document.documentElement.requestFullscreen()`.
2. **IDE Chrome Suppression**:
   - Zone 1 Top Menu Bar is completely hidden.
   - Zone 1 Activity Bar (48px) is completely hidden.
   - Zone 2 Primary Project Explorer is completely hidden.
   - Zone 5 Bottom Console and compiler output drawers are completely hidden.
3. **Dedicated Operational Shell**:
   - **Top Operational Alarm Banner (32px)**: High-priority ISA-18.2 active alarm ticker with direct acknowledge buttons, node health indicator, and timestamp.
   - **Central Plant Canvas (100% Flex)**: Uncluttered SCADA graphics reflecting committed screens.
   - **Contextual Slide-Over Drawer (384px)**: When an operator taps a pump or valve, the standardized 384px Faceplate drawer slides over from the right edge with high-density actuation controls.
   - **Bottom Navigation Ribbon (36px)**: High-contrast touch buttons allowing operators to flip between primary screens (`[Screen 01]`, `[Screen 02]`) and execute plant emergency actions.

### 12.2 Secure IEC 62443 Operator Exit Handshake
Field operators on touch kiosks must not have unauthorized access to drop into the Monaco ST logic editor or alter network parameters.

1. **Discrete Exit Triggers**:
   - Top-right security lock icon: `[ 🚪 Exit Runtime ]`.
   - Keyboard hotkey combinations: `Ctrl + Alt + Q` or continuous 2-second hold of `Escape`.
2. **Credential Challenge Dialog (`ExitRuntimeDialog.tsx`)**:
   - Invokes an authentication modal requiring PIN or Smart Card credentials.
   - Must evaluate role `Engineer` or `Admin` via `/api/v1/auth/verify-role`.
3. **Safe Workspace Restoration**:
   - Upon successful verification: calls `document.exitFullscreen()`.
   - Re-mounts the 5-zone Dockview engineering workbench shell (`WorkbenchShell.tsx`).
   - Restores editor tabs, file tree positions, and console histories with zero state corruption.

### 12.3 Runtime Kiosk Shell Component (`RuntimeKioskShell.tsx`)

```typescript
// frontend/src/features/runtime-kiosk/RuntimeKioskShell.tsx
import React, { useState } from 'react';
import { useHmiStore } from '../../stores/useHmiStore';
import { useTagStore } from '../../stores/useTagStore';
import { KioskAlarmBanner } from './KioskAlarmBanner';
import { ExitRuntimeDialog } from './ExitRuntimeDialog';
import { ScreenCanvasHost } from '../hmi-screens/ScreenCanvasHost';

export const RuntimeKioskShell: React.FC = () => {
  const activeScreenId = useHmiStore((s) => s.activeScreenId) || 'root';
  const activeScreens = useHmiStore((s) => s.activeScreens);
  const screens = useHmiStore((s) => s.screens);
  const openScreenTab = useHmiStore((s) => s.openScreenTab);
  const requestExitRuntime = useHmiStore((s) => s.requestExitRuntime);

  const [showExitDialog, setShowExitDialog] = useState(false);

  return (
    <div className="flex flex-col w-screen h-screen bg-neutral-950 text-neutral-100 overflow-hidden select-none">
      {/* Top ISA-18.2 Operational Alarm Ticker Banner (32px) */}
      <KioskAlarmBanner onTriggerExit={() => setShowExitDialog(true)} />

      {/* Main Touch Canvas Viewport (100% Flex) */}
      <div className="flex-1 relative w-full h-full overflow-hidden bg-neutral-900">
        <ScreenCanvasHost
          screenId={activeScreenId}
          canvasData={activeScreens.get(activeScreenId)}
          readOnly={true}
          isLiveActuation={true}
        />
      </div>

      {/* Bottom Screen Navigation Ribbon (36px) */}
      <div className="flex items-center justify-between px-4 h-9 bg-neutral-950 border-t border-neutral-800">
        <div className="flex items-center space-x-2">
          {screens.map((sc) => (
            <button
              key={sc.id}
              onClick={() => openScreenTab(sc.id)}
              className={`px-3 py-1 text-xs font-semibold rounded transition-colors ${
                activeScreenId === sc.id
                  ? 'bg-blue-600 text-white'
                  : 'bg-neutral-800 text-neutral-300 hover:bg-neutral-700'
              }`}
            >
              {sc.name}
            </button>
          ))}
        </div>

        <div className="flex items-center space-x-3">
          <span className="text-[11px] font-mono text-emerald-400">● 100ms TELEMETRY STREAMING</span>
          <button
            onClick={() => setShowExitDialog(true)}
            className="px-2.5 py-1 text-xs font-semibold text-neutral-300 bg-neutral-800 hover:bg-neutral-700 rounded border border-neutral-700 flex items-center space-x-1"
          >
            <span>🚪</span>
            <span>Exit Kiosk</span>
          </button>
        </div>
      </div>

      {/* IEC 62443 Role Challenge Dialog */}
      {showExitDialog && (
        <ExitRuntimeDialog
          onCancel={() => setShowExitDialog(false)}
          onConfirm={async (creds) => {
            const success = await requestExitRuntime(creds);
            if (success) setShowExitDialog(false);
          }}
        />
      )}
    </div>
  );
};
```
