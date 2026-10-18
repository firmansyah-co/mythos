/**
 * MythOS Faceplate & Tag Binding TypeScript Contracts
 * Standards: ANSI/ISA-101.01-2015, ANSI/ISA-18.2-2016, IEC 61131-3
 */

export type FaceplateTier = 1 | 2 | 3;
export type FaceplatePresentationMode = 'PID' | 'INTERACTIVE' | 'POPUP' | 'SIDEBAR';

export type AlarmSeverity = 'CRITICAL' | 'MAJOR' | 'MINOR' | 'DIAGNOSTIC' | 'NORMAL' | 'SHELVED';
export type OperatingMode = 'MANUAL' | 'AUTO' | 'CASCADE' | 'LOCAL' | 'REMOTE';

export interface InterlockCondition {
  id: string;
  name: string;
  isSatisfied: boolean;
  bypassed?: boolean;
}

export interface BaseFaceplateProps {
  /** Target device tag identifier (e.g. "P101", "XV101", "TIC101") */
  tag: string;
  /** Plant namespace or path prefix (e.g. "area01.slurry") */
  namespace?: string;
  /** Human-readable equipment description */
  label: string;
  /** Presentation tier: 1 = P&ID Symbol, 2 = Summary Card, 3 = Diagnostic Drawer/Modal */
  tier?: FaceplateTier;
  /** Mode presentation style */
  mode?: FaceplatePresentationMode;
  /** Alarm severity level */
  alarm?: AlarmSeverity;
  /** Alarm active state */
  alarmActive?: boolean;
  /** Unacknowledged alarm flag (triggers ISA-18.2 1Hz flashing) */
  alarmUnacknowledged?: boolean;
  /** Operating control mode */
  operatingMode?: OperatingMode;
  /** Maintenance lock enabled */
  maintenanceLock?: boolean;
  /** Interactive callbacks */
  onTierChange?: (newTier: FaceplateTier) => void;
  onModeToggle?: (newMode: OperatingMode) => void;
  onAcknowledgeAlarm?: () => void;
  className?: string;
}

export interface MotorFaceplateProps extends BaseFaceplateProps {
  /** Motor running confirmation */
  running: boolean;
  /** Motor tripped / faulted */
  tripped: boolean;
  /** Motor actual speed in RPM or Hz */
  actualSpeed: number;
  /** Motor target speed setpoint */
  speedSetpoint: number;
  /** Motor current draw in Amps */
  currentAmps?: number;
  /** Torque percentage */
  torquePercent?: number;
  /** Maximum speed range */
  maxSpeed?: number;
  /** Interlocks list */
  interlocks?: InterlockCondition[];
  /** Control actions */
  onStart?: () => void;
  onStop?: () => void;
  onResetFault?: () => void;
  onSpeedChange?: (speed: number) => void;
}

export interface ValveFaceplateProps extends BaseFaceplateProps {
  /** Valve fully open limit switch */
  openLimit: boolean;
  /** Valve fully closed limit switch */
  closedLimit: boolean;
  /** Valve travel transit fault */
  transitFault: boolean;
  /** Analog opening percentage (0-100%) for control valves */
  positionPercent?: number;
  /** Position target setpoint (0-100%) */
  positionSetpoint?: number;
  /** Interlocks list */
  interlocks?: InterlockCondition[];
  /** Control actions */
  onOpen?: () => void;
  onClose?: () => void;
  onPositionChange?: (position: number) => void;
  onResetFault?: () => void;
}

export interface AnalogTransmitterFaceplateProps extends BaseFaceplateProps {
  /** Process Variable */
  pv: number;
  /** Engineering unit (e.g. "bar", "°C", "m³/h", "%") */
  unit: string;
  /** Measurement type */
  transmitterType?: 'PRESSURE' | 'TEMPERATURE' | 'FLOW' | 'LEVEL';
  /** Range limits */
  minRange?: number;
  maxRange?: number;
  /** Alarm thresholds */
  highHighLimit?: number;
  highLimit?: number;
  lowLimit?: number;
  lowLowLimit?: number;
  /** Sensor fail / wire break */
  sensorFault?: boolean;
}

export interface PidControllerFaceplateProps extends BaseFaceplateProps {
  /** Process Variable (PV) */
  pv: number;
  /** Setpoint (SP) */
  sp: number;
  /** Controller Output % (CV) */
  cv: number;
  /** Engineering unit */
  unit: string;
  /** Tuning parameters */
  kp?: number;
  tn?: number; // Integral time (s)
  tv?: number; // Derivative time (s)
  deadband?: number;
  /** Limits */
  spMin?: number;
  spMax?: number;
  highHighLimit?: number;
  highLimit?: number;
  lowLimit?: number;
  lowLowLimit?: number;
  /** Actions */
  onSpChange?: (newSp: number) => void;
  onCvChange?: (newCv: number) => void;
  onTuneUpdate?: (kp: number, tn: number, tv: number) => void;
}
