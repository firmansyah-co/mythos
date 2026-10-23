import { create } from 'zustand';
import {
  createFaceplateInstance as apiCreateInstance,
  createFaceplateType as apiCreateType,
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
  ResolvedFaceplateInstance,
  UdtDefinition,
} from '../types/faceplate';

export type FaceplateLoadStatus = 'IDLE' | 'LOADING' | 'READY' | 'ERROR';

export interface FaceplateStoreState {
  /** Project whose faceplate library is loaded; null when no project is open. */
  projectId: string | null;
  status: FaceplateLoadStatus;
  errorMessage: string | null;

  types: FaceplateType[];
  /** Instances exactly as the daemon resolved them against their masters at read time. */
  instances: ResolvedFaceplateInstance[];

  /** The last release outcome, shown by the designer until dismissed. */
  lastRelease: FaceplateReleaseResult | null;

  load: (projectId: string) => Promise<boolean>;
  refresh: () => Promise<boolean>;
  clearError: () => void;
  reset: () => void;
  dismissRelease: () => void;

  createType: (input: { name: string; udt: UdtDefinition }) => Promise<FaceplateType | null>;
  saveDraft: (type: FaceplateType) => Promise<boolean>;
  release: (typeId: string) => Promise<FaceplateReleaseResult | null>;

  createInstance: (input: { name: string; typeId: string }) => Promise<FaceplateInstance | null>;
  setInstancePin: (instanceId: string, typeVersion: number | null) => Promise<boolean>;

  /** UDT names already declared by a master type in this project (for the binding picker). */
  knownUdtNames: () => string[];

  /**
   * Re-read the library when the daemon announces a faceplate release on the existing
   * telemetry stream. Returns an unsubscribe function; a no-op when EventSource is absent.
   */
  watchPropagation: () => () => void;
}

/** One subscription per panel host; the designer is the only caller. */
let propagation: EventSource | null = null;

/** Coalesce concurrent loads so the tree and Zone 4 do not race on mount. */
let inFlight: { projectId: string; promise: Promise<boolean> } | null = null;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

const EMPTY = {
  status: 'IDLE' as FaceplateLoadStatus,
  errorMessage: null,
  types: [] as FaceplateType[],
  instances: [] as ResolvedFaceplateInstance[],
  lastRelease: null,
};

export const useFaceplateStore = create<FaceplateStoreState>((set, get) => ({
  projectId: null,
  ...EMPTY,

  load: async (projectId) => {
    if (!projectId) return false;
    if (inFlight && inFlight.projectId === projectId) return inFlight.promise;

    // Never show one project's library while another one loads.
    const switched = get().projectId !== projectId;
    set({
      projectId,
      status: 'LOADING',
      errorMessage: null,
      ...(switched ? { types: [], instances: [], lastRelease: null } : {}),
    });

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
    // Reuse the in-flight load if there is one; otherwise issue a real read. `load` short-circuits
    // on a matching in-flight request, so this never silently keeps stale rows.
    return get().load(projectId);
  },

  clearError: () => set({ errorMessage: null }),

  reset: () => {
    inFlight = null;
    set({ projectId: null, ...EMPTY });
  },

  dismissRelease: () => set({ lastRelease: null }),

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
      // Instances resolve against the master at read time, so the only honest
      // post-release state is a fresh read from the daemon.
      await get().refresh();
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
      const created = await apiCreateInstance(projectId, { name: name.trim(), typeId, bindings: [] });
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
        errorMessage: typeVersion === null
          ? `Instance was not unfrozen: ${message(error)}`
          : `Instance was not frozen: ${message(error)}`,
      });
      return false;
    }
  },

  knownUdtNames: () => {
    const names = get().types.map((type) => type.udt?.name).filter(Boolean);
    return Array.from(new Set(names));
  },

  watchPropagation: () => {
    if (typeof EventSource === 'undefined') return () => {};
    if (propagation) return () => stopPropagation();

    propagation = new EventSource('/api/v1/telemetry/stream');
    // Instances resolve against the master at read time, so the honest reaction to a release is a
    // re-read, never a local version bump.
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