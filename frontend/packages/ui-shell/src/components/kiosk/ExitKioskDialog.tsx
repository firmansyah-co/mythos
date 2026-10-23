import React, { useState } from 'react';
import { LogOut, ShieldAlert, X } from 'lucide-react';
import { useHmiStore } from '../../stores/useHmiStore';

export interface ExitKioskDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

/** Typed acknowledgement that guards against an accidental touch-screen exit. */
export const EXIT_CONFIRMATION_WORD = 'EXIT';

export const ExitKioskDialog: React.FC<ExitKioskDialogProps> = ({ isOpen, onClose }) => {
  const [confirmation, setConfirmation] = useState('');
  const [isExiting, setIsExiting] = useState(false);

  const requestExitRuntime = useHmiStore((s) => s.requestExitRuntime);

  if (!isOpen) return null;

  const isConfirmed = confirmation.trim().toUpperCase() === EXIT_CONFIRMATION_WORD;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!isConfirmed) return;
    setIsExiting(true);
    try {
      await requestExitRuntime();
      setConfirmation('');
      onClose();
    } finally {
      setIsExiting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
      <div className="w-full max-w-md bg-[var(--bg-surface)] border border-neutral-700 rounded-lg shadow-2xl overflow-hidden text-neutral-200">
        {/* Modal Header */}
        <div className="flex items-center justify-between px-4 py-3 bg-neutral-900 border-b border-neutral-800">
          <div className="flex items-center gap-2 text-rose-400 font-semibold text-sm">
            <ShieldAlert className="w-4 h-4" />
            <span>EXIT RUNTIME KIOSK</span>
          </div>
          <button
            onClick={onClose}
            className="text-neutral-400 hover:text-neutral-200 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Modal Body */}
        <form onSubmit={handleSubmit} className="p-4 space-y-4">
          <p className="text-xs text-neutral-400 leading-relaxed">
            Runtime kiosk mode suppresses the engineering workbench to protect an operating line.
            Exiting returns the session to EDIT mode and restores the engineering tools.
          </p>

          <div className="p-2.5 rounded bg-amber-950/40 border border-amber-800/80 text-amber-200 text-[11px] leading-relaxed">
            Credential-based exit is not available in this build: the backend implements no
            authorization endpoint, so this exit is an unauthenticated local action rather than an
            IEC 62443 role check. Anyone at this terminal can perform it.
          </div>

          <div className="space-y-3 text-xs">
            <div>
              <label htmlFor="exit-confirmation" className="block text-neutral-400 mb-1 font-medium">
                Type {EXIT_CONFIRMATION_WORD} to confirm
              </label>
              <input
                id="exit-confirmation"
                type="text"
                autoComplete="off"
                spellCheck={false}
                placeholder={EXIT_CONFIRMATION_WORD}
                value={confirmation}
                onChange={(e) => setConfirmation(e.target.value)}
                data-testid="input-exit-confirmation"
                className="w-full px-3 py-1.5 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 font-mono text-xs focus:outline-none focus:border-blue-500"
              />
            </div>
          </div>

          <div className="flex items-center justify-end gap-2 pt-2 border-t border-neutral-800">
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1.5 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-300 text-xs transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isExiting || !isConfirmed}
              data-testid="btn-confirm-exit-kiosk"
              className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-rose-600 hover:bg-rose-500 disabled:bg-neutral-800 disabled:text-neutral-500 disabled:cursor-not-allowed text-white font-medium text-xs transition-colors shadow-sm"
            >
              <LogOut className="w-3.5 h-3.5" />
              <span>{isExiting ? 'Exiting...' : 'Exit Runtime'}</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
