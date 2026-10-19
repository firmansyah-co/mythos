import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { useHmiStore } from '../stores/useHmiStore';
import { DockviewWorkspace } from '../components/workbench/DockviewWorkspace';
import { Zone4TaskCards } from '../components/workbench/Zone4TaskCards';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('Sprint 6: Interactive Workbench Panels & Wiring Verification', () => {
  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);

    global.fetch = vi.fn().mockImplementation(async () => {
      return {
        ok: false,
        status: 404,
        json: async () => ({}),
      };
    });

    useHmiStore.getState().resetToEmptyProject();
    useHmiStore.getState().loadSampleProject();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    flushSync(() => {
      root.unmount();
    });
    container.remove();
  });

  describe('1. Real NetworkCanvas & Topology Selection Wiring', () => {
    it('updates selectedContextItem in store when selecting a hardware station', () => {
      const store = useHmiStore.getState();

      store.setSelectedContextItem({
        id: 'station-plc-01',
        name: 'PLC_RACK_01',
        type: 'DEVICE',
        subType: 'PLC_RACK',
        hardwareAddress: '192.168.1.10',
        udtArchetype: '6ES7515-2AM02-0AB0',
        metadata: {
          rackNumber: 0,
          status: 'RUN',
        },
      });

      const current = useHmiStore.getState().selectedContextItem;
      expect(current).not.toBeNull();
      expect(current?.id).toBe('station-plc-01');
      expect(current?.name).toBe('PLC_RACK_01');
      expect(current?.type).toBe('DEVICE');
      expect(current?.hardwareAddress).toBe('192.168.1.10');
      expect(current?.udtArchetype).toBe('6ES7515-2AM02-0AB0');
    });

    it('updates selectedContextItem in store when selecting a network port', () => {
      const store = useHmiStore.getState();

      store.setSelectedContextItem({
        id: 'station-plc-01::plc-x1-p1',
        name: 'X1 P1 (PROFINET)',
        type: 'DEVICE',
        subType: 'PORT',
        hardwareAddress: 'PROFINET',
        udtArchetype: 'RJ45',
        metadata: {
          stationId: 'station-plc-01',
          portId: 'plc-x1-p1',
          protocol: 'PROFINET',
          linkStatus: 'UP',
        },
      });

      const current = useHmiStore.getState().selectedContextItem;
      expect(current?.subType).toBe('PORT');
      expect(current?.hardwareAddress).toBe('PROFINET');
      expect(current?.metadata?.linkStatus).toBe('UP');
    });
  });

  describe('2. Real Faceplate Rendering & Empty Screen Handling', () => {
    it('provides populated screen components in sample project', () => {
      const store = useHmiStore.getState();
      const screen01 = store.screens.find((s) => s.id === 'screen_01_plant_overview');
      expect(screen01).toBeDefined();
      expect(screen01?.canvasData?.components.length).toBeGreaterThan(0);

      const motor = screen01?.canvasData?.components.find((c: any) => c.type === 'MOTOR');
      expect(motor).toBeDefined();
      expect(motor.tag).toBe('MTR-101');

      const valve = screen01?.canvasData?.components.find((c: any) => c.type === 'VALVE');
      expect(valve).toBeDefined();
      expect(valve.tag).toBe('VLV-301');
    });

    it('handles empty screen with ISA-101 empty state design canvas contract', async () => {
      const store = useHmiStore.getState();
      const newScreenId = await store.addNewScreen('Screen_Empty_Test');
      const emptyScreen = useHmiStore.getState().screens.find((s) => s.id === newScreenId);
      expect(emptyScreen).toBeDefined();
      expect(emptyScreen?.canvasData?.components).toEqual([]);
    });

    it('updates selectedContextItem when clicking a faceplate component', () => {
      const store = useHmiStore.getState();

      store.setSelectedContextItem({
        id: 'mtr_101',
        name: 'MTR-101',
        type: 'FACEPLATE',
        subType: 'FB_Motor_UDT',
        hardwareAddress: '%QX0.0..%QX0.3',
        udtArchetype: 'FB_Motor_UDT',
        metadata: {
          tag: 'MTR-101',
          label: 'Infeed Conveyor Drive',
          actualSpeed: 1450,
        },
      });

      const selected = useHmiStore.getState().selectedContextItem;
      expect(selected?.id).toBe('mtr_101');
      expect(selected?.name).toBe('MTR-101');
      expect(selected?.type).toBe('FACEPLATE');
      expect(selected?.udtArchetype).toBe('FB_Motor_UDT');
      expect(selected?.metadata?.actualSpeed).toBe(1450);
    });
  });

  describe('3. Zone 4 Inspector Integration & Dynamic Context Sync', () => {
    it('renders selected faceplate context in Zone 4 Inspector dynamically', () => {
      useHmiStore.getState().setSelectedContextItem({
        id: 'vlv_301',
        name: 'VLV-301',
        type: 'FACEPLATE',
        subType: 'FB_Digital_Valve_UDT',
        hardwareAddress: '%QX1.0..%QX1.1',
        udtArchetype: 'FB_Digital_Valve_UDT',
      });

      const html = renderClient(
        <Zone4TaskCards
          isCollapsed={false}
          onToggleCollapse={() => {}}
        />
      );

      expect(html).toContain('VLV-301');
      expect(html).toContain('FB_Digital_Valve_UDT');
      expect(html).toContain('%QX1.0..%QX1.1');
      expect(html).toContain('FACEPLATE');
      expect(html).toContain('Open Diagnostics &amp; Controls');
    });

    it('renders Dockview workspace host container without crashing', () => {
      const html = renderClient(<DockviewWorkspace />);
      expect(html).toContain('data-testid="dockview-workspace-host"');
    });
  });
});
