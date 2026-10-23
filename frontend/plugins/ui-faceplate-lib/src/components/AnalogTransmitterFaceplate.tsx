import React, { useState } from 'react';
import { AnalogTransmitterFaceplateProps, FaceplateTier } from '../types';
import {
  NO_DATA_TEXT,
  formatProcessValue,
  hasDisplayableValue,
  isLiveQuality,
  processBarPercent,
} from '../processData';
import { DataQualityBadge } from './DataQualityBadge';

/**
 * AnalogTransmitterFaceplate — ISA-101 3-Tier Analog Transmitter (4-20mA Sensor)
 * Covers: Pressure, Temperature, Flow, Level
 * Tier 1: Vector P&ID Transmitter Bubble with dynamic PV and ISA-18.2 alarm annunciation
 * Tier 2: Summary Card with digital readout, analog range bar, and limit markers (HH, H, L, LL)
 * Tier 3: Diagnostic Faceplate modal/drawer with sensor health status, raw mA reading, and trip limits
 *
 * Without live telemetry the transmitter shows `----` and a quiescent bubble: no PV, no alarm
 * colour, no loop-current mA reading. A transmitter that is not reporting is not "reading zero".
 */
export const AnalogTransmitterFaceplate: React.FC<AnalogTransmitterFaceplateProps> = ({
  tag,
  label,
  pv,
  unit,
  transmitterType,
  minRange = null,
  maxRange = null,
  highHighLimit = null,
  highLimit = null,
  lowLimit = null,
  lowLowLimit = null,
  sensorFault = null,
  tier = 2,
  quality,
  alarm,
  alarmActive,
  alarmUnacknowledged,
  onTierChange,
  className = '',
}) => {
  const [activeTier, setActiveTier] = useState<FaceplateTier>(tier);

  const live = isLiveQuality(quality);
  const displayable = hasDisplayableValue(quality);
  const hasRange =
    minRange !== null && maxRange !== null && minRange !== undefined && maxRange !== undefined && maxRange !== minRange;

  // Thresholds are only meaningful against a real reading, so they are evaluated only when the
  // PV is displayable. A missing reading never trips a limit.
  const value: number | null = pv ?? null;
  const canJudgeLimits = displayable && value !== null && Number.isFinite(value);
  const isSensorFault = live && sensorFault === true;
  const exceedsHighHigh = canJudgeLimits && highHighLimit != null && value >= highHighLimit;
  const exceedsHigh = canJudgeLimits && highLimit != null && value >= highLimit;
  const belowLowLow = canJudgeLimits && lowLowLimit != null && value <= lowLowLimit;
  const belowLow = canJudgeLimits && lowLimit != null && value <= lowLimit;

  const alarmLabel =
    alarm ??
    (isSensorFault
      ? 'DIAGNOSTIC'
      : exceedsHighHigh || belowLowLow
      ? 'CRITICAL'
      : exceedsHigh
      ? 'MAJOR'
      : belowLow
      ? 'MINOR'
      : 'NORMAL');
  const liveAlarmActive = isSensorFault || exceedsHigh || belowLow;
  const showAlarm = live && (alarmActive ?? liveAlarmActive);
  const showUnacknowledged = live && (alarmUnacknowledged ?? liveAlarmActive);

  const pvText = formatProcessValue(pv, quality, 2);
  const pvCompact = formatProcessValue(pv, quality, 1);
  const rangeBarPercent = hasRange
    ? processBarPercent(pv, quality, maxRange as number, minRange as number)
    : 0;
  const loopCurrentText = hasRange
    ? formatProcessValue(4 + (rangeBarPercent / 100) * 16, quality, 2)
    : NO_DATA_TEXT;

  /** A configuration value the design genuinely stores, or `----` when it stores none. */
  const configText = (v: number | null | undefined): string =>
    v === null || v === undefined ? NO_DATA_TEXT : String(v);

  const handleTierSwitch = (newTier: FaceplateTier) => {
    setActiveTier(newTier);
    onTierChange?.(newTier);
  };

  const getAlarmColor = () => {
    if (!live) return 'var(--border-default, var(--border-default))';
    if (isSensorFault) return 'var(--alarm-diagnostic, var(--alarm-diagnostic))';
    if (alarmLabel === 'CRITICAL') return 'var(--alarm-critical, var(--alarm-critical))';
    if (alarmLabel === 'MAJOR') return 'var(--alarm-major, var(--alarm-major))';
    if (alarmLabel === 'MINOR') return 'var(--alarm-minor, var(--alarm-minor))';
    return 'var(--border-default, var(--border-default))';
  };

  // Tier 1: ISA-101 Transmitter Circle Bubble
  const renderTier1 = () => (
    <div
      data-testid="analog-tier-1"
      className={`relative inline-flex flex-col items-center justify-center p-2 cursor-pointer select-none group ${className}`}
      onClick={() => handleTierSwitch(2)}
      title={`${tag} (${label}) - Click to inspect`}
    >
      <div
        className={`relative flex items-center justify-center w-14 h-14 rounded-full border-2 bg-slate-900 transition-all duration-200 ${
          showAlarm && showUnacknowledged ? 'isa-alarm-flash ring-2 ring-red-500' : ''
        }`}
        style={{ borderColor: getAlarmColor() }}
      >
        {/* Transmitter Instrument Circle */}
        <div className="flex flex-col items-center justify-center text-center">
          <span className="font-mono text-[10px] font-bold text-sky-400 leading-tight">{tag}</span>
          <div className="w-8 h-[1px] bg-slate-700 my-0.5" />
          <span className="font-mono text-[10px] text-slate-200 leading-tight">
            {displayable ? (isSensorFault ? 'FLT' : pvCompact) : NO_DATA_TEXT}
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
      <span className="mt-1 font-mono text-[10px] text-slate-400">
        {displayable ? unit : NO_DATA_TEXT}
      </span>
      <DataQualityBadge quality={quality} className="mt-0.5" />
    </div>
  );

  // Tier 2: Summary Card
  const renderTier2 = () => (
    <div
      data-testid="analog-tier-2"
      className={`flex flex-col w-80 bg-slate-900/95 border border-slate-700/80 rounded-lg p-3 shadow-xl backdrop-blur-sm text-slate-100 ${className}`}
      style={{ borderTop: `3px solid ${getAlarmColor()}` }}
    >
      <div className="flex items-center justify-between pb-2 mb-2 border-b border-slate-800">
        <div className="flex flex-col">
          <span className="font-mono text-sm font-bold text-sky-400 tracking-wide">{tag}</span>
          <span className="text-xs text-slate-400 truncate max-w-[170px]" title={label}>{label}</span>
        </div>
        <div className="flex items-center gap-1.5">
          <DataQualityBadge quality={quality} />
          {transmitterType && (
            <span
              data-testid="transmitter-type"
              className="px-1.5 py-0.5 rounded text-[10px] font-mono font-semibold bg-slate-800 text-slate-300 border border-slate-700"
            >
              {transmitterType}
            </span>
          )}
          <button
            onClick={() => handleTierSwitch(3)}
            className="px-2 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs border border-slate-700 transition-colors"
            data-testid="btn-open-diagnostic"
          >
            Diag
          </button>
        </div>
      </div>

      {/* Primary Value Readout */}
      <div className="flex items-baseline justify-between py-1">
        <div className="flex flex-col">
          <span className="text-[11px] uppercase tracking-wider text-slate-400">Process Variable</span>
          <div className="flex items-baseline gap-1.5">
            <span data-testid="transmitter-pv" className="font-mono text-3xl font-bold text-slate-100 tabular-nums">
              {displayable && !isSensorFault ? pvText : NO_DATA_TEXT}
            </span>
            <span className="text-sm font-mono text-slate-400">{displayable ? unit ?? '' : ''}</span>
          </div>
        </div>
        {live && isSensorFault && (
          <span className="px-2 py-1 bg-blue-950 border border-blue-700 text-blue-300 text-xs font-mono font-bold rounded">
            SENSOR FAULT
          </span>
        )}
      </div>

      {/* Range Bar with Limit Markers */}
      <div className="relative w-full bg-slate-800 h-3 rounded-full my-2 border border-slate-700/60 overflow-hidden">
        <div
          className={`h-full transition-all duration-300 ${
            showAlarm ? 'bg-amber-500' : 'bg-sky-500'
          }`}
          style={{ width: `${rangeBarPercent}%` }}
        />
      </div>

      {/* Range Limits Indicators. A range the design does not store is shown as `----`, never
          as an invented 0..100 span. */}
      <div className="flex justify-between text-[10px] font-mono text-slate-500 px-0.5">
        <span>{configText(minRange)} {minRange !== null ? unit : ''}</span>
        <div className="flex gap-2 text-slate-400">
          <span>L: {configText(lowLimit)}</span>
          <span>H: {configText(highLimit)}</span>
        </div>
        <span>{configText(maxRange)} {maxRange !== null ? unit : ''}</span>
      </div>
    </div>
  );

  // Tier 3: Diagnostic Modal / Drawer
  const renderTier3 = () => (
    <div
      data-testid="analog-tier-3"
      className={`w-96 max-w-full bg-slate-900 border border-slate-700 rounded-lg shadow-2xl p-4 text-slate-100 flex flex-col gap-3 ${className}`}
    >
      <div className="flex items-center justify-between border-b border-slate-800 pb-2">
        <div className="flex flex-col">
          <div className="flex items-center gap-2">
            <span className="font-mono text-base font-bold text-sky-400">{tag}</span>
            <span className="text-xs px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-300 font-mono">
              FB_Analog_Transmitter
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

      {/* 4-20mA Diagnostic Telemetry */}
      <div className="bg-slate-950/60 p-2.5 rounded border border-slate-800 flex justify-between items-center text-xs">
        <span className="text-slate-400">Loop Current (4-20mA):</span>
        <span className="font-mono font-bold text-slate-200">{displayable ? `${loopCurrentText} mA` : loopCurrentText}</span>
      </div>

      {/* Limit Table */}
      <div className="flex flex-col gap-1 bg-slate-950/40 p-2.5 rounded border border-slate-800 text-xs">
        <span className="text-xs font-semibold uppercase tracking-wider text-slate-400 mb-1">
          Alarm Setpoint Thresholds
        </span>
        <div className="grid grid-cols-2 gap-2">
          <div className="flex justify-between p-1 bg-slate-900 rounded">
            <span className="text-rose-400">High-High (HH):</span>
            <span className="font-mono text-slate-200">{configText(highHighLimit)} {highHighLimit !== null ? unit : ''}</span>
          </div>
          <div className="flex justify-between p-1 bg-slate-900 rounded">
            <span className="text-amber-400">High (H):</span>
            <span className="font-mono text-slate-200">{configText(highLimit)} {highLimit !== null ? unit : ''}</span>
          </div>
          <div className="flex justify-between p-1 bg-slate-900 rounded">
            <span className="text-yellow-400">Low (L):</span>
            <span className="font-mono text-slate-200">{configText(lowLimit)} {lowLimit !== null ? unit : ''}</span>
          </div>
          <div className="flex justify-between p-1 bg-slate-900 rounded">
            <span className="text-rose-400">Low-Low (LL):</span>
            <span className="font-mono text-slate-200">{configText(lowLowLimit)} {lowLowLimit !== null ? unit : ''}</span>
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
