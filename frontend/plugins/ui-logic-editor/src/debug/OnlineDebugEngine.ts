import {
  DebugConnectionState,
  TelemetryFrame,
  ForcedVariableEntry,
  ForcingAuditLog,
  OnlineDebugState,
} from './types';

export type TelemetryListener = (state: OnlineDebugState) => void;

/**
 * Online Debug Engine compliant with SYS-REQ-096 safety invariants.
 * Connects to runtime telemetry (SSE / WebSocket) and governs variable forcing
 * with strict audit logs. A missing or failed transport becomes FAULT; the engine
 * never synthesizes a process image of its own.
 */
export class OnlineDebugEngine {
  private state: DebugConnectionState = 'DISCONNECTED';
  private telemetry: Record<string, boolean | number | string> = {};
  private forcedVariables: Record<string, ForcedVariableEntry> = {};
  private auditLogs: ForcingAuditLog[] = [];
  private listeners: Set<TelemetryListener> = new Set();
  private cycleTimeMs: number = 10;
  private ws: WebSocket | null = null;
  private sse: EventSource | null = null;

  public subscribe(listener: TelemetryListener): () => void {
    this.listeners.add(listener);
    listener(this.getSnapshot());
    return () => {
      this.listeners.delete(listener);
    };
  }

  public getSnapshot(): OnlineDebugState {
    return {
      state: this.state,
      cycleTimeMs: this.cycleTimeMs,
      telemetryRateHz: 50,
      telemetry: { ...this.telemetry },
      forcedVariables: { ...this.forcedVariables },
      auditLogs: [...this.auditLogs],
    };
  }

  private notify() {
    const snap = this.getSnapshot();
    for (const fn of this.listeners) {
      fn(snap);
    }
  }

  /**
   * Connects to the live PLC online debugging stream. No URL, or a failed
   * transport, is reported as FAULT rather than simulated.
   */
  public connect(url?: string): void {
    if (this.state === 'ONLINE' || this.state === 'CONNECTING') return;

    this.state = 'CONNECTING';
    this.notify();

    if (url && typeof window !== 'undefined' && 'WebSocket' in window) {
      try {
        this.ws = new WebSocket(url);
        this.ws.onopen = () => {
          this.state = 'ONLINE';
          this.notify();
        };
        this.ws.onmessage = (event) => {
          try {
            const data: TelemetryFrame = JSON.parse(event.data);
            this.handleIncomingTelemetry(data);
          } catch {
            // ignore malformed frame
          }
        };
        this.ws.onerror = () => {
          this.markFault();
        };
        this.ws.onclose = () => {
          if (this.state !== 'FAULT') {
            this.state = 'DISCONNECTED';
            this.notify();
          }
        };
        return;
      } catch {
        this.markFault();
        return;
      }
    }

    this.markFault();
  }

  private markFault(): void {
    this.state = 'FAULT';
    this.telemetry = {};
    this.notify();
  }

  private handleIncomingTelemetry(frame: TelemetryFrame): void {
    const nextTelemetry = { ...frame.variables };
    for (const [varName, forced] of Object.entries(this.forcedVariables)) {
      if (forced.active) {
        nextTelemetry[varName] = forced.value;
      }
    }
    this.telemetry = nextTelemetry;
    this.cycleTimeMs = frame.cycleTimeMs;
    this.notify();
  }

  public disconnect(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    if (this.sse) {
      this.sse.close();
      this.sse = null;
    }
    this.state = 'DISCONNECTED';
    this.notify();
  }

  /**
   * Forces a variable value with SYS-REQ-096 audit logging
   */
  public forceVariable(
    variableName: string,
    value: boolean | number | string,
    dataType: string = 'BOOL',
    operator: string = 'Operator_Field'
  ): void {
    const entry: ForcedVariableEntry = {
      variableName,
      value,
      dataType,
      forcedAt: new Date().toISOString(),
      operator,
      active: true,
    };
    this.forcedVariables[variableName] = entry;
    this.telemetry[variableName] = value;

    const audit: ForcingAuditLog = {
      id: `audit-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
      timestamp: new Date().toISOString(),
      action: 'FORCE',
      variableName,
      value,
      operator,
      comment: `Variable ${variableName} forced to ${value}`,
    };
    this.auditLogs.unshift(audit);

    this.notify();
  }

  /**
   * Unforces a specific variable and releases it back to controller process image
   */
  public unforceVariable(variableName: string, operator: string = 'Operator_Field'): void {
    if (this.forcedVariables[variableName]) {
      delete this.forcedVariables[variableName];

      const audit: ForcingAuditLog = {
        id: `audit-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
        timestamp: new Date().toISOString(),
        action: 'UNFORCE',
        variableName,
        operator,
        comment: `Forced override released for ${variableName}`,
      };
      this.auditLogs.unshift(audit);

      this.notify();
    }
  }

  /**
   * Emergency Force All Release: immediately unforces every locked variable
   */
  public releaseAllForces(operator: string = 'Supervisor'): void {
    const count = Object.keys(this.forcedVariables).length;
    this.forcedVariables = {};

    const audit: ForcingAuditLog = {
      id: `audit-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
      timestamp: new Date().toISOString(),
      action: 'RELEASE_ALL',
      operator,
      comment: `Emergency Release All: cleared ${count} forced variables`,
    };
    this.auditLogs.unshift(audit);

    this.notify();
  }
}

export const sharedDebugEngine = new OnlineDebugEngine();
