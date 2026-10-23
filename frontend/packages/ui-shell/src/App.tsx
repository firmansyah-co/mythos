import React, { useCallback, useEffect, useState } from 'react';
import { Zone1TopNav } from './components/workbench/Zone1TopNav';
import { Zone2ProjectTree } from './components/workbench/Zone2ProjectTree';
import { DockviewWorkspace } from './components/workbench/DockviewWorkspace';
import { Zone4TaskCards } from './components/workbench/Zone4TaskCards';
import { Zone5BottomConsole } from './components/workbench/Zone5BottomConsole';
import { NonActuatingPreviewBanner } from './components/banner/NonActuatingPreviewBanner';
import { ExitKioskDialog } from './components/kiosk/ExitKioskDialog';
import { PortalView } from './components/portal/PortalView';
import { CreateBlockModal } from './components/modals/CreateBlockModal';
import { CreateScreenModal } from './components/modals/CreateScreenModal';
import { useHmiStore } from './stores/useHmiStore';
import { LogicBlockType, LogicBlockLanguage, WorkbenchDocType } from './types/hmi';

export const App: React.FC = () => {
  const isRuntimeFullscreen = useHmiStore((s) => s.isRuntimeFullscreen);
  const workspaceMode = useHmiStore((s) => s.workspaceMode);
  const setWorkspaceMode = useHmiStore((s) => s.setWorkspaceMode);
  const addProjectBlock = useHmiStore((s) => s.addProjectBlock);
  const addNewScreen = useHmiStore((s) => s.addNewScreen);
  const openWorkbenchDocument = useHmiStore((s) => s.openWorkbenchDocument);
  const errorMessage = useHmiStore((s) => s.errorMessage);
  const clearError = useHmiStore((s) => s.clearError);
  const saveScreenDraft = useHmiStore((s) => s.saveScreenDraft);
  const commitHmiDraft = useHmiStore((s) => s.commitHmiDraft);

  // Workbench-wide Save / Commit shortcuts. Prefixed keys fire while focus sits on a faceplate,
  // a dockview tab strip or anywhere outside the canvas, which is where the operator actually is.
  const handleSaveShortcut = useCallback(
    (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 's') return;
      const state = useHmiStore.getState();
      if (!state.currentProject || state.isRuntimeFullscreen) return;
      if (state.screenSaveStatus === 'SAVING' || state.screenSaveStatus === 'COMMITTING') return;
      event.preventDefault();
      if (event.shiftKey) {
        void commitHmiDraft();
        return;
      }
      const activeId = state.activeWorkbenchTabId;
      if (!activeId) return;
      if (!state.draftScreens.has(activeId)) return;
      void saveScreenDraft(activeId);
    },
    [saveScreenDraft, commitHmiDraft]
  );

  useEffect(() => {
    window.addEventListener('keydown', handleSaveShortcut);
    return () => window.removeEventListener('keydown', handleSaveShortcut);
  }, [handleSaveShortcut]);

  const [isExitKioskOpen, setIsExitKioskOpen] = useState(false);
  const [isZone4Collapsed, setIsZone4Collapsed] = useState(false);
  const [isZone5Collapsed, setIsZone5Collapsed] = useState(false);

  // Modal dialog states (TSK-604)
  const [isCreateBlockModalOpen, setIsCreateBlockModalOpen] = useState(false);
  const [isCreateScreenModalOpen, setIsCreateScreenModalOpen] = useState(false);

  // Dynamic document opener handler (TSK-603)
  const handleOpenDocument = (
    docType: string,
    id: string,
    title: string,
    params?: Record<string, any>
  ) => {
    if (workspaceMode === 'PORTAL') {
      setWorkspaceMode('PROJECT');
    }
    openWorkbenchDocument(docType as WorkbenchDocType, id, title, params);
  };

  const handleCreateBlock = (block: {
    name: string;
    blockType: LogicBlockType;
    language: LogicBlockLanguage;
    number?: number;
  }) => {
    const newBlock = addProjectBlock(block);
    const title = `${newBlock.name} [${newBlock.blockType}${newBlock.number ?? ''}]`;
    handleOpenDocument('logic', newBlock.id, title, { targetBlock: newBlock.id });
  };

  const handleCreateScreen = async (name: string, isRoot: boolean) => {
    const screenId = await addNewScreen(name, isRoot);
    if (screenId) {
      handleOpenDocument('screen', screenId, name, { screenId, title: name });
      return true;
    }
    return false;
  };

  return (
    <div className="flex flex-col w-screen h-screen overflow-hidden bg-[var(--bg-base)] text-neutral-200">
      {/* Non-Actuating Preview Mode Warning Banner (TSK-308) */}
      <NonActuatingPreviewBanner />

      {errorMessage && (
        <div
          role="alert"
          data-testid="hmi-error"
          className="fixed top-2 left-1/2 z-[60] flex -translate-x-1/2 items-center gap-3 border border-rose-700 bg-rose-950 px-4 py-2 text-xs text-rose-100 shadow-lg"
        >
          <span>{errorMessage}</span>
          <button type="button" onClick={clearError} className="font-semibold text-rose-300 hover:text-white">
            Dismiss
          </button>
        </div>
      )}

      {/* IEC 62443 Password Challenge Gate (TSK-310) */}
      <ExitKioskDialog
        isOpen={isExitKioskOpen}
        onClose={() => setIsExitKioskOpen(false)}
      />

      {/* Dynamic Creation Modals (TSK-604) */}
      <CreateBlockModal
        isOpen={isCreateBlockModalOpen}
        onClose={() => setIsCreateBlockModalOpen(false)}
        onCreate={handleCreateBlock}
      />

      <CreateScreenModal
        isOpen={isCreateScreenModalOpen}
        onClose={() => setIsCreateScreenModalOpen(false)}
        onCreate={handleCreateScreen}
      />

      {/* If in Runtime Kiosk mode or Portal View, hide Zone 1 Top Nav */}
      {!isRuntimeFullscreen && workspaceMode === 'PROJECT' && (
        <Zone1TopNav onOpenExitDialog={() => setIsExitKioskOpen(true)} />
      )}

      {/* Main Workspace Area: Dual-Mode Portal View vs 5-Zone Workbench (TSK-601) */}
      {workspaceMode === 'PORTAL' && !isRuntimeFullscreen ? (
        <div className="flex-1 w-full min-h-0 overflow-hidden relative">
          <PortalView
            onOpenCreateBlockModal={() => setIsCreateBlockModalOpen(true)}
            onOpenCreateScreenModal={() => setIsCreateScreenModalOpen(true)}
          />
        </div>
      ) : (
        /* Central 5-Zone Workbench Layout */
        <div className="flex-1 flex w-full min-h-0 overflow-hidden relative">
          {/* Zone 2: Project Tree (Suppressed in Fullscreen Kiosk) */}
          {!isRuntimeFullscreen && (
            <div className="w-64 min-w-[220px] max-w-[360px] h-full flex-shrink-0">
              <Zone2ProjectTree
                onOpenDocument={handleOpenDocument}
                onOpenCreateBlockModal={() => setIsCreateBlockModalOpen(true)}
                onOpenCreateScreenModal={() => setIsCreateScreenModalOpen(true)}
              />
            </div>
          )}

          {/* Zone 3: Central Document & Canvas Dockview Host */}
          <main className="flex-1 h-full min-w-0 relative">
            <DockviewWorkspace
              onOpenCreateBlockModal={() => setIsCreateBlockModalOpen(true)}
              onOpenCreateScreenModal={() => setIsCreateScreenModalOpen(true)}
            />
          </main>

          {/* Zone 4: Right Slide-Over Inspector Drawer & Task Cards dock (384px) */}
          <Zone4TaskCards
            isCollapsed={isZone4Collapsed}
            onToggleCollapse={() => setIsZone4Collapsed(!isZone4Collapsed)}
          />
        </div>
      )}

      {/* Zone 5: Bottom Console & ISA-18.2 Alarm Banner (Only in Project View, Suppressed in Kiosk) */}
      {!isRuntimeFullscreen && workspaceMode === 'PROJECT' && (
        <Zone5BottomConsole
          isCollapsed={isZone5Collapsed}
          onToggleCollapse={() => setIsZone5Collapsed(!isZone5Collapsed)}
        />
      )}

      {/* Persistent Bottom-Left Mode Switcher Toggle: Portal View <--> Project View (TSK-601) */}
      {!isRuntimeFullscreen && (
        <div
          className="fixed bottom-2 left-2 z-40 flex items-center bg-[var(--bg-surface)] border border-neutral-700/80 rounded-md p-0.5 shadow-2xl select-none"
          data-testid="workspace-mode-switcher"
        >
          <button
            onClick={() => setWorkspaceMode('PORTAL')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded text-xs font-medium transition-all cursor-pointer ${
              workspaceMode === 'PORTAL'
                ? 'bg-blue-600 text-white shadow'
                : 'text-neutral-400 hover:text-white hover:bg-neutral-800'
            }`}
            data-testid="portal-view-toggle"
          >
            <span>🌐 Portal View</span>
          </button>
          <button
            onClick={() => setWorkspaceMode('PROJECT')}
            className={`flex items-center gap-1.5 px-3 py-1 rounded text-xs font-medium transition-all cursor-pointer ${
              workspaceMode === 'PROJECT'
                ? 'bg-blue-600 text-white shadow'
                : 'text-neutral-400 hover:text-white hover:bg-neutral-800'
            }`}
            data-testid="project-view-toggle"
          >
            <span>🎛️ Project View</span>
          </button>
        </div>
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
