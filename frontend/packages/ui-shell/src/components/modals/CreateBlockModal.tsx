import React, { useState } from 'react';
import { LogicBlockType, LogicBlockLanguage } from '../../types/hmi';
import { X, FileCode2 } from 'lucide-react';

export interface CreateBlockModalProps {
  isOpen: boolean;
  onClose: () => void;
  onCreate: (block: {
    name: string;
    blockType: LogicBlockType;
    language: LogicBlockLanguage;
    number?: number;
  }) => void;
}

export const CreateBlockModal: React.FC<CreateBlockModalProps> = ({
  isOpen,
  onClose,
  onCreate,
}) => {
  const [name, setName] = useState('Main');
  const [blockType, setBlockType] = useState<LogicBlockType>('OB');
  const [language, setLanguage] = useState<LogicBlockLanguage>('ST');
  const [number, setNumber] = useState(1);

  if (!isOpen) return null;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    onCreate({
      name: name.trim(),
      blockType,
      language,
      number: Number(number) || 1,
    });
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
      <div className="w-full max-w-md bg-[#0F141C] border border-neutral-800 rounded-lg shadow-2xl text-neutral-200 overflow-hidden font-sans">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 bg-[#0A0D12] border-b border-neutral-800">
          <div className="flex items-center gap-2">
            <FileCode2 className="w-4 h-4 text-cyan-400" />
            <h3 className="text-sm font-semibold text-white tracking-wide">
              Add New PLC Program Block
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
            <label className="block text-neutral-400 font-medium mb-1">Block Type</label>
            <select
              value={blockType}
              onChange={(e) => {
                const val = e.target.value as LogicBlockType;
                setBlockType(val);
                if (val === 'OB') setNumber(1);
                if (val === 'FB') setNumber(10);
                if (val === 'FC') setNumber(1);
                if (val === 'DB') setNumber(1);
              }}
              data-testid="modal-block-type"
              className="w-full px-2.5 py-1.5 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-cyan-500"
            >
              <option value="OB">OB - Organization Block (Cyclic / Interrupt)</option>
              <option value="FB">FB - Function Block (With Instance Memory)</option>
              <option value="FC">FC - Function (Stateless Logic Routine)</option>
              <option value="DB">DB - Data Block (Shared Global Variables)</option>
            </select>
          </div>

          <div>
            <label className="block text-neutral-400 font-medium mb-1">Block Name</label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. MainTask, ConveyorControl"
              data-testid="modal-block-name"
              className="w-full px-2.5 py-1.5 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-cyan-500 font-mono"
              required
            />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-neutral-400 font-medium mb-1">Block Number</label>
              <input
                type="number"
                value={number}
                onChange={(e) => setNumber(parseInt(e.target.value, 10) || 0)}
                data-testid="modal-block-number"
                className="w-full px-2.5 py-1.5 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-cyan-500 font-mono"
              />
            </div>
            <div>
              <label className="block text-neutral-400 font-medium mb-1">Language</label>
              <select
                value={language}
                onChange={(e) => setLanguage(e.target.value as LogicBlockLanguage)}
                data-testid="modal-block-language"
                className="w-full px-2.5 py-1.5 bg-neutral-900 border border-neutral-700 rounded text-neutral-200 focus:outline-none focus:border-cyan-500"
              >
                <option value="ST">Structured Text (ST)</option>
                <option value="IL">Instruction List (IL)</option>
                <option value="LD">Ladder Diagram (LD)</option>
                <option value="FBD">Function Block Diagram (FBD)</option>
                <option value="SFC">Sequential Function Chart (SFC)</option>
              </select>
            </div>
          </div>

          {/* Action Buttons */}
          <div className="flex items-center justify-end gap-2 pt-3 border-t border-neutral-800">
            <button
              type="button"
              onClick={onClose}
              data-testid="modal-block-cancel"
              className="px-3 py-1.5 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-300 font-medium transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              data-testid="modal-block-submit"
              className="px-3 py-1.5 rounded bg-blue-600 hover:bg-blue-500 text-white font-medium shadow transition-colors"
            >
              Create Block
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
