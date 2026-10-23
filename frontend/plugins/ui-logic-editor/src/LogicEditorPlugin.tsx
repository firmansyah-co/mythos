import React, { useState, useEffect } from 'react';
import * as Y from 'yjs';
import type { Awareness } from 'y-protocols/awareness';
import { MonacoStEditor, TextualLogicLanguage } from './MonacoStEditor';
import { LadderCanvas } from './LadderCanvas';
import { FbdCanvas } from './fbd/FbdCanvas';
import { SfcCanvas } from './sfc/SfcCanvas';
import { VariableTableGrid } from './variables/VariableTableGrid';
import { DataTypeEditor } from './variables/DataTypeEditor';
import { DeployControlBar, DeployResultInfo } from './DeployControlBar';
import { sharedDebugEngine } from './debug/OnlineDebugEngine';
import { PowerFlowOverlay } from './debug/PowerFlowOverlay';
import { VariableForcingDeck } from './debug/VariableForcingDeck';
import { OnlineDebugState } from './debug/types';
import {
  Code2,
  GitMerge,
  FileCode,
  Layers,
  Table,
  Workflow,
  Play,
  Square,
  Sliders,
} from 'lucide-react';

export type LogicEditorViewMode =
  | 'ST'         // Structured Text Monaco Editor
  | 'IL'         // Instruction List Monaco Editor
  | 'LADDER'     // Ladder Diagram Graphical Canvas
  | 'FBD'        // Function Block Diagram Canvas
  | 'SFC'        // Sequential Function Chart Canvas
  | 'VARIABLES'  // Variable Table Grid
  | 'DATATYPES'; // Data Type Editor (DUT / UDT)

export interface LogicEditorPluginProps {
  initialCode?: string;
  initialMode?: LogicEditorViewMode;
  targetBlock?: string;
  readOnly?: boolean;
  onCodeChange?: (code: string) => void;
  onDeployResult?: (result: DeployResultInfo) => void;
  className?: string;

  // Collaborative editing support (TSK-311)
  yText?: Y.Text;
  awareness?: Awareness;
  roomId?: string;
}

export const LogicEditorPlugin: React.FC<LogicEditorPluginProps> = ({
  initialCode,
  initialMode = 'ST',
  targetBlock = 'MainTask',
  readOnly = false,
  onCodeChange,
  onDeployResult,
  className = '',
  yText,
  awareness,
  roomId,
}) => {
  const [code, setCode] = useState<string>(initialCode ?? '');
  const [viewMode, setViewMode] = useState<LogicEditorViewMode>(initialMode);
  const [_textualLanguage, setTextualLanguage] = useState<TextualLogicLanguage>(
    initialMode === 'IL' ? 'IL' : 'ST'
  );

  // Online Debug & Forcing Deck State
  const [debugState, setDebugState] = useState<OnlineDebugState>(sharedDebugEngine.getSnapshot());
  const [isForceDeckOpen, setIsForceDeckOpen] = useState(false);
  const isOnlineDebug = debugState.state === 'ONLINE';

  useEffect(() => {
    const unsubscribe = sharedDebugEngine.subscribe((state) => {
      setDebugState(state);
    });
    return () => {
      unsubscribe();
    };
  }, []);

  const handleToggleDebug = () => {
    if (isOnlineDebug) {
      sharedDebugEngine.disconnect();
    } else {
      sharedDebugEngine.connect();
    }
  };

  const handleCodeChange = (newVal: string | undefined) => {
    const next = newVal ?? '';
    setCode(next);
    onCodeChange?.(next);
  };

  const handleViewModeChange = (mode: LogicEditorViewMode) => {
    setViewMode(mode);
    if (mode === 'ST') setTextualLanguage('ST');
    if (mode === 'IL') setTextualLanguage('IL');
  };

  return (
    <div
      className={`flex flex-col w-full h-full bg-[var(--bg-base)] overflow-hidden ${className}`}
      data-testid="logic-editor-plugin"
    >
      {/* Top Deploy & Controller Action Bar */}
      <DeployControlBar
        code={code}
        targetBlock={targetBlock}
        onDeployResult={onDeployResult}
      />

      {/* Editor Main Sub-Toolbar / Language & Canvas Mode Switcher */}
      <div className="flex items-center justify-between px-3 py-1 bg-[var(--bg-surface)] border-b border-neutral-800 text-xs">
        {/* Multi-language Navigation Tabs */}
        <div className="flex items-center gap-1 bg-neutral-900 p-0.5 rounded border border-neutral-800 overflow-x-auto">
          {/* ST */}
          <button
            onClick={() => handleViewModeChange('ST')}
            data-testid="tab-st"
            className={`flex items-center gap-1 px-2.5 py-0.5 rounded text-[11px] font-medium transition-colors cursor-pointer ${
              viewMode === 'ST'
                ? 'bg-blue-600 text-white shadow-sm'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Code2 className="w-3.5 h-3.5" />
            <span>Structured Text (ST)</span>
          </button>

          {/* IL */}
          <button
            onClick={() => handleViewModeChange('IL')}
            data-testid="tab-il"
            className={`flex items-center gap-1 px-2.5 py-0.5 rounded text-[11px] font-medium transition-colors cursor-pointer ${
              viewMode === 'IL'
                ? 'bg-blue-600 text-white shadow-sm'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <FileCode className="w-3.5 h-3.5" />
            <span>IL Editor</span>
          </button>

          {/* LD */}
          <button
            onClick={() => handleViewModeChange('LADDER')}
            data-testid="tab-ladder"
            className={`flex items-center gap-1 px-2.5 py-0.5 rounded text-[11px] font-medium transition-colors cursor-pointer ${
              viewMode === 'LADDER'
                ? 'bg-blue-600 text-white shadow-sm'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <GitMerge className="w-3.5 h-3.5" />
            <span>Ladder Diagram (LD)</span>
          </button>

          {/* FBD */}
          <button
            onClick={() => handleViewModeChange('FBD')}
            data-testid="tab-fbd"
            className={`flex items-center gap-1 px-2.5 py-0.5 rounded text-[11px] font-medium transition-colors cursor-pointer ${
              viewMode === 'FBD'
                ? 'bg-blue-600 text-white shadow-sm'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Layers className="w-3.5 h-3.5" />
            <span>FBD Canvas</span>
          </button>

          {/* SFC */}
          <button
            onClick={() => handleViewModeChange('SFC')}
            data-testid="tab-sfc"
            className={`flex items-center gap-1 px-2.5 py-0.5 rounded text-[11px] font-medium transition-colors cursor-pointer ${
              viewMode === 'SFC'
                ? 'bg-blue-600 text-white shadow-sm'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Workflow className="w-3.5 h-3.5" />
            <span>SFC Canvas</span>
          </button>

          {/* Variables Table */}
          <button
            onClick={() => handleViewModeChange('VARIABLES')}
            data-testid="tab-variables"
            className={`flex items-center gap-1 px-2.5 py-0.5 rounded text-[11px] font-medium transition-colors cursor-pointer ${
              viewMode === 'VARIABLES'
                ? 'bg-blue-600 text-white shadow-sm'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Table className="w-3.5 h-3.5" />
            <span>Variables</span>
          </button>

          {/* Data Types */}
          <button
            onClick={() => handleViewModeChange('DATATYPES')}
            data-testid="tab-datatypes"
            className={`flex items-center gap-1 px-2.5 py-0.5 rounded text-[11px] font-medium transition-colors cursor-pointer ${
              viewMode === 'DATATYPES'
                ? 'bg-blue-600 text-white shadow-sm'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Sliders className="w-3.5 h-3.5" />
            <span>Data Types</span>
          </button>
        </div>

        {/* Live Debug Toggle & Status Overlay */}
        <div className="flex items-center gap-3">
          {/* Debug Mode Run Toggle */}
          <button
            onClick={handleToggleDebug}
            data-testid="debug-toggle-btn"
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded text-[11px] font-mono font-bold transition-all shadow cursor-pointer ${
              isOnlineDebug
                ? 'bg-emerald-600 hover:bg-emerald-500 text-white shadow-emerald-900/50'
                : 'bg-neutral-800 hover:bg-neutral-700 text-neutral-300 border border-neutral-700'
            }`}
          >
            {isOnlineDebug ? (
              <>
                <Square className="w-3 h-3 fill-current text-white" />
                <span>Stop Debug</span>
              </>
            ) : (
              <>
                <Play className="w-3 h-3 fill-current text-emerald-400" />
                <span>Online Debug</span>
              </>
            )}
          </button>

          {debugState.state === 'FAULT' && (
            <span
              role="alert"
              data-testid="debug-connection-state"
              className="text-rose-300 border border-rose-800 bg-rose-950/50 px-2 py-0.5 rounded text-[11px] font-mono"
            >
              FAULT: no live telemetry
            </span>
          )}

          {/* Power Flow Floating Overlay */}
          <PowerFlowOverlay
            isOnline={isOnlineDebug}
            cycleTimeMs={debugState.cycleTimeMs}
            telemetryRateHz={debugState.telemetryRateHz}
            forcedCount={Object.keys(debugState.forcedVariables).length}
            onOpenForceDeck={() => setIsForceDeckOpen(true)}
          />

          {roomId && (
            <span className="text-cyan-400 flex items-center gap-1 text-[11px] font-mono">
              <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-pulse" />
              Room: {roomId}
            </span>
          )}
        </div>
      </div>

      {/* Main Canvas / Editor Container */}
      <div className="flex-1 w-full h-full min-h-0 relative">
        {viewMode === 'ST' && (
          <MonacoStEditor
            value={code}
            onChange={handleCodeChange}
            readOnly={readOnly}
            language="ST"
            yText={yText}
            awareness={awareness}
            roomId={roomId}
            liveValues={isOnlineDebug ? (debugState.telemetry as any) : undefined}
          />
        )}

        {viewMode === 'IL' && (
          <MonacoStEditor
            value={code}
            onChange={handleCodeChange}
            readOnly={readOnly}
            language="IL"
            yText={yText}
            awareness={awareness}
            roomId={roomId}
            liveValues={isOnlineDebug ? (debugState.telemetry as any) : undefined}
          />
        )}

        {viewMode === 'LADDER' && (
          <LadderCanvas
            targetBlock={targetBlock}
            readOnly={readOnly}
            debugActive={isOnlineDebug}
            liveTelemetry={debugState.telemetry}
            onForceVariable={(name, val) => sharedDebugEngine.forceVariable(name, val)}
          />
        )}

        {viewMode === 'FBD' && (
          <FbdCanvas
            readOnly={readOnly}
            debugActive={isOnlineDebug}
            liveTelemetry={debugState.telemetry}
          />
        )}

        {viewMode === 'SFC' && (
          <SfcCanvas
            readOnly={readOnly}
            debugActive={isOnlineDebug}
            liveTelemetry={debugState.telemetry}
          />
        )}

        {viewMode === 'VARIABLES' && (
          <VariableTableGrid
            readOnly={readOnly}
            debugActive={isOnlineDebug}
            onForceVariable={(name, val) => sharedDebugEngine.forceVariable(name, val)}
            onUnforceVariable={(name) => sharedDebugEngine.unforceVariable(name)}
          />
        )}

        {viewMode === 'DATATYPES' && (
          <DataTypeEditor readOnly={readOnly} />
        )}
      </div>

      {/* SYS-REQ-096 Variable Forcing Deck Modal */}
      <VariableForcingDeck
        isOpen={isForceDeckOpen}
        onClose={() => setIsForceDeckOpen(false)}
        forcedVariables={debugState.forcedVariables}
        auditLogs={debugState.auditLogs}
        onForceVariable={(name, val, type) => sharedDebugEngine.forceVariable(name, val, type)}
        onUnforceVariable={(name) => sharedDebugEngine.unforceVariable(name)}
        onReleaseAll={() => sharedDebugEngine.releaseAllForces()}
      />
    </div>
  );
};
