import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { SfcCanvas, DEFAULT_SFC_STEPS, DEFAULT_SFC_TRANSITIONS } from '../sfc/SfcCanvas';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('TSK-703: Sequential Function Chart (SFC) Canvas Suite', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    flushSync(() => {
      root.unmount();
    });
    container.remove();
  });

  it('renders SFC Canvas header with initial step and transitions', () => {
    const html = renderClient(<SfcCanvas />);
    expect(html).toContain('IEC 61131-3 Sequential Function Chart (SFC) Canvas');
    expect(html).toContain('Init_Step');
    expect(html).toContain('Initial Step');
    expect(html).toContain('Conveyor_Run');
    expect(html).toContain('Cap_Cylinder_Dwell');
  });

  it('renders SFC Action Blocks with IEC 61131-3 Action Qualifiers (N, S, R, P, L, D)', () => {
    const html = renderClient(<SfcCanvas />);
    expect(html).toContain('DO_IdleIndicator := TRUE;');
    expect(html).toContain('DO_ConveyorMotor');
    expect(html).toContain('FeedWarningLight');
    expect(html).toContain('TriggerBottleCountInc');
    expect(html).toContain('ExtendCappingArm');
  });

  it('renders boolean transition conditions and directed links', () => {
    const html = renderClient(<SfcCanvas />);
    expect(html).toContain('T1');
    expect(html).toContain('DI_StartPB = TRUE');
    expect(html).toContain('T2');
    expect(html).toContain('DI_BottleInPosition = TRUE');
  });

  it('allows adding a new step to the SFC chart', () => {
    const onAddStep = vi.fn();
    renderClient(<SfcCanvas onAddStep={onAddStep} />);

    const buttons = Array.from(container.querySelectorAll('button'));
    const addStepBtn = buttons.find((b) => b.textContent?.includes('Add Step'));
    expect(addStepBtn).toBeTruthy();

    flushSync(() => {
      addStepBtn?.click();
    });

    expect(onAddStep).toHaveBeenCalled();
    expect(container.innerHTML).toContain('Step_');
  });
});
