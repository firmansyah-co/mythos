import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { VariableTableGrid } from '../variables/VariableTableGrid';
import { VARIABLE_FIXTURE_ROWS } from './fixtures/logicFixtures';
import { VariableRow } from '../variables/types';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('TSK-704: Variable Table Grid Suite (POU Scopes & IEC Addressing)', () => {
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

  it('renders spreadsheet-style grid with POU scopes (VAR, VAR_INPUT, VAR_OUTPUT) and types', () => {
    const html = renderClient(<VariableTableGrid variables={VARIABLE_FIXTURE_ROWS} />);
    expect(html).toContain('DI_StartPB');
    expect(html).toContain('DI_EStop_PB');
    expect(html).toContain('DO_ConveyorRun');
    expect(html).toContain('Bottle_Count');
    expect(html).toContain('Conveyor_Velocity_Setpoint');
  });

  it('displays IEC memory addresses (%IX, %QX, %MW)', () => {
    const html = renderClient(<VariableTableGrid variables={VARIABLE_FIXTURE_ROWS} />);
    expect(html).toContain('%IX0.0');
    expect(html).toContain('%IX0.1');
    expect(html).toContain('%QX0.0');
    expect(html).toContain('%MW100');
  });

  it('filters rows by variable name or address', () => {
    renderClient(<VariableTableGrid variables={VARIABLE_FIXTURE_ROWS} />);
    const searchInput = container.querySelector('input[placeholder*="Filter"]') as HTMLInputElement;
    expect(searchInput).toBeTruthy();

    const proto = Object.getPrototypeOf(searchInput);
    const set = Object.getOwnPropertyDescriptor(proto, 'value')?.set;
    flushSync(() => {
      set?.call(searchInput, 'EStop');
      searchInput.dispatchEvent(new Event('input', { bubbles: true }));
      searchInput.dispatchEvent(new Event('change', { bubbles: true }));
    });

    expect(container.innerHTML).toContain('DI_EStop_PB');
    expect(container.innerHTML).not.toContain('Conveyor_Velocity_Setpoint');
  });

  it('adds a new variable row with default parameters when "+ Add Variable" button is clicked', () => {
    let capturedVars: VariableRow[] = [];
    renderClient(
      <VariableTableGrid
        variables={VARIABLE_FIXTURE_ROWS}
        onChange={(v) => {
          capturedVars = v;
        }}
      />
    );

    const addBtn = container.querySelector('[data-testid="add-variable-btn"]') as HTMLButtonElement;
    expect(addBtn).toBeTruthy();

    flushSync(() => {
      addBtn.click();
    });

    expect(capturedVars.length).toBe(VARIABLE_FIXTURE_ROWS.length + 1);
    expect(capturedVars[capturedVars.length - 1].scope).toBe('VAR');
    expect(capturedVars[capturedVars.length - 1].dataType).toBe('BOOL');
  });
});
