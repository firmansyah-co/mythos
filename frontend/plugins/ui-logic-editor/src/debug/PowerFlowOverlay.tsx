import React from 'react';
import { Activity, Zap } from 'lucide-react';

export interface PowerFlowOverlayProps {
  isOnline: boolean;
  cycleTimeMs: number;
  telemetryRateHz?: number;
  forcedCount: number;
  onOpenForceDeck: () => void;
  className?: string;
}

export const PowerFlowOverlay: React.FC<PowerFlowOverlayProps> = ({
  isOnline,
  cycleTimeMs,
  telemetryRateHz = 50,
  forcedCount,
  onOpenForceDeck,
  className = '',
}) => {
  if (!isOnline) return null;

  return (
    <div
      className={`flex items-center gap-3 px-3 py-1 bg-neutral-900/90 border border-neutral-700/80 rounded-full shadow-lg text-[11px] font-mono select-none backdrop-blur-md ${className}`}
      data-testid="power-flow-overlay"
    >
      {/* Live Power Flow Indicator */}
      <div className="flex items-center gap-1.5 text-emerald-400">
        <span className="relative flex h-2 w-2">
          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75" />
          <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500" />
        </span>
        <span className="font-bold tracking-tight">ONLINE POWER-FLOW</span>
      </div>

      <span className="text-neutral-600">|</span>

      {/* Cycle Time & Rate */}
      <div className="flex items-center gap-1 text-neutral-300">
        <Activity className="w-3 h-3 text-cyan-400" />
        <span>Cycle:</span>
        <span className="text-cyan-400 font-semibold">{cycleTimeMs}ms</span>
        <span className="text-neutral-500">({telemetryRateHz}Hz)</span>
      </div>

      <span className="text-neutral-600">|</span>

      {/* Forced Variables Badge & Trigger */}
      <button
        onClick={onOpenForceDeck}
        className={`flex items-center gap-1.5 px-2 py-0.5 rounded transition-colors ${
          forcedCount > 0
            ? 'bg-amber-950/80 text-amber-300 border border-amber-600/80 hover:bg-amber-900 animate-pulse'
            : 'bg-neutral-800 text-neutral-400 hover:text-white'
        }`}
        title="Open Variable Force Deck"
      >
        <Zap className="w-3 h-3 text-amber-400" />
        <span>Forced:</span>
        <span className="font-bold">{forcedCount}</span>
      </button>
    </div>
  );
};
