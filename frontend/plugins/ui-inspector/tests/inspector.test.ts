import { describe, it, expect } from 'vitest';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { InspectorDrawer } from '../src/InspectorDrawer';
import { InspectorTabType } from '../src/types';

describe('Slide-Over Inspector Drawer Component (Fixed 384px) (TSK-306)', () => {
  it('enforces Adrian Ergonomic Invariant: fixed 384px width with zero horizontal resizing', () => {
    const html = renderToString(
      React.createElement(InspectorDrawer, {
        isOpen: true,
        onClose: () => {},
        quality: 'GOOD',
        initialTab: 'MANUAL_CONTROL',
      })
    );

    // Assert fixed 384px styles in rendered HTML
    expect(html).toContain('width:384px');
    expect(html).toContain('min-width:384px');
    expect(html).toContain('max-width:384px');
    expect(html).toContain('flex:0 0 384px');
  });

  it('renders all 4 standardized tabs properly', () => {
    // 1. Manual Control Tab
    const manualHtml = renderToString(
      React.createElement(InspectorDrawer, {
        isOpen: true,
        onClose: () => {},
        quality: 'GOOD',
        initialTab: 'MANUAL_CONTROL',
      })
    );
    expect(manualHtml).toContain('Control Mode');
    expect(manualHtml).toContain('JOG FWD');
    expect(manualHtml).toContain('JOG REV');
    expect(manualHtml).toContain('Target Setpoint (SP)');

    // 2. Interlock Matrix Checklist Tab — the drawer renders only interlocks it is given
    //    (SYS-REQ-096: it used to supply five fabricated ones), so this passes real states.
    const interlockHtml = renderToString(
      React.createElement(InspectorDrawer, {
        isOpen: true,
        onClose: () => {},
        quality: 'GOOD',
        initialTab: 'INTERLOCK_MATRIX',
        interlocks: [
          { id: 'i1', name: 'Guard Closed', expression: '%I0.0', isPermissive: true, status: 'SATISFIED' },
          { id: 'i2', name: 'Seal Flow', expression: '%IW24', isPermissive: false, status: 'TRIPPED' },
        ],
      })
    );
    expect(interlockHtml).toContain('Permissive Safety Logic Matrix');
    expect(interlockHtml).toContain('[✓] Permissive Satisfied');
    expect(interlockHtml).toContain('[✗] Interlock Tripped');

    // 3. Mini Trend Tab
    const trendHtml = renderToString(
      React.createElement(InspectorDrawer, {
        isOpen: true,
        onClose: () => {},
        quality: 'GOOD',
        initialTab: 'MINI_TREND',
        trendHistory: [
          { timestamp: 1_700_000_000_000, pv: 41.2, sp: 42.0 },
          { timestamp: 1_700_000_001_000, pv: 41.6, sp: 42.0 },
        ],
      })
    );
    expect(trendHtml).toContain('PV vs SP');
    expect(trendHtml).toContain('Loop Error (SP - PV)');
    // SSR inserts comment separators between adjacent text nodes; compare without them.
    expect(trendHtml.replace(/<!-- -->/g, '')).toContain('2 samples');

    // 4. Maintenance Tab — the record must come from the caller, never a fabricated one.
    const maintHtml = renderToString(
      React.createElement(InspectorDrawer, {
        isOpen: true,
        onClose: () => {},
        quality: 'GOOD',
        initialTab: 'MAINTENANCE',
        maintenanceInfo: { operatingHours: 120.5, dcpMacAddress: 'AA:BB:CC:DD:EE:FF', firmwareVersion: 'FW-1.0.0' },
      })
    );
    expect(maintHtml).toContain('Total Operating Hours');
    expect(maintHtml).toContain('DCP MAC Address');
    expect(maintHtml).toContain('Firmware Build');
    expect(maintHtml).toContain('FW-1.0.0');
  });

  it('renders nothing when isOpen is false', () => {
    const html = renderToString(
      React.createElement(InspectorDrawer, {
        isOpen: false,
        onClose: () => {},
        quality: 'NOT_BOUND',
      })
    );
    expect(html).toBe('');
  });
});
