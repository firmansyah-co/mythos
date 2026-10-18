import React, { useState } from 'react';
import { MonacoStEditor } from './MonacoStEditor';
import { LadderCanvas } from './LadderCanvas';
import { DeployControlBar, DeployResultInfo } from './DeployControlBar';
import { Code2, GitMerge } from 'lucide-react';

export interface LogicEditorPluginProps {
  initialCode?: string;
  targetBlock?: string;
  readOnly?: boolean;
  onCodeChange?: (code: string) => void;
  onDeployResult?: (result: DeployResultInfo) => void;
  className?: string;
}

export const LogicEditorPlugin: React.FC<LogicEditorPluginProps> = ({
  initialCode,
  targetBlock = 'MainTask',
  readOnly = false,
  onCodeChange,
  onDeployResult,
  className = '',
}) => {
  const [code, setCode] = useState<string>(initialCode ?? '');
  const [viewMode, setViewMode] = useState<'ST' | 'LADDER'>('ST');

  const handleCodeChange = (newVal: string | undefined) => {
    const next = newVal ?? '';
    setCode(next);
    onCodeChange?.(next);
  };

  return (
    <div className={`flex flex-col w-full h-full bg-[#0A0D12] overflow-hidden ${className}`}>
      {/* Top Deploy & View Mode Toolbar */}
      <DeployControlBar
        code={code}
        targetBlock={targetBlock}
        onDeployResult={onDeployResult}
      />

      {/* Editor Sub-toolbar / View Mode Switcher */}
      <div className="flex items-center justify-between px-3 py-1 bg-[#131924] border-b border-neutral-800 text-xs">
        <div className="flex items-center gap-1 bg-neutral-900 p-0.5 rounded border border-neutral-800">
          <button
            onClick={() => setViewMode('ST')}
            className={`flex items-center gap-1.5 px-2.5 py-0.5 rounded text-[11px] font-medium transition-colors ${
              viewMode === 'ST'
                ? 'bg-blue-600 text-white shadow-sm'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Code2 className="w-3.5 h-3.5" />
            <span>Structured Text (ST)</span>
          </button>
          <button
            onClick={() => setViewMode('LADDER')}
            className={`flex items-center gap-1.5 px-2.5 py-0.5 rounded text-[11px] font-medium transition-colors ${
              viewMode === 'LADDER'
                ? 'bg-blue-600 text-white shadow-sm'
                : 'text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <GitMerge className="w-3.5 h-3.5" />
            <span>Ladder Diagram (LD)</span>
          </button>
        </div>

        <div className="text-[11px] text-neutral-500 font-mono">
          {viewMode === 'ST' ? 'Monaco Engine • ISO/IEC 61131-3' : 'Graphical Rung Engine'}
        </div>
      </div>

      {/* Main Canvas Host */}
      <div className="flex-1 w-full h-full min-h-0 relative">
        {viewMode === 'ST' ? (
          <MonacoStEditor
            value={code}
            onChange={handleCodeChange}
            readOnly={readOnly}
          />
        ) : (
          <LadderCanvas />
        )}
      </div>
    </div>
  );
};
