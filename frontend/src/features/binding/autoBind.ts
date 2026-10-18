/**
 * MythOS Function Block Tag Auto-Binding Engine
 * Maps formal IEC 61131-3 UDT properties directly to canonical external TagBus paths:
 * `external.<namespace>.<device>.<property>`
 * Standards: IEC 61131-3, IEC 81346, ISA-101.01-2015
 */

export interface UdtPropertyMapping {
  /** Relative property path inside PLC UDT (e.g. "bRunning", "rSpeedActual") */
  udtProperty: string;
  /** Component state key (e.g. "running", "actualSpeed") */
  faceplateKey: string;
  /** Access permission */
  access: 'READ_ONLY' | 'READ_WRITE';
  /** Optional engineering unit */
  unit?: string;
  /** Value scale or multiplier */
  scale?: number;
}

export interface DeviceBindingResult {
  /** Fully qualified base namespace (e.g. "external.plant1.slurry.P101") */
  canonicalTagPrefix: string;
  /** Individual tag subscription paths mapped to component properties */
  tagMap: Record<string, string>;
  /** Directional write capability */
  writeAccess: Record<string, boolean>;
  /** Reverse lookup: given a canonical tag path, find the faceplate property key */
  resolveTagKey: (tagPath: string) => string | undefined;
  /** Helper to format write commands */
  createWritePayload: (key: string, value: unknown) => { tag: string; value: unknown };
}

/**
 * Standard Canonical UDT Signatures for Industrial Field Equipment
 */
export const STANDARD_UDT_MAPPINGS = {
  // Motor / VFD (Variable Frequency Drive)
  FB_Motor_VFD: [
    { udtProperty: 'bRunning', faceplateKey: 'running', access: 'READ_ONLY' },
    { udtProperty: 'bTripped', faceplateKey: 'tripped', access: 'READ_ONLY' },
    { udtProperty: 'rSpeedFeedback', faceplateKey: 'actualSpeed', access: 'READ_ONLY', unit: 'RPM' },
    { udtProperty: 'rSpeedSetpoint', faceplateKey: 'speedSetpoint', access: 'READ_WRITE', unit: 'RPM' },
    { udtProperty: 'rCurrentAmps', faceplateKey: 'currentAmps', access: 'READ_ONLY', unit: 'A' },
    { udtProperty: 'rTorquePercent', faceplateKey: 'torquePercent', access: 'READ_ONLY', unit: '%' },
    { udtProperty: 'bStartCommand', faceplateKey: 'startCommand', access: 'READ_WRITE' },
    { udtProperty: 'bStopCommand', faceplateKey: 'stopCommand', access: 'READ_WRITE' },
    { udtProperty: 'bFaultReset', faceplateKey: 'faultReset', access: 'READ_WRITE' },
    { udtProperty: 'bManualMode', faceplateKey: 'manualMode', access: 'READ_WRITE' },
  ] as UdtPropertyMapping[],

  // Discrete & Pneumatic Valve
  FB_Digital_Valve: [
    { udtProperty: 'bOpenLimit', faceplateKey: 'openLimit', access: 'READ_ONLY' },
    { udtProperty: 'bClosedLimit', faceplateKey: 'closedLimit', access: 'READ_ONLY' },
    { udtProperty: 'bTransitFault', faceplateKey: 'transitFault', access: 'READ_ONLY' },
    { udtProperty: 'rPositionPercent', faceplateKey: 'positionPercent', access: 'READ_ONLY', unit: '%' },
    { udtProperty: 'rPositionSetpoint', faceplateKey: 'positionSetpoint', access: 'READ_WRITE', unit: '%' },
    { udtProperty: 'bOpenCommand', faceplateKey: 'openCommand', access: 'READ_WRITE' },
    { udtProperty: 'bCloseCommand', faceplateKey: 'closeCommand', access: 'READ_WRITE' },
    { udtProperty: 'bFaultReset', faceplateKey: 'faultReset', access: 'READ_WRITE' },
    { udtProperty: 'bManualMode', faceplateKey: 'manualMode', access: 'READ_WRITE' },
  ] as UdtPropertyMapping[],

  // Analog Transmitter (4-20mA Sensor)
  FB_Analog_Transmitter: [
    { udtProperty: 'rPV', faceplateKey: 'pv', access: 'READ_ONLY' },
    { udtProperty: 'bSensorFault', faceplateKey: 'sensorFault', access: 'READ_ONLY' },
    { udtProperty: 'rHighHighLimit', faceplateKey: 'highHighLimit', access: 'READ_ONLY' },
    { udtProperty: 'rHighLimit', faceplateKey: 'highLimit', access: 'READ_ONLY' },
    { udtProperty: 'rLowLimit', faceplateKey: 'lowLimit', access: 'READ_ONLY' },
    { udtProperty: 'rLowLowLimit', faceplateKey: 'lowLowLimit', access: 'READ_ONLY' },
  ] as UdtPropertyMapping[],

  // PID Loop Controller
  FB_PID_Controller: [
    { udtProperty: 'rPV', faceplateKey: 'pv', access: 'READ_ONLY' },
    { udtProperty: 'rSP', faceplateKey: 'sp', access: 'READ_WRITE' },
    { udtProperty: 'rCV', faceplateKey: 'cv', access: 'READ_ONLY', unit: '%' },
    { udtProperty: 'rKp', faceplateKey: 'kp', access: 'READ_WRITE' },
    { udtProperty: 'rTn', faceplateKey: 'tn', access: 'READ_WRITE', unit: 's' },
    { udtProperty: 'rTv', faceplateKey: 'tv', access: 'READ_WRITE', unit: 's' },
    { udtProperty: 'rDeadband', faceplateKey: 'deadband', access: 'READ_WRITE' },
    { udtProperty: 'eMode', faceplateKey: 'operatingMode', access: 'READ_WRITE' },
  ] as UdtPropertyMapping[],
};

/**
 * Automatically binds a device within a given namespace to its formal canonical TagBus addresses.
 *
 * Pattern: `external.<namespace>.<deviceName>.<udtProperty>`
 *
 * @param deviceNamespace Subsystem or area path (e.g. "plant1.crusher" or "water_treatment")
 * @param deviceName Unique device tag identifier (e.g. "P101", "XV202")
 * @param udtMapping Array of UDT property mappings or one of standard UDT signature keys
 */
export function bindFaceplateTags(
  deviceNamespace: string,
  deviceName: string,
  udtMapping: UdtPropertyMapping[] | keyof typeof STANDARD_UDT_MAPPINGS
): DeviceBindingResult {
  const mappings = typeof udtMapping === 'string' ? STANDARD_UDT_MAPPINGS[udtMapping] : udtMapping;

  if (!mappings) {
    throw new Error(`Unknown or undefined UDT mapping schema: ${String(udtMapping)}`);
  }

  const cleanNamespace = deviceNamespace.trim().replace(/^\.+|\.+$/g, '');
  const cleanDevice = deviceName.trim();
  const canonicalTagPrefix = cleanNamespace
    ? `external.${cleanNamespace}.${cleanDevice}`
    : `external.${cleanDevice}`;

  const tagMap: Record<string, string> = {};
  const writeAccess: Record<string, boolean> = {};
  const reverseTagMap: Record<string, string> = {};

  for (const m of mappings) {
    const fullTagPath = `${canonicalTagPrefix}.${m.udtProperty}`;
    tagMap[m.faceplateKey] = fullTagPath;
    writeAccess[m.faceplateKey] = m.access === 'READ_WRITE';
    reverseTagMap[fullTagPath] = m.faceplateKey;
  }

  return {
    canonicalTagPrefix,
    tagMap,
    writeAccess,
    resolveTagKey: (tagPath: string) => reverseTagMap[tagPath],
    createWritePayload: (key: string, value: unknown) => {
      const tag = tagMap[key];
      if (!tag) {
        throw new Error(`Property key "${key}" not mapped for device ${cleanDevice}`);
      }
      if (!writeAccess[key]) {
        throw new Error(`Property key "${key}" (${tag}) is READ_ONLY and cannot be written`);
      }
      return { tag, value };
    },
  };
}
