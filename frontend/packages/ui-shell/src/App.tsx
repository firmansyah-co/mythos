import React, { useState } from 'react';
import { Zone1TopNav } from './components/workbench/Zone1TopNav';
import { Zone2ProjectTree } from './components/workbench/Zone2ProjectTree';
import { DockviewWorkspace } from './components/workbench/DockviewWorkspace';
import { Zone4TaskCards } from './components/workbench/Zone4TaskCards';
import { Zone5BottomConsole } from './components/workbench/Zone5BottomConsole';
import { NonActuatingPreviewBanner } from './components/banner/NonActuatingPreviewBanner';
import { ExitKioskDialog } from './components/kiosk/ExitKioskDialog';
import { useHmiStore } from './stores/useHmiStore';

export const App: React.FC = () => {
  const isRuntimeFullscreen = useHmiStore((s) => s.isRuntimeFullscreen);
  const [isExitKioskOpen, setIsExitKioskOpen] = useState(false);
  const [isZone4Collapsed, setIsZone4Collapsed] = useState(false);
  const [isZone5Collapsed, setIsZone5Collapsed] = useState(false);

  return (
    <div className="flex flex-col w-screen h-screen overflow-hidden bg-[#0A0D12] text-neutral-200">
      {/* Non-Actuating Preview Mode Warning Banner (TSK-308) */}
      <NonActuatingPreviewBanner />

      {/* IEC 62443 Password Challenge Gate (TSK-310) */}
      <ExitKioskDialog
        isOpen={isExitKioskOpen}
        onClose={() => setIsExitKioskOpen(false)}
      />

      {/* If in Runtime Kiosk mode, hide Zone 1, Zone 2, Zone 5 IDE Chrome (WinCC Fullscreen Paradigm) */}
      {!isRuntimeFullscreen && (
        <Zone1TopNav onOpenExitDialog={() => setIsExitKioskOpen(true)} />
      )}

      {/* Central 5-Zone Workbench Layout */}
      <div className="flex-1 flex w-full min-h-0 overflow-hidden relative">
        {/* Zone 2: Project Tree (Suppressed in Fullscreen Kiosk) */}
        {!isRuntimeFullscreen && (
          <div className="w-64 min-w-[220px] max-w-[360px] h-full flex-shrink-0">
            <Zone2ProjectTree />
          </div>
        )}

        {/* Zone 3: Central Document & Canvas Dockview Host */}
        <main className="flex-1 h-full min-w-0 relative">
          <DockviewWorkspace />
        </main>

        {/* Zone 4: Right Slide-Over Inspector Drawer & Task Cards dock (384px) */}
        <Zone4TaskCards
          isCollapsed={isZone4Collapsed}
          onToggleCollapse={() => setIsZone4Collapsed(!isZone4Collapsed)}
        />
      </div>

      {/* Zone 5: Bottom Console & ISA-18.2 Alarm Banner (Suppressed in Fullscreen Kiosk) */}
      {!isRuntimeFullscreen && (
        <Zone5BottomConsole
          isCollapsed={isZone5Collapsed}
          onToggleCollapse={() => setIsZone5Collapsed(!isZone5Collapsed)}
        />
      )}

      {/* Floating Kiosk Exit Button for Fullscreen Mode */}
      {isRuntimeFullscreen && (
        <button
          onClick={() => setIsExitKioskOpen(true)}
          className="fixed top-3 right-3 z-50 px-3 py-1.5 rounded bg-rose-950/80 hover:bg-rose-900 border border-rose-700/80 text-rose-200 text-xs font-mono shadow-xl flex items-center gap-1.5 cursor-pointer"
          data-testid="kiosk-exit-trigger"
        >
          <span>🚪 EXIT KIOSK (IEC 62443)</span>
        </button>
      )}
    </div>
  );
};

export default App;
