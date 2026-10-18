import React, { useState, useMemo } from 'react';
import {
  InspectorDrawerProps,
  InspectorTabType,
  InterlockCondition,
  MiniTrendPoint,
  DeviceMaintenanceInfo,
  ManualControlState,
} from './types';
import {
  X,
  Sliders,
  CheckCircle2,
  XCircle,
  AlertOctagon,
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

const DEFAULT_CONTROL: ManualControlState = {
  isAutoMode: false,
  isJogForward: false,
  isJogReverse: false,
  setpoint: 50.0,
  processValue: 48.7,
  minSetpoint: 0,
  maxSetpoint: 100,
  engineeringUnit: '%',
  outputPowerPct: 62.4,
};

const DEFAULT_INTERLOCKS: InterlockCondition[] = [
  {
    id: 'int-01',
    name: 'Emergency Stop Chain',
    expression: '%I0.0 AND %I0.1 == TRUE',
    isPermissive: true,
    status: 'SATISFIED',
    channelAddress: '%I0.0',
    description: 'Hardware dual-channel safety relays closed',
  },
  {
    id: 'int-02',
    name: 'Motor Thermal Overload',
    expression: 'PTC_TEMP < 95.0 C',
    isPermissive: true,
    status: 'SATISFIED',
    channelAddress: '%IW20',
    description: 'Stator winding temperature in safe zone (62.3 C)',
  },
  {
    id: 'int-03',
    name: 'Suction Line Pressure Permissive',
    expression: 'PT_101 >= 1.5 BAR',
    isPermissive: true,
    status: 'SATISFIED',
    channelAddress: '%IW22',
    description: 'Minimum head pressure verified (2.34 BAR)',
  },
  {
    id: 'int-04',
    name: 'Discharge Isolation Valve Limit Switch',
    expression: 'ZSO_102 == OPEN',
    isPermissive: false,
    status: 'SATISFIED',
    channelAddress: '%I1.2',
    description: 'Full stroke open feedback confirmed',
  },
  {
    id: 'int-05',
    name: 'Seal Flush Lubrication Flow',
    expression: 'FS_105 >= 0.8 L/MIN',
    isPermissive: false,
    status: 'TRIPPED',
    channelAddress: '%IW24',
    description: 'Mechanical seal flush flow low (0.32 L/MIN)',
  },
];

const DEFAULT_MAINTENANCE: DeviceMaintenanceInfo = {
  operatingHours: 4328.5,
  totalStarts: 1420,
  dcpMacAddress: '00:1B:1B:4F:92:0A',
  serialNumber: 'S-VPA2026-99482',
  firmwareVersion: 'V2.9.2-RT',
  lastCalibrationDate: '2026-08-15',
  nextCalibrationDate: '2027-02-15',
  temperatureCelsius: 41.2,
  nominalVoltageV: 24.1,
};

// Generates 60-second synthetic sparkline trend points
function generateSampleTrend(): MiniTrendPoint[] {
  const points: MiniTrendPoint[] = [];
  const now = Date.now();
  let pv = 48.0;
  for (let i = 60; i >= 0; i--) {
    pv += (Math.random() - 0.48) * 1.2;
    points.push({
      timestamp: now - i * 1000,
      pv: Math.max(0, Math.min(100, pv)),
      sp: 50.0,
    });
  }
  return points;
}

export const InspectorDrawer: React.FC<InspectorDrawerProps> = ({
  isOpen,
  onClose,
  deviceId = 'DEV-PUMP-101',
  deviceName = 'Main Feed Pump 101',
  deviceType = 'Sinamics G120 / S7-1515 Drop',
  initialTab = 'MANUAL_CONTROL',
  controlState = {},
  interlocks = DEFAULT_INTERLOCKS,
  trendHistory,
  maintenanceInfo = {},
  onSetpointChange,
  onModeToggle,
  onJogForward,
  onJogReverse,
  onAcknowledgeInterlock,
  className = '',
}) => {
  const [activeTab, setActiveTab] = useState<InspectorTabType>(initialTab);
  const [ctrl, setCtrl] = useState<ManualControlState>({ ...DEFAULT_CONTROL, ...controlState });
  const [maint] = useState<DeviceMaintenanceInfo>({ ...DEFAULT_MAINTENANCE, ...maintenanceInfo });
  const trend = useMemo(() => trendHistory || generateSampleTrend(), [trendHistory]);

  const [jogFwdActive, setJogFwdActive] = useState(false);
  const [jogRevActive, setJogRevActive] = useState(false);

  if (!isOpen) return null;

  const handleSetpointSlider = (val: number) => {
    setCtrl((prev) => ({ ...prev, setpoint: val }));
    onSetpointChange?.(val);
  };

  const handleToggleAuto = () => {
    const nextAuto = !ctrl.isAutoMode;
    setCtrl((prev) => ({ ...prev, isAutoMode: nextAuto }));
    onModeToggle?.(nextAuto);
  };

  // SVG Sparkline Math for 60s Mini Trend
  const minVal = 0;
  const maxVal = 100;
  const svgWidth = 340;
  const svgHeight = 140;

  const getSvgY = (val: number) => {
    const clamped = Math.max(minVal, Math.min(maxVal, val));
    return svgHeight - ((clamped - minVal) / (maxVal - minVal)) * (svgHeight - 16) - 8;
  };

  const pvPolyline = trend
    .map((pt, i) => {
      const x = (i / (trend.length - 1)) * svgWidth;
      const y = getSvgY(pt.pv);
      return `${x},${y}`;
    })
    .join(' ');

  const spY = getSvgY(ctrl.setpoint);

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
      className={`h-full bg-[#0F141C] border-l border-[#232B3B] flex flex-col z-30 shadow-2xl select-none font-sans overflow-hidden transition-all duration-200 ${className}`}
    >
      {/* 1. Header Bar: Device ID, Name & Close Button */}
      <div className="flex items-center justify-between px-3.5 py-2.5 border-b border-[#232B3B] bg-[#161E2E]">
        <div className="flex items-center gap-2 overflow-hidden">
          <div className="p-1 rounded bg-[#0A0D12] border border-[#232B3B]">
            <Cpu className="w-4 h-4 text-sky-400" />
          </div>
          <div className="truncate">
            <h2 className="text-xs font-bold text-[#F7F8F8] truncate leading-tight">{deviceName}</h2>
            <div className="flex items-center gap-1.5 text-[10px] text-[#8A8F98] font-mono">
              <span className="text-sky-400">{deviceId}</span>
              <span>•</span>
              <span className="truncate">{deviceType}</span>
            </div>
          </div>
        </div>

        <button
          type="button"
          onClick={onClose}
          aria-label="Close Inspector Drawer"
          className="p-1 rounded-md text-[#8A8F98] hover:text-white hover:bg-[#232B3B] transition-colors"
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      {/* 2. Standardized 4 Tabs Navigation Bar */}
      <div className="grid grid-cols-4 border-b border-[#232B3B] bg-[#0A0D12]">
        <button
          type="button"
          onClick={() => setActiveTab('MANUAL_CONTROL')}
          className={`flex flex-col items-center justify-center py-2 text-[10px] font-medium transition-all ${
            activeTab === 'MANUAL_CONTROL'
              ? 'text-sky-400 border-b-2 border-sky-400 bg-[#161E2E]'
              : 'text-[#8A8F98] hover:text-[#D0D6E0] hover:bg-[#121824]'
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
              ? 'text-sky-400 border-b-2 border-sky-400 bg-[#161E2E]'
              : 'text-[#8A8F98] hover:text-[#D0D6E0] hover:bg-[#121824]'
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
              ? 'text-sky-400 border-b-2 border-sky-400 bg-[#161E2E]'
              : 'text-[#8A8F98] hover:text-[#D0D6E0] hover:bg-[#121824]'
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
              ? 'text-sky-400 border-b-2 border-sky-400 bg-[#161E2E]'
              : 'text-[#8A8F98] hover:text-[#D0D6E0] hover:bg-[#121824]'
          }`}
        >
          <Wrench className="w-3.5 h-3.5 mb-0.5" />
          <span>Maint</span>
        </button>
      </div>

      {/* 3. Main Drawer Body (Scrollable containment, 2-Column Grid) */}
      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        {/* TAB 1: MANUAL CONTROL */}
        {activeTab === 'MANUAL_CONTROL' && (
          <div className="space-y-3">
            {/* Auto / Manual Mode Pill Bar */}
            <div className="flex items-center justify-between p-2 rounded-lg bg-[#161E2E] border border-[#232B3B]">
              <div className="flex items-center gap-2">
                <span className="text-xs font-semibold text-[#D0D6E0]">Control Mode</span>
                <span
                  className={`text-[10px] font-mono px-2 py-0.5 rounded font-bold ${
                    ctrl.isAutoMode
                      ? 'bg-emerald-950 text-emerald-400 border border-emerald-500/40'
                      : 'bg-amber-950 text-amber-400 border border-amber-500/40'
                  }`}
                >
                  {ctrl.isAutoMode ? 'CASCADE / AUTO' : 'HAND / MANUAL'}
                </span>
              </div>
              <button
                type="button"
                onClick={handleToggleAuto}
                className="px-2.5 py-1 rounded bg-[#232B3B] hover:bg-sky-600 text-white text-[11px] font-medium transition-colors"
              >
                Switch to {ctrl.isAutoMode ? 'Manual' : 'Auto'}
              </button>
            </div>

            {/* Modular 2-Column Grid (184px x 96px half-width cards) */}
            <div className="grid grid-cols-2 gap-2">
              {/* Half-width Card 1: Process Value (PV) */}
              <div className="w-[172px] h-[88px] p-2.5 rounded-lg bg-[#161E2E] border border-[#232B3B] flex flex-col justify-between">
                <div className="flex items-center justify-between text-[10px] font-mono uppercase text-[#8A8F98]">
                  <span>Process Value</span>
                  <Activity className="w-3 h-3 text-emerald-400" />
                </div>
                <div className="flex items-baseline gap-1">
                  <span className="text-2xl font-bold font-mono text-[#F7F8F8]">
                    {ctrl.processValue.toFixed(1)}
                  </span>
                  <span className="text-xs font-mono text-[#8A8F98]">{ctrl.engineeringUnit}</span>
                </div>
                <div className="w-full bg-[#0A0D12] h-1.5 rounded-full overflow-hidden">
                  <div
                    className="bg-emerald-400 h-full rounded-full transition-all duration-300"
                    style={{ width: `${Math.min(100, Math.max(0, ctrl.processValue))}%` }}
                  />
                </div>
              </div>

              {/* Half-width Card 2: Output Power */}
              <div className="w-[172px] h-[88px] p-2.5 rounded-lg bg-[#161E2E] border border-[#232B3B] flex flex-col justify-between">
                <div className="flex items-center justify-between text-[10px] font-mono uppercase text-[#8A8F98]">
                  <span>VFD Output</span>
                  <span className="text-[9px] font-mono text-sky-400">4-20mA</span>
                </div>
                <div className="flex items-baseline gap-1">
                  <span className="text-2xl font-bold font-mono text-sky-400">
                    {ctrl.outputPowerPct.toFixed(1)}
                  </span>
                  <span className="text-xs font-mono text-[#8A8F98]">%</span>
                </div>
                <div className="w-full bg-[#0A0D12] h-1.5 rounded-full overflow-hidden">
                  <div
                    className="bg-sky-400 h-full rounded-full transition-all duration-300"
                    style={{ width: `${ctrl.outputPowerPct}%` }}
                  />
                </div>
              </div>
            </div>

            {/* Setpoint (SP) Slider Card (Full-width 360px x 96px) */}
            <div className="p-3 rounded-lg bg-[#161E2E] border border-[#232B3B] space-y-2">
              <div className="flex items-center justify-between text-xs font-medium">
                <span className="text-[#D0D6E0]">Target Setpoint (SP)</span>
                <span className="font-mono text-amber-400 font-bold">
                  {ctrl.setpoint.toFixed(1)} {ctrl.engineeringUnit}
                </span>
              </div>
              <input
                type="range"
                min={ctrl.minSetpoint}
                max={ctrl.maxSetpoint}
                step={0.5}
                disabled={ctrl.isAutoMode}
                value={ctrl.setpoint}
                onChange={(e) => handleSetpointSlider(parseFloat(e.target.value))}
                className="w-full h-2 bg-[#0A0D12] rounded-lg appearance-none cursor-pointer accent-sky-400 disabled:opacity-40"
              />
              <div className="flex justify-between text-[10px] text-[#8A8F98] font-mono">
                <span>{ctrl.minSetpoint} {ctrl.engineeringUnit}</span>
                <span>50%</span>
                <span>{ctrl.maxSetpoint} {ctrl.engineeringUnit}</span>
              </div>
            </div>

            {/* Tactile Jog Pushbuttons (Half-width 2-column) */}
            <div className="grid grid-cols-2 gap-2">
              <button
                type="button"
                disabled={ctrl.isAutoMode}
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
                className={`h-[48px] rounded-lg border font-mono font-bold text-xs flex items-center justify-center gap-1.5 transition-all active:scale-95 ${
                  jogFwdActive
                    ? 'bg-emerald-600 border-emerald-400 text-white shadow-[0_0_12px_rgba(16,185,129,0.5)]'
                    : 'bg-[#161E2E] border-[#232B3B] text-emerald-400 hover:border-emerald-500/50 disabled:opacity-40'
                }`}
              >
                <RotateCw className={`w-4 h-4 ${jogFwdActive ? 'animate-spin' : ''}`} />
                <span>JOG FWD</span>
              </button>

              <button
                type="button"
                disabled={ctrl.isAutoMode}
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
                className={`h-[48px] rounded-lg border font-mono font-bold text-xs flex items-center justify-center gap-1.5 transition-all active:scale-95 ${
                  jogRevActive
                    ? 'bg-amber-600 border-amber-400 text-white shadow-[0_0_12px_rgba(245,158,11,0.5)]'
                    : 'bg-[#161E2E] border-[#232B3B] text-amber-400 hover:border-amber-500/50 disabled:opacity-40'
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
            <div className="flex items-center justify-between text-xs font-medium text-[#8A8F98] pb-1 border-b border-[#232B3B]">
              <span>Permissive Safety Logic Matrix</span>
              <span className="font-mono text-[10px]">IEC 62443 / SIL-3</span>
            </div>

            {interlocks.map((item) => {
              const isSatisfied = item.status === 'SATISFIED';
              const isBypassed = item.status === 'BYPASSED';

              return (
                <div
                  key={item.id}
                  className={`p-2.5 rounded-lg border transition-all ${
                    isSatisfied
                      ? 'bg-[#161E2E] border-[#232B3B]'
                      : isBypassed
                      ? 'bg-amber-950/30 border-amber-600/40'
                      : 'bg-rose-950/40 border-rose-600/50'
                  }`}
                >
                  <div className="flex items-start justify-between">
                    <div className="flex items-center gap-1.5">
                      {isSatisfied ? (
                        <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                      ) : isBypassed ? (
                        <AlertOctagon className="w-4 h-4 text-amber-400 shrink-0" />
                      ) : (
                        <XCircle className="w-4 h-4 text-rose-500 shrink-0 animate-pulse" />
                      )}
                      <span
                        className={`text-xs font-semibold ${
                          isSatisfied ? 'text-[#F7F8F8]' : isBypassed ? 'text-amber-300' : 'text-rose-300'
                        }`}
                      >
                        {item.name}
                      </span>
                    </div>

                    <span
                      className={`text-[9px] font-mono px-1.5 py-0.5 rounded font-bold ${
                        isSatisfied
                          ? 'bg-emerald-950 text-emerald-400 border border-emerald-500/30'
                          : isBypassed
                          ? 'bg-amber-950 text-amber-400 border border-amber-500/30'
                          : 'bg-rose-950 text-rose-400 border border-rose-500/30'
                      }`}
                    >
                      {isSatisfied
                        ? '[✓] Permissive Satisfied'
                        : isBypassed
                        ? '[!] Interlock Bypassed'
                        : '[✗] Interlock Tripped'}
                    </span>
                  </div>

                  <div className="text-[10px] font-mono text-sky-400 mt-1 pl-5">
                    {item.expression}
                  </div>
                  {item.description && (
                    <div className="text-[10px] text-[#8A8F98] mt-0.5 pl-5">
                      {item.description}
                    </div>
                  )}

                  {!isSatisfied && (
                    <div className="mt-2 pl-5 flex justify-end">
                      <button
                        type="button"
                        onClick={() => onAcknowledgeInterlock?.(item.id)}
                        className="px-2 py-0.5 rounded bg-rose-600 hover:bg-rose-500 text-white text-[10px] font-mono font-medium transition-colors"
                      >
                        Acknowledge & Clear
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}

        {/* TAB 3: MINI TREND (60-SECOND SPARKLINE PV vs SP) */}
        {activeTab === 'MINI_TREND' && (
          <div className="space-y-3">
            <div className="p-3 rounded-lg bg-[#161E2E] border border-[#232B3B]">
              <div className="flex items-center justify-between mb-2">
                <span className="text-xs font-semibold text-[#D0D6E0]">
                  60s Real-Time PV vs SP
                </span>
                <div className="flex items-center gap-3 text-[10px] font-mono">
                  <span className="flex items-center gap-1 text-emerald-400">
                    <span className="w-2 h-0.5 bg-emerald-400 inline-block" /> PV ({ctrl.processValue.toFixed(1)})
                  </span>
                  <span className="flex items-center gap-1 text-amber-400">
                    <span className="w-2 h-0.5 bg-amber-400 border-dashed inline-block" /> SP ({ctrl.setpoint.toFixed(1)})
                  </span>
                </div>
              </div>

              {/* Sparkline Canvas Area */}
              <div className="w-full bg-[#0A0D12] rounded border border-[#232B3B] p-2 relative overflow-hidden">
                <svg width={svgWidth} height={svgHeight} className="overflow-visible">
                  {/* Grid Lines */}
                  <line x1="0" y1="20" x2={svgWidth} y2="20" stroke="#1F2937" strokeDasharray="2,2" />
                  <line x1="0" y1={svgHeight / 2} x2={svgWidth} y2={svgHeight / 2} stroke="#1F2937" strokeDasharray="2,2" />
                  <line x1="0" y1={svgHeight - 20} x2={svgWidth} y2={svgHeight - 20} stroke="#1F2937" strokeDasharray="2,2" />

                  {/* Setpoint (SP) Reference Line */}
                  <line
                    x1="0"
                    y1={spY}
                    x2={svgWidth}
                    y2={spY}
                    stroke="#F59E0B"
                    strokeWidth="1.5"
                    strokeDasharray="4,3"
                  />

                  {/* Process Value (PV) Sparkline Curve */}
                  <polyline
                    fill="none"
                    stroke="#10B981"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    points={pvPolyline}
                  />
                </svg>

                <div className="flex justify-between text-[9px] font-mono text-[#62666D] mt-1.5">
                  <span>-60 sec</span>
                  <span>-30 sec</span>
                  <span>0 sec (Now)</span>
                </div>
              </div>
            </div>

            {/* Quick KPI stats under trend */}
            <div className="grid grid-cols-2 gap-2">
              <div className="p-2 rounded bg-[#161E2E] border border-[#232B3B]">
                <div className="text-[10px] text-[#8A8F98] font-mono">Loop Error (SP - PV)</div>
                <div className="text-base font-mono font-bold text-sky-400">
                  {(ctrl.setpoint - ctrl.processValue).toFixed(2)} {ctrl.engineeringUnit}
                </div>
              </div>

              <div className="p-2 rounded bg-[#161E2E] border border-[#232B3B]">
                <div className="text-[10px] text-[#8A8F98] font-mono">Sample Rate</div>
                <div className="text-base font-mono font-bold text-[#F7F8F8]">100 ms (10 Hz)</div>
              </div>
            </div>
          </div>
        )}

        {/* TAB 4: MAINTENANCE & DCP / ASSET INFO */}
        {activeTab === 'MAINTENANCE' && (
          <div className="space-y-2.5">
            {/* Odometer Hours Card */}
            <div className="p-3 rounded-lg bg-[#161E2E] border border-[#232B3B] flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Clock className="w-5 h-5 text-sky-400" />
                <div>
                  <div className="text-xs font-semibold text-[#F7F8F8]">Total Operating Hours</div>
                  <div className="text-[10px] text-[#8A8F98]">Non-volatile EEPROM accumulator</div>
                </div>
              </div>
              <div className="text-xl font-bold font-mono text-sky-400">
                {maint.operatingHours.toLocaleString()} h
              </div>
            </div>

            {/* DCP & Hardware Spec Attributes */}
            <div className="p-3 rounded-lg bg-[#161E2E] border border-[#232B3B] space-y-2 text-xs">
              <div className="flex items-center justify-between border-b border-[#232B3B] pb-1.5">
                <span className="text-[#8A8F98] flex items-center gap-1.5">
                  <Radio className="w-3.5 h-3.5 text-emerald-400" /> DCP MAC Address
                </span>
                <span className="font-mono text-[#F7F8F8] font-medium">{maint.dcpMacAddress}</span>
              </div>

              <div className="flex items-center justify-between border-b border-[#232B3B] pb-1.5">
                <span className="text-[#8A8F98]">Serial Number</span>
                <span className="font-mono text-[#D0D6E0]">{maint.serialNumber}</span>
              </div>

              <div className="flex items-center justify-between border-b border-[#232B3B] pb-1.5">
                <span className="text-[#8A8F98]">Firmware Build</span>
                <span className="font-mono text-sky-400">{maint.firmwareVersion}</span>
              </div>

              <div className="flex items-center justify-between border-b border-[#232B3B] pb-1.5">
                <span className="text-[#8A8F98] flex items-center gap-1.5">
                  <Calendar className="w-3.5 h-3.5 text-amber-400" /> Last Calibration
                </span>
                <span className="font-mono text-[#D0D6E0]">{maint.lastCalibrationDate}</span>
              </div>

              <div className="flex items-center justify-between border-b border-[#232B3B] pb-1.5">
                <span className="text-[#8A8F98]">Next Due Date</span>
                <span className="font-mono text-emerald-400 font-semibold">
                  {maint.nextCalibrationDate}
                </span>
              </div>

              <div className="flex items-center justify-between">
                <span className="text-[#8A8F98]">Total Starts / Inrush Cycles</span>
                <span className="font-mono text-[#F7F8F8] font-bold">
                  {maint.totalStarts.toLocaleString()}
                </span>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
