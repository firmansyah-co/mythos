import React, { useState } from 'react';
import { X, Layers } from 'lucide-react';

export interface CreateScreenModalProps {
  isOpen: boolean;
  onClose: () => void;
  onCreate: (name: string, isRoot: boolean) => void | boolean | Promise<boolean>;
}

export const CreateScreenModal: React.FC<CreateScreenModalProps> = ({
  isOpen,
  onClose,
  onCreate,
}) => {
  const [name, setName] = useState('');
  const [isRoot, setIsRoot] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || isSaving) return;
    setIsSaving(true);
    let result;
    try { result = await onCreate(name.trim(), isRoot); } finally { setIsSaving(false); }
    if (result === false) return;
    setName('');
    setIsRoot(false);
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="w-full max-w-md bg-[var(--bg-surface)] border border-neutral-800 rounded-lg shadow-2xl text-neutral-200 overflow-hidden font-sans">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 bg-[var(--bg-base)] border-b border-neutral-800">
          <div className="flex items-center gap-2">
            <Layers className="w-4 h-4 text-sky-400" />
            <h3 className="text-sm font-semibold text-white tracking-wide">
              Add New HMI Graphic Screen
            </h3>
          </div>
          <button
            onClick={onClose}
            className="p-1 hover:bg-neutral-800 rounded text-neutral-400 hover:text-white"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Form Body */}
        <form onSubmit={handleSubmit} className="p-4 space-y-4 text-xs">
          <div>
            <label className="block text-neutral-400 font-medium mb-1">Screen Name</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Screen_01_PlantOverview"
              data-testid="modal-screen-name"
              className="w-full px-2.5 py-1.5 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-cyan-500 font-mono"
              required
              autoFocus
            />
          </div>

          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="modal-screen-is-root"
              checked={isRoot}
              onChange={(e) => setIsRoot(e.target.checked)}
              data-testid="modal-screen-is-root"
              className="rounded bg-neutral-900 border-neutral-700 text-blue-600 focus:ring-0"
            />
            <label htmlFor="modal-screen-is-root" className="text-neutral-300 cursor-pointer">
              Set as Root Process Mimic (Default Startup Screen)
            </label>
          </div>

          {/* Action Buttons */}
          <div className="flex items-center justify-end gap-2 pt-3 border-t border-neutral-800">
            <button
              type="button"
              onClick={onClose}
              data-testid="modal-screen-cancel"
              className="px-3 py-1.5 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-300 font-medium transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              data-testid="modal-screen-submit"
              disabled={isSaving}
              className="px-3 py-1.5 rounded bg-blue-600 hover:bg-blue-500 text-white font-medium shadow transition-colors"
            >
              Add Screen
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
