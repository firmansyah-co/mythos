import React, { useState } from 'react';
import { PidControllerFaceplateProps, FaceplateTier, OperatingMode } from '../types';
import {
  NO_DATA_TEXT,
  formatProcessValue,
  hasDisplayableValue,
  isLiveQuality,
  processBarPercent,
} from '../processData';
import { DataQualityBadge } from './DataQualityBadge';

/**
 * PidControllerFaceplate — ISA-101 3-Tier Closed Loop Controller
 * Tier 1: Vector P&ID Loop Symbol with PV, SP, CV bar, and alarm indicators
 * Tier 2: Summary Card with PV/SP readouts, CV bar, quick SP adjustment, Mode switch (Auto/Manual)
 * Tier 3: Diagnostic Faceplate modal/drawer with PID loop tuning (Kp, Tn, Tv), deadband, and trip limits
 *
 * Without live telemetry the loop shows `----` for PV, SP and CV and stays quiescent: an
 * unobserved controller is not "holding setpoint". Setpoint writes are disabled because there
 * is no confirmed device to write to.
 */
export const PidControllerFaceplate: React.FC<PidControllerFaceplateProps> = ({
  tag,
  label,
  pv,
  sp,
  cv,
  unit,
  kp = null,
  tn = null,
  tv = null,
  spMin = null,
  spMax = null,
  highHighLimit = null,
  highLimit = null,
  lowLimit = null,
  lowLowLimit = null,
  tier = 2,
  quality,
  alarmActive,
  alarmUnacknowledged,
  operatingMode,
  onTierChange,
  onModeToggle,
  onSpChange,
  onCvChange,
  onTuneUpdate,
  className = '',
}) => {
  const [activeTier, setActiveTier] = useState<FaceplateTier>(tier);
  const [targetSp, setTargetSp] = useState<number | null>(sp ?? null);
  const [targetCv, setTargetCv] = useState<number | null>(cv ?? null);
  const [tuneKp, setTuneKp] = useState<number | null>(kp ?? null);
  const [tuneTn, setTuneTn] = useState<number | null>(tn ?? null);
  const [tuneTv, setTuneTv] = useState<number | null>(tv ?? null);

  const live = isLiveQuality(quality);
  const displayable = hasDisplayableValue(quality);
  const pvValue: number | null = pv ?? null;
  // Control mode is device state: announce it only for live telemetry, never as a default.
  // A control mode is device state, read from the prop on every render. Mounting a copy in
  // state pinned the badge to whatever the first render saw, so a mode that arrived with
  // telemetry after mount was never announced (the canvas re-renders these in place).
  const announcedMode: OperatingMode | null = live ? operatingMode ?? null : null;

  // Alarm thresholds are judged only against a real reading; missing data never trips a limit.
  const canJudgeLimits = displayable && pvValue !== null && Number.isFinite(pvValue);
  const exceedsLimit =
    canJudgeLimits &&
    ((highHighLimit != null && pvValue >= highHighLimit) ||
      (highLimit != null && pvValue >= highLimit) ||
      (lowLimit != null && pvValue <= lowLimit) ||
      (lowLowLimit != null && pvValue <= lowLowLimit));
  const showAlarm = live && (alarmActive ?? exceedsLimit);
  const showUnacknowledged = live && (alarmUnacknowledged ?? exceedsLimit);

  const pvText = formatProcessValue(pv, quality, 1);
  const spText = formatProcessValue(targetSp, quality, 1);
  const cvText = formatProcessValue(cv, quality, 1);
  const cvBarPercent = processBarPercent(cv, quality, 100, 0);
  const spSliderMin = spMin ?? 0;
  const spSliderMax = spMax ?? (displayable && targetSp !== null ? Math.max(100, targetSp) : 100);

  const configText = (v: number | null | undefined): string =>
    v === null || v === undefined ? NO_DATA_TEXT : String(v);

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
          showAlarm && showUnacknowledged ? 'isa-alarm-flash ring-2 ring-red-500' : 'border-slate-600'
        }`}
      >
        <div className="flex flex-col items-center justify-center text-center">
          <span className="font-mono text-[10px] font-bold text-sky-400">{tag}</span>
          <div className="flex items-center gap-1 text-[10px] font-mono">
            <span className="text-slate-200">{displayable ? pvText : NO_DATA_TEXT}</span>
            <span className="text-slate-500">/</span>
            <span className="text-sky-300">{displayable ? spText : NO_DATA_TEXT}</span>
          </div>
          <span className="text-[9px] text-slate-500 font-mono">
            CV:{displayable ? formatProcessValue(cv, quality, 0) : NO_DATA_TEXT}%
          </span>
        </div>

        {showAlarm && (
          <div
            data-testid="isa18-alarm-badge"
            className="absolute -top-1 -right-1 w-4 h-4 flex items-center justify-center rounded-full bg-red-600 text-white font-mono text-[9px] font-bold shadow"
          >
            !
          </div>
        )}
      </div>
      {announcedMode ? (
        <span data-testid="pid-mode" className="mt-1 font-mono text-[10px] text-slate-400">{announcedMode}</span>
      ) : (
        <span data-testid="pid-mode" className="mt-1 font-mono text-[10px] text-slate-400">{NO_DATA_TEXT}</span>
      )}
      <DataQualityBadge quality={quality} className="mt-0.5" />
    </div>
  );

  // Tier 2: Summary Card
  const renderTier2 = () => (
    <div
      data-testid="pid-tier-2"
      className={`flex flex-col w-84 bg-slate-900/95 border border-slate-700/80 rounded-lg p-3 shadow-xl backdrop-blur-sm text-slate-100 ${className}`}
      style={{
        borderTop: !live
          ? '3px solid var(--border-default, var(--border-default))'
          : showAlarm
          ? '3px solid var(--alarm-major, var(--alarm-major))'
          : '3px solid var(--isa101-pump-running, var(--bus-serial))',
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
              data-testid="pid-mode-badge"
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
            <span data-testid="pid-pv" className="font-mono text-2xl font-bold text-slate-100 tabular-nums">{pvText}</span>
            <span className="text-xs text-slate-400 font-mono">{displayable ? unit ?? '' : ''}</span>
          </div>
        </div>
        <div className="flex flex-col bg-slate-950/40 p-2 rounded border border-slate-800">
          <span className="text-[10px] uppercase tracking-wider text-slate-400">Setpoint (SP)</span>
          <div className="flex items-baseline gap-1 mt-0.5">
            <span data-testid="pid-sp" className="font-mono text-2xl font-bold text-sky-300 tabular-nums">{spText}</span>
            <span className="text-xs text-slate-400 font-mono">{displayable ? unit ?? '' : ''}</span>
          </div>
        </div>
      </div>

      {/* Controller Output (CV) Bar */}
      <div className="flex flex-col gap-1 my-2">
        <div className="flex justify-between text-xs text-slate-400">
          <span>Output (CV)</span>
          <span data-testid="pid-cv" className="font-mono text-slate-200">{displayable ? `${cvText} %` : cvText}</span>
        </div>
        <div className="w-full bg-slate-800 h-2 rounded-full overflow-hidden border border-slate-700/50">
          <div className="h-full bg-sky-500 transition-all duration-300" style={{ width: `${cvBarPercent}%` }} />
        </div>
      </div>

      {/* SP Slider Adjustment */}
      <div className="flex flex-col gap-1 pt-2 border-t border-slate-800">
        <div className="flex justify-between text-[11px] text-slate-400">
          <span>Target Setpoint</span>
          <span className="font-mono text-slate-300">{displayable ? `${spText} ${unit ?? ''}`.trim() : NO_DATA_TEXT}</span>
        </div>
        <input
          type="range"
          min={spSliderMin}
          max={spSliderMax}
          step="0.5"
          value={live ? targetSp ?? spSliderMin : spSliderMin}
          disabled={!live}
          onChange={(e) => handleSpCommit(Number(e.target.value))}
          className="w-full accent-sky-500 cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
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

      {/* Mode Selector */}
      <div className="flex items-center gap-2 bg-slate-950/60 p-2 rounded border border-slate-800">
        {(['MANUAL', 'AUTO', 'CASCADE'] as OperatingMode[]).map((m) => (
          <button
            key={m}
            onClick={() => handleModeSwitch(m)}
            disabled={!live}
            title={live ? undefined : 'Device is not reporting: mode cannot be changed'}
            className={`flex-1 py-1 rounded text-xs font-mono font-semibold transition-colors disabled:opacity-40 disabled:cursor-not-allowed ${
              operatingMode === m ? 'bg-sky-600 text-white' : 'bg-slate-800 text-slate-400 hover:bg-slate-700'
            }`}
          >
            {m}
          </button>
        ))}
      </div>

      {/* Manual CV Override */}
      {operatingMode === 'MANUAL' && (
        <div className="flex flex-col gap-1 bg-amber-950/30 p-2.5 rounded border border-amber-800/60">
          <div className="flex justify-between text-xs text-amber-300">
            <span>Manual Output (CV Override)</span>
            <span className="font-mono font-bold">
              {displayable ? `${formatProcessValue(targetCv, quality, 1)} %` : NO_DATA_TEXT}
            </span>
          </div>
          <input
            type="range"
            min="0"
            max="100"
            step="1"
            value={targetCv ?? 0}
            disabled={!live}
            onChange={(e) => handleCvCommit(Number(e.target.value))}
            className="w-full accent-amber-500 cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
          />
        </div>
      )}

      {/* PID Loop Tuning Parameters. A stored value is shown as stored; a parameter the design
          does not carry stays empty rather than being seeded with a plausible default. */}
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
              value={tuneKp ?? ''}
              onChange={(e) => setTuneKp(e.target.value === '' ? null : Number(e.target.value))}
              className="bg-slate-900 border border-slate-700 rounded px-2 py-1 font-mono text-slate-100"
            />
          </div>
          <div className="flex flex-col gap-1">
            <span className="text-slate-400">Integral (Tn s)</span>
            <input
              type="number"
              step="0.5"
              value={tuneTn ?? ''}
              onChange={(e) => setTuneTn(e.target.value === '' ? null : Number(e.target.value))}
              className="bg-slate-900 border border-slate-700 rounded px-2 py-1 font-mono text-slate-100"
            />
          </div>
          <div className="flex flex-col gap-1">
            <span className="text-slate-400">Deriv (Tv s)</span>
            <input
              type="number"
              step="0.1"
              value={tuneTv ?? ''}
              onChange={(e) => setTuneTv(e.target.value === '' ? null : Number(e.target.value))}
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

      {/* Limit table. Only the thresholds the design stores are shown. */}
      <div className="flex flex-col gap-1 bg-slate-950/40 p-2.5 rounded border border-slate-800 text-xs">
        <span className="text-xs font-semibold uppercase tracking-wider text-slate-400 mb-1">
          Alarm Setpoint Thresholds
        </span>
        <div className="grid grid-cols-2 gap-2">
          <div className="flex justify-between p-1 bg-slate-900 rounded">
            <span className="text-rose-400">High-High (HH):</span>
            <span className="font-mono text-slate-200">{configText(highHighLimit)}</span>
          </div>
          <div className="flex justify-between p-1 bg-slate-900 rounded">
            <span className="text-amber-400">High (H):</span>
            <span className="font-mono text-slate-200">{configText(highLimit)}</span>
          </div>
          <div className="flex justify-between p-1 bg-slate-900 rounded">
            <span className="text-yellow-400">Low (L):</span>
            <span className="font-mono text-slate-200">{configText(lowLimit)}</span>
          </div>
          <div className="flex justify-between p-1 bg-slate-900 rounded">
            <span className="text-rose-400">Low-Low (LL):</span>
            <span className="font-mono text-slate-200">{configText(lowLowLimit)}</span>
          </div>
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
