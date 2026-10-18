import React, { useRef, useCallback } from 'react';
import {
  DockviewReact,
  DockviewReadyEvent,
  IDockviewPanelProps,
} from 'dockview-react';
import 'dockview-react/dist/styles/dockview.css';

import { LogicEditorPlugin } from '@mythos/ui-logic-editor';
import { useHmiStore } from '../../stores/useHmiStore';

// Panel 1: Monaco Logic Editor tab
const MonacoLogicPanel: React.FC<IDockviewPanelProps<{ targetBlock?: string }>> = (props) => {
  const targetBlock = props.params?.targetBlock || 'MainTask';
  return (
    <div className="w-full h-full">
      <LogicEditorPlugin targetBlock={targetBlock} />
    </div>
  );
};

// Panel 2: HMI Screen Canvas tab
const HmiScreenCanvasPanel: React.FC<IDockviewPanelProps<{ screenId: string; title: string }>> = (props) => {
  const screenId = props.params?.screenId || 'screen_01_plant_overview';
  const title = props.params?.title || 'Screen Canvas';
  const lifecycleMode = useHmiStore((s) => s.lifecycleMode);

  return (
    <div className="w-full h-full flex flex-col bg-[#0A0D12] text-neutral-200 select-none overflow-hidden">
      {/* Canvas Top Bar */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-[#0F141C] border-b border-neutral-800 text-xs">
        <div className="flex items-center gap-2">
          <span className="font-semibold text-neutral-300">{title}</span>
          <span className="text-[10px] font-mono text-neutral-500">[{screenId}]</span>
        </div>
        <div className="text-[11px] font-mono text-neutral-400">
          RESOLUTION: 1920x1080 (FHD) • GRID: 16px SNAP
        </div>
      </div>

      {/* Industrial Canvas Area */}
      <div
        className="flex-1 w-full relative overflow-auto p-8 flex items-center justify-center"
        style={{
          backgroundImage:
            'radial-gradient(circle, rgba(255,255,255,0.06) 1px, transparent 1px)',
          backgroundSize: '16px 16px',
        }}
      >
        <div className="w-[800px] h-[450px] bg-[#0F141C] border border-neutral-800 rounded-lg shadow-2xl relative p-6 flex flex-col justify-between">
          <div className="flex items-center justify-between border-b border-neutral-800 pb-3">
            <div>
              <h2 className="text-sm font-bold text-white tracking-wide">PLANT PROCESS OVERVIEW</h2>
              <p className="text-xs text-neutral-400">Bottling & Conveyor Subsystem 01</p>
            </div>
            <span
              className={`px-2 py-0.5 rounded text-[10px] font-mono font-semibold ${
                lifecycleMode === 'EDIT'
                  ? 'bg-amber-950/60 text-amber-400 border border-amber-800'
                  : lifecycleMode === 'PREVIEW'
                  ? 'bg-sky-950/60 text-sky-400 border border-sky-800'
                  : 'bg-emerald-950/60 text-emerald-400 border border-emerald-800'
              }`}
            >
              MODE: {lifecycleMode}
            </span>
          </div>

          {/* Plant Graphic Mimic Elements */}
          <div className="grid grid-cols-3 gap-4 my-auto">
            {/* Element 1: Infeed Conveyor */}
            <div className="p-3 bg-neutral-900 border border-neutral-800 rounded-md">
              <span className="text-[10px] font-mono text-neutral-500 block mb-1">MTR-101</span>
              <div className="text-xs font-semibold text-white">Infeed Conveyor</div>
              <div className="flex items-center justify-between mt-2">
                <span className="text-[11px] text-emerald-400 font-mono">1,450 RPM</span>
                <span className="w-2.5 h-2.5 rounded-full bg-emerald-500 animate-pulse"></span>
              </div>
            </div>

            {/* Element 2: Capping Unit */}
            <div className="p-3 bg-neutral-900 border border-neutral-800 rounded-md">
              <span className="text-[10px] font-mono text-neutral-500 block mb-1">CAP-201</span>
              <div className="text-xs font-semibold text-white">Capping Unit</div>
              <div className="flex items-center justify-between mt-2">
                <span className="text-[11px] text-cyan-400 font-mono">CYCLE 120/m</span>
                <span className="w-2.5 h-2.5 rounded-full bg-cyan-500"></span>
              </div>
            </div>

            {/* Element 3: Outfeed Gate */}
            <div className="p-3 bg-neutral-900 border border-neutral-800 rounded-md">
              <span className="text-[10px] font-mono text-neutral-500 block mb-1">VLV-301</span>
              <div className="text-xs font-semibold text-white">Diverter Valve</div>
              <div className="flex items-center justify-between mt-2">
                <span className="text-[11px] text-neutral-300 font-mono">OPEN (100%)</span>
                <span className="w-2.5 h-2.5 rounded-full bg-blue-500"></span>
              </div>
            </div>
          </div>

          <div className="flex items-center justify-between text-[11px] text-neutral-500 pt-3 border-t border-neutral-800">
            <span>TagBus Status: <strong className="text-emerald-400 font-mono">ACTIVE (100ms)</strong></span>
            <span>Uncommitted Draft WAL: <strong className="text-amber-400 font-mono">0 deltas</strong></span>
          </div>
        </div>
      </div>
    </div>
  );
};

// Panel 3: Network Topology Canvas tab
const NetworkTopologyPanel: React.FC<IDockviewPanelProps<{ title: string }>> = (props) => {
  return (
    <div className="w-full h-full bg-[#0A0D12] text-neutral-200 p-6 flex flex-col justify-between">
      <div className="border-b border-neutral-800 pb-3">
        <h3 className="text-xs font-bold text-white tracking-wide uppercase">
          {props.params?.title || 'PROFINET / Fieldbus Network Topology'}
        </h3>
        <p className="text-[11px] text-neutral-500">
          Guided Protocol Discovery & Dual-Role DCP Neighborhood
        </p>
      </div>

      <div className="flex items-center justify-center my-auto">
        <div className="flex items-center gap-8">
          <div className="px-4 py-3 bg-neutral-900 border border-cyan-500 rounded-lg text-center shadow-lg shadow-cyan-950/40">
            <span className="text-[10px] font-mono text-cyan-400 block">CONTROLLER</span>
            <span className="text-xs font-bold text-white">MythOS Soft-PLC</span>
            <span className="text-[10px] text-neutral-400 block mt-1">192.168.1.10</span>
          </div>

          <div className="h-0.5 w-16 bg-emerald-500 relative">
            <span className="absolute -top-4 left-2 text-[9px] font-mono text-emerald-400">PROFINET</span>
          </div>

          <div className="px-4 py-3 bg-neutral-900 border border-emerald-500/80 rounded-lg text-center shadow-lg">
            <span className="text-[10px] font-mono text-emerald-400 block">REMOTE I/O</span>
            <span className="text-xs font-bold text-white">ET 200SP Rack 0</span>
            <span className="text-[10px] text-neutral-400 block mt-1">192.168.1.20</span>
          </div>
        </div>
      </div>

      <div className="text-[11px] text-neutral-500 font-mono">
        LLDP Port Link Status: Port 1 & 2 Linked • Line Redundancy (MRP) Enabled
      </div>
    </div>
  );
};

export interface DockviewWorkspaceProps {
  className?: string;
}

export const DockviewWorkspace: React.FC<DockviewWorkspaceProps> = ({ className = '' }) => {
  const dockviewApiRef = useRef<any>(null);

  const onReady = useCallback((event: DockviewReadyEvent) => {
    dockviewApiRef.current = event.api;

    // Add default initial tabs in Zone 3
    event.api.addPanel({
      id: 'panel_monaco_st',
      component: 'monacoLogic',
      title: 'Main [OB1] (Monaco ST)',
      params: { targetBlock: 'MainTask' },
    });

    event.api.addPanel({
      id: 'panel_hmi_screen',
      component: 'hmiScreenCanvas',
      title: 'Screen_01_PlantOverview',
      params: {
        screenId: 'screen_01_plant_overview',
        title: 'Screen 01: Plant Overview',
      },
      position: { referencePanel: 'panel_monaco_st', direction: 'within' },
    });
  }, []);

  return (
    <div
      className={`w-full h-full bg-[#0A0D12] overflow-hidden ${className}`}
      data-testid="dockview-workspace-host"
    >
      <DockviewReact
        components={{
          monacoLogic: MonacoLogicPanel,
          hmiScreenCanvas: HmiScreenCanvasPanel,
          networkTopology: NetworkTopologyPanel,
        }}
        onReady={onReady}
        className="dockview-theme-dark h-full w-full"
      />
    </div>
  );
};
