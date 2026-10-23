import React from 'react';
import { Eye, AlertOctagon } from 'lucide-react';
import { useHmiStore } from '../../stores/useHmiStore';

export interface NonActuatingPreviewBannerProps {
  className?: string;
}

export const NonActuatingPreviewBanner: React.FC<NonActuatingPreviewBannerProps> = ({
  className = '',
}) => {
  const lifecycleMode = useHmiStore((s) => s.lifecycleMode);
  const enterEditMode = useHmiStore((s) => s.enterEditMode);

  if (lifecycleMode !== 'PREVIEW') {
    return null;
  }

  return (
    <div
      data-testid="preview-mode-banner"
      className={`w-full flex items-center justify-between px-4 py-1.5 shadow-md border-b text-white font-mono text-xs z-50 select-none ${className}`}
      style={{
        backgroundColor: 'var(--bus-serial)', // Tailwind sky-600 / var(--bus-serial)
        borderColor: 'var(--bus-serial)',     // Tailwind sky-700 / var(--bus-serial)
      }}
    >
      <div className="flex items-center gap-2.5">
        <div className="p-1 rounded bg-sky-800/80 animate-pulse">
          <Eye className="w-4 h-4 text-sky-100" />
        </div>
        <div className="flex items-center gap-2">
          <span className="font-bold tracking-wider text-sky-50 uppercase">
            NON-ACTUATING PREVIEW MODE
          </span>
          <span className="text-sky-200">|</span>
          <span className="text-[11px] text-sky-100 flex items-center gap-1">
            <AlertOctagon className="w-3.5 h-3.5 text-amber-300" />
            CONTROL COMMANDS INHIBITED • HARDWARE OUTPUT ISOLATED
          </span>
        </div>
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={enterEditMode}
          className="px-2.5 py-0.5 rounded bg-sky-900/90 hover:bg-sky-950 text-white text-[11px] font-sans font-medium border border-sky-400/40 transition-colors cursor-pointer"
        >
          Return to Edit Mode
        </button>
      </div>
    </div>
  );
};
