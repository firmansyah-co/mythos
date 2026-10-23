import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { useCollabStore } from '../collab/useCollabStore';
import * as Y from 'yjs';

describe('useCollabStore (TSK-311 CRDT Collaboration)', () => {
  beforeEach(() => {
    // Reset store state
    useCollabStore.getState().disconnectCollab();
    useCollabStore.setState({
      connected: false,
      roomId: 'test-room',
      peers: [],
      currentScreen: 'screen_01_plant_overview',
      currentTab: 'screen_01_plant_overview',
    });
  });

  afterEach(() => {
    useCollabStore.getState().disconnectCollab();
  });

  it('initializes with default user and disconnected state', () => {
    const state = useCollabStore.getState();
    expect(state.connected).toBe(false);
    expect(state.currentUser).toBeDefined();
    expect(state.currentUser.name).toMatch(/^Engineer-\d+/);
    expect(state.currentUser.color).toMatch(/^var\(--collab-[a-z]+\)$/);
    expect(state.peers).toEqual([]);
    expect(state.doc).toBeNull();
    expect(state.provider).toBeNull();
  });

  it('updates local user details and updates awareness if active', () => {
    const { setCurrentUser } = useCollabStore.getState();

    setCurrentUser({ name: 'Neo-Software', color: '#10B981' });
    const updated = useCollabStore.getState().currentUser;
    expect(updated.name).toBe('Neo-Software');
    expect(updated.color).toBe('#10B981');
  });

  it('updates current screen, tab, and cursor', () => {
    const { setCurrentScreen, setCurrentTab, updateCursor } = useCollabStore.getState();

    setCurrentScreen('screen_02_fermentation');
    expect(useCollabStore.getState().currentScreen).toBe('screen_02_fermentation');

    setCurrentTab('tab_logic_main');
    expect(useCollabStore.getState().currentTab).toBe('tab_logic_main');

    // Should not throw when provider is not yet initialized
    expect(() => updateCursor({ lineNumber: 10, column: 5 })).not.toThrow();
  });

  it('initializes provider, doc, and awareness tracking', () => {
    const { initializeCollab } = useCollabStore.getState();

    initializeCollab('room-plc-alpha', 'ws://localhost:9999/api/v1/collab/ws');

    const state = useCollabStore.getState();
    expect(state.roomId).toBe('room-plc-alpha');
    expect(state.serverUrl).toBe('ws://localhost:9999/api/v1/collab/ws');
    expect(state.doc).toBeInstanceOf(Y.Doc);
    expect(state.provider).toBeTruthy();

    const provider = state.provider!;
    const awareness = provider.awareness;

    // Check local awareness fields were seeded
    const localState = awareness.getLocalState();
    expect(localState?.user?.name).toBe(state.currentUser.name);
    expect(localState?.activeScreen).toBe(state.currentScreen);
    expect(localState?.activeTab).toBe(state.currentTab);

    // Simulate awareness update with a remote peer
    const mockPeerState = {
      user: { name: 'Iris-Design', color: '#EC4899' },
      activeScreen: 'screen_01_plant_overview',
      activeTab: 'tab_hmi_canvas',
      cursor: { lineNumber: 42, column: 12 },
    };

    awareness.setLocalState(mockPeerState);

    // State peers should reflect the updated awareness
    const peersAfter = useCollabStore.getState().peers;
    expect(peersAfter.length).toBeGreaterThanOrEqual(1);
    const peer = peersAfter.find((p) => p.user?.name === 'Iris-Design');
    expect(peer).toBeDefined();
    expect(peer?.user.color).toBe('#EC4899');
    expect(peer?.activeTab).toBe('tab_hmi_canvas');
    expect(peer?.cursor).toEqual({ lineNumber: 42, column: 12 });
  });

  it('handles provider status connection events', () => {
    const { initializeCollab } = useCollabStore.getState();
    initializeCollab('status-test-room', 'ws://localhost:9999/api/v1/collab/ws');

    const provider = useCollabStore.getState().provider!;

    // Emit connected status
    provider.emit('status', [{ status: 'connected' }]);
    expect(useCollabStore.getState().connected).toBe(true);

    // Emit disconnected status
    provider.emit('status', [{ status: 'disconnected' }]);
    expect(useCollabStore.getState().connected).toBe(false);
  });

  it('disconnectCollab properly cleans up resources', () => {
    const { initializeCollab, disconnectCollab } = useCollabStore.getState();
    initializeCollab('cleanup-room', 'ws://localhost:9999/api/v1/collab/ws');

    expect(useCollabStore.getState().doc).not.toBeNull();
    expect(useCollabStore.getState().provider).not.toBeNull();

    disconnectCollab();

    const state = useCollabStore.getState();
    expect(state.connected).toBe(false);
    expect(state.doc).toBeNull();
    expect(state.provider).toBeNull();
    expect(state.peers).toEqual([]);
  });
});
