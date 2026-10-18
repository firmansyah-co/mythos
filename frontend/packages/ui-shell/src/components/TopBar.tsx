import React from 'react';
import { Maximize2, Minimize2 } from 'lucide-react';
import { useHmiStore } from '../stores/useHmiStore';
import { PresenceAvatarStack } from './collab/PresenceAvatarStack';

export interface TopBarProps {
  onOpenExitDialog?: () => void;
  className?: string;
}

export const TopBar: React.FC<TopBarProps> = ({ onOpenExitDialog, className = '' }) => {
  const lifecycleMode = useHmiStore((s) => s.lifecycleMode);
  const enterEditMode = useHmiStore((s) => s.enterEditMode);
  const enterPreviewMode = useHmiStore((s) => s.enterPreviewMode);
  const enterDeployMode = useHmiStore((s) => s.enterDeployMode);
  const isRuntimeFullscreen = useHmiStore((s) => s.isRuntimeFullscreen);
  const requestEnterRuntime = useHmiStore((s) => s.requestEnterRuntime);

  const menuItems = ['File', 'Edit', 'View', 'PLC', 'Online', 'Tools', 'Help'];

  return (
    <header className={`w-full bg-[#0A0D12] border-b border-neutral-800 select-none text-neutral-300 ${className}`}>
      {/* Primary Menu & Brand Strip */}
      <div className="flex items-center justify-between px-3 h-9">
        <div className="flex items-center gap-4">
          {/* Brand Logo / Badge */}
          <div className="flex items-center gap-2 pr-2 border-r border-neutral-800">
            <div className="w-5 h-5 rounded bg-blue-600 flex items-center justify-center font-bold text-white text-xs tracking-tighter">
              M
            </div>
            <span className="font-semibold text-xs text-white tracking-wide">
              MythOS <span className="text-[10px] text-cyan-400 font-normal">v0.3.0</span>
            </span>
          </div>

          {/* Siemens TIA Portal Top Menus */}
          <nav className="flex items-center gap-1">
            {menuItems.map((item) => (
              <button
                key={item}
                className="px-2 py-1 rounded text-xs hover:bg-neutral-800/80 hover:text-white transition-colors cursor-pointer text-neutral-400"
              >
                {item}
              </button>
            ))}
          </nav>
        </div>

        {/* Global Controls: CRDT Collaboration Presence + 3-Stage Lifecycle + Kiosk Mode */}
        <div className="flex items-center gap-3">
          {/* Real-Time CRDT Collaboration Presence Avatar Stack */}
          <PresenceAvatarStack />

          {/* 3-Stage Lifecycle Mode Pill Switcher */}
          <div className="flex items-center bg-neutral-900 border border-neutral-800 rounded-md p-0.5">
            <button
              onClick={enterEditMode}
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-medium transition-all ${
                lifecycleMode === 'EDIT'
                  ? 'bg-amber-600 text-white shadow-sm'
                  : 'text-neutral-400 hover:text-neutral-200'
              }`}
            >
              <span className="w-2 h-2 rounded-full bg-amber-300"></span>
              <span>EDIT</span>
            </button>

            <button
              onClick={enterPreviewMode}
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-medium transition-all ${
                lifecycleMode === 'PREVIEW'
                  ? 'bg-sky-600 text-white shadow-sm'
                  : 'text-neutral-400 hover:text-neutral-200'
              }`}
            >
              <span className="w-2 h-2 rounded-full bg-sky-300"></span>
              <span>PREVIEW</span>
            </button>

            <button
              onClick={enterDeployMode}
              className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-medium transition-all ${
                lifecycleMode === 'DEPLOY' || lifecycleMode === 'RUNTIME'
                  ? 'bg-emerald-600 text-white shadow-sm'
                  : 'text-neutral-400 hover:text-neutral-200'
              }`}
            >
              <span className="w-2 h-2 rounded-full bg-emerald-300"></span>
              <span>DEPLOY</span>
            </button>
          </div>

          {/* Fullscreen Kiosk Toggle */}
          <div className="flex items-center gap-1 pl-2 border-l border-neutral-800">
            {isRuntimeFullscreen ? (
              <button
                onClick={onOpenExitDialog}
                className="flex items-center gap-1 px-2.5 py-1 rounded bg-rose-950/60 hover:bg-rose-900 border border-rose-800/80 text-rose-300 text-xs font-mono transition-colors"
                title="Exit Fullscreen Kiosk (IEC 62443 Gate)"
              >
                <Minimize2 className="w-3.5 h-3.5 text-rose-400" />
                <span>EXIT KIOSK</span>
              </button>
            ) : (
              <button
                onClick={requestEnterRuntime}
                className="flex items-center gap-1 px-2.5 py-1 rounded bg-neutral-900 hover:bg-neutral-800 border border-neutral-700 text-neutral-300 text-xs transition-colors"
                title="Immersive Industrial Runtime Fullscreen"
              >
                <Maximize2 className="w-3.5 h-3.5 text-cyan-400" />
                <span>KIOSK RUNTIME</span>
              </button>
            )}
          </div>
        </div>
      </div>
    </header>
  );
};

// Aliased export for Zone1TopNav backwards compatibility
export const Zone1TopNav = TopBar;
export type Zone1TopNavProps = TopBarProps;
