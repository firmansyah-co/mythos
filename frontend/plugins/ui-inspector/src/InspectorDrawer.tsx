import React, { useState } from 'react';
import {
  InspectorDrawerProps,
  InspectorTabType,
  InterlockCondition,
  MiniTrendPoint,
  DeviceMaintenanceInfo,
  ManualControlState,
  InterlockStatus,
} from './types';
import {
  DEFAULT_INSPECTOR_QUALITY,
  NO_DATA_TEXT,
  hasAnyField,
  inspectorIsLive,
  inspectorNoDataReason,
  inspectorQualityLabel,
  inspectorText,
  inspectorValue,
} from './deviceData';
import {
  X,
  Sliders,
  CheckCircle2,
  XCircle,
  AlertOctagon,
  HelpCircle,
  TrendingUp,
  Wrench,
  RotateCw,
  RotateCcw,
  ShieldCheck,
  Cpu,
  Clock,
  Radio,
  Calendar,
  Activity,
} from 'lucide-react';

/**
 * InspectorDrawer — slide-over diagnostic surface.
 *
 * SYS-REQ-096: this component holds no default process values, no default interlock states, no
 * default device identity and no synthetic trend. Everything it shows comes from the caller, and
 * everything is gated on `quality`. When there is no live device data it says so in words and
 * renders `----` for each reading, instead of presenting an invented plant.
 */

/** Quiescent banner shown whenever the drawer has no live device data. */
const NoDataBanner: React.FC<{ quality: InspectorDrawerProps['quality'] }> = ({ quality }) => {
  const reason = inspectorNoDataReason(quality);
  if (!reason) return null;
  return (
    <div
      data-testid="inspector-no-data-banner"
      data-quality={quality}
      role="status"
      className="p-2.5 rounded-lg border border-neutral-600 bg-neutral-800/80 text-[11px] leading-snug text-neutral-300"
    >
      <div className="font-mono font-semibold tracking-wider text-neutral-200 mb-0.5">
        {inspectorQualityLabel(quality)}
      </div>
      <div>{reason}</div>
    </div>
  );
};

/** Empty state for a tab that has nothing real to show. */
const EmptyTabState: React.FC<{ title: string; detail: string; testId: string }> = ({ title, detail, testId }) => (
  <div
    data-testid={testId}
    className="p-3 rounded-lg border border-neutral-700 bg-[var(--bg-elevated)] text-[11px] leading-snug text-neutral-300"
  >
    <div className="font-semibold text-neutral-200 mb-0.5">{title}</div>
    <div className="text-neutral-400">{detail}</div>
  </div>
);

const INTERLOCK_PRESENTATION: Record<
  InterlockStatus,
  { label: string; className: string; Icon: React.FC<{ className?: string }>; cardClass: string }
> = {
  SATISFIED: {
    label: '[✓] Permissive Satisfied',
    className: 'bg-emerald-950 text-emerald-400 border border-emerald-500/30',
    Icon: CheckCircle2,
    cardClass: 'bg-[var(--bg-elevated)] border-[var(--bg-highlight)]',
  },
  TRIPPED: {
    label: '[✗] Interlock Tripped',
    className: 'bg-rose-950 text-rose-400 border border-rose-500/30',
    Icon: XCircle,
    cardClass: 'bg-rose-950/40 border-rose-600/50',
  },
  BYPASSED: {
    label: '[!] Interlock Bypassed',
    className: 'bg-amber-950 text-amber-400 border border-amber-500/30',
    Icon: AlertOctagon,
    cardClass: 'bg-amber-950/30 border-amber-600/40',
  },
  UNKNOWN: {
    label: '[?] State Unknown',
    className: 'bg-neutral-800 text-neutral-300 border border-neutral-500/40',
    Icon: HelpCircle,
    cardClass: 'bg-[var(--bg-elevated)] border-neutral-600',
  },
};

export const InspectorDrawer: React.FC<InspectorDrawerProps> = ({
  isOpen,
  onClose,
  quality = DEFAULT_INSPECTOR_QUALITY,
  deviceId,
  deviceName,
  deviceType,
  initialTab = 'MANUAL_CONTROL',
  controlState,
  interlocks,
  trendHistory,
  maintenanceInfo,
  onSetpointChange,
  onModeToggle,
  onJogForward,
  onJogReverse,
  onAcknowledgeInterlock,
  className = '',
}) => {
  const [activeTab, setActiveTab] = useState<InspectorTabType>(initialTab);
  const [ctrl, setCtrl] = useState<ManualControlState>(controlState ?? {});
  const [jogFwdActive, setJogFwdActive] = useState(false);
  const [jogRevActive, setJogRevActive] = useState(false);

  // Only live telemetry may be presented as a reading. Local control intent (a slider the
  // operator is dragging) is separate from device feedback and is handled below.
  const live = inspectorIsLive(quality);
  const reason = inspectorNoDataReason(quality);

  if (!isOpen) return null;

  const handleSetpointSlider = (val: number) => {
    setCtrl((prev) => ({ ...prev, setpoint: val }));
    onSetpointChange?.(val);
  };

  const handleToggleAuto = () => {
    if (!live) return;
    const nextAuto = !ctrl.isAutoMode;
    setCtrl((prev) => ({ ...prev, isAutoMode: nextAuto }));
    onModeToggle?.(nextAuto);
  };

  const unit = ctrl.engineeringUnit ?? '';
  const processValueText = inspectorValue(ctrl.processValue, quality, 1);
  const outputPowerText = inspectorValue(ctrl.outputPowerPct, quality, 1);
  const setpointText = inspectorValue(ctrl.setpoint, quality, 1);

  // A control mode is device state. `undefined` is falsy, so `ctrl.isAutoMode ? AUTO : MANUAL`
  // would silently report MANUAL for a device that never reported a mode — the same fabrication
  // the faceplates had. The mode is announced only when a real boolean was supplied.
  const hasMode = ctrl.isAutoMode === true || ctrl.isAutoMode === false;
  const modeKnown = live && hasMode;
  const modeText = !modeKnown ? NO_DATA_TEXT : ctrl.isAutoMode ? 'CASCADE / AUTO' : 'HAND / MANUAL';

  // Trend: draw only real samples. No synthetic walk, no invented sample rate.
  const trend: MiniTrendPoint[] = live && Array.isArray(trendHistory) ? trendHistory : [];
  const hasTrend = trend.length >= 2;
  const svgWidth = 340;
  const svgHeight = 140;
  const minVal = 0;
  const maxVal = 100;

  const getSvgY = (val: number) => {
    const clamped = Math.max(minVal, Math.min(maxVal, val));
    return svgHeight - ((clamped - minVal) / (maxVal - minVal)) * (svgHeight - 16) - 8;
  };

  const pvPolyline = hasTrend
    ? trend.map((pt, i) => `${(i / (trend.length - 1)) * svgWidth},${getSvgY(pt.pv)}`).join(' ')
    : '';

  const spPolyline = hasTrend
    ? trend.map((pt, i) => `${(i / (trend.length - 1)) * svgWidth},${getSvgY(pt.sp)}`).join(' ')
    : '';

  const loopErrorText =
    live && ctrl.setpoint !== null && ctrl.setpoint !== undefined &&
    ctrl.processValue !== null && ctrl.processValue !== undefined
      ? inspectorValue(ctrl.setpoint - ctrl.processValue, quality, 2)
      : NO_DATA_TEXT;

  const maintenance: DeviceMaintenanceInfo = live ? maintenanceInfo ?? {} : {};
  const hasMaintenance = hasAnyField(maintenance as Record<string, unknown>);

  const interlockList: InterlockCondition[] = live && Array.isArray(interlocks) ? interlocks : [];

  return (
    <div
      role="complementary"
      aria-label="Inspector Drawer"
      style={{
        width: 384,
        minWidth: 384,
        maxWidth: 384,
        flex: '0 0 384px',
      }}
      className={`h-full bg-[var(--bg-surface)] border-l border-[var(--bg-highlight)] flex flex-col z-30 shadow-2xl select-none font-sans overflow-hidden transition-all duration-200 ${className}`}
    >
      {/* 1. Header Bar: Device ID, Name & Close Button. Identity strings are shown only when the
             caller supplied them; the drawer never invents a device. */}
      <div className="flex items-center justify-between px-3.5 py-2.5 border-b border-[var(--bg-highlight)] bg-[var(--bg-elevated)]">
        <div className="flex items-center gap-2 overflow-hidden">
          <div className="p-1 rounded bg-[var(--bg-base)] border border-[var(--bg-highlight)]">
            <Cpu className="w-4 h-4 text-sky-400" />
          </div>
          <div className="truncate">
            <h2
              data-testid="inspector-device-name"
              className="text-xs font-bold text-[var(--text-primary)] truncate leading-tight"
            >
              {deviceName && deviceName.trim() !== '' ? deviceName : NO_DATA_TEXT}
            </h2>
            <div className="flex items-center gap-1.5 text-[10px] text-[var(--text-secondary)] font-mono">
              <span data-testid="inspector-device-id" className="text-sky-400">
                {deviceId && deviceId.trim() !== '' ? deviceId : NO_DATA_TEXT}
              </span>
              <span>•</span>
              <span data-testid="inspector-device-type" className="truncate">
                {deviceType && deviceType.trim() !== '' ? deviceType : NO_DATA_TEXT}
              </span>
            </div>
          </div>
        </div>

        <button
          type="button"
          onClick={onClose}
          aria-label="Close Inspector Drawer"
          className="p-1 rounded-md text-[var(--text-secondary)] hover:text-white hover:bg-[var(--bg-highlight)] transition-colors"
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      {/* 2. Standardized 4 Tabs Navigation Bar */}
      <div className="grid grid-cols-4 border-b border-[var(--bg-highlight)] bg-[var(--bg-base)]">
        <button
          type="button"
          onClick={() => setActiveTab('MANUAL_CONTROL')}
          className={`flex flex-col items-center justify-center py-2 text-[10px] font-medium transition-all ${
            activeTab === 'MANUAL_CONTROL'
              ? 'text-sky-400 border-b-2 border-sky-400 bg-[var(--bg-elevated)]'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface)]'
          }`}
        >
          <Sliders className="w-3.5 h-3.5 mb-0.5" />
          <span>Manual</span>
        </button>

        <button
          type="button"
          onClick={() => setActiveTab('INTERLOCK_MATRIX')}
          className={`flex flex-col items-center justify-center py-2 text-[10px] font-medium transition-all ${
            activeTab === 'INTERLOCK_MATRIX'
              ? 'text-sky-400 border-b-2 border-sky-400 bg-[var(--bg-elevated)]'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface)]'
          }`}
        >
          <ShieldCheck className="w-3.5 h-3.5 mb-0.5" />
          <span>Interlocks</span>
        </button>

        <button
          type="button"
          onClick={() => setActiveTab('MINI_TREND')}
          className={`flex flex-col items-center justify-center py-2 text-[10px] font-medium transition-all ${
            activeTab === 'MINI_TREND'
              ? 'text-sky-400 border-b-2 border-sky-400 bg-[var(--bg-elevated)]'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface)]'
          }`}
        >
          <TrendingUp className="w-3.5 h-3.5 mb-0.5" />
          <span>Trend</span>
        </button>

        <button
          type="button"
          onClick={() => setActiveTab('MAINTENANCE')}
          className={`flex flex-col items-center justify-center py-2 text-[10px] font-medium transition-all ${
            activeTab === 'MAINTENANCE'
              ? 'text-sky-400 border-b-2 border-sky-400 bg-[var(--bg-elevated)]'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-surface)]'
          }`}
        >
          <Wrench className="w-3.5 h-3.5 mb-0.5" />
          <span>Maint</span>
        </button>
      </div>

      {/* 3. Main Drawer Body (Scrollable containment, 2-Column Grid) */}
      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        {reason && <NoDataBanner quality={quality} />}

        {/* TAB 1: MANUAL CONTROL */}
        {activeTab === 'MANUAL_CONTROL' && (
          <div className="space-y-3">
            {/* Auto / Manual Mode Pill Bar */}
            <div className="flex items-center justify-between p-2 rounded-lg bg-[var(--bg-elevated)] border border-[var(--bg-highlight)]">
              <div className="flex items-center gap-2">
                <span className="text-xs font-semibold text-[var(--text-primary)]">Control Mode</span>
                <span
                  data-testid="inspector-control-mode"
                  className={`text-[10px] font-mono px-2 py-0.5 rounded font-bold ${
                    !modeKnown
                      ? 'bg-neutral-800 text-neutral-300 border border-neutral-600'
                      : ctrl.isAutoMode
                      ? 'bg-emerald-950 text-emerald-400 border border-emerald-500/40'
                      : 'bg-amber-950 text-amber-400 border border-amber-500/40'
                  }`}
                >
                  {modeText}
                </span>
              </div>
              <button
                type="button"
                onClick={handleToggleAuto}
                disabled={!modeKnown}
                title={modeKnown ? undefined : 'Device has not reported a control mode'}
                data-testid="inspector-mode-toggle"
                className="px-2.5 py-1 rounded bg-[var(--bg-highlight)] hover:bg-sky-600 text-white text-[11px] font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {modeKnown ? `Switch to ${ctrl.isAutoMode ? 'Manual' : 'Auto'}` : 'Mode unavailable'}
              </button>
            </div>

            {/* Modular 2-Column Grid (184px x 96px half-width cards) */}
            <div className="grid grid-cols-2 gap-2">
              {/* Half-width Card 1: Process Value (PV) */}
              <div className="w-[172px] h-[88px] p-2.5 rounded-lg bg-[var(--bg-elevated)] border border-[var(--bg-highlight)] flex flex-col justify-between">
                <div className="flex items-center justify-between text-[10px] font-mono uppercase text-[var(--text-secondary)]">
                  <span>Process Value</span>
                  <Activity className="w-3 h-3 text-emerald-400" />
                </div>
                <div className="flex items-baseline gap-1">
                  <span
                    data-testid="inspector-process-value"
                    className="text-2xl font-bold font-mono text-[var(--text-primary)]"
                  >
                    {processValueText}
                  </span>
                  {live && unit && <span className="text-xs font-mono text-[var(--text-secondary)]">{unit}</span>}
                </div>
                <div className="w-full bg-[var(--bg-base)] h-1.5 rounded-full overflow-hidden">
                  <div
                    className="bg-emerald-400 h-full rounded-full transition-all duration-300"
                    style={{ width: `${live ? Math.min(100, Math.max(0, ctrl.processValue ?? 0)) : 0}%` }}
                  />
                </div>
              </div>

              {/* Half-width Card 2: Output Power */}
              <div className="w-[172px] h-[88px] p-2.5 rounded-lg bg-[var(--bg-elevated)] border border-[var(--bg-highlight)] flex flex-col justify-between">
                <div className="flex items-center justify-between text-[10px] font-mono uppercase text-[var(--text-secondary)]">
                  <span>VFD Output</span>
                  <span className="text-[9px] font-mono text-sky-400">4-20mA</span>
                </div>
                <div className="flex items-baseline gap-1">
                  <span data-testid="inspector-output-power" className="text-2xl font-bold font-mono text-sky-400">
                    {outputPowerText}
                  </span>
                  {live && <span className="text-xs font-mono text-[var(--text-secondary)]">%</span>}
                </div>
                <div className="w-full bg-[var(--bg-base)] h-1.5 rounded-full overflow-hidden">
                  <div
                    className="bg-sky-400 h-full rounded-full transition-all duration-300"
                    style={{ width: `${live ? ctrl.outputPowerPct ?? 0 : 0}%` }}
                  />
                </div>
              </div>
            </div>

            {/* Setpoint (SP) Slider Card (Full-width 360px x 96px) */}
            <div className="p-3 rounded-lg bg-[var(--bg-elevated)] border border-[var(--bg-highlight)] space-y-2">
              <div className="flex items-center justify-between text-xs font-medium">
                <span className="text-[var(--text-primary)]">Target Setpoint (SP)</span>
                <span data-testid="inspector-setpoint" className="font-mono text-amber-400 font-bold">
                  {live ? `${setpointText} ${unit}`.trim() : NO_DATA_TEXT}
                </span>
              </div>
              <input
                type="range"
                min={ctrl.minSetpoint ?? 0}
                max={ctrl.maxSetpoint ?? 100}
                step={0.5}
                disabled={!live || ctrl.isAutoMode === true}
                value={live ? ctrl.setpoint ?? 0 : 0}
                onChange={(e) => handleSetpointSlider(parseFloat(e.target.value))}
                className="w-full h-2 bg-[var(--bg-base)] rounded-lg appearance-none cursor-pointer accent-sky-400 disabled:opacity-40"
              />
              <div className="flex justify-between text-[10px] text-[var(--text-secondary)] font-mono">
                <span>{live ? `${ctrl.minSetpoint ?? NO_DATA_TEXT} ${unit}`.trim() : NO_DATA_TEXT}</span>
                <span>{live ? `${ctrl.maxSetpoint ?? NO_DATA_TEXT} ${unit}`.trim() : NO_DATA_TEXT}</span>
              </div>
            </div>

            {/* Tactile Jog Pushbuttons (Half-width 2-column) */}
            <div className="grid grid-cols-2 gap-2">
              <button
                type="button"
                disabled={!live || ctrl.isAutoMode === true}
                onMouseDown={() => {
                  setJogFwdActive(true);
                  onJogForward?.(true);
                }}
                onMouseUp={() => {
                  setJogFwdActive(false);
                  onJogForward?.(false);
                }}
                onMouseLeave={() => {
                  if (jogFwdActive) {
                    setJogFwdActive(false);
                    onJogForward?.(false);
                  }
                }}
                data-testid="inspector-jog-fwd"
                className={`h-[48px] rounded-lg border font-mono font-bold text-xs flex items-center justify-center gap-1.5 transition-all active:scale-95 ${
                  jogFwdActive
                    ? 'bg-emerald-600 border-emerald-400 text-white shadow-[0_0_12px_rgba(16,185,129,0.5)]'
                    : 'bg-[var(--bg-elevated)] border-[var(--bg-highlight)] text-emerald-400 hover:border-emerald-500/50 disabled:opacity-40'
                }`}
              >
                <RotateCw className={`w-4 h-4 ${jogFwdActive ? 'animate-spin' : ''}`} />
                <span>JOG FWD</span>
              </button>

              <button
                type="button"
                disabled={!live || ctrl.isAutoMode === true}
                onMouseDown={() => {
                  setJogRevActive(true);
                  onJogReverse?.(true);
                }}
                onMouseUp={() => {
                  setJogRevActive(false);
                  onJogReverse?.(false);
                }}
                onMouseLeave={() => {
                  if (jogRevActive) {
                    setJogRevActive(false);
                    onJogReverse?.(false);
                  }
                }}
                data-testid="inspector-jog-rev"
                className={`h-[48px] rounded-lg border font-mono font-bold text-xs flex items-center justify-center gap-1.5 transition-all active:scale-95 ${
                  jogRevActive
                    ? 'bg-amber-600 border-amber-400 text-white shadow-[0_0_12px_rgba(245,158,11,0.5)]'
                    : 'bg-[var(--bg-elevated)] border-[var(--bg-highlight)] text-amber-400 hover:border-amber-500/50 disabled:opacity-40'
                }`}
              >
                <RotateCcw className={`w-4 h-4 ${jogRevActive ? 'animate-spin' : ''}`} />
                <span>JOG REV</span>
              </button>
            </div>
          </div>
        )}

        {/* TAB 2: INTERLOCK MATRIX CHECKLIST */}
        {activeTab === 'INTERLOCK_MATRIX' && (
          <div className="space-y-2">
            <div className="flex items-center justify-between text-xs font-medium text-[var(--text-secondary)] pb-1 border-b border-[var(--bg-highlight)]">
              <span>Permissive Safety Logic Matrix</span>
            </div>

            {!live && (
              <EmptyTabState
                testId="inspector-interlocks-empty"
                title="Interlock states unavailable"
                detail={
                  reason ??
                  'No device data. Interlock states are unknown, not satisfied — this matrix cannot confirm a safe condition.'
                }
              />
            )}

            {live && interlockList.length === 0 && (
              <EmptyTabState
                testId="inspector-interlocks-empty"
                title="No interlock telemetry"
                detail="No interlock is bound to this device, so no permissive state can be reported."
              />
            )}

            {live &&
              interlockList.map((item) => {
                const present = INTERLOCK_PRESENTATION[item.status] ?? INTERLOCK_PRESENTATION.UNKNOWN;
                const { Icon } = present;
                return (
                  <div
                    key={item.id}
                    data-testid="inspector-interlock-row"
                    data-status={item.status}
                    className={`p-2.5 rounded-lg border transition-all ${present.cardClass}`}
                  >
                    <div className="flex items-start justify-between">
                      <div className="flex items-center gap-1.5">
                        <Icon
                          className={`w-4 h-4 shrink-0 ${
                            item.status === 'SATISFIED'
                              ? 'text-emerald-400'
                              : item.status === 'TRIPPED'
                              ? 'text-rose-500'
                              : item.status === 'BYPASSED'
                              ? 'text-amber-400'
                              : 'text-neutral-400'
                          }`}
                        />
                        <span className="text-xs font-semibold text-[var(--text-primary)]">{item.name}</span>
                      </div>

                      <span className={`text-[9px] font-mono px-1.5 py-0.5 rounded font-bold ${present.className}`}>
                        {present.label}
                      </span>
                    </div>

                    <div className="text-[10px] font-mono text-sky-400 mt-1 pl-5">{item.expression}</div>
                    {item.description && (
                      <div className="text-[10px] text-[var(--text-secondary)] mt-0.5 pl-5">{item.description}</div>
                    )}

                    {item.status === 'TRIPPED' && (
                      <div className="mt-2 pl-5 flex justify-end">
                        <button
                          type="button"
                          onClick={() => onAcknowledgeInterlock?.(item.id)}
                          className="px-2 py-0.5 rounded bg-rose-600 hover:bg-rose-500 text-white text-[10px] font-mono font-medium transition-colors"
                        >
                          Acknowledge &amp; Clear
                        </button>
                      </div>
                    )}
                  </div>
                );
              })}
          </div>
        )}

        {/* TAB 3: MINI TREND (PV vs SP from real samples only) */}
        {activeTab === 'MINI_TREND' && (
          <div className="space-y-3">
            {!hasTrend ? (
              <EmptyTabState
                testId="inspector-trend-empty"
                title="No trend data"
                detail={
                  reason ??
                  'No trend samples have been supplied for this device. Nothing is plotted, so no curve is shown.'
                }
              />
            ) : (
              <div className="p-3 rounded-lg bg-[var(--bg-elevated)] border border-[var(--bg-highlight)]">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-xs font-semibold text-[var(--text-primary)]">PV vs SP</span>
                  <div className="flex items-center gap-3 text-[10px] font-mono">
                    <span className="flex items-center gap-1 text-emerald-400">
                      <span className="w-2 h-0.5 bg-emerald-400 inline-block" /> PV ({processValueText})
                    </span>
                    <span className="flex items-center gap-1 text-amber-400">
                      <span className="w-2 h-0.5 bg-amber-400 border-dashed inline-block" /> SP ({setpointText})
                    </span>
                  </div>
                </div>

                <div className="w-full bg-[var(--bg-base)] rounded border border-[var(--bg-highlight)] p-2 relative overflow-hidden">
                  <svg width={svgWidth} height={svgHeight} className="overflow-visible">
                    <line x1="0" y1="20" x2={svgWidth} y2="20" stroke="var(--border-subtle)" strokeDasharray="2,2" />
                    <line
                      x1="0"
                      y1={svgHeight / 2}
                      x2={svgWidth}
                      y2={svgHeight / 2}
                      stroke="var(--border-subtle)"
                      strokeDasharray="2,2"
                    />
                    <line
                      x1="0"
                      y1={svgHeight - 20}
                      x2={svgWidth}
                      y2={svgHeight - 20}
                      stroke="var(--border-subtle)"
                      strokeDasharray="2,2"
                    />

                    <polyline
                      fill="none"
                      stroke="var(--collab-amber)"
                      strokeWidth="1.5"
                      strokeDasharray="4,3"
                      points={spPolyline}
                    />
                    <polyline
                      fill="none"
                      stroke="var(--collab-emerald)"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      points={pvPolyline}
                    />
                  </svg>

                  <div className="flex justify-between text-[9px] font-mono text-[var(--text-muted)] mt-1.5">
                    <span>{new Date(trend[0].timestamp).toLocaleTimeString([], { hour12: false })}</span>
                    <span>{trend.length} samples</span>
                    <span>{new Date(trend[trend.length - 1].timestamp).toLocaleTimeString([], { hour12: false })}</span>
                  </div>
                </div>
              </div>
            )}

            {/* Quick KPI stats under trend */}
            <div className="grid grid-cols-2 gap-2">
              <div className="p-2 rounded bg-[var(--bg-elevated)] border border-[var(--bg-highlight)]">
                <div className="text-[10px] text-[var(--text-secondary)] font-mono">Loop Error (SP - PV)</div>
                <div className="text-base font-mono font-bold text-sky-400">{loopErrorText}</div>
              </div>

              <div className="p-2 rounded bg-[var(--bg-elevated)] border border-[var(--bg-highlight)]">
                <div className="text-[10px] text-[var(--text-secondary)] font-mono">Samples</div>
                <div className="text-base font-mono font-bold text-[var(--text-primary)]">
                  {hasTrend ? trend.length : NO_DATA_TEXT}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* TAB 4: MAINTENANCE & DCP / ASSET INFO */}
        {activeTab === 'MAINTENANCE' && (
          <div className="space-y-2.5">
            {!hasMaintenance ? (
              <EmptyTabState
                testId="inspector-maintenance-empty"
                title="No maintenance record"
                detail={
                  reason ??
                  'This device has not reported asset, service or calibration data. Nothing is shown rather than a plausible record.'
                }
              />
            ) : (
              <>
                {/* Odometer Hours Card */}
                <div className="p-3 rounded-lg bg-[var(--bg-elevated)] border border-[var(--bg-highlight)] flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Clock className="w-5 h-5 text-sky-400" />
                    <div>
                      <div className="text-xs font-semibold text-[var(--text-primary)]">Total Operating Hours</div>
                      <div className="text-[10px] text-[var(--text-secondary)]">Non-volatile EEPROM accumulator</div>
                    </div>
                  </div>
                  <div data-testid="inspector-operating-hours" className="text-xl font-bold font-mono text-sky-400">
                    {maintenance.operatingHours !== null && maintenance.operatingHours !== undefined
                      ? `${maintenance.operatingHours.toLocaleString()} h`
                      : NO_DATA_TEXT}
                  </div>
                </div>

                {/* DCP & Hardware Spec Attributes */}
                <div className="p-3 rounded-lg bg-[var(--bg-elevated)] border border-[var(--bg-highlight)] space-y-2 text-xs">
                  <div className="flex items-center justify-between border-b border-[var(--bg-highlight)] pb-1.5">
                    <span className="text-[var(--text-secondary)] flex items-center gap-1.5">
                      <Radio className="w-3.5 h-3.5 text-emerald-400" /> DCP MAC Address
                    </span>
                    <span data-testid="inspector-dcp-mac" className="font-mono text-[var(--text-primary)] font-medium">
                      {inspectorText(maintenance.dcpMacAddress, quality)}
                    </span>
                  </div>

                  <div className="flex items-center justify-between border-b border-[var(--bg-highlight)] pb-1.5">
                    <span className="text-[var(--text-secondary)]">Serial Number</span>
                    <span data-testid="inspector-serial" className="font-mono text-[var(--text-primary)]">
                      {inspectorText(maintenance.serialNumber, quality)}
                    </span>
                  </div>

                  <div className="flex items-center justify-between border-b border-[var(--bg-highlight)] pb-1.5">
                    <span className="text-[var(--text-secondary)]">Firmware Build</span>
                    <span data-testid="inspector-firmware" className="font-mono text-sky-400">
                      {inspectorText(maintenance.firmwareVersion, quality)}
                    </span>
                  </div>

                  <div className="flex items-center justify-between border-b border-[var(--bg-highlight)] pb-1.5">
                    <span className="text-[var(--text-secondary)] flex items-center gap-1.5">
                      <Calendar className="w-3.5 h-3.5 text-amber-400" /> Last Calibration
                    </span>
                    <span data-testid="inspector-last-calibration" className="font-mono text-[var(--text-primary)]">
                      {inspectorText(maintenance.lastCalibrationDate, quality)}
                    </span>
                  </div>

                  <div className="flex items-center justify-between border-b border-[var(--bg-highlight)] pb-1.5">
                    <span className="text-[var(--text-secondary)]">Next Due Date</span>
                    <span data-testid="inspector-next-calibration" className="font-mono text-emerald-400 font-semibold">
                      {inspectorText(maintenance.nextCalibrationDate, quality)}
                    </span>
                  </div>

                  <div className="flex items-center justify-between">
                    <span className="text-[var(--text-secondary)]">Total Starts / Inrush Cycles</span>
                    <span data-testid="inspector-total-starts" className="font-mono text-[var(--text-primary)] font-bold">
                      {maintenance.totalStarts !== null && maintenance.totalStarts !== undefined
                        ? maintenance.totalStarts.toLocaleString()
                        : NO_DATA_TEXT}
                    </span>
                  </div>
                </div>
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
};
