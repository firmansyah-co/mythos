import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { OnlineDebugEngine } from '../debug/OnlineDebugEngine';

describe('TSK-705: OnlineDebugEngine (Telemetry, Power-Flow & SYS-REQ-096 Audit Trail)', () => {
  let engine: OnlineDebugEngine;

  beforeEach(() => {
    engine = new OnlineDebugEngine();
  });

  afterEach(() => {
    engine.disconnect();
  });

  it('initializes in DISCONNECTED state with default process image and empty force list', () => {
    const snap = engine.getSnapshot();
    expect(snap.state).toBe('DISCONNECTED');
    expect(snap.forcedVariables).toEqual({});
    expect(snap.auditLogs).toHaveLength(0);
    expect(snap.telemetry['external.DI_StartPB']).toBe(true);
  });

  it('transitions to SIMULATING when connected in test environment and publishes telemetry stream updates', () => {
    const listener = vi.fn();
    const unsub = engine.subscribe(listener);

    engine.connect();
    expect(engine.getSnapshot().state).toBe('SIMULATING');
    expect(listener).toHaveBeenCalled();

    unsub();
  });

  it('manages variable force overrides with SYS-REQ-096 compliant immutable audit ledger', () => {
    engine.connect();

    // Force variable
    engine.forceVariable('DI_StartPB', true, 'BOOL');
    const snap = engine.getSnapshot();

    expect(snap.forcedVariables['DI_StartPB']).toBeDefined();
    expect(snap.forcedVariables['DI_StartPB'].value).toBe(true);
    expect(snap.telemetry['DI_StartPB']).toBe(true);

    // Verify SYS-REQ-096 Audit Log
    expect(snap.auditLogs.length).toBeGreaterThan(0);
    const lastLog = snap.auditLogs[0];
    expect(lastLog.variableName).toBe('DI_StartPB');
    expect(lastLog.action).toBe('FORCE');
    expect(lastLog.value).toBe(true);
    expect(lastLog.comment).toContain('forced');
    expect(lastLog.timestamp).toBeDefined();

    // Unforce variable
    engine.unforceVariable('DI_StartPB');
    const snapAfterUnforce = engine.getSnapshot();
    expect(snapAfterUnforce.forcedVariables['DI_StartPB']).toBeUndefined();

    const unforceLog = snapAfterUnforce.auditLogs[0];
    expect(unforceLog.action).toBe('UNFORCE');
  });

  it('supports releaseAllForces to clear all overrides at once with audit trail', () => {
    engine.connect();
    engine.forceVariable('Var_A', true);
    engine.forceVariable('Var_B', 123);
    expect(Object.keys(engine.getSnapshot().forcedVariables)).toHaveLength(2);

    engine.releaseAllForces();
    expect(Object.keys(engine.getSnapshot().forcedVariables)).toHaveLength(0);

    const logs = engine.getSnapshot().auditLogs;
    expect(logs.some((l) => l.action === 'RELEASE_ALL')).toBe(true);
  });
});
