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
      className={`w-full bg-[var(--bg-surface)] border-t border-neutral-800 text-neutral-300 text-xs select-none flex flex-col ${
        isCollapsed ? 'h-7' : 'h-48'
      } ${className}`}
      data-testid="zone5-bottom-console"
    >
      {/* Tab Header Strip */}
      <div className="flex items-center justify-between px-3 bg-[var(--bg-base)] border-b border-neutral-800 h-7 text-[11px]">
        <div className="flex items-center gap-1">
          <button
            onClick={() => setActiveTab('ALARMS')}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded transition-colors ${
              activeTab === 'ALARMS'
                ? 'bg-neutral-800 text-amber-400 font-medium'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <BellRing className="w-3.5 h-3.5 text-neutral-400" />
            <span>ISA-18.2 Alarms (0 Active)</span>
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
            CYCLE: <span className="text-neutral-300 font-semibold">—</span> | DEADTIME: <span className="text-neutral-300">—</span>
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
        <div className="flex-1 p-3 overflow-y-auto font-mono text-[11px] space-y-1 bg-[var(--bg-base)]">
          {activeTab === 'ALARMS' && (
            <div className="p-2 rounded border border-neutral-800 bg-neutral-900/60 text-neutral-400">
              No alarms are published by the backend. Alarm rows appear here once the controller raises them.
            </div>
          )}

          {activeTab === 'CONSOLE' && (
            <div className="p-2 rounded border border-neutral-800 bg-neutral-900/60 text-neutral-400">
              No compiler output for this session. Deploy a logic block to stream the build and hot-reload log here.
            </div>
          )}

          {activeTab === 'DIAGNOSTICS' && (
            <div className="p-2 rounded border border-neutral-800 bg-neutral-900/60 text-neutral-400">
              No diagnostics reported by the backend. Controller cycle time, fieldbus health and core isolation
              appear here when the daemon publishes them.
            </div>
          )}
        </div>
      )}
    </div>
  );
};
