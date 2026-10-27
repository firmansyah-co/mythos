import React, { useState } from 'react';
import { MotorFaceplateProps, FaceplateTier, OperatingMode, InterlockCondition } from '../types';
import {
  NO_DATA_TEXT,
  formatProcessState,
  formatProcessValue,
  hasDisplayableValue,
  isLiveQuality,
  processBarPercent,
} from '../processData';
import { DataQualityBadge } from './DataQualityBadge';

/**
 * MotorFaceplate — ISA-101 3-Tier Motor / VFD Component
 * Tier 1: Vector P&ID Symbol with dynamic process state & ISA-18.2 alarm annunciation
 * Tier 2: Summary Card with live RPM readout, status badge, SP input, alarm badge
 * Tier 3: Diagnostic Faceplate modal/drawer with Manual/Auto toggle, interlock matrix checklist, and maintenance lock
 *
 * When `quality` is not `GOOD` the faceplate renders an explicit quiescent state (SYS-REQ-096):
 * no invented RPM, no invented current, no alarm colour, and the command buttons are disabled
 * because an unbound or non-reporting device cannot be commanded.
 */
export const MotorFaceplate: React.FC<MotorFaceplateProps> = ({
  tag,
  label,
  running,
  tripped,
  actualSpeed,
  speedSetpoint,
  currentAmps = null,
  torquePercent = null,
  maxSpeed,
  interlocks = [],
  tier = 2,
  quality,
  alarm,
  alarmActive,
  alarmUnacknowledged,
  operatingMode,
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
  const [inputSpeed, setInputSpeed] = useState<number>(speedSetpoint ?? 0);
  const [isLocked, setIsLocked] = useState<boolean>(maintenanceLock);

  // Provenance: only live telemetry may drive colour, animation, alarm annunciation and commands.
  const live = isLiveQuality(quality);
  const displayable = hasDisplayableValue(quality);
  const isTripped = live && tripped === true;
  const isRunning = live && running === true;
  const showAlarm = isTripped || (live && alarmActive === true);
  const alarmLabel = alarm ?? (isTripped ? 'CRITICAL' : 'NORMAL');

  // A speed range the design does not store is not invented: the bar is simply not drawn and
  // the setpoint control cannot be used.
  const rangeMax: number | null = maxSpeed ?? null;
  const speedText = formatProcessValue(actualSpeed, quality, 0);
  const setpointText = formatProcessValue(speedSetpoint, quality, 0);
  const ampsText = formatProcessValue(currentAmps, quality, 1);
  const torqueText = formatProcessValue(torquePercent, quality, 0);
  const speedBarPercent = rangeMax === null ? 0 : processBarPercent(actualSpeed, quality, rangeMax, 0);
  const stateText = formatProcessState(running, quality, 'RUNNING', 'STOPPED');
  // Control mode is device state, not design configuration: announce it only when live
  // telemetry supplied one. An unobserved device has no mode, and "AUTO" is not a safe guess.
  // A control mode is device state, read from the prop on every render. Mounting a copy in
  // state pinned the badge to whatever the first render saw, so a mode that arrived with
  // telemetry after mount was never announced (the canvas re-renders these in place).
  const announcedMode: OperatingMode | null = live ? operatingMode ?? null : null;

  // Interlock states are device state: a satisfied permissive is a measurement, so the rows
  // follow the same provenance gate as every other reading. A checkmark a caller passed
  // alongside an unbound device would otherwise sit there in the reassuring colour.
  const interlockList: InterlockCondition[] = live && Array.isArray(interlocks) ? interlocks : [];

  const handleTierSwitch = (newTier: FaceplateTier) => {
    setActiveTier(newTier);
    onTierChange?.(newTier);
  };

  // The command is a request, not a state change: the selector and the badge keep showing what
  // the device last reported, and the mode moves when telemetry says it moved. This is also the
  // safe direction for the controls gated on the mode — a live controller in AUTO does not
  // become commandable because the operator asked for MANUAL.
  const handleModeSwitch = (m: OperatingMode) => {
    onModeToggle?.(m);
  };

  const handleSpeedCommit = (val: number) => {
    setInputSpeed(val);
    onSpeedChange?.(val);
  };

  // Status color determination adhering strictly to ISA-101. Without live data the symbol
  // stays quiescent grey: neither "running" nor "stopped" is known, so neither is drawn.
  const getSymbolFill = () => {
    if (!live) return 'var(--isa101-pump-stopped)';
    if (isTripped) return 'var(--alarm-critical, var(--alarm-critical))';
    if (isRunning) return 'var(--isa101-pump-running, var(--bus-serial))';
    return 'var(--isa101-pump-stopped)';
  };

  const symbolFrameClass = !live
    ? 'border-slate-700 bg-slate-900/40'
    : isTripped
    ? 'border-red-600 bg-red-950/40'
    : isRunning
    ? 'border-sky-500 bg-sky-950/30'
    : 'border-slate-600 bg-slate-900/40';

  // Tier 1: ISA-101 P&ID Vector Symbol
  const renderTier1 = () => (
    <div
      data-testid="faceplate-tier-1"
      className={`relative inline-flex flex-col items-center justify-center p-2 cursor-pointer select-none group ${className}`}
      onClick={() => handleTierSwitch(2)}
      title={`${tag} (${label}) - Click to inspect`}
    >
      <div
        className={`relative flex items-center justify-center w-16 h-16 rounded-full border-2 transition-all duration-200 ${symbolFrameClass} ${
          showAlarm && (alarmUnacknowledged ?? isTripped)
            ? 'isa-alarm-flash ring-2 ring-red-500 ring-offset-2 ring-offset-slate-950'
            : ''
        }`}
        style={{ borderColor: getSymbolFill() }}
      >
        {/* Centrifugal Pump / Motor Vector SVG */}
        <svg
          viewBox="0 0 48 48"
          className="w-10 h-10 transition-transform duration-300"
          style={{
            transform: isRunning ? 'rotate(360deg)' : 'none',
            transition: isRunning ? 'transform 3s linear infinite' : 'none',
          }}
        >
          <circle cx="24" cy="24" r="18" fill="none" stroke="currentColor" strokeWidth="2.5" className="text-slate-400" />
          <path d="M 24 6 L 24 24 L 38 32" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" className="text-slate-300" />
          <circle cx="24" cy="24" r="4" fill={getSymbolFill()} />
        </svg>

        {/* ISA-18.2 Alarm Severity Badge */}
        {showAlarm && (
          <div
            data-testid="isa18-alarm-badge"
            className="absolute -top-1 -right-1 w-5 h-5 flex items-center justify-center rounded bg-red-600 text-white font-mono text-[10px] font-bold shadow-md animate-pulse"
            title={`Alarm Priority: ${alarmLabel}`}
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
        {displayable ? (running === true ? `${speedText} RPM` : stateText) : NO_DATA_TEXT}
      </span>
      <DataQualityBadge quality={quality} className="mt-0.5" />
    </div>
  );

  // Tier 2: Summary Card (Interactive Mimic Widget)
  const renderTier2 = () => (
    <div
      data-testid="faceplate-tier-2"
      className={`flex flex-col w-80 bg-slate-900/95 border border-slate-700/80 rounded-lg p-3 shadow-xl backdrop-blur-sm text-slate-100 ${className}`}
      style={{
        borderTop: !live
          ? '3px solid var(--border-default, var(--border-default))'
          : isTripped
          ? '3px solid var(--alarm-critical, var(--alarm-critical))'
          : isRunning
          ? '3px solid var(--isa101-pump-running, var(--bus-serial))'
          : '3px solid var(--border-default, var(--border-default))',
      }}
    >
      {/* Header */}
      <div className="flex items-center justify-between pb-2 mb-2 border-b border-slate-800">
        <div className="flex flex-col">
          <span className="font-mono text-sm font-bold text-sky-400 tracking-wide">{tag}</span>
          <span className="text-xs text-slate-400 truncate max-w-[170px]" title={label}>{label}</span>
        </div>
        <div className="flex items-center gap-1.5">
          <DataQualityBadge quality={quality} />
          {announcedMode && (
            <span
              data-testid="motor-mode"
              className={`px-1.5 py-0.5 rounded text-[10px] font-mono font-semibold tracking-wider ${
                announcedMode === 'AUTO' ? 'bg-sky-950 text-sky-300 border border-sky-800' : 'bg-amber-950 text-amber-300 border border-amber-800'
              }`}
            >
              {announcedMode}
            </span>
          )}
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

      {/* Equipment state. Both the run state and the trip state are shown explicitly, and both
          say `----` when the device has not reported: an absent `tripped` is unknown, never
          "healthy" (SYS-REQ-096). */}
      <div className="flex items-center justify-between text-[11px] font-mono pb-1">
        <span
          data-testid="motor-run-state"
          className={isRunning ? 'text-emerald-300' : 'text-slate-300'}
        >
          {formatProcessState(running, quality, 'RUNNING', 'STOPPED')}
        </span>
        <span
          data-testid="motor-trip-state"
          className={isTripped ? 'text-red-300 font-bold' : 'text-slate-300'}
        >
          {formatProcessState(tripped, quality, 'TRIPPED', 'NO TRIP')}
        </span>
      </div>

      {/* Main Process Readout */}
      <div className="flex items-baseline justify-between py-1">
        <div className="flex flex-col">
          <span className="text-[11px] uppercase tracking-wider text-slate-400">Actual Speed</span>
          <div className="flex items-baseline gap-1">
            <span data-testid="motor-speed" className="font-mono text-2xl font-bold text-slate-100 tabular-nums">{speedText}</span>
            {displayable && <span className="text-xs text-slate-400 font-mono">RPM</span>}
          </div>
        </div>
        <div className="flex flex-col items-end">
          <span className="text-[11px] uppercase tracking-wider text-slate-400">Setpoint</span>
          <div className="flex items-baseline gap-1">
            <span data-testid="motor-setpoint" className="font-mono text-lg font-medium text-slate-300 tabular-nums">{setpointText}</span>
            {displayable && <span className="text-xs text-slate-400 font-mono">RPM</span>}
          </div>
        </div>
      </div>

      {/* Analog Speed Bar */}
      <div className="w-full bg-slate-800 h-2 rounded-full overflow-hidden my-2 border border-slate-700/50">
        <div
          className={`h-full transition-all duration-300 ${
            isTripped ? 'bg-red-500' : isRunning ? 'bg-sky-500' : 'bg-slate-600'
          }`}
          style={{ width: `${speedBarPercent}%` }}
        />
      </div>

      {/* Metrics Row: Current Amps & Torque */}
      <div className="grid grid-cols-2 gap-2 text-xs py-1 border-t border-slate-800/80">
        <div className="flex justify-between items-center bg-slate-950/40 px-2 py-1 rounded">
          <span className="text-slate-400">Current:</span>
          <span data-testid="motor-current" className="font-mono font-semibold text-slate-200">{displayable ? `${ampsText} A` : ampsText}</span>
        </div>
        <div className="flex justify-between items-center bg-slate-950/40 px-2 py-1 rounded">
          <span className="text-slate-400">Torque:</span>
          <span data-testid="motor-torque" className="font-mono font-semibold text-slate-200">{displayable ? `${torqueText} %` : torqueText}</span>
        </div>
      </div>

      {/* ISA-101: pushbuttons stay neutral. Saturated red/green are reserved for abnormal
          process state (trips, alarms), so START/STOP are light-grey bordered buttons here. */}
      {/* Operator Primary Action Bar */}
      <div className="flex items-center gap-2 mt-3 pt-2 border-t border-slate-800">
        <button
          onClick={onStart}
          disabled={!live || isRunning || isTripped || isLocked}
          data-testid="btn-motor-start"
          className="flex-1 py-1.5 px-3 rounded border border-slate-600 bg-slate-800 hover:bg-slate-700 disabled:opacity-40 disabled:cursor-not-allowed text-slate-200 font-medium text-xs shadow transition-colors"
        >
          START
        </button>
        <button
          onClick={onStop}
          disabled={!live || !isRunning || isLocked}
          data-testid="btn-motor-stop"
          className="flex-1 py-1.5 px-3 rounded border border-slate-600 bg-slate-800 hover:bg-slate-700 disabled:opacity-40 disabled:cursor-not-allowed text-slate-200 font-medium text-xs shadow transition-colors"
        >
          STOP
        </button>
        {isTripped && (
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
        <div className="flex items-center gap-2">
          <DataQualityBadge quality={quality} />
          <button
            onClick={() => handleTierSwitch(2)}
            className="text-slate-400 hover:text-slate-200 text-sm p-1 rounded hover:bg-slate-800"
            title="Return to Summary Card"
            data-testid="btn-close-diagnostic"
          >
            ✕
          </button>
        </div>
      </div>

      {/* Control Mode & Maintenance Lock */}
      <div className="flex items-center justify-between bg-slate-950/60 p-2 rounded border border-slate-800">
        <div className="flex items-center gap-1">
          <button
            onClick={() => handleModeSwitch('AUTO')}
            disabled={!live}
            title={live ? undefined : 'Device is not reporting: mode cannot be changed'}
            className={`px-3 py-1 rounded text-xs font-mono font-semibold transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${
              operatingMode === 'AUTO' ? 'bg-sky-600 text-white' : 'bg-slate-800 text-slate-400 hover:bg-slate-700'
            }`}
          >
            AUTO
          </button>
          <button
            onClick={() => handleModeSwitch('MANUAL')}
            disabled={!live}
            title={live ? undefined : 'Device is not reporting: mode cannot be changed'}
            className={`px-3 py-1 rounded text-xs font-mono font-semibold transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${
              operatingMode === 'MANUAL' ? 'bg-amber-600 text-white' : 'bg-slate-800 text-slate-400 hover:bg-slate-700'
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

      {/* Speed Setpoint Regulation. A range the design does not store is shown as `----` and
          the control is unusable, rather than being given an invented 0-1500 span. */}
      <div className="flex flex-col gap-1 bg-slate-950/40 p-2.5 rounded border border-slate-800">
        <div className="flex justify-between text-xs">
          <span className="text-slate-400">
            Speed Target (0 - {rangeMax === null ? NO_DATA_TEXT : `${rangeMax} RPM`})
          </span>
          <span className="font-mono font-bold text-sky-400">{displayable ? `${inputSpeed} RPM` : NO_DATA_TEXT}</span>
        </div>
        <input
          type="range"
          min="0"
          max={rangeMax ?? 0}
          step="10"
          value={inputSpeed}
          disabled={!live || rangeMax === null || operatingMode !== 'MANUAL' || isLocked}
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
          {interlockList.length === 0 && (
            <div className="text-xs text-slate-400 py-0.5">
              {live
                ? 'No interlock telemetry — none bound to this device.'
                : 'No interlock telemetry — device is not reporting.'}
            </div>
          )}
          {interlockList.map((item: InterlockCondition) => (
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
      {showAlarm && (
        <div className="flex items-center justify-between bg-red-950/60 border border-red-800 p-2 rounded">
          <div className="flex items-center gap-2">
            <span className="w-2.5 h-2.5 rounded-full bg-red-500 animate-ping" />
            <span className="text-xs text-red-200 font-semibold">ALARM: {alarmLabel}</span>
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
