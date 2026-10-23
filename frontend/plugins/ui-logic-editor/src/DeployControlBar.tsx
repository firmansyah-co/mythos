import React, { useState } from 'react';
import { Play, CheckCircle2, AlertTriangle, RefreshCw, Cpu } from 'lucide-react';

export interface DeployResultInfo {
  success: boolean;
  message: string;
  compilationTimeMs: number;
  binaryHash?: string | null;
  errorLines?: number[];
}

export interface DeployControlBarProps {
  code: string;
  targetBlock?: string;
  onDeployResult?: (result: DeployResultInfo) => void;
  className?: string;
}

export const DeployControlBar: React.FC<DeployControlBarProps> = ({
  code,
  targetBlock = 'MainTask',
  onDeployResult,
  className = '',
}) => {
  const [isDeploying, setIsDeploying] = useState(false);
  const [statusInfo, setStatusInfo] = useState<DeployResultInfo | null>(null);

  const handleDeploy = async () => {
    setIsDeploying(true);
    setStatusInfo(null);
    const startTime = performance.now();

    try {
      const payload = {
        code,
        language: 'ST',
        target_block: targetBlock,
        hot_reload: true,
      };

      const res = await fetch('/api/v1/control/deploy', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });

      const elapsed = Math.round(performance.now() - startTime);

      if (res.ok) {
        const data = await res.json();
        const result: DeployResultInfo = {
          success: data.success ?? true,
          message: data.message || `Deployed to ${targetBlock} in ${data.compilation_time_ms ?? elapsed}ms`,
          compilationTimeMs: Number(data.compilation_time_ms ?? elapsed),
          binaryHash: data.binary_hash || null,
        };
        setStatusInfo(result);
        onDeployResult?.(result);
      } else {
        const errorText = await res.text();
        let parsedMessage = errorText;
        try {
          const parsed = JSON.parse(errorText);
          parsedMessage = parsed.message || errorText;
        } catch {
          // keep errorText
        }
        const result: DeployResultInfo = {
          success: false,
          message: `Deployment failed (${res.status}): ${parsedMessage}`,
          compilationTimeMs: elapsed,
        };
        setStatusInfo(result);
        onDeployResult?.(result);
      }
    } catch (err: any) {
      const elapsed = Math.round(performance.now() - startTime);
      const result: DeployResultInfo = {
        success: false,
        message: `Network/RPC error: ${err?.message || 'Connection refused'}`,
        compilationTimeMs: elapsed,
      };
      setStatusInfo(result);
      onDeployResult?.(result);
    } finally {
      setIsDeploying(false);
    }
  };

  return (
    <div
      className={`flex items-center justify-between px-3 py-1.5 bg-[var(--bg-surface)] border-b border-neutral-800 text-xs ${className}`}
    >
      <div className="flex items-center gap-2">
        <div className="flex items-center gap-1.5 px-2 py-0.5 rounded bg-neutral-900 border border-neutral-800 font-mono text-[11px] text-neutral-400">
          <Cpu className="w-3.5 h-3.5 text-cyan-400" />
          <span>TARGET:</span>
          <span className="text-emerald-400 font-semibold">{targetBlock}</span>
          <span className="text-neutral-600">|</span>
          <span className="text-neutral-400">IEC 61131-3 ST</span>
        </div>

        {statusInfo && (
          <div
            className={`flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-mono ${
              statusInfo.success
                ? 'bg-emerald-950/40 text-emerald-400 border border-emerald-800/60'
                : 'bg-rose-950/40 text-rose-400 border border-rose-800/60'
            }`}
          >
            {statusInfo.success ? (
              <CheckCircle2 className="w-3.5 h-3.5" />
            ) : (
              <AlertTriangle className="w-3.5 h-3.5" />
            )}
            <span className="truncate max-w-xs">{statusInfo.message}</span>
            <span className="text-neutral-500 font-sans">({statusInfo.compilationTimeMs}ms)</span>
          </div>
        )}
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={handleDeploy}
          disabled={isDeploying}
          className="flex items-center gap-1.5 px-3 py-1 rounded bg-blue-600 hover:bg-blue-500 disabled:bg-neutral-800 disabled:text-neutral-500 text-white font-medium text-xs shadow-sm transition-colors cursor-pointer"
        >
          {isDeploying ? (
            <>
              <RefreshCw className="w-3.5 h-3.5 animate-spin" />
              <span>Compiling & Hot-Reloading...</span>
            </>
          ) : (
            <>
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>Deploy to Controller</span>
            </>
          )}
        </button>
      </div>
    </div>
  );
};
