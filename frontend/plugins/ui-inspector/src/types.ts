/**
 * MythOS Inspector Drawer Type Definitions
 * Strict Adrian Ergonomic Invariant: Fixed 384px width, 2-column modular grid
 *
 * SYS-REQ-096: nothing in this contract has a fabricated default. Every process value,
 * interlock state, identity string and maintenance record is optional and, when absent,
 * renders an explicit no-data state rather than a plausible invention.
 */

import type { ProcessDataQuality } from '@mythos/ui-faceplate-lib';

export type InspectorTabType = 'MANUAL_CONTROL' | 'INTERLOCK_MATRIX' | 'MINI_TREND' | 'MAINTENANCE';

/**
 * `UNKNOWN` is a first-class interlock state: a permissive whose input is not readable is not
 * satisfied, and must never be reported as if it were.
 */
export type InterlockStatus = 'SATISFIED' | 'TRIPPED' | 'BYPASSED' | 'UNKNOWN';

export interface InterlockCondition {
  id: string;
  name: string;
  expression: string;
  isPermissive: boolean;
  status: InterlockStatus;
  description?: string;
  channelAddress?: string;
}

export interface MiniTrendPoint {
  timestamp: number; // Unix epoch ms
  pv: number;        // Process Value
  sp: number;        // Setpoint Value
}

/**
 * Maintenance and asset record. Every field is optional: a device that has not reported a
 * serial number has no serial number, and the drawer says so instead of inventing one.
 */
export interface DeviceMaintenanceInfo {
  operatingHours?: number | null;
  totalStarts?: number | null;
  dcpMacAddress?: string | null;
  serialNumber?: string | null;
  firmwareVersion?: string | null;
  lastCalibrationDate?: string | null;
  nextCalibrationDate?: string | null;
  temperatureCelsius?: number | null;
  nominalVoltageV?: number | null;
}

export interface ManualControlState {
  isAutoMode?: boolean | null;
  isJogForward?: boolean | null;
  isJogReverse?: boolean | null;
  setpoint?: number | null;
  processValue?: number | null;
  minSetpoint?: number | null;
  maxSetpoint?: number | null;
  engineeringUnit?: string | null;
  outputPowerPct?: number | null;
}

export interface InspectorDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  /**
   * Provenance of everything this drawer displays. REQUIRED, with no default.
   *
   * - `GOOD`: live device telemetry; values are rendered.
   * - `BAD_QUALITY`: the device is bound but not reporting; values are suppressed.
   * - `NOT_BOUND`: no device binding; values are suppressed.
   *
   * Omitting it is a compile error. The runtime backstop in `deviceData.ts` fails closed.
   */
  quality: ProcessDataQuality;
  deviceId?: string;
  deviceName?: string;
  deviceType?: string;
  initialTab?: InspectorTabType;
  controlState?: ManualControlState;
  interlocks?: InterlockCondition[];
  trendHistory?: MiniTrendPoint[];
  maintenanceInfo?: DeviceMaintenanceInfo;
  onSetpointChange?: (val: number) => void;
  onModeToggle?: (isAuto: boolean) => void;
  onJogForward?: (active: boolean) => void;
  onJogReverse?: (active: boolean) => void;
  onAcknowledgeInterlock?: (id: string) => void;
  className?: string;
}
