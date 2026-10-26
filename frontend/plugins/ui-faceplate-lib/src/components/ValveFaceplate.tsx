import React, { useState } from 'react';
import { ValveFaceplateProps, FaceplateTier, OperatingMode, InterlockCondition } from '../types';
import {
  NO_DATA_TEXT,
  formatProcessValue,
  hasDisplayableValue,
  isLiveQuality,
  processBarPercent,
} from '../processData';
import { DataQualityBadge } from './DataQualityBadge';

/**
 * ValveFaceplate — ISA-101 3-Tier Pneumatic / Discrete Valve Component
 * Tier 1: Vector P&ID Symbol with dynamic process state (open/closed/transit) & alarm borders
 * Tier 2: Summary Card with open/closed status badge, position readout, open/close buttons
 * Tier 3: Diagnostic Faceplate modal/drawer with Manual/Auto toggle, interlock matrix checklist, and travel timeout status
 *
 * Without live telemetry the valve shows no position and no limit-switch state: "not reporting"
 * is neither open nor closed, and inventing a position would be exactly the silent fallback
 * SYS-REQ-096 forbids. Commands are disabled while the device cannot be observed.
 */
export const ValveFaceplate: React.FC<ValveFaceplateProps> = ({
  tag,
  label,
  openLimit,
  closedLimit,
  transitFault,
  positionPercent = null,
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
  onOpen,
  onClose,
  onResetFault,
  className = '',
}) => {
  const [activeTier, setActiveTier] = useState<FaceplateTier>(tier);
  const [isLocked, setIsLocked] = useState<boolean>(maintenanceLock);

  const live = isLiveQuality(quality);
  const displayable = hasDisplayableValue(quality);
  // Displayed state. Only live telemetry may drive saturated colour, flashing and annunciation;
  // without it the valve is quiescent and reports no position at all.
  const openState = displayable && openLimit === true;
  const closedState = displayable && closedLimit === true;
  const isFaulted = live && transitFault === true;
  const isOpen = live && openLimit === true;
  const isClosed = live && closedLimit === true;
  // "In transit" is a real observation only when at least one limit switch positively reports
  // not-made. With both limits unknown the valve state is unknown, not "IN MOTION".
  const inTransit = live && (openLimit === false || closedLimit === false) && transitFault !== true;
  const showAlarm = isFaulted || (live && alarmActive === true);
  const alarmLabel = alarm ?? (isFaulted ? 'MAJOR' : 'NORMAL');

  const valveStateText = !displayable
    ? NO_DATA_TEXT
    : isFaulted
    ? 'FAULT'
    : isOpen
    ? 'OPEN'
    : isClosed
    ? 'CLOSED'
    : inTransit
    ? 'TRANSIT'
    : NO_DATA_TEXT;

  const valveStatusText = !displayable
    ? NO_DATA_TEXT
    : isFaulted
    ? 'TRANSIT FAULT'
    : isOpen
    ? 'FULLY OPEN'
    : isClosed
    ? 'FULLY CLOSED'
    : inTransit
    ? 'IN MOTION'
    : NO_DATA_TEXT;

  // Control mode is device state: announce it only for live telemetry, never as a default.
  // A control mode is device state, read from the prop on every render. Mounting a copy in
  // state pinned the badge to whatever the first render saw, so a mode that arrived with
  // telemetry after mount was never announced (the canvas re-renders these in place).
  const announcedMode: OperatingMode | null = live ? operatingMode ?? null : null;

  const positionText = formatProcessValue(positionPercent, quality, 0);
  const positionBarPercent = processBarPercent(positionPercent, quality, 100, 0);

  // Interlock states are device state: a satisfied permissive is a measurement, so the rows
  // follow the same provenance gate as every other reading.
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

  // State color determination adhering to ISA-101. Without live data the symbol stays neutral:
  // an unobserved valve is not open, not closed and not faulted.
  const getValveColor = () => {
    if (!live) return 'var(--isa101-valve-closed)';
    if (isFaulted) return 'var(--alarm-major, var(--alarm-major))';
    if (isOpen) return 'var(--isa101-valve-open, var(--text-secondary))';
    return 'var(--isa101-valve-closed)';
  };

  const symbolFrameClass = !live
    ? 'border-slate-700 bg-slate-900/40'
    : isFaulted
    ? 'border-amber-600 bg-amber-950/40'
    : isOpen
    ? 'border-slate-400 bg-slate-800/40'
    : 'border-slate-600 bg-slate-900/40';

  // Tier 1: Vector P&ID Symbol
  const renderTier1 = () => (
    <div
      data-testid="valve-tier-1"
      className={`relative inline-flex flex-col items-center justify-center p-2 cursor-pointer select-none group ${className}`}
      onClick={() => handleTierSwitch(2)}
      title={`${tag} (${label}) - Click to inspect`}
    >
      <div
        className={`relative flex items-center justify-center w-16 h-16 rounded-lg border-2 transition-all duration-200 ${symbolFrameClass} ${
          showAlarm && (alarmUnacknowledged ?? isFaulted)
            ? 'isa-alarm-flash ring-2 ring-amber-500 ring-offset-2 ring-offset-slate-950'
            : ''
        }`}
        style={{ borderColor: getValveColor() }}
      >
        {/* Opposing Triangles ISA-101 Valve Vector SVG */}
        <svg viewBox="0 0 48 48" className="w-10 h-10">
          <polygon
            points="8,14 24,24 8,34"
            fill={openState ? getValveColor() : 'none'}
            stroke="currentColor"
            strokeWidth="2.5"
            className="text-slate-300"
          />
          <polygon
            points="40,14 24,24 40,34"
            fill={openState ? getValveColor() : 'none'}
            stroke="currentColor"
            strokeWidth="2.5"
            className="text-slate-300"
          />
          <line x1="24" y1="24" x2="24" y2="8" stroke="currentColor" strokeWidth="2.5" className="text-slate-400" />
          <line x1="16" y1="8" x2="32" y2="8" stroke="currentColor" strokeWidth="2.5" className="text-slate-400" />
        </svg>

        {/* Alarm Badge */}
        {showAlarm && (
          <div
            data-testid="isa18-alarm-badge"
            className="absolute -top-1 -right-1 w-5 h-5 flex items-center justify-center rounded bg-amber-600 text-slate-950 font-mono text-[10px] font-bold shadow-md animate-pulse"
            title={`Alarm Priority: ${alarmLabel}`}
          >
            ▲
          </div>
        )}
      </div>

      <span className="mt-1 font-mono text-xs font-semibold tracking-wider text-slate-200 group-hover:text-sky-400 transition-colors">
        {tag}
      </span>
      <span className="font-mono text-[10px] text-slate-400">
        {valveStateText}
      </span>
      <DataQualityBadge quality={quality} className="mt-0.5" />
    </div>
  );

  // Tier 2: Summary Card
  const renderTier2 = () => (
    <div
      data-testid="valve-tier-2"
      className={`flex flex-col w-80 bg-slate-900/95 border border-slate-700/80 rounded-lg p-3 shadow-xl backdrop-blur-sm text-slate-100 ${className}`}
      style={{
        borderTop: !live
          ? '3px solid var(--border-default, var(--border-default))'
          : isFaulted
          ? '3px solid var(--alarm-major, var(--alarm-major))'
          : isOpen
          ? '3px solid var(--isa101-valve-open, var(--text-secondary))'
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
              data-testid="valve-mode"
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

      {/* Valve Status Readout */}
      <div className="flex items-center justify-between py-1">
        <div className="flex flex-col">
          <span className="text-[11px] uppercase tracking-wider text-slate-400">Status</span>
          <span data-testid="valve-status" className={`font-mono text-xl font-bold ${
            !displayable ? 'text-slate-500' : isFaulted ? 'text-amber-400' : openState ? 'text-slate-100' : 'text-slate-400'
          }`}>
            {valveStatusText}
          </span>
        </div>
        <div className="flex items-end flex-col">
          <span className="text-[11px] uppercase tracking-wider text-slate-400">Position</span>
          <span data-testid="valve-position" className="font-mono text-lg font-medium text-slate-200">{displayable ? `${positionText} %` : NO_DATA_TEXT}</span>
        </div>
      </div>

      {/* Visual Position Bar */}
      <div className="w-full bg-slate-800 h-2 rounded-full overflow-hidden my-2 border border-slate-700/50">
        <div
          className={`h-full transition-all duration-300 ${
            isFaulted ? 'bg-amber-500' : isOpen ? 'bg-sky-400' : 'bg-slate-600'
          }`}
          style={{ width: `${positionBarPercent}%` }}
        />
      </div>

      {/* ISA-101: OPEN/CLOSE are ordinary pushbuttons and stay neutral; saturated red/green
          are reserved for abnormal process state. Fault RESET keeps its amber attention tone. */}
      {/* Actions */}
      <div className="flex items-center gap-2 mt-3 pt-2 border-t border-slate-800">
        <button
          onClick={onOpen}
          disabled={!live || isOpen || isFaulted || isLocked}
          data-testid="btn-valve-open"
          className="flex-1 py-1.5 px-3 rounded border border-slate-600 bg-slate-800 hover:bg-slate-700 disabled:opacity-40 disabled:cursor-not-allowed text-slate-200 font-medium text-xs shadow transition-colors"
        >
          OPEN
        </button>
        <button
          onClick={onClose}
          disabled={!live || isClosed || isFaulted || isLocked}
          data-testid="btn-valve-close"
          className="flex-1 py-1.5 px-3 rounded border border-slate-600 bg-slate-800 hover:bg-slate-700 disabled:opacity-40 disabled:cursor-not-allowed text-slate-200 font-medium text-xs shadow transition-colors"
        >
          CLOSE
        </button>
        {isFaulted && (
          <button
            onClick={onResetFault}
            data-testid="btn-valve-reset"
            className="py-1.5 px-3 rounded bg-amber-600 hover:bg-amber-500 text-slate-950 font-bold text-xs shadow animate-pulse"
          >
            RESET
          </button>
        )}
      </div>
    </div>
  );

  // Tier 3: Diagnostic Faceplate
  const renderTier3 = () => (
    <div
      data-testid="valve-tier-3"
      className={`w-96 max-w-full bg-slate-900 border border-slate-700 rounded-lg shadow-2xl p-4 text-slate-100 flex flex-col gap-3 ${className}`}
    >
      <div className="flex items-center justify-between border-b border-slate-800 pb-2">
        <div className="flex flex-col">
          <div className="flex items-center gap-2">
            <span className="font-mono text-base font-bold text-sky-400">{tag}</span>
            <span className="text-xs px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-300 font-mono">
              FB_Digital_Valve
            </span>
          </div>
          <span className="text-xs text-slate-400">{label}</span>
        </div>
        <div className="flex items-center gap-2">
          <DataQualityBadge quality={quality} />
          <button
            onClick={() => handleTierSwitch(2)}
            className="text-slate-400 hover:text-slate-200 text-sm p-1 rounded hover:bg-slate-800"
            data-testid="btn-close-diagnostic"
          >
            ✕
          </button>
        </div>
      </div>

      {/* Mode & Lock */}
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

      {/* Limit Switch Feedback */}
      <div className="grid grid-cols-2 gap-2 text-xs">
        <div className="bg-slate-950/40 p-2 rounded border border-slate-800 flex justify-between items-center">
          <span className="text-slate-400">Open Limit:</span>
          <span className={`font-mono font-bold ${!displayable ? 'text-slate-500' : openState ? 'text-emerald-400' : 'text-slate-400'}`}>
            {displayable ? (openState ? 'TRUE' : 'FALSE') : NO_DATA_TEXT}
          </span>
        </div>
        <div className="bg-slate-950/40 p-2 rounded border border-slate-800 flex justify-between items-center">
          <span className="text-slate-400">Closed Limit:</span>
          <span className={`font-mono font-bold ${!displayable ? 'text-slate-500' : closedState ? 'text-emerald-400' : 'text-slate-400'}`}>
            {displayable ? (closedState ? 'TRUE' : 'FALSE') : NO_DATA_TEXT}
          </span>
        </div>
      </div>

      {/* Interlock Safety Checklist */}
      <div className="flex flex-col gap-1.5 bg-slate-950/60 p-2.5 rounded border border-slate-800">
        <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
          Valve Permissive Interlocks
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
