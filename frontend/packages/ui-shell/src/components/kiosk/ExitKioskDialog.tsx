import React, { useState } from 'react';
import { Lock, ShieldAlert, KeyRound, X } from 'lucide-react';
import { useHmiStore } from '../../stores/useHmiStore';

export interface ExitKioskDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

export const ExitKioskDialog: React.FC<ExitKioskDialogProps> = ({ isOpen, onClose }) => {
  const [role, setRole] = useState<'Engineer' | 'Admin'>('Engineer');
  const [password, setPassword] = useState('');
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [isVerifying, setIsVerifying] = useState(false);

  const requestExitRuntime = useHmiStore((s) => s.requestExitRuntime);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsVerifying(true);
    setErrorMsg(null);

    try {
      const success = await requestExitRuntime({
        role,
        password,
      });

      if (success) {
        onClose();
      } else {
        setErrorMsg('Authentication failed: Invalid credentials or insufficient IEC 62443 privileges.');
      }
    } catch (err: any) {
      setErrorMsg(`Verification error: ${err?.message || 'Server unavailable'}`);
    } finally {
      setIsVerifying(false);
    }
  };

  return (
    <div className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/80 backdrop-blur-sm p-4">
      <div className="w-full max-w-md bg-[#0F141C] border border-neutral-700 rounded-lg shadow-2xl overflow-hidden text-neutral-200">
        {/* Modal Header */}
        <div className="flex items-center justify-between px-4 py-3 bg-neutral-900 border-b border-neutral-800">
          <div className="flex items-center gap-2 text-rose-400 font-semibold text-sm">
            <ShieldAlert className="w-4 h-4" />
            <span>IEC 62443 KIOSK EXIT CHALLENGE</span>
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
            Industrial runtime kiosk mode is locked to protect operating production lines. 
            Enter authorization credentials with <span className="text-amber-400 font-semibold">Engineer</span> or <span className="text-amber-400 font-semibold">Admin</span> privilege to unlock workbench engineering tools.
          </p>

          {errorMsg && (
            <div className="p-2.5 rounded bg-rose-950/60 border border-rose-800/80 text-rose-300 text-xs font-mono">
              {errorMsg}
            </div>
          )}

          <div className="space-y-3 text-xs">
            <div>
              <label className="block text-neutral-400 mb-1 font-medium">Security Role</label>
              <select
                value={role}
                onChange={(e) => setRole(e.target.value as 'Engineer' | 'Admin')}
                className="w-full px-3 py-1.5 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-blue-500"
              >
                <option value="Engineer">Engineer (Engineering Workbench Access)</option>
                <option value="Admin">Admin (Full System Administrator)</option>
              </select>
            </div>

            <div>
              <label className="block text-neutral-400 mb-1 font-medium">Password / PIN Token</label>
              <div className="relative">
                <input
                  type="password"
                  placeholder="Enter credential challenge PIN..."
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  className="w-full px-3 py-1.5 pl-8 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 font-mono text-xs focus:outline-none focus:border-blue-500"
                  required
                />
                <KeyRound className="w-4 h-4 text-neutral-500 absolute left-2.5 top-2" />
              </div>
              <span className="text-[10px] text-neutral-500 mt-1 block">
                Standard maintenance token: admin123
              </span>
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
              disabled={isVerifying}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-rose-600 hover:bg-rose-500 disabled:bg-neutral-800 text-white font-medium text-xs transition-colors shadow-sm"
            >
              <Lock className="w-3.5 h-3.5" />
              <span>{isVerifying ? 'Verifying...' : 'Authenticate & Exit Kiosk'}</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
