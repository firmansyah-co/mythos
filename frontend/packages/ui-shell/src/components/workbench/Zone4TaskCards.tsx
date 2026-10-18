import React, { useState } from 'react';
import {
  BookOpen,
  Server,
  Library,
  Sliders,
  CheckCircle,
  XCircle,
  Activity,
  ChevronLeft,
  ChevronRight,
} from 'lucide-react';

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

  if (isCollapsed) {
    return (
      <div
        className={`w-9 bg-[#0F141C] border-l border-neutral-800 flex flex-col items-center py-2 select-none ${className}`}
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
          Task Cards & Inspector
        </div>
      </div>
    );
  }

  return (
    <div
      className={`w-96 flex flex-col bg-[#0F141C] border-l border-neutral-800 text-neutral-300 text-xs select-none ${className}`}
      data-testid="zone4-task-cards"
    >
      {/* Header & Tabs */}
      <div className="flex items-center justify-between px-2 bg-[#0A0D12] border-b border-neutral-800 h-9">
        <div className="flex items-center gap-1 overflow-x-auto">
          <button
            onClick={() => setActiveTab('INSPECTOR')}
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

        <button
          onClick={onToggleCollapse}
          className="p-1 hover:bg-neutral-800 rounded text-neutral-400 hover:text-white"
          title="Collapse Panel"
        >
          <ChevronRight className="w-4 h-4" />
        </button>
      </div>

      {/* Tab Contents */}
      <div className="flex-1 p-3 overflow-y-auto space-y-3">
        {activeTab === 'INSPECTOR' && (
          <div className="space-y-4">
            <div className="p-2.5 rounded bg-neutral-900/80 border border-neutral-800 space-y-2">
              <span className="font-semibold text-neutral-200 text-xs flex items-center gap-1.5">
                <Sliders className="w-3.5 h-3.5 text-blue-400" />
                Active Selection Context
              </span>
              <div className="grid grid-cols-2 gap-2 text-[11px]">
                <div className="text-neutral-500">Target Object:</div>
                <div className="font-mono text-cyan-400">FP_Motor_VFD_01</div>
                <div className="text-neutral-500">UDT Archetype:</div>
                <div className="font-mono text-neutral-300">FB_Motor_UDT</div>
                <div className="text-neutral-500">Hardware Address:</div>
                <div className="font-mono text-emerald-400">%QX0.0..%QX0.3</div>
              </div>
            </div>

            {/* Interlock Matrix Checklist (TSK-306) */}
            <div className="p-2.5 rounded bg-neutral-900/80 border border-neutral-800 space-y-2">
              <span className="font-semibold text-neutral-200 text-xs flex items-center gap-1.5">
                <Activity className="w-3.5 h-3.5 text-amber-400" />
                Interlock Matrix Checklist
              </span>
              <div className="space-y-1.5 text-[11px]">
                <div className="flex items-center justify-between py-1 border-b border-neutral-800">
                  <span className="text-neutral-300">E-Stop Circuit Permissive</span>
                  <span className="flex items-center gap-1 text-emerald-400 font-mono">
                    <CheckCircle className="w-3.5 h-3.5" /> [✓] OK
                  </span>
                </div>
                <div className="flex items-center justify-between py-1 border-b border-neutral-800">
                  <span className="text-neutral-300">Guard Door Closed Interlock</span>
                  <span className="flex items-center gap-1 text-emerald-400 font-mono">
                    <CheckCircle className="w-3.5 h-3.5" /> [✓] OK
                  </span>
                </div>
                <div className="flex items-center justify-between py-1 border-b border-neutral-800">
                  <span className="text-neutral-300">Motor Overtemp Thermal Trip</span>
                  <span className="flex items-center gap-1 text-emerald-400 font-mono">
                    <CheckCircle className="w-3.5 h-3.5" /> [✓] CLEAR
                  </span>
                </div>
                <div className="flex items-center justify-between py-1">
                  <span className="text-neutral-300">Lubrication Pressure Delta</span>
                  <span className="flex items-center gap-1 text-rose-400 font-mono">
                    <XCircle className="w-3.5 h-3.5" /> [✗] LOW (1.1 bar)
                  </span>
                </div>
              </div>
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
              <div className="text-[11px] text-neutral-400">4 Archetype Views: P&ID, Card, Popup, Alarm</div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
