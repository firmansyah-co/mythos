import React, { useState } from 'react';
import {
  BookOpen,
  Server,
  Library,
  Sliders,
  Activity,
  ChevronLeft,
  ChevronRight,
  Maximize2,
  Minimize2,
} from 'lucide-react';
import { useHmiStore } from '../../stores/useHmiStore';
import { InspectorDrawer } from '@mythos/ui-inspector';

export interface Zone4TaskCardsProps {
  className?: string;
  isCollapsed?: boolean;
  onToggleCollapse?: () => void;
}

export const Zone4TaskCards: React.FC<Zone4TaskCardsProps> = ({
  className = '',
  isCollapsed = false,
  onToggleCollapse,
}) => {
  const [activeTab, setActiveTab] = useState<'INSTRUCTIONS' | 'CATALOG' | 'LIBRARIES' | 'INSPECTOR'>(
    'INSPECTOR'
  );
  const [isFullDrawerMode, setIsFullDrawerMode] = useState<boolean>(false);

  const selectedContextItem = useHmiStore((s) => s.selectedContextItem);

  if (isCollapsed) {
    return (
      <div
        className={`w-9 bg-[var(--bg-surface)] border-l border-neutral-800 flex flex-col items-center py-2 select-none ${className}`}
      >
        <button
          onClick={onToggleCollapse}
          className="p-1 hover:bg-neutral-800 rounded text-neutral-400 hover:text-white mb-4"
          title="Expand Task Cards (384px)"
        >
          <ChevronLeft className="w-4 h-4" />
        </button>
        <div
          className="text-[11px] font-mono text-neutral-500 uppercase tracking-widest"
          style={{ writingMode: 'vertical-rl' }}
        >
          Task Cards &amp; Inspector
        </div>
      </div>
    );
  }

  // Dynamic context attributes
  const targetObjectName = selectedContextItem?.name || 'FP_Motor_VFD_01';
  const udtArchetype =
    selectedContextItem?.udtArchetype ||
    selectedContextItem?.subType ||
    (selectedContextItem?.type ? `${selectedContextItem.type}_UDT` : 'FB_Motor_UDT');
  const hardwareAddress =
    selectedContextItem?.hardwareAddress ||
    (selectedContextItem?.metadata?.hardwareAddress as string) ||
    '%QX0.0..%QX0.3';
  const category = selectedContextItem?.type || 'DEVICE';

  return (
    <div
      className={`w-96 flex flex-col bg-[var(--bg-surface)] border-l border-neutral-800 text-neutral-300 text-xs select-none ${className}`}
      data-testid="zone4-task-cards"
    >
      {/* Header & Tabs */}
      <div className="flex items-center justify-between px-2 bg-[var(--bg-base)] border-b border-neutral-800 h-9">
        <div className="flex items-center gap-1 overflow-x-auto">
          <button
            onClick={() => {
              setActiveTab('INSPECTOR');
            }}
            className={`px-2 py-1 rounded text-[11px] font-medium flex items-center gap-1 ${
              activeTab === 'INSPECTOR'
                ? 'bg-neutral-800 text-cyan-400'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Sliders className="w-3.5 h-3.5" />
            <span>Inspector</span>
          </button>

          <button
            onClick={() => setActiveTab('INSTRUCTIONS')}
            className={`px-2 py-1 rounded text-[11px] font-medium flex items-center gap-1 ${
              activeTab === 'INSTRUCTIONS'
                ? 'bg-neutral-800 text-cyan-400'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <BookOpen className="w-3.5 h-3.5" />
            <span>Instructions</span>
          </button>

          <button
            onClick={() => setActiveTab('CATALOG')}
            className={`px-2 py-1 rounded text-[11px] font-medium flex items-center gap-1 ${
              activeTab === 'CATALOG'
                ? 'bg-neutral-800 text-cyan-400'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Server className="w-3.5 h-3.5" />
            <span>Catalog</span>
          </button>

          <button
            onClick={() => setActiveTab('LIBRARIES')}
            className={`px-2 py-1 rounded text-[11px] font-medium flex items-center gap-1 ${
              activeTab === 'LIBRARIES'
                ? 'bg-neutral-800 text-cyan-400'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Library className="w-3.5 h-3.5" />
            <span>Libraries</span>
          </button>
        </div>

        <div className="flex items-center gap-1">
          {activeTab === 'INSPECTOR' && (
            <button
              onClick={() => setIsFullDrawerMode(!isFullDrawerMode)}
              className="p-1 hover:bg-neutral-800 rounded text-neutral-400 hover:text-white"
              title={isFullDrawerMode ? 'Switch to Compact View' : 'Switch to Full Diagnostic Drawer'}
            >
              {isFullDrawerMode ? <Minimize2 className="w-3.5 h-3.5 text-cyan-400" /> : <Maximize2 className="w-3.5 h-3.5" />}
            </button>
          )}
          <button
            onClick={onToggleCollapse}
            className="p-1 hover:bg-neutral-800 rounded text-neutral-400 hover:text-white"
            title="Collapse Panel"
          >
            <ChevronRight className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Tab Contents */}
      {activeTab === 'INSPECTOR' && isFullDrawerMode ? (
        /* Full Diagnostic Inspector Drawer integration */
        <div className="flex-1 w-full overflow-hidden flex flex-col">
          <InspectorDrawer
            isOpen={true}
            onClose={() => setIsFullDrawerMode(false)}
            deviceId={selectedContextItem?.id || targetObjectName}
            deviceName={targetObjectName}
            deviceType={udtArchetype}
            className="w-full border-none shadow-none"
          />
        </div>
      ) : (
        <div className="flex-1 p-3 overflow-y-auto space-y-3">
          {activeTab === 'INSPECTOR' && (
            <div className="space-y-4">
              {/* Dynamic Active Selection Context (TSK-605) */}
              <div className="p-2.5 rounded bg-neutral-900/80 border border-neutral-800 space-y-2">
                <span className="font-semibold text-neutral-200 text-xs flex items-center justify-between">
                  <span className="flex items-center gap-1.5">
                    <Sliders className="w-3.5 h-3.5 text-blue-400" />
                    Active Selection Context
                  </span>
                  <span
                    data-testid="inspector-item-category"
                    className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-neutral-800 text-cyan-400"
                  >
                    {category}
                  </span>
                </span>
                <div className="grid grid-cols-2 gap-2 text-[11px]">
                  <div className="text-neutral-500">Target Object:</div>
                  <div data-testid="inspector-target-object" className="font-mono text-cyan-400 truncate">
                    {targetObjectName}
                  </div>
                  <div className="text-neutral-500">UDT Archetype:</div>
                  <div data-testid="inspector-udt-archetype" className="font-mono text-neutral-300 truncate">
                    {udtArchetype}
                  </div>
                  <div className="text-neutral-500">Hardware Address:</div>
                  <div data-testid="inspector-hardware-address" className="font-mono text-emerald-400 truncate">
                    {hardwareAddress}
                  </div>
                </div>

                <div className="pt-2 border-t border-neutral-800/80 flex items-center justify-between">
                  <button
                    type="button"
                    onClick={() => setIsFullDrawerMode(true)}
                    className="w-full py-1 px-2 rounded bg-blue-950/60 hover:bg-blue-900/80 border border-blue-800/80 text-sky-400 text-[11px] font-medium flex items-center justify-center gap-1.5 transition-colors cursor-pointer"
                  >
                    <Sliders className="w-3 h-3" />
                    <span>Open Diagnostics &amp; Controls</span>
                  </button>
                </div>
              </div>

              {/* Interlock Matrix Checklist (TSK-306) */}
              <div className="p-2.5 rounded bg-neutral-900/80 border border-neutral-800 space-y-2">
                <span className="font-semibold text-neutral-200 text-xs flex items-center gap-1.5">
                  <Activity className="w-3.5 h-3.5 text-neutral-400" />
                  Interlock Matrix Checklist
                </span>
                <p className="text-[11px] text-neutral-400 leading-relaxed">
                  No interlock telemetry is available for this selection. Interlock states are read from the
                  controller process image and nothing is bound here yet.
                </p>
              </div>
            </div>
          )}

          {activeTab === 'INSTRUCTIONS' && (
            <div className="space-y-2 text-xs">
              <span className="font-semibold text-neutral-400 text-[11px] uppercase tracking-wider">
                Standard IEC 61131-3 Instructions
              </span>
              <div className="space-y-1">
                <div className="p-2 rounded bg-neutral-900 border border-neutral-800">
                  <div className="font-semibold text-cyan-400 font-mono">TON / TOF / TP</div>
                  <div className="text-[11px] text-neutral-400">On-Delay, Off-Delay and Pulse Timers</div>
                </div>
                <div className="p-2 rounded bg-neutral-900 border border-neutral-800">
                  <div className="font-semibold text-cyan-400 font-mono">CTU / CTD / CTUD</div>
                  <div className="text-[11px] text-neutral-400">Up, Down, and Bidirectional Counters</div>
                </div>
                <div className="p-2 rounded bg-neutral-900 border border-neutral-800">
                  <div className="font-semibold text-cyan-400 font-mono">PID_Compact</div>
                  <div className="text-[11px] text-neutral-400">Self-tuning continuous industrial controller</div>
                </div>
              </div>
            </div>
          )}

          {activeTab === 'CATALOG' && (
            <div className="space-y-2 text-xs">
              <span className="font-semibold text-neutral-400 text-[11px] uppercase tracking-wider">
                Hardware Module Catalog
              </span>
              <div className="space-y-1 text-neutral-300">
                <div className="p-2 rounded bg-neutral-900 border border-neutral-800">
                  <div className="font-semibold text-emerald-400">MythOS Microkernel Soft-PLC</div>
                  <div className="text-[11px] text-neutral-400">Native POSIX Real-Time STruC++ Runtime</div>
                </div>
                <div className="p-2 rounded bg-neutral-900 border border-neutral-800">
                  <div className="font-semibold text-emerald-400">ET 200SP / PROFINET IO Interface</div>
                  <div className="text-[11px] text-neutral-400">IM 155-6 PN BA / 16x DI / 16x DQ</div>
                </div>
              </div>
            </div>
          )}

          {activeTab === 'LIBRARIES' && (
            <div className="space-y-2 text-xs">
              <span className="font-semibold text-neutral-400 text-[11px] uppercase tracking-wider">
                Master Faceplate Types
              </span>
              <div className="p-2 rounded bg-neutral-900 border border-neutral-800 space-y-1">
                <div className="font-mono text-cyan-400">FP_Motor_VFD (v1.0.0)</div>
                <div className="text-[11px] text-neutral-400">4 Archetype Views: P&amp;ID, Card, Popup, Alarm</div>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
