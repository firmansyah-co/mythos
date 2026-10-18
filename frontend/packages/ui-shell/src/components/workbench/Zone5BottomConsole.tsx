import React, { useState } from 'react';
import {
  Terminal,
  ChevronUp,
  ChevronDown,
  BellRing,
  Cpu,
} from 'lucide-react';

export interface Zone5BottomConsoleProps {
  className?: string;
  isCollapsed?: boolean;
  onToggleCollapse?: () => void;
}

export const Zone5BottomConsole: React.FC<Zone5BottomConsoleProps> = ({
  className = '',
  isCollapsed = false,
  onToggleCollapse,
}) => {
  const [activeTab, setActiveTab] = useState<'CONSOLE' | 'ALARMS' | 'DIAGNOSTICS'>('ALARMS');

  return (
    <div
      className={`w-full bg-[#0F141C] border-t border-neutral-800 text-neutral-300 text-xs select-none flex flex-col ${
        isCollapsed ? 'h-7' : 'h-48'
      } ${className}`}
      data-testid="zone5-bottom-console"
    >
      {/* Tab Header Strip */}
      <div className="flex items-center justify-between px-3 bg-[#0A0D12] border-b border-neutral-800 h-7 text-[11px]">
        <div className="flex items-center gap-1">
          <button
            onClick={() => setActiveTab('ALARMS')}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded transition-colors ${
              activeTab === 'ALARMS'
                ? 'bg-neutral-800 text-amber-400 font-medium'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <BellRing className="w-3.5 h-3.5 text-rose-400 animate-pulse" />
            <span>ISA-18.2 Alarms (1 Active)</span>
          </button>

          <button
            onClick={() => setActiveTab('CONSOLE')}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded transition-colors ${
              activeTab === 'CONSOLE'
                ? 'bg-neutral-800 text-cyan-400 font-medium'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Terminal className="w-3.5 h-3.5" />
            <span>Compiler & Hot-Reload Console</span>
          </button>

          <button
            onClick={() => setActiveTab('DIAGNOSTICS')}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded transition-colors ${
              activeTab === 'DIAGNOSTICS'
                ? 'bg-neutral-800 text-emerald-400 font-medium'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Cpu className="w-3.5 h-3.5" />
            <span>Diagnostic Buffer</span>
          </button>
        </div>

        <div className="flex items-center gap-3">
          <span className="font-mono text-[10px] text-neutral-500">
            CYCLE: <span className="text-emerald-400 font-semibold">410µs</span> | DEADTIME: <span className="text-cyan-400">0µs</span>
          </span>
          <button
            onClick={onToggleCollapse}
            className="p-1 hover:bg-neutral-800 rounded text-neutral-400 hover:text-white"
          >
            {isCollapsed ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
          </button>
        </div>
      </div>

      {/* Content Area */}
      {!isCollapsed && (
        <div className="flex-1 p-3 overflow-y-auto font-mono text-[11px] space-y-1 bg-[#0A0D12]">
          {activeTab === 'ALARMS' && (
            <div className="space-y-1.5">
              <div className="p-2 rounded bg-rose-950/40 border border-rose-800/60 text-rose-300 flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="px-1.5 py-0.5 bg-rose-600 text-white font-bold rounded text-[9px]">
                    CRITICAL / PRIORITY 1
                  </span>
                  <span className="font-semibold">Boiler 02 High Pressure Trip Threshold Exceeded (48.2 bar)</span>
                </div>
                <div className="text-neutral-400 text-[10px] flex items-center gap-2">
                  <span>TAG: external.PT_Boiler02</span>
                  <button className="px-2 py-0.5 bg-rose-900 hover:bg-rose-800 text-white rounded text-[10px] font-sans">
                    ACKNOWLEDGE
                  </button>
                </div>
              </div>

              <div className="p-1.5 rounded bg-amber-950/30 border border-amber-800/40 text-amber-300 flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="px-1.5 py-0.5 bg-amber-600 text-white font-bold rounded text-[9px]">
                    WARNING / PRIORITY 2
                  </span>
                  <span>Feedwater VFD Motor Inverter Temperature High (78°C)</span>
                </div>
                <span className="text-neutral-500 text-[10px]">14:22:01.402 UTC</span>
              </div>
            </div>
          )}

          {activeTab === 'CONSOLE' && (
            <div className="space-y-1 text-neutral-400">
              <div className="text-cyan-400">[STruC++ Hot-Reload Supervisor] Online. Ready for deploy commands.</div>
              <div>[14:22:00] Initialized SHM RingBuffer: 262,144 bytes mapped at /dev/shm/mythos_tagbus.</div>
              <div className="text-emerald-400">[14:22:01] POST /api/v1/control/deploy dispatched for MainTask.</div>
              <div>[14:22:01] STruC++ compiler: generated.cpp produced (0 warnings).</div>
              <div className="text-emerald-400">[14:22:02] libstrucpp_program_v1.so hot-swapped during 410µs scan dead-time.</div>
              <div className="text-neutral-500">[14:22:02] Hot-reload SUCCESSFUL. Production control loop unaffected.</div>
            </div>
          )}

          {activeTab === 'DIAGNOSTICS' && (
            <div className="space-y-1 text-neutral-400">
              <div className="flex items-center justify-between text-neutral-300 pb-1 border-b border-neutral-800">
                <span>CPU Core 1 Isolation: SCHED_FIFO Priority 99</span>
                <span className="text-emerald-400">JITTER &lt; 5µs</span>
              </div>
              <div>PROFINET IO Controller: 8 Devices configured, 0 packet drops.</div>
              <div>Modbus TCP Gateway: Slave ID 1 responding in 1.4ms.</div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
