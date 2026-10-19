import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { LadderCanvas, DEFAULT_LD_RUNGS } from '../LadderCanvas';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('TSK-702: LadderCanvas Component (Dual Power Rails, Interactivity, Parity)', () => {
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

  it('renders dual power rails: left hot rail (+24V HOT) and right neutral rail (0V RET)', () => {
    const html = renderClient(<LadderCanvas targetBlock="OB1_Process" />);
    expect(html).toContain('+24V');
    expect(html).toContain('HOT');
    expect(html).toContain('0V');
    expect(html).toContain('RET');
    expect(html).toContain('Target: OB1_Process');
  });

  it('renders default rungs with comments, contacts, and coils', () => {
    const html = renderClient(<LadderCanvas />);
    expect(html).toContain('RUNG 1');
    expect(html).toContain('RUNG 2');
    expect(html).toContain('RUNG 3');
    expect(html).toContain('Feed Conveyor Run permissive interlock');
    expect(html).toContain('Bottle Detect');
    expect(html).toContain('external.DI_StartPB');
    expect(html).toContain('external.DI_EStop_PB');
    expect(html).toContain('external.DO_ConveyorRun');
  });

  it('adds a new rung when "+ Add Rung" button is clicked', () => {
    const onAddRung = vi.fn();
    renderClient(<LadderCanvas onAddRung={onAddRung} />);

    const addBtn = container.querySelector('[data-testid="add-rung-button"]') as HTMLButtonElement;
    expect(addBtn).toBeTruthy();

    flushSync(() => {
      addBtn.click();
    });

    expect(onAddRung).toHaveBeenCalledTimes(1);
    expect(container.innerHTML).toContain('RUNG 4');
  });

  it('deletes a rung when delete rung button is clicked', () => {
    const onDeleteRung = vi.fn();
    renderClient(<LadderCanvas onDeleteRung={onDeleteRung} />);

    const firstRungDeleteBtn = container.querySelector('[data-testid="rung-container-rung-1"] button[title="Delete Rung"]') as HTMLButtonElement;
    expect(firstRungDeleteBtn).toBeTruthy();

    flushSync(() => {
      firstRungDeleteBtn.click();
    });

    expect(onDeleteRung).toHaveBeenCalledWith('rung-1');
  });

  it('animates power rails and rungs in vibrant green (#22C55E) when debugActive and energized', () => {
    const html = renderClient(
      <LadderCanvas
        debugActive={true}
        liveTelemetry={{
          'external.DI_StartPB': true,
          'external.DI_EStop_PB': false,
          'external.DO_ConveyorRun': true,
        }}
      />
    );
    expect(html).toContain('#22C55E');
  });
});
