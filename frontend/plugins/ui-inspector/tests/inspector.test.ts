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
        initialTab: 'MANUAL_CONTROL',
      })
    );
    expect(manualHtml).toContain('Control Mode');
    expect(manualHtml).toContain('JOG FWD');
    expect(manualHtml).toContain('JOG REV');
    expect(manualHtml).toContain('Target Setpoint (SP)');

    // 2. Interlock Matrix Checklist Tab
    const interlockHtml = renderToString(
      React.createElement(InspectorDrawer, {
        isOpen: true,
        onClose: () => {},
        initialTab: 'INTERLOCK_MATRIX',
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
        initialTab: 'MINI_TREND',
      })
    );
    expect(trendHtml).toContain('60s Real-Time PV vs SP');
    expect(trendHtml).toContain('Loop Error (SP - PV)');
    expect(trendHtml).toContain('100 ms (10 Hz)');

    // 4. Maintenance Tab
    const maintHtml = renderToString(
      React.createElement(InspectorDrawer, {
        isOpen: true,
        onClose: () => {},
        initialTab: 'MAINTENANCE',
      })
    );
    expect(maintHtml).toContain('Total Operating Hours');
    expect(maintHtml).toContain('DCP MAC Address');
    expect(maintHtml).toContain('Firmware Build');
  });

  it('renders nothing when isOpen is false', () => {
    const html = renderToString(
      React.createElement(InspectorDrawer, {
        isOpen: false,
        onClose: () => {},
      })
    );
    expect(html).toBe('');
  });
});
