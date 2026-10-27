/**
 * SYS-REQ-096 live path: the operator-reachable inspector must not assert a satisfied safety chain.
 *
 * `Zone4TaskCards` is the only caller of `InspectorDrawer`, and it renders the full-diagnostic
 * branch when the panel is expanded. Nothing drove that branch before this test, so the honesty
 * suite covered the drawer in isolation while the path an operator actually opens was unproven.
 *
 * What is asserted is the shape of the *operator* path, not the drawer's internals: with no live
 * telemetry reaching the workbench, the opened drawer must say interlock state is unknown and must
 * not render a fabricated permissive — in particular not a satisfied "Emergency Stop Chain"
 * (ISO 13849-1), which would read a machine as safe when nothing was read.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { Zone4TaskCards } from '../components/workbench/Zone4TaskCards';
import { useHmiStore } from '../stores/useHmiStore';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): void {
  flushSync(() => {
    root.render(element);
  });
}

/**
 * Read a rendered element by its testid.
 *
 * Deliberately not `container.innerHTML.includes(...)`: substring matching over raw HTML also
 * matches SVG path data and CSS class fragments, which produces both false positives and false
 * negatives. Assertions read text through the element.
 */
function byTestId(id: string): HTMLElement | null {
  return container.querySelector(`[data-testid="${id}"]`);
}

/** Every fabricated interlock name the pre-fix drawer asserted as satisfied. */
const FABRICATED_SAFETY_CLAIMS = [
  'Emergency Stop Chain',
  'Lube Oil Pressure',
  'Motor Thermal Overload',
  'Safety Guard Closed',
  'Downstream Pressure Relief',
  'Instrument Air Supply',
  'Travel Time Within Spec',
];

describe('SYS-REQ-096: the operator-reachable inspector invents no safety interlock', () => {
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

  /** The drawer root has no testid; it is the labelled complementary landmark. */
  function drawer(): HTMLElement | null {
    return container.querySelector('[role="complementary"][aria-label="Inspector Drawer"]');
  }

  /** Open the panel's full-diagnostic branch, then select the interlock matrix tab. */
  function openFullDrawerInterlocks() {
    renderClient(<Zone4TaskCards isCollapsed={false} onToggleCollapse={() => {}} />);

    const toggle = Array.from(container.querySelectorAll('button')).find(
      (button) => button.getAttribute('title') === 'Switch to Full Diagnostic Drawer'
    ) as HTMLButtonElement | undefined;
    expect(toggle, 'full-drawer toggle not found on the inspector tab').toBeDefined();

    flushSync(() => {
      toggle!.click();
    });

    const interlockTab = Array.from(container.querySelectorAll('button')).find(
      (button) => button.textContent?.trim() === 'Interlocks'
    ) as HTMLButtonElement | undefined;
    expect(interlockTab, 'interlock matrix tab not found in the opened drawer').toBeDefined();

    flushSync(() => {
      interlockTab!.click();
    });
  }

  it('renders the drawer in its full-drawer branch', () => {
    openFullDrawerInterlocks();
    // Guard the guard: if the branch is never entered the assertions below prove nothing.
    expect(drawer(), 'full-drawer branch was not entered').not.toBeNull();
  });

  it('reports interlock state as unknown rather than rendering a satisfied chain', () => {
    openFullDrawerInterlocks();

    const empty = byTestId('inspector-interlocks-empty');
    expect(empty, 'no explicit interlock no-data state was rendered').not.toBeNull();
    expect(empty!.textContent).toContain('Interlock states unavailable');

    const drawerText = drawer()!.textContent ?? '';
    for (const claim of FABRICATED_SAFETY_CLAIMS) {
      expect(drawerText, `fabricated safety claim rendered: ${claim}`).not.toContain(claim);
    }
    // The satisfied marker must not appear at all on this path.
    expect(drawerText).not.toContain('[✓] OK');
  });

  it('declares its provenance as not bound, not as a live device', () => {
    openFullDrawerInterlocks();

    // The drawer is rendered with quality="NOT_BOUND" because no telemetry transport exists yet.
    // Assert on the attribute, not on a colour class: this is a provenance claim.
    const declared = container.querySelector('[data-quality="NOT_BOUND"]');
    expect(declared, 'the drawer did not declare NOT_BOUND provenance').not.toBeNull();
  });
});
