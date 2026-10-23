import React, { useState } from 'react';
import { MotorFaceplateProps, FaceplateTier, OperatingMode, InterlockCondition } from './types';

/**
 * MotorFaceplate — ISA-101 3-Tier Motor / VFD Component
 * Tier 1: Vector P&ID Symbol with dynamic process state & ISA-18.2 alarm annunciation
 * Tier 2: Summary Card with live RPM readout, status badge, SP input, alarm badge
 * Tier 3: Diagnostic Faceplate modal/drawer with Manual/Auto toggle, interlock matrix checklist, and maintenance lock
 */
export const MotorFaceplate: React.FC<MotorFaceplateProps> = ({
  tag,
  label,
  running,
  tripped,
  actualSpeed,
  speedSetpoint,
  currentAmps = 0,
  torquePercent = 0,
  maxSpeed = 1500,
  interlocks = [
    { id: 'estop', name: 'Emergency Stop Circuit OK (ISO 13849-1)', isSatisfied: true },
    { id: 'lube', name: 'Lube Oil Pressure Normal', isSatisfied: true },
    { id: 'thermal', name: 'Motor Thermal Overload Relay Normal', isSatisfied: !tripped },
    { id: 'guard', name: 'Safety Guard Closed & Latched', isSatisfied: true },
  ],
  tier = 2,
  alarm = tripped ? 'CRITICAL' : 'NORMAL',
  alarmActive = tripped,
  alarmUnacknowledged = tripped,
  operatingMode = 'AUTO',
  maintenanceLock = false,
  onTierChange,
  onModeToggle,
  onAcknowledgeAlarm,
  onStart,
  onStop,
  onResetFault,
  onSpeedChange,
  className = '',
}) => {
  const [activeTier, setActiveTier] = useState<FaceplateTier>(tier);
  const [internalMode, setInternalMode] = useState<OperatingMode>(operatingMode);
  const [inputSpeed, setInputSpeed] = useState<number>(speedSetpoint);
  const [isLocked, setIsLocked] = useState<boolean>(maintenanceLock);

  const handleTierSwitch = (newTier: FaceplateTier) => {
    setActiveTier(newTier);
    onTierChange?.(newTier);
  };

  const handleModeSwitch = (m: OperatingMode) => {
    setInternalMode(m);
    onModeToggle?.(m);
  };

  const handleSpeedCommit = (val: number) => {
    setInputSpeed(val);
    onSpeedChange?.(val);
  };

  // Status color determination adhering strictly to ISA-101
  const getSymbolFill = () => {
    if (tripped) return 'var(--alarm-critical)';
    if (running) return 'var(--isa101-pump-running)';
    return 'var(--isa101-pump-stopped)';
  };

  // Tier 1: ISA-101 P&ID Vector Symbol
  const renderTier1 = () => (
    <div
      data-testid="faceplate-tier-1"
      className={`relative inline-flex flex-col items-center justify-center p-2 cursor-pointer select-none group ${className}`}
      onClick={() => handleTierSwitch(2)}
      title={`${tag} (${label}) - Click to inspect`}
    >
      <div
        className={`relative flex items-center justify-center w-16 h-16 rounded-full border-2 transition-all duration-200 ${
          tripped
            ? 'border-red-600 bg-red-950/40'
            : running
            ? 'border-sky-500 bg-sky-950/30'
            : 'border-slate-600 bg-slate-900/40'
        } ${alarmActive && alarmUnacknowledged ? 'isa-alarm-flash ring-2 ring-red-500 ring-offset-2 ring-offset-slate-950' : ''}`}
        style={{ borderColor: getSymbolFill() }}
      >
        {/* Centrifugal Pump / Motor Vector SVG */}
        <svg
          viewBox="0 0 48 48"
          className="w-10 h-10 transition-transform duration-300"
          style={{ transform: running ? 'rotate(360deg)' : 'none', transition: running ? 'transform 3s linear infinite' : 'none' }}
        >
          <circle cx="24" cy="24" r="18" fill="none" stroke="currentColor" strokeWidth="2.5" className="text-slate-400" />
          <path d="M 24 6 L 24 24 L 38 32" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" className="text-slate-300" />
          <circle cx="24" cy="24" r="4" fill={getSymbolFill()} />
        </svg>

        {/* ISA-18.2 Alarm Severity Badge */}
        {alarmActive && (
          <div
            data-testid="isa18-alarm-badge"
            className="absolute -top-1 -right-1 w-5 h-5 flex items-center justify-center rounded bg-red-600 text-white font-mono text-[10px] font-bold shadow-md animate-pulse"
            title={`Alarm Priority: ${alarm}`}
          >
            !
          </div>
        )}
      </div>

      {/* Equipment Identification Tag */}
      <span className="mt-1 font-mono text-xs font-semibold tracking-wider text-slate-200 group-hover:text-sky-400 transition-colors">
        {tag}
      </span>
      <span className="font-mono text-[10px] text-slate-400">
        {running ? `${actualSpeed} RPM` : 'STOPPED'}
      </span>
    </div>
  );

  // Tier 2: Summary Card (Interactive Mimic Widget)
  const renderTier2 = () => (
    <div
      data-testid="faceplate-tier-2"
      className={`flex flex-col w-80 bg-slate-900/95 border border-slate-700/80 rounded-lg p-3 shadow-xl backdrop-blur-sm text-slate-100 ${className}`}
      style={{
        borderTop: tripped
          ? '3px solid var(--alarm-critical)'
          : running
          ? '3px solid var(--isa101-pump-running)'
          : '3px solid var(--border-default)',
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
            title="Open Diagnostic Drawer (Tier 3)"
            data-testid="btn-open-diagnostic"
          >
            Diag
          </button>
        </div>
      </div>

      {/* Main Process Readout */}
      <div className="flex items-baseline justify-between py-1">
        <div className="flex flex-col">
          <span className="text-[11px] uppercase tracking-wider text-slate-400">Actual Speed</span>
          <div className="flex items-baseline gap-1">
            <span className="font-mono text-2xl font-bold text-slate-100 tabular-nums">{actualSpeed}</span>
            <span className="text-xs text-slate-400 font-mono">RPM</span>
          </div>
        </div>
        <div className="flex flex-col items-end">
          <span className="text-[11px] uppercase tracking-wider text-slate-400">Setpoint</span>
          <div className="flex items-baseline gap-1">
            <span className="font-mono text-lg font-medium text-slate-300 tabular-nums">{speedSetpoint}</span>
            <span className="text-xs text-slate-400 font-mono">RPM</span>
          </div>
        </div>
      </div>

      {/* Analog Speed Bar */}
      <div className="w-full bg-slate-800 h-2 rounded-full overflow-hidden my-2 border border-slate-700/50">
        <div
          className={`h-full transition-all duration-300 ${
            tripped ? 'bg-red-500' : running ? 'bg-sky-500' : 'bg-slate-600'
          }`}
          style={{ width: `${Math.min(100, Math.max(0, (actualSpeed / maxSpeed) * 100))}%` }}
        />
      </div>

      {/* Metrics Row: Current Amps & Torque */}
      <div className="grid grid-cols-2 gap-2 text-xs py-1 border-t border-slate-800/80">
        <div className="flex justify-between items-center bg-slate-950/40 px-2 py-1 rounded">
          <span className="text-slate-400">Current:</span>
          <span className="font-mono font-semibold text-slate-200">{currentAmps.toFixed(1)} A</span>
        </div>
        <div className="flex justify-between items-center bg-slate-950/40 px-2 py-1 rounded">
          <span className="text-slate-400">Torque:</span>
          <span className="font-mono font-semibold text-slate-200">{torquePercent.toFixed(0)} %</span>
        </div>
      </div>

      {/* Operator Primary Action Bar */}
      <div className="flex items-center gap-2 mt-3 pt-2 border-t border-slate-800">
        <button
          onClick={onStart}
          disabled={running || tripped || isLocked}
          data-testid="btn-motor-start"
          className="flex-1 py-1.5 px-3 rounded bg-emerald-700/80 hover:bg-emerald-600 disabled:opacity-40 disabled:cursor-not-allowed text-white font-medium text-xs shadow transition-colors"
        >
          START
        </button>
        <button
          onClick={onStop}
          disabled={!running || isLocked}
          data-testid="btn-motor-stop"
          className="flex-1 py-1.5 px-3 rounded bg-rose-800/80 hover:bg-rose-700 disabled:opacity-40 disabled:cursor-not-allowed text-white font-medium text-xs shadow transition-colors"
        >
          STOP
        </button>
        {tripped && (
          <button
            onClick={onResetFault}
            data-testid="btn-motor-reset"
            className="py-1.5 px-3 rounded bg-amber-600 hover:bg-amber-500 text-slate-950 font-bold text-xs shadow animate-pulse"
          >
            RESET
          </button>
        )}
      </div>
    </div>
  );

  // Tier 3: Diagnostic Drawer / Popup Modal
  const renderTier3 = () => (
    <div
      data-testid="faceplate-tier-3"
      className={`w-96 max-w-full bg-slate-900 border border-slate-700 rounded-lg shadow-2xl p-4 text-slate-100 flex flex-col gap-3 ${className}`}
    >
      {/* Header */}
      <div className="flex items-center justify-between border-b border-slate-800 pb-2">
        <div className="flex flex-col">
          <div className="flex items-center gap-2">
            <span className="font-mono text-base font-bold text-sky-400">{tag}</span>
            <span className="text-xs px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-300 font-mono">
              FB_Motor_VFD
            </span>
          </div>
          <span className="text-xs text-slate-400">{label}</span>
        </div>
        <button
          onClick={() => handleTierSwitch(2)}
          className="text-slate-400 hover:text-slate-200 text-sm p-1 rounded hover:bg-slate-800"
          title="Return to Summary Card"
          data-testid="btn-close-diagnostic"
        >
          ✕
        </button>
      </div>

      {/* Control Mode & Maintenance Lock */}
      <div className="flex items-center justify-between bg-slate-950/60 p-2 rounded border border-slate-800">
        <div className="flex items-center gap-1">
          <button
            onClick={() => handleModeSwitch('AUTO')}
            className={`px-3 py-1 rounded text-xs font-mono font-semibold transition-colors ${
              internalMode === 'AUTO' ? 'bg-sky-600 text-white' : 'bg-slate-800 text-slate-400 hover:bg-slate-700'
            }`}
          >
            AUTO
          </button>
          <button
            onClick={() => handleModeSwitch('MANUAL')}
            className={`px-3 py-1 rounded text-xs font-mono font-semibold transition-colors ${
              internalMode === 'MANUAL' ? 'bg-amber-600 text-white' : 'bg-slate-800 text-slate-400 hover:bg-slate-700'
            }`}
          >
            MANUAL
          </button>
        </div>
        <label className="flex items-center gap-1.5 text-xs text-slate-300 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={isLocked}
            onChange={(e) => setIsLocked(e.target.checked)}
            className="rounded border-slate-700 text-sky-600 focus:ring-0"
          />
          <span>Maint. Lock</span>
        </label>
      </div>

      {/* Speed Setpoint Regulation */}
      <div className="flex flex-col gap-1 bg-slate-950/40 p-2.5 rounded border border-slate-800">
        <div className="flex justify-between text-xs">
          <span className="text-slate-400">Speed Target (0 - {maxSpeed} RPM)</span>
          <span className="font-mono font-bold text-sky-400">{inputSpeed} RPM</span>
        </div>
        <input
          type="range"
          min="0"
          max={maxSpeed}
          step="10"
          value={inputSpeed}
          disabled={internalMode !== 'MANUAL' || isLocked}
          onChange={(e) => handleSpeedCommit(Number(e.target.value))}
          className="w-full accent-sky-500 cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
        />
      </div>

      {/* Interlock Safety Matrix Checklist */}
      <div className="flex flex-col gap-1.5 bg-slate-950/60 p-2.5 rounded border border-slate-800">
        <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
          Safety Interlock Matrix (ISO 13849-1)
        </span>
        <div className="flex flex-col gap-1 mt-1">
          {interlocks.map((item: InterlockCondition) => (
            <div key={item.id} className="flex items-center justify-between text-xs py-0.5 border-b border-slate-900/50 last:border-0">
              <span className="text-slate-300 truncate max-w-[260px]">{item.name}</span>
              <span className={`font-mono font-bold px-1.5 py-0.2 rounded text-[11px] ${
                item.isSatisfied ? 'text-emerald-400 bg-emerald-950/50' : 'text-rose-400 bg-rose-950/50'
              }`}>
                {item.isSatisfied ? '[✓] OK' : '[✗] TRIP'}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* Alarm Status & Acknowledge */}
      {alarmActive && (
        <div className="flex items-center justify-between bg-red-950/60 border border-red-800 p-2 rounded">
          <div className="flex items-center gap-2">
            <span className="w-2.5 h-2.5 rounded-full bg-red-500 animate-ping" />
            <span className="text-xs text-red-200 font-semibold">ALARM: {alarm}</span>
          </div>
          <button
            onClick={onAcknowledgeAlarm}
            className="px-2.5 py-1 rounded bg-red-800 hover:bg-red-700 text-white text-xs font-bold transition-colors"
          >
            ACK
          </button>
        </div>
      )}
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
