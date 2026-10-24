/**
 * MythOS Faceplate & Tag Binding TypeScript Contracts
 * Standards: ANSI/ISA-101.01-2015, ANSI/ISA-18.2-2016, IEC 61131-3
 */

export type FaceplateTier = 1 | 2 | 3;
export type FaceplatePresentationMode = 'PID' | 'INTERACTIVE' | 'POPUP' | 'SIDEBAR';

export type AlarmSeverity = 'CRITICAL' | 'MAJOR' | 'MINOR' | 'DIAGNOSTIC' | 'NORMAL' | 'SHELVED';
export type OperatingMode = 'MANUAL' | 'AUTO' | 'CASCADE' | 'LOCAL' | 'REMOTE';

/**
 * Provenance of the process values a faceplate is showing.
 *
 * SYS-REQ-096 (Zero Silent Fallback) forbids presenting invented values as process data, so a
 * faceplate has to know where its numbers came from:
 *
 * - `GOOD`        — live, good-quality telemetry from the bound device. The only state in
 *                   which a numeric process value is displayed.
 * - `BAD_QUALITY` — the device is bound but it is not reporting usable data (comm failure,
 *                   stale or missing payload). Process values are suppressed.
 * - `NOT_BOUND`   — no device binding exists. The faceplate is not connected to a plant.
 *                   Process values are suppressed.
 *
 * Stored design configuration (tag, unit, ranges, alarm limits, tuning) is not a process value
 * and is displayed independently of this quality.
 */
export type ProcessDataQuality = 'GOOD' | 'BAD_QUALITY' | 'NOT_BOUND';

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
  /**
   * Provenance of the process values in this faceplate. REQUIRED, with no default.
   *
   * - `GOOD`: values come from live, good-quality telemetry and are rendered as such.
   * - `BAD_QUALITY`: the device is bound but not reporting usable data. Values are suppressed
   *   and an explicit `BAD COMM FAILURE` indication is shown.
   * - `NOT_BOUND`: no tag binding exists. Values are suppressed and `NOT BOUND` is shown.
   *
   * There is deliberately no permissive default. A caller that cannot vouch for its values must
   * say so, and omitting the prop is a compile error — a faceplate that presents unverified
   * numbers as live is the defect SYS-REQ-096 exists to prevent.
   */
  quality: ProcessDataQuality;
  /** Mode presentation style */
  mode?: FaceplatePresentationMode;
  /** Alarm severity level */
  alarm?: AlarmSeverity;
  /** Alarm active state */
  alarmActive?: boolean;
  /** Unacknowledged alarm flag (triggers ISA-18.2 1Hz flashing) */
  alarmUnacknowledged?: boolean;
  /**
   * Operating control mode as reported by the device. Optional: a faceplate announces a mode
   * only when live telemetry supplies one. There is deliberately no 'AUTO' default — asserting
   * automatic control for an unobserved device is a fabricated plant state (SYS-REQ-096).
   */
  operatingMode?: OperatingMode | null;
  /** Maintenance lock enabled */
  maintenanceLock?: boolean;
  /** Interactive callbacks */
  onTierChange?: (newTier: FaceplateTier) => void;
  onModeToggle?: (newMode: OperatingMode) => void;
  onAcknowledgeAlarm?: () => void;
  className?: string;
}

export interface MotorFaceplateProps extends BaseFaceplateProps {
  /** Motor running confirmation. `null`/absent = no value available from the device. */
  running?: boolean | null;
  /** Motor tripped / faulted. `null`/absent = no value available from the device. */
  tripped?: boolean | null;
  /** Motor actual speed in RPM or Hz. `null`/absent = no value available from the device. */
  actualSpeed?: number | null;
  /** Motor target speed setpoint. `null`/absent = no value available from the device. */
  speedSetpoint?: number | null;
  /** Motor current draw in Amps. `null`/absent = not measured / not available. */
  currentAmps?: number | null;
  /** Torque percentage. `null`/absent = not measured / not available. */
  torquePercent?: number | null;
  /** Maximum speed range. `null`/absent = the design stores none. */
  maxSpeed?: number | null;
  /** Interlocks list */
  interlocks?: InterlockCondition[];
  /** Control actions */
  onStart?: () => void;
  onStop?: () => void;
  onResetFault?: () => void;
  onSpeedChange?: (speed: number) => void;
}

export interface ValveFaceplateProps extends BaseFaceplateProps {
  /** Valve fully open limit switch. `null`/absent = no value available from the device. */
  openLimit?: boolean | null;
  /** Valve fully closed limit switch. `null`/absent = no value available from the device. */
  closedLimit?: boolean | null;
  /** Valve travel transit fault. `null`/absent = no value available from the device. */
  transitFault?: boolean | null;
  /** Analog opening percentage (0-100%) for control valves. `null`/absent = not available. */
  positionPercent?: number | null;
  /** Position target setpoint (0-100%). `null`/absent = not available. */
  positionSetpoint?: number | null;
  /** Interlocks list */
  interlocks?: InterlockCondition[];
  /** Control actions */
  onOpen?: () => void;
  onClose?: () => void;
  onPositionChange?: (position: number) => void;
  onResetFault?: () => void;
}

export interface AnalogTransmitterFaceplateProps extends BaseFaceplateProps {
  /** Process Variable. `null`/absent = no value available from the device. */
  pv?: number | null;
  /** Engineering unit (e.g. "bar", "°C", "m³/h", "%"). Absent = the design stores none. */
  unit?: string;
  /** Measurement type. Absent = the design stores none; no type chip is shown. */
  transmitterType?: 'PRESSURE' | 'TEMPERATURE' | 'FLOW' | 'LEVEL';
  /** Range limits. Absent = the design stores none; the range bar is not drawn. */
  minRange?: number | null;
  maxRange?: number | null;
  /** Alarm thresholds. Absent = the design stores none. */
  highHighLimit?: number | null;
  highLimit?: number | null;
  lowLimit?: number | null;
  lowLowLimit?: number | null;
  /** Sensor fail / wire break. `null` = no value available from the device. */
  sensorFault?: boolean | null;
}

export interface PidControllerFaceplateProps extends BaseFaceplateProps {
  /** Process Variable (PV). `null`/absent = no value available from the device. */
  pv?: number | null;
  /** Setpoint (SP). `null`/absent = no value available from the device. */
  sp?: number | null;
  /** Controller Output % (CV). `null`/absent = no value available from the device. */
  cv?: number | null;
  /** Engineering unit. Absent = the design stores none. */
  unit?: string;
  /** Tuning parameters. Absent = the design stores none; the field stays empty, not defaulted. */
  kp?: number | null;
  tn?: number | null; // Integral time (s)
  tv?: number | null; // Derivative time (s)
  deadband?: number | null;
  /** Limits. Absent = the design stores none. */
  spMin?: number | null;
  spMax?: number | null;
  highHighLimit?: number | null;
  highLimit?: number | null;
  lowLimit?: number | null;
  lowLowLimit?: number | null;
  /** Actions */
  onSpChange?: (newSp: number) => void;
  onCvChange?: (newCv: number) => void;
  onTuneUpdate?: (kp: number | null, tn: number | null, tv: number | null) => void;
}
