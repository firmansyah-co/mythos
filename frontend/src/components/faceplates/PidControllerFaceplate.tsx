import React, { useState } from 'react';
import { PidControllerFaceplateProps, FaceplateTier, OperatingMode } from './types';

/**
 * PidControllerFaceplate — ISA-101 3-Tier Closed Loop Controller
 * Tier 1: Vector P&ID Loop Symbol with PV, SP, CV bar, and alarm indicators
 * Tier 2: Summary Card with PV/SP readouts, CV bar, quick SP adjustment, Mode switch (Auto/Manual)
 * Tier 3: Diagnostic Faceplate modal/drawer with PID loop tuning (Kp, Tn, Tv), deadband, and trip limits
 */
export const PidControllerFaceplate: React.FC<PidControllerFaceplateProps> = ({
  tag,
  label,
  pv,
  sp,
  cv,
  unit,
  kp = 1.25,
  tn = 15.0,
  tv = 2.5,
  spMin = 0,
  spMax = 100,
  highHighLimit = 95,
  highLimit = 85,
  lowLimit = 15,
  lowLowLimit = 5,
  tier = 2,
  alarmActive = pv >= highHighLimit || pv >= highLimit || pv <= lowLimit || pv <= lowLowLimit,
  alarmUnacknowledged = pv >= highLimit || pv <= lowLimit,
  operatingMode = 'AUTO',
  onTierChange,
  onModeToggle,
  onSpChange,
  onCvChange,
  onTuneUpdate,
  className = '',
}) => {
  const [activeTier, setActiveTier] = useState<FaceplateTier>(tier);
  const [internalMode, setInternalMode] = useState<OperatingMode>(operatingMode);
  const [targetSp, setTargetSp] = useState<number>(sp);
  const [targetCv, setTargetCv] = useState<number>(cv);
  const [tuneKp, setTuneKp] = useState<number>(kp);
  const [tuneTn, setTuneTn] = useState<number>(tn);
  const [tuneTv, setTuneTv] = useState<number>(tv);

  const handleTierSwitch = (newTier: FaceplateTier) => {
    setActiveTier(newTier);
    onTierChange?.(newTier);
  };

  const handleModeSwitch = (m: OperatingMode) => {
    setInternalMode(m);
    onModeToggle?.(m);
  };

  const handleSpCommit = (val: number) => {
    setTargetSp(val);
    onSpChange?.(val);
  };

  const handleCvCommit = (val: number) => {
    setTargetCv(val);
    onCvChange?.(val);
  };

  const handleTuningSave = () => {
    onTuneUpdate?.(tuneKp, tuneTn, tuneTv);
  };

  // Tier 1: Vector P&ID Symbol
  const renderTier1 = () => (
    <div
      data-testid="pid-tier-1"
      className={`relative inline-flex flex-col items-center justify-center p-2 cursor-pointer select-none group ${className}`}
      onClick={() => handleTierSwitch(2)}
      title={`${tag} (${label}) - Click to inspect`}
    >
      <div
        className={`relative flex items-center justify-center w-16 h-16 rounded border-2 bg-slate-900 transition-all duration-200 ${
          alarmActive && alarmUnacknowledged ? 'isa-alarm-flash ring-2 ring-red-500' : 'border-slate-600'
        }`}
      >
        <div className="flex flex-col items-center justify-center text-center">
          <span className="font-mono text-[10px] font-bold text-sky-400">{tag}</span>
          <div className="flex items-center gap-1 text-[10px] font-mono">
            <span className="text-slate-200">{pv.toFixed(1)}</span>
            <span className="text-slate-500">/</span>
            <span className="text-sky-300">{sp.toFixed(1)}</span>
          </div>
          <span className="text-[9px] text-slate-500 font-mono">CV:{cv.toFixed(0)}%</span>
        </div>

        {alarmActive && (
          <div
            data-testid="isa18-alarm-badge"
            className="absolute -top-1 -right-1 w-4 h-4 flex items-center justify-center rounded-full bg-red-600 text-white font-mono text-[9px] font-bold shadow"
          >
            !
          </div>
        )}
      </div>
      <span className="mt-1 font-mono text-[10px] text-slate-400">{internalMode}</span>
    </div>
  );

  // Tier 2: Summary Card
  const renderTier2 = () => (
    <div
      data-testid="pid-tier-2"
      className={`flex flex-col w-84 bg-slate-900/95 border border-slate-700/80 rounded-lg p-3 shadow-xl backdrop-blur-sm text-slate-100 ${className}`}
      style={{
        borderTop: alarmActive
          ? '3px solid var(--alarm-major)'
          : '3px solid var(--isa101-pump-running)',
      }}
    >
      {/* Header */}
      <div className="flex items-center justify-between pb-2 mb-2 border-b border-slate-800">
        <div className="flex flex-col">
          <span className="font-mono text-sm font-bold text-sky-400 tracking-wide">{tag}</span>
          <span className="text-xs text-slate-400 truncate max-w-[170px]" title={label}>{label}</span>
        </div>
        <div className="flex items-center gap-1.5">
          <span className={`px-1.5 py-0.5 rounded text-[10px] font-mono font-semibold tracking-wider ${
            internalMode === 'AUTO' ? 'bg-sky-950 text-sky-300 border border-sky-800' : 'bg-amber-950 text-amber-300 border border-amber-800'
          }`}>
            {internalMode}
          </span>
          <button
            onClick={() => handleTierSwitch(3)}
            className="px-2 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs border border-slate-700 transition-colors"
            data-testid="btn-open-diagnostic"
          >
            Tune
          </button>
        </div>
      </div>

      {/* PV and SP Dual Display */}
      <div className="grid grid-cols-2 gap-3 py-1">
        <div className="flex flex-col bg-slate-950/40 p-2 rounded border border-slate-800">
          <span className="text-[10px] uppercase tracking-wider text-slate-400">Process (PV)</span>
          <div className="flex items-baseline gap-1 mt-0.5">
            <span className="font-mono text-2xl font-bold text-slate-100 tabular-nums">{pv.toFixed(1)}</span>
            <span className="text-xs text-slate-400 font-mono">{unit}</span>
          </div>
        </div>
        <div className="flex flex-col bg-slate-950/40 p-2 rounded border border-slate-800">
          <span className="text-[10px] uppercase tracking-wider text-slate-400">Setpoint (SP)</span>
          <div className="flex items-baseline gap-1 mt-0.5">
            <span className="font-mono text-2xl font-bold text-sky-300 tabular-nums">{targetSp.toFixed(1)}</span>
            <span className="text-xs text-slate-400 font-mono">{unit}</span>
          </div>
        </div>
      </div>

      {/* Controller Output (CV) Bar */}
      <div className="flex flex-col gap-1 my-2">
        <div className="flex justify-between text-xs text-slate-400">
          <span>Output (CV)</span>
          <span className="font-mono text-slate-200">{cv.toFixed(1)} %</span>
        </div>
        <div className="w-full bg-slate-800 h-2 rounded-full overflow-hidden border border-slate-700/50">
          <div className="h-full bg-sky-500 transition-all duration-300" style={{ width: `${Math.min(100, Math.max(0, cv))}%` }} />
        </div>
      </div>

      {/* SP Slider Adjustment */}
      <div className="flex flex-col gap-1 pt-2 border-t border-slate-800">
        <div className="flex justify-between text-[11px] text-slate-400">
          <span>Target Setpoint</span>
          <span className="font-mono text-slate-300">{targetSp} {unit}</span>
        </div>
        <input
          type="range"
          min={spMin}
          max={spMax}
          step="0.5"
          value={targetSp}
          onChange={(e) => handleSpCommit(Number(e.target.value))}
          className="w-full accent-sky-500 cursor-pointer"
        />
      </div>
    </div>
  );

  // Tier 3: Diagnostic & Tuning Modal
  const renderTier3 = () => (
    <div
      data-testid="pid-tier-3"
      className={`w-96 max-w-full bg-slate-900 border border-slate-700 rounded-lg shadow-2xl p-4 text-slate-100 flex flex-col gap-3 ${className}`}
    >
      <div className="flex items-center justify-between border-b border-slate-800 pb-2">
        <div className="flex flex-col">
          <div className="flex items-center gap-2">
            <span className="font-mono text-base font-bold text-sky-400">{tag}</span>
            <span className="text-xs px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-300 font-mono">
              FB_PID_Controller
            </span>
          </div>
          <span className="text-xs text-slate-400">{label}</span>
        </div>
        <button
          onClick={() => handleTierSwitch(2)}
          className="text-slate-400 hover:text-slate-200 text-sm p-1 rounded hover:bg-slate-800"
          data-testid="btn-close-diagnostic"
        >
          ✕
        </button>
      </div>

      {/* Mode Selector */}
      <div className="flex items-center gap-2 bg-slate-950/60 p-2 rounded border border-slate-800">
        {(['MANUAL', 'AUTO', 'CASCADE'] as OperatingMode[]).map((m) => (
          <button
            key={m}
            onClick={() => handleModeSwitch(m)}
            className={`flex-1 py-1 rounded text-xs font-mono font-semibold transition-colors ${
              internalMode === m ? 'bg-sky-600 text-white' : 'bg-slate-800 text-slate-400 hover:bg-slate-700'
            }`}
          >
            {m}
          </button>
        ))}
      </div>

      {/* Manual CV Override */}
      {internalMode === 'MANUAL' && (
        <div className="flex flex-col gap-1 bg-amber-950/30 p-2.5 rounded border border-amber-800/60">
          <div className="flex justify-between text-xs text-amber-300">
            <span>Manual Output (CV Override)</span>
            <span className="font-mono font-bold">{targetCv.toFixed(1)} %</span>
          </div>
          <input
            type="range"
            min="0"
            max="100"
            step="1"
            value={targetCv}
            onChange={(e) => handleCvCommit(Number(e.target.value))}
            className="w-full accent-amber-500 cursor-pointer"
          />
        </div>
      )}

      {/* PID Loop Tuning Parameters */}
      <div className="flex flex-col gap-2 bg-slate-950/40 p-2.5 rounded border border-slate-800 text-xs">
        <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
          Loop Tuning Parameters
        </span>
        <div className="grid grid-cols-3 gap-2">
          <div className="flex flex-col gap-1">
            <span className="text-slate-400">Gain (Kp)</span>
            <input
              type="number"
              step="0.05"
              value={tuneKp}
              onChange={(e) => setTuneKp(Number(e.target.value))}
              className="bg-slate-900 border border-slate-700 rounded px-2 py-1 font-mono text-slate-100"
            />
          </div>
          <div className="flex flex-col gap-1">
            <span className="text-slate-400">Integral (Tn s)</span>
            <input
              type="number"
              step="0.5"
              value={tuneTn}
              onChange={(e) => setTuneTn(Number(e.target.value))}
              className="bg-slate-900 border border-slate-700 rounded px-2 py-1 font-mono text-slate-100"
            />
          </div>
          <div className="flex flex-col gap-1">
            <span className="text-slate-400">Deriv (Tv s)</span>
            <input
              type="number"
              step="0.1"
              value={tuneTv}
              onChange={(e) => setTuneTv(Number(e.target.value))}
              className="bg-slate-900 border border-slate-700 rounded px-2 py-1 font-mono text-slate-100"
            />
          </div>
        </div>
        <button
          onClick={handleTuningSave}
          className="mt-1 py-1 px-3 rounded bg-sky-700 hover:bg-sky-600 text-white font-medium transition-colors"
        >
          Apply Tuning
        </button>
      </div>
    </div>
  );

  switch (activeTier) {
    case 1:
      return renderTier1();
    case 3:
      return renderTier3();
    case 2:
    default:
      return renderTier2();
  }
};
