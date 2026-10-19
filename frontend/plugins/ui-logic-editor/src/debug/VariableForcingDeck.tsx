import React, { useState } from 'react';
import { ForcedVariableEntry, ForcingAuditLog } from './types';
import { ShieldAlert, Lock, Unlock, AlertTriangle, X, ShieldCheck } from 'lucide-react';

export interface VariableForcingDeckProps {
  isOpen: boolean;
  onClose: () => void;
  forcedVariables: Record<string, ForcedVariableEntry>;
  auditLogs: ForcingAuditLog[];
  onForceVariable: (name: string, value: any, type: string) => void;
  onUnforceVariable: (name: string) => void;
  onReleaseAll: () => void;
  className?: string;
}

export const VariableForcingDeck: React.FC<VariableForcingDeckProps> = ({
  isOpen,
  onClose,
  forcedVariables,
  auditLogs,
  onForceVariable,
  onUnforceVariable,
  onReleaseAll,
  className = '',
}) => {
  const [targetName, setTargetName] = useState('');
  const [targetVal, setTargetVal] = useState('TRUE');
  const [targetType, setTargetType] = useState('BOOL');
  const [activeTab, setActiveTab] = useState<'FORCED' | 'AUDIT'>('FORCED');

  if (!isOpen) return null;

  const forcedList = Object.values(forcedVariables);

  const handleApplyForce = (e: React.FormEvent) => {
    e.preventDefault();
    if (!targetName.trim()) return;

    let parsedVal: any = targetVal;
    if (targetType === 'BOOL') {
      parsedVal = targetVal.toUpperCase() === 'TRUE';
    } else if (targetType === 'INT' || targetType === 'DINT' || targetType === 'REAL') {
      parsedVal = Number(targetVal) || 0;
    }

    onForceVariable(targetName.trim(), parsedVal, targetType);
    setTargetName('');
  };

  return (
    <div className={`fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-4 font-sans ${className}`}>
      <div className="w-full max-w-2xl bg-[#0F141C] border border-amber-600/70 rounded-lg shadow-2xl overflow-hidden flex flex-col max-h-[85vh]">
        {/* Header with Safety Warning Badge */}
        <div className="flex items-center justify-between px-4 py-3 bg-[#171E2B] border-b border-amber-600/50">
          <div className="flex items-center gap-2.5">
            <ShieldAlert className="w-5 h-5 text-amber-400 animate-pulse" />
            <div>
              <h3 className="text-sm font-bold text-white tracking-wide flex items-center gap-2">
                PLC Variable Force Deck
                <span className="px-1.5 py-0.2 bg-amber-500/20 text-amber-400 border border-amber-500/40 text-[10px] rounded font-mono">
                  SYS-REQ-096 SAFETY LOCK
                </span>
              </h3>
              <p className="text-[11px] text-neutral-400">
                Direct I/O & Memory Override • Manual Process Image Intervention
              </p>
            </div>
          </div>

          <button
            onClick={onClose}
            className="p-1 text-neutral-400 hover:text-white rounded hover:bg-neutral-800"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Warning Notification */}
        <div className="px-4 py-2 bg-amber-950/40 border-b border-amber-800/40 text-[11px] text-amber-300 flex items-center gap-2 font-mono">
          <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0" />
          <span>
            CAUTION: Forcing overrides physical sensor inputs and hardware interlocks. Ensure physical perimeter safety.
          </span>
        </div>

        {/* Forcing Input Form */}
        <form onSubmit={handleApplyForce} className="p-4 bg-neutral-900/60 border-b border-neutral-800 flex gap-2 items-end text-xs font-mono">
          <div className="flex-1">
            <label className="block text-[10px] text-neutral-400 mb-1">Variable Name</label>
            <input
              type="text"
              value={targetName}
              onChange={(e) => setTargetName(e.target.value)}
              placeholder="e.g. DI_StartPB, Conveyor_Motor"
              className="w-full px-2.5 py-1.5 bg-[#0A0D12] border border-neutral-700 rounded text-cyan-300 focus:outline-none focus:border-amber-500"
              required
            />
          </div>

          <div className="w-28">
            <label className="block text-[10px] text-neutral-400 mb-1">Type</label>
            <select
              value={targetType}
              onChange={(e) => setTargetType(e.target.value)}
              className="w-full px-2 py-1.5 bg-[#0A0D12] border border-neutral-700 rounded text-neutral-200 focus:outline-none"
            >
              <option value="BOOL">BOOL</option>
              <option value="INT">INT</option>
              <option value="DINT">DINT</option>
              <option value="REAL">REAL</option>
            </select>
          </div>

          <div className="w-28">
            <label className="block text-[10px] text-neutral-400 mb-1">Force Value</label>
            {targetType === 'BOOL' ? (
              <select
                value={targetVal}
                onChange={(e) => setTargetVal(e.target.value)}
                className="w-full px-2 py-1.5 bg-[#0A0D12] border border-neutral-700 rounded text-emerald-400 font-bold focus:outline-none"
              >
                <option value="TRUE">TRUE (1)</option>
                <option value="FALSE">FALSE (0)</option>
              </select>
            ) : (
              <input
                type="text"
                value={targetVal}
                onChange={(e) => setTargetVal(e.target.value)}
                className="w-full px-2 py-1.5 bg-[#0A0D12] border border-neutral-700 rounded text-neutral-200 focus:outline-none font-mono"
              />
            )}
          </div>

          <button
            type="submit"
            className="flex items-center gap-1.5 px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-black font-bold rounded shadow transition-colors"
          >
            <Lock className="w-3.5 h-3.5" />
            <span>Force</span>
          </button>
        </form>

        {/* View Switcher Tabs & Emergency Release All */}
        <div className="flex items-center justify-between px-4 py-2 bg-[#121822] border-b border-neutral-800 text-xs">
          <div className="flex items-center gap-2">
            <button
              onClick={() => setActiveTab('FORCED')}
              className={`px-2.5 py-1 rounded font-mono text-[11px] font-semibold ${
                activeTab === 'FORCED'
                  ? 'bg-amber-600/30 text-amber-300 border border-amber-600/60'
                  : 'text-neutral-400 hover:text-white'
              }`}
            >
              Active Forced Variables ({forcedList.length})
            </button>
            <button
              onClick={() => setActiveTab('AUDIT')}
              className={`px-2.5 py-1 rounded font-mono text-[11px] font-semibold ${
                activeTab === 'AUDIT'
                  ? 'bg-blue-600/30 text-cyan-300 border border-blue-600/60'
                  : 'text-neutral-400 hover:text-white'
              }`}
            >
              Forcing Audit Trail ({auditLogs.length})
            </button>
          </div>

          {forcedList.length > 0 && (
            <button
              onClick={onReleaseAll}
              data-testid="release-all-forces-btn"
              className="flex items-center gap-1.5 px-2.5 py-1 bg-rose-700 hover:bg-rose-600 text-white font-bold rounded text-xs shadow"
            >
              <Unlock className="w-3.5 h-3.5" />
              <span>Release All Forces</span>
            </button>
          )}
        </div>

        {/* Table Content */}
        <div className="flex-1 overflow-y-auto p-4 font-mono text-xs">
          {activeTab === 'FORCED' ? (
            forcedList.length === 0 ? (
              <div className="py-8 text-center text-neutral-500 flex flex-col items-center gap-2">
                <ShieldCheck className="w-8 h-8 text-emerald-500/60" />
                <span>No active forced variables. All process image values are automated.</span>
              </div>
            ) : (
              <table className="w-full text-left border-collapse">
                <thead className="bg-[#141B26] text-neutral-400 text-[11px] border-b border-neutral-800">
                  <tr>
                    <th className="py-2 px-3">Variable</th>
                    <th className="py-2 px-3">Forced Value</th>
                    <th className="py-2 px-3">Type</th>
                    <th className="py-2 px-3">Operator</th>
                    <th className="py-2 px-3">Timestamp</th>
                    <th className="py-2 px-2 text-center">Action</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-neutral-800/60">
                  {forcedList.map((f) => (
                    <tr key={f.variableName} className="hover:bg-neutral-800/40">
                      <td className="py-2 px-3 text-cyan-300 font-bold">{f.variableName}</td>
                      <td className="py-2 px-3">
                        <span className="px-2 py-0.5 rounded bg-amber-500/20 text-amber-300 font-bold border border-amber-500/40">
                          {String(f.value)}
                        </span>
                      </td>
                      <td className="py-2 px-3 text-neutral-400">{f.dataType}</td>
                      <td className="py-2 px-3 text-neutral-300">{f.operator}</td>
                      <td className="py-2 px-3 text-neutral-500 text-[10px]">
                        {new Date(f.forcedAt).toLocaleTimeString()}
                      </td>
                      <td className="py-2 px-2 text-center">
                        <button
                          onClick={() => onUnforceVariable(f.variableName)}
                          className="px-2 py-0.5 bg-neutral-800 hover:bg-neutral-700 text-rose-400 rounded border border-neutral-700"
                        >
                          Unforce
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )
          ) : (
            <div className="space-y-1.5">
              {auditLogs.map((log) => (
                <div
                  key={log.id}
                  className="p-2 rounded bg-neutral-900 border border-neutral-800 flex items-center justify-between text-[11px]"
                >
                  <div className="flex items-center gap-2">
                    <span
                      className={`px-1.5 py-0.2 rounded text-[10px] font-bold ${
                        log.action === 'FORCE'
                          ? 'bg-amber-950 text-amber-400'
                          : log.action === 'UNFORCE'
                          ? 'bg-blue-950 text-cyan-400'
                          : 'bg-rose-950 text-rose-400'
                      }`}
                    >
                      {log.action}
                    </span>
                    <span className="text-neutral-300">{log.comment}</span>
                  </div>
                  <div className="flex items-center gap-2 text-neutral-500 text-[10px]">
                    <span>{log.operator}</span>
                    <span>{new Date(log.timestamp).toLocaleTimeString()}</span>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
