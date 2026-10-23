/**
 * MythOS Devices & Networks Schema Definitions
 * Parity with Siemens TIA Portal Hardware Architecture (SYS-REQ-097)
 */

export type HardwareViewMode = 'NETWORK' | 'DEVICE' | 'TOPOLOGY';

export type ProtocolType =
  | 'PROFINET'
  | 'MODBUS_TCP'
  | 'MODBUS_RTU'
  | 'PROFIBUS_DP'
  | 'CANOPEN'
  | 'IOLINK'
  | 'SAFETY_PROFISAFE'
  | 'ETHERNET_BACKBONE';

export type DeviceStatus = 'RUN' | 'STOP' | 'FAULT' | 'OFFLINE';

export interface NetworkPort {
  id: string;
  name: string; // e.g. "X1 P1", "X1 P2", "Port 1"
  protocol: ProtocolType;
  connectorType: 'RJ45' | 'M12_D_CODED' | 'DB9' | 'TERMINAL_BLOCK' | 'M12_A_CODED';
  ipAddress?: string;
  subnetMask?: string;
  macAddress?: string;
  baudRate?: number;
  slaveId?: number;
  linkStatus: 'UP' | 'DOWN' | 'DEGRADED';
  rxBytesSec?: number;
  txBytesSec?: number;
  crcErrors?: number;
}

export interface HardwareModuleSlot {
  slotNumber: number; // 0..31
  catalogArticleNo: string; // e.g. "6ES7521-1BH00-0AB0"
  name: string; // e.g. "DI 16x24VDC HF"
  moduleCategory: 'POWER' | 'CPU' | 'DI' | 'DQ' | 'AI' | 'AQ' | 'COMM' | 'TECH';
  startAddressI?: string; // e.g. "%I10.0"
  endAddressI?: string;   // e.g. "%I11.7"
  startAddressQ?: string; // e.g. "%Q10.0"
  endAddressQ?: string;   // e.g. "%Q11.7"
  ports: NetworkPort[];
  diagnostics: {
    runLed: boolean;
    errLed: boolean;
    maintLed: boolean;
    channelBits: boolean[];
    temperatureC?: number;
  };
}

export interface HardwareStationNode {
  id: string;
  stationName: string; // e.g. "PLC_RACK_01", "ET200SP_ISLAND"
  stationType: 'PLC_RACK' | 'REMOTE_IO' | 'VFD_DRIVE' | 'FIELD_INSTRUMENT' | 'SWITCH';
  rackNumber: number;  // Default 0
  status: DeviceStatus;
  ipAddress?: string;
  articleNumber?: string;
  slots: HardwareModuleSlot[];
  ports?: NetworkPort[];
  position: { x: number; y: number };
  isLocked?: boolean;
}

export interface TopologyCableEdge {
  id: string;
  sourceStationId: string;
  sourcePortId: string; // e.g. "X1 P1"
  targetStationId: string;
  targetPortId: string; // e.g. "Port 1"
  protocol: ProtocolType;
  cableLengthMeters?: number;
  nominalMatch: boolean;
  health: {
    status: 'OPTIMAL' | 'WARNING' | 'CRITICAL' | 'OFFLINE';
    latencyMs: number;
    packetLossPct: number;
    retriesPerSec: number;
  };
}

export interface IndustrialCatalogItem {
  id: string;
  name: string;
  articleNumber: string;
  stationType: HardwareStationNode['stationType'];
  defaultProtocol: ProtocolType;
  description: string;
  category: 'Controllers' | 'Distributed I/O' | 'Drives' | 'Field Instruments' | 'Switches';
  defaultPorts: NetworkPort[];
}

export interface BusColorDefinition {
  stroke: string;
  strokeWidth: number;
  strokeDasharray?: string;
  label: string;
}

export const PROTOCOL_COLORS: Record<ProtocolType, BusColorDefinition> = {
  PROFINET: {
    stroke: 'var(--bus-ethernet)', // Green
    strokeWidth: 3,
    label: 'PROFINET',
  },
  MODBUS_TCP: {
    stroke: 'var(--bus-ethernet)', // Green
    strokeWidth: 3,
    label: 'Modbus TCP',
  },
  MODBUS_RTU: {
    stroke: 'var(--bus-serial)', // Blue
    strokeWidth: 2.5,
    label: 'Modbus RTU',
  },
  PROFIBUS_DP: {
    stroke: 'var(--bus-serial)', // Blue
    strokeWidth: 2.5,
    label: 'PROFIBUS DP',
  },
  CANOPEN: {
    stroke: 'var(--bus-can)', // Amber/Yellow
    strokeWidth: 2.5,
    strokeDasharray: '6,3',
    label: 'CANopen',
  },
  ETHERNET_BACKBONE: {
    stroke: 'var(--bus-backbone)', // Slate Blue / Sky Blue
    strokeWidth: 3,
    label: 'Ethernet Backbone',
  },
  SAFETY_PROFISAFE: {
    stroke: 'var(--bus-safety)', // Crimson Red
    strokeWidth: 4,
    label: 'Safety Loop / SIL-3 STO',
  },
  IOLINK: {
    stroke: 'var(--bus-io-link)', // Purple
    strokeWidth: 2,
    label: 'IO-Link',
  },
};

export const OFFLINE_BUS_COLOR: BusColorDefinition = {
  stroke: 'var(--bus-offline)',
  strokeWidth: 2,
  strokeDasharray: '4,4',
  label: 'Offline / Severed',
};
