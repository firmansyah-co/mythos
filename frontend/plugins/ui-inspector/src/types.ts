/**
 * MythOS Inspector Drawer Type Definitions
 * Strict Adrian Ergonomic Invariant: Fixed 384px width, 2-column modular grid
 */

export type InspectorTabType = 'MANUAL_CONTROL' | 'INTERLOCK_MATRIX' | 'MINI_TREND' | 'MAINTENANCE';

export interface InterlockCondition {
  id: string;
  name: string;
  expression: string;
  isPermissive: boolean;
  status: 'SATISFIED' | 'TRIPPED' | 'BYPASSED';
  description?: string;
  channelAddress?: string;
}

export interface MiniTrendPoint {
  timestamp: number; // Unix epoch ms
  pv: number;        // Process Value
  sp: number;        // Setpoint Value
}

export interface DeviceMaintenanceInfo {
  operatingHours: number;
  totalStarts: number;
  dcpMacAddress: string;
  serialNumber: string;
  firmwareVersion: string;
  lastCalibrationDate: string;
  nextCalibrationDate: string;
  temperatureCelsius: number;
  nominalVoltageV: number;
}

export interface ManualControlState {
  isAutoMode: boolean;
  isJogForward: boolean;
  isJogReverse: boolean;
  setpoint: number;
  processValue: number;
  minSetpoint: number;
  maxSetpoint: number;
  engineeringUnit: string;
  outputPowerPct: number;
}

export interface InspectorDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  deviceId?: string;
  deviceName?: string;
  deviceType?: string;
  initialTab?: InspectorTabType;
  controlState?: Partial<ManualControlState>;
  interlocks?: InterlockCondition[];
  trendHistory?: MiniTrendPoint[];
  maintenanceInfo?: Partial<DeviceMaintenanceInfo>;
  onSetpointChange?: (val: number) => void;
  onModeToggle?: (isAuto: boolean) => void;
  onJogForward?: (active: boolean) => void;
  onJogReverse?: (active: boolean) => void;
  onAcknowledgeInterlock?: (id: string) => void;
  className?: string;
}
