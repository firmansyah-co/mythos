/**
 * Online Debugging & Variable Forcing Types (SYS-REQ-096 Parity).
 */

export type DebugConnectionState = 'DISCONNECTED' | 'CONNECTING' | 'ONLINE' | 'FAULT';

export interface TelemetryFrame {
  timestampMs: number;
  cycleTimeMs: number;
  variables: Record<string, boolean | number | string>;
}

export interface ForcedVariableEntry {
  variableName: string;
  value: boolean | number | string;
  dataType: string;
  forcedAt: string; // ISO timestamp
  operator: string;
  active: boolean;
}

export interface ForcingAuditLog {
  id: string;
  timestamp: string;
  action: 'FORCE' | 'UNFORCE' | 'RELEASE_ALL';
  variableName?: string;
  value?: boolean | number | string;
  operator: string;
  comment?: string;
}

export interface OnlineDebugState {
  state: DebugConnectionState;
  cycleTimeMs: number;
  telemetryRateHz: number;
  telemetry: Record<string, boolean | number | string>;
  forcedVariables: Record<string, ForcedVariableEntry>;
  auditLogs: ForcingAuditLog[];
}
