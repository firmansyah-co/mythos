/**
 * SYS-REQ-096 live path: with nothing selected, the inspector reports nothing as a device.
 *
 * The `||` fallbacks in this component chain were not a no-data contract — a fallback supplies a
 * value, so the panel reported `FP_Motor_VFD_01` as the target, `FB_Motor_UDT` as its archetype and
 * `%QX0.0..%QX0.3` as its address for an object nobody had selected. The class is distinct from the
 * `DEFAULT_*` default-parameter defect: that one supplied content at a component boundary, this one
 * supplies it inline at a call site, which is why the parameter-default guard cannot see it.
 *
 * Assertions read text through testid-resolved elements rather than scanning innerHTML: substring
 * matching over raw HTML also matches SVG path data and class fragments.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { Zone4TaskCards } from '../components/workbench/Zone4TaskCards';
import { useHmiStore } from '../stores/useHmiStore';

let container: HTMLDivElement;
let root: Root;

function render(): void {
  flushSync(() => {
    root.render(<Zone4TaskCards isCollapsed={false} onToggleCollapse={() => {}} />);
  });
}

function byTestId(id: string): HTMLElement | null {
  return container.querySelector(`[data-testid="${id}"]`);
}

/**
 * Values that were reported as a device before the fix.
 *
 * Kept broad: the compact fields now render explicit absence markers (`No selection`, `—`, `NONE`)
 * rather than the shell's `NO_DATA_TEXT`, because an earlier change on this surface landed that
 * wording. What matters is the property — no plausible device — not which marker is used.
 */
const FABRICATED_IDENTITY = [
  'FP_Motor_VFD_01',
  'FB_Motor_UDT',
  '%QX0.0..%QX0.3',
  'DEV-01',
];

describe('SYS-REQ-096: the inspector invents no device identity when nothing is selected', () => {
  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    useHmiStore.getState().resetToEmptyProject();
  });

  afterEach(() => {
    flushSync(() => {
      root.unmount();
    });
    container.remove();
    vi.restoreAllMocks();
  });

  it('shows an absence marker rather than a named target, archetype and address', () => {
    render();

    const target = byTestId('inspector-target-object');
    const archetype = byTestId('inspector-udt-archetype');
    const address = byTestId('inspector-hardware-address');
    expect(target, 'target object field not rendered').not.toBeNull();
    expect(archetype).not.toBeNull();
    expect(address).not.toBeNull();

    // An absence marker, and specifically not a plausible device. Asserting the exact marker would
    // pin the wording rather than the property; asserting it is *not* device-shaped is the point.
    for (const [label, node] of [
      ['target object', target],
      ['udt archetype', archetype],
      ['hardware address', address],
    ] as const) {
      const text = (node!.textContent ?? '').trim();
      expect(text, `${label} rendered nothing at all`).not.toBe('');
      expect(text, `${label} still names a device`).not.toMatch(/^[A-Z]*_|%[IQM]|[A-Z]{2,}[-_]\d/);
    }
    expect(target!.textContent).toBe('No selection');

    const panelText = byTestId('zone4-task-cards')!.textContent ?? '';
    for (const value of FABRICATED_IDENTITY) {
      expect(panelText, `fabricated identity rendered: ${value}`).not.toContain(value);
    }
  });

  it('reports no selection rather than defaulting the category to a device', () => {
    render();
    const category = (byTestId('inspector-item-category')!.textContent ?? '').trim();
    expect(category).not.toBe('DEVICE');
    expect(category.toUpperCase()).toContain('NO');
  });

  it('still renders the selected object\'s own identity', () => {
    // The positive control: removing the fallback must not remove real values.
    useHmiStore.getState().setSelectedContextItem({
      id: 'dev_1',
      name: 'Authored_Motor',
      type: 'DEVICE',
      udtArchetype: 'UDT_Authored',
      hardwareAddress: '%QW20.0',
    });
    render();

    expect(byTestId('inspector-target-object')!.textContent).toBe('Authored_Motor');
    expect(byTestId('inspector-udt-archetype')!.textContent).toBe('UDT_Authored');
    expect(byTestId('inspector-hardware-address')!.textContent).toBe('%QW20.0');
    expect(byTestId('inspector-hardware-address')!.textContent).not.toBe('----');
  });
});
