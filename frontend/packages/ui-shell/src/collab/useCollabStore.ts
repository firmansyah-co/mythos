import { create } from 'zustand';
import * as Y from 'yjs';
import { WebsocketProvider } from 'y-websocket';

export interface CursorCoordinates {
  lineNumber: number;
  column: number;
}

export interface CollabUser {
  name: string;
  color: string;
  avatar?: string;
}

export interface CollabPeer {
  clientId: number;
  user: CollabUser;
  activeTab?: string;
  activeScreen?: string;
  cursor?: CursorCoordinates;
}

export interface CollabState {
  connected: boolean;
  roomId: string;
  serverUrl: string;
  currentUser: CollabUser;
  currentScreen: string;
  currentTab: string;
  peers: CollabPeer[];
  doc: Y.Doc | null;
  provider: WebsocketProvider | null;

  // Actions
  initializeCollab: (roomId?: string, serverUrl?: string) => void;
  disconnectCollab: () => void;
  setCurrentUser: (user: Partial<CollabUser>) => void;
  setCurrentScreen: (screenId: string) => void;
  setCurrentTab: (tabId: string) => void;
  updateCursor: (cursor?: CursorCoordinates) => void;
}

const PRESET_COLORS = [
  'var(--collab-blue)', // Blue
  'var(--collab-emerald)', // Emerald
  'var(--collab-amber)', // Amber
  'var(--collab-red)', // Red
  'var(--collab-purple)', // Purple
  'var(--collab-pink)', // Pink
  'var(--collab-cyan)', // Cyan
  'var(--collab-teal)', // Teal
];

function getRandomColor(): string {
  const idx = Math.floor(Math.random() * PRESET_COLORS.length);
  return PRESET_COLORS[idx];
}

function getDefaultUsername(): string {
  const randNum = Math.floor(100 + Math.random() * 900);
  return `Engineer-${randNum}`;
}

export function getDefaultWsUrl(): string {
  if (typeof window !== 'undefined' && window.location) {
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = window.location.host;
    return `${protocol}//${host}/api/v1/collab/ws`;
  }
  return 'ws://localhost:8080/api/v1/collab/ws';
}

export const useCollabStore = create<CollabState>((set, get) => {
  const initialUser: CollabUser = {
    name: getDefaultUsername(),
    color: getRandomColor(),
  };

  return {
    connected: false,
    roomId: 'default-room',
    serverUrl: getDefaultWsUrl(),
    currentUser: initialUser,
    currentScreen: 'screen_01_plant_overview',
    currentTab: 'screen_01_plant_overview',
    peers: [],
    doc: null,
    provider: null,

    initializeCollab: (roomId = 'default-room', customServerUrl?: string) => {
      // Disconnect existing if any
      const existingProvider = get().provider;
      const existingDoc = get().doc;
      if (existingProvider) {
        existingProvider.destroy();
      }
      if (existingDoc) {
        existingDoc.destroy();
      }

      const serverUrl = customServerUrl || get().serverUrl || getDefaultWsUrl();
      const doc = new Y.Doc();
      const provider = new WebsocketProvider(serverUrl, roomId, doc, {
        connect: true,
      });

      const awareness = provider.awareness;

      // Set initial awareness state for local user
      awareness.setLocalStateField('user', get().currentUser);
      awareness.setLocalStateField('activeScreen', get().currentScreen);
      awareness.setLocalStateField('activeTab', get().currentTab);

      const updatePeersFromAwareness = () => {
        const states = awareness.getStates();
        const peerList: CollabPeer[] = [];

        states.forEach((state: any, clientId: number) => {
          if (state && state.user) {
            peerList.push({
              clientId,
              user: state.user,
              activeTab: state.activeTab,
              activeScreen: state.activeScreen,
              cursor: state.cursor,
            });
          }
        });

        set({ peers: peerList });
      };

      awareness.on('change', updatePeersFromAwareness);

      provider.on('status', (event: { status: 'connected' | 'disconnected' | 'connecting' }) => {
        set({ connected: event.status === 'connected' });
      });

      // provider.on('connection-close', ...)

      set({
        connected: false,
        roomId,
        serverUrl,
        doc,
        provider,
      });

      // Call once initially
      updatePeersFromAwareness();
    },

    disconnectCollab: () => {
      const { provider, doc } = get();
      if (provider) {
        provider.destroy();
      }
      if (doc) {
        doc.destroy();
      }
      set({
        connected: false,
        doc: null,
        provider: null,
        peers: [],
      });
    },

    setCurrentUser: (userUpdates: Partial<CollabUser>) => {
      const updatedUser = { ...get().currentUser, ...userUpdates };
      set({ currentUser: updatedUser });
      const provider = get().provider;
      if (provider && provider.awareness) {
        provider.awareness.setLocalStateField('user', updatedUser);
      }
    },

    setCurrentScreen: (screenId: string) => {
      set({ currentScreen: screenId });
      const provider = get().provider;
      if (provider && provider.awareness) {
        provider.awareness.setLocalStateField('activeScreen', screenId);
      }
    },

    setCurrentTab: (tabId: string) => {
      set({ currentTab: tabId });
      const provider = get().provider;
      if (provider && provider.awareness) {
        provider.awareness.setLocalStateField('activeTab', tabId);
      }
    },

    updateCursor: (cursor?: CursorCoordinates) => {
      const provider = get().provider;
      if (provider && provider.awareness) {
        provider.awareness.setLocalStateField('cursor', cursor);
      }
    },
  };
});
