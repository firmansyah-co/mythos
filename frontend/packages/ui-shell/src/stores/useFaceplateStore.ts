import { create } from 'zustand';
import {
  createFaceplateInstance as apiCreateInstance,
  createFaceplateType as apiCreateType,
  getFaceplateType,
  listFaceplateInstances,
  listFaceplateTypes,
  releaseFaceplateType,
  saveFaceplateTypeDraft,
  updateFaceplateInstance,
} from '../api/faceplates';
import {
  FaceplateInstance,
  FaceplateReleaseResult,
  FaceplateType,
  FaceplateTypeSummary,
  ResolvedInstance,
  UdtDefinition,
} from '../types/faceplate';

export type FaceplateLoadStatus = 'IDLE' | 'LOADING' | 'READY' | 'ERROR';

export interface FaceplateStoreState {
  /** Project whose faceplate library is loaded; null when no project is open. */
  projectId: string | null;
  status: FaceplateLoadStatus;
  errorMessage: string | null;

  /** Tree summaries, read from the draft stage. */
  types: FaceplateTypeSummary[];
  /** Instances exactly as the daemon resolved them against their masters at read time. */
  instances: ResolvedInstance[];

  /** Full authored definition of the type open in the designer, when one is open. */
  openType: FaceplateType | null;
  openTypeStatus: FaceplateLoadStatus;
  openTypeError: string | null;

  /** The last release outcome, shown by the designer until dismissed. */
  lastRelease: FaceplateReleaseResult | null;

  load: (projectId: string) => Promise<boolean>;
  refresh: () => Promise<boolean>;
  clearError: () => void;
  reset: () => void;
  dismissRelease: () => void;
  closeType: () => void;

  loadType: (typeId: string) => Promise<FaceplateType | null>;
  createType: (input: { name: string; udt: UdtDefinition }) => Promise<FaceplateType | null>;
  saveDraft: (type: FaceplateType) => Promise<boolean>;
  release: (typeId: string) => Promise<FaceplateReleaseResult | null>;

  createInstance: (input: { name: string; typeId: string }) => Promise<FaceplateInstance | null>;
  setInstancePin: (instanceId: string, typeVersion: number | null) => Promise<boolean>;

  /** Re-read the library when the daemon announces a faceplate release on the telemetry stream. */
  watchPropagation: () => () => void;
}

/** Coalesce concurrent loads so the tree, Zone 4 and the designer do not race on mount. */
let inFlight: { projectId: string; promise: Promise<boolean> } | null = null;
/** One telemetry subscription for the whole shell. */
let propagation: EventSource | null = null;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

const EMPTY = {
  status: 'IDLE' as FaceplateLoadStatus,
  errorMessage: null,
  types: [] as FaceplateTypeSummary[],
  instances: [] as ResolvedInstance[],
  openType: null,
  openTypeStatus: 'IDLE' as FaceplateLoadStatus,
  openTypeError: null,
  lastRelease: null,
};

export const useFaceplateStore = create<FaceplateStoreState>((set, get) => ({
  projectId: null,
  ...EMPTY,

  load: async (projectId) => {
    if (!projectId) return false;
    if (inFlight && inFlight.projectId === projectId) return inFlight.promise;

    // A different project's library must not stay on screen while this one loads.
    if (get().projectId !== projectId) {
      set({
        projectId,
        status: 'LOADING',
        errorMessage: null,
        types: [],
        instances: [],
        openType: null,
        openTypeStatus: 'IDLE',
        openTypeError: null,
        lastRelease: null,
      });
    } else {
      set({ projectId, status: 'LOADING', errorMessage: null });
    }

    const promise = (async () => {
      try {
        const [types, instances] = await Promise.all([
          listFaceplateTypes(projectId),
          listFaceplateInstances(projectId),
        ]);
        // A newer load may have started for a different project while this was in flight.
        if (get().projectId !== projectId) return false;
        set({ status: 'READY', types, instances, errorMessage: null });
        return true;
      } catch (error) {
        if (get().projectId !== projectId) return false;
        // An unavailable library is reported as such; it is never rendered as an empty project.
        set({
          status: 'ERROR',
          types: [],
          instances: [],
          errorMessage: `Faceplate library unavailable: ${message(error)}`,
        });
        return false;
      } finally {
        inFlight = null;
      }
    })();

    inFlight = { projectId, promise };
    return promise;
  },

  refresh: async () => {
    const projectId = get().projectId;
    if (!projectId) return false;
    return get().load(projectId);
  },

  clearError: () => set({ errorMessage: null }),

  reset: () => {
    inFlight = null;
    stopPropagation();
    set({ projectId: null, ...EMPTY });
  },

  dismissRelease: () => set({ lastRelease: null }),

  closeType: () => set({ openType: null, openTypeStatus: 'IDLE', openTypeError: null }),

  loadType: async (typeId) => {
    const projectId = get().projectId;
    if (!projectId) return null;
    set({ openTypeStatus: 'LOADING', openTypeError: null });
    try {
      // The designer edits the authored (draft) definition, which is what the tree lists.
      const type = await getFaceplateType(projectId, typeId, 'draft');
      set({ openType: type, openTypeStatus: 'READY', openTypeError: null });
      return type;
    } catch (error) {
      set({
        openType: null,
        openTypeStatus: 'ERROR',
        openTypeError: `Faceplate type could not be read: ${message(error)}`,
      });
      return null;
    }
  },

  createType: async ({ name, udt }) => {
    const projectId = get().projectId;
    if (!projectId) {
      set({ errorMessage: 'Open a project before creating a faceplate type' });
      return null;
    }
    if (!name.trim()) {
      set({ errorMessage: 'Faceplate type name is required' });
      return null;
    }
    try {
      const created = await apiCreateType(projectId, { name: name.trim(), udt });
      await get().refresh();
      set({ openType: created, openTypeStatus: 'READY', openTypeError: null });
      return created;
    } catch (error) {
      set({ errorMessage: `Faceplate type was not created: ${message(error)}` });
      return null;
    }
  },

  saveDraft: async (type) => {
    const projectId = get().projectId;
    if (!projectId) {
      set({ errorMessage: 'Open a project before saving a faceplate type' });
      return false;
    }
    try {
      await saveFaceplateTypeDraft(projectId, type.id, type);
      // Re-read rather than assume: the daemon assigns ids and version bookkeeping.
      await get().loadType(type.id);
      await get().refresh();
      return true;
    } catch (error) {
      set({ errorMessage: `Faceplate type draft was not saved: ${message(error)}` });
      return false;
    }
  },

  release: async (typeId) => {
    const projectId = get().projectId;
    if (!projectId) {
      set({ errorMessage: 'Open a project before releasing a faceplate type' });
      return null;
    }
    try {
      const result = await releaseFaceplateType(projectId, typeId);
      set({ lastRelease: result });
      // Instances resolve against the master at read time, so the only honest post-release
      // state is a fresh read from the daemon.
      await get().refresh();
      await get().loadType(typeId);
      return result;
    } catch (error) {
      set({ errorMessage: `Faceplate release failed: ${message(error)}` });
      return null;
    }
  },

  createInstance: async ({ name, typeId }) => {
    const projectId = get().projectId;
    if (!projectId) {
      set({ errorMessage: 'Open a project before creating a faceplate instance' });
      return null;
    }
    if (!name.trim()) {
      set({ errorMessage: 'Faceplate instance name is required' });
      return null;
    }
    try {
      const created = await apiCreateInstance(projectId, {
        name: name.trim(),
        typeId,
        bindings: [],
      });
      await get().refresh();
      return created;
    } catch (error) {
      set({ errorMessage: `Faceplate instance was not created: ${message(error)}` });
      return null;
    }
  },

  setInstancePin: async (instanceId, typeVersion) => {
    const projectId = get().projectId;
    if (!projectId) return false;
    try {
      await updateFaceplateInstance(projectId, instanceId, { typeVersion });
      await get().refresh();
      return true;
    } catch (error) {
      set({
        errorMessage:
          typeVersion === null
            ? `Instance was not unfrozen: ${message(error)}`
            : `Instance was not frozen: ${message(error)}`,
      });
      return false;
    }
  },

  watchPropagation: () => {
    if (typeof EventSource === 'undefined') return () => {};
    if (propagation) return () => stopPropagation();

    propagation = new EventSource('/api/v1/telemetry/stream');
    // Instances resolve against the master at read time, so the honest reaction to a release is
    // a re-read, never a local version bump.
    propagation.addEventListener('FACEPLATE_PROPAGATED', () => {
      void get().refresh();
    });
    return () => stopPropagation();
  },
}));

function stopPropagation() {
  propagation?.close();
  propagation = null;
}