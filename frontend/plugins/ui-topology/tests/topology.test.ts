import { describe, it, expect } from 'vitest';
import { computeOrthogonalBusPath, generateSvgPathWithFillets } from '../src/busRouting';
import { PROTOCOL_COLORS, OFFLINE_BUS_COLOR } from '../src/types';
import { INDUSTRIAL_CATALOG } from '../src/catalogData';
import { Position } from '@xyflow/react';

describe('A* Orthogonal Bus Cable Routing & Visual Tokens (TSK-303)', () => {
  it('enforces exact industrial color codes for PROFINET, Modbus, Backbone, Safety, IO-Link', () => {
    // PROFINET / Modbus TCP: Green (#16A34A)
    expect(PROTOCOL_COLORS.PROFINET.stroke).toBe('#16A34A');
    expect(PROTOCOL_COLORS.MODBUS_TCP.stroke).toBe('#16A34A');

    // RS-485 Modbus RTU / PROFIBUS: Blue (#0284C7)
    expect(PROTOCOL_COLORS.MODBUS_RTU.stroke).toBe('#0284C7');
    expect(PROTOCOL_COLORS.PROFIBUS_DP.stroke).toBe('#0284C7');

    // CANopen: Amber (#CA8A04)
    expect(PROTOCOL_COLORS.CANOPEN.stroke).toBe('#CA8A04');

    // Ethernet Backbone: Slate Blue / Sky Blue (#38BDF8)
    expect(PROTOCOL_COLORS.ETHERNET_BACKBONE.stroke).toBe('#38BDF8');

    // Safety Loop / SIL-3 STO: Crimson Red (#DC2626)
    expect(PROTOCOL_COLORS.SAFETY_PROFISAFE.stroke).toBe('#DC2626');

    // IO-Link: Purple (#9333EA)
    expect(PROTOCOL_COLORS.IOLINK.stroke).toBe('#9333EA');

    // Offline / Severed link
    expect(OFFLINE_BUS_COLOR.stroke).toBe('#475569');
  });

  it('computes orthogonal bus paths with fillets between ports', () => {
    const path = computeOrthogonalBusPath({
      sourceX: 100,
      sourceY: 100,
      targetX: 300,
      targetY: 200,
      sourcePosition: Position.Right,
      targetPosition: Position.Left,
      gridSize: 16,
      cornerRadius: 6,
    });

    expect(path).toContain('M 100 100');
    expect(path).toContain('Q'); // Fillet quadratic curve
    expect(path).toContain('300 200');
  });

  it('contains comprehensive industrial hardware catalog entries', () => {
    expect(INDUSTRIAL_CATALOG.length).toBeGreaterThanOrEqual(5);

    const s71500 = INDUSTRIAL_CATALOG.find((c) => c.articleNumber === '6ES7515-2AM02-0AB0');
    expect(s71500).toBeDefined();
    expect(s71500?.category).toBe('Controllers');
    expect(s71500?.stationType).toBe('PLC_RACK');
    expect(s71500?.defaultPorts.length).toBeGreaterThanOrEqual(2);

    const scalance = INDUSTRIAL_CATALOG.find((c) => c.stationType === 'SWITCH');
    expect(scalance).toBeDefined();
    expect(scalance?.defaultPorts.length).toBe(4);
  });
});
