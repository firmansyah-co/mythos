import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { DataTypeEditor, DEFAULT_SAMPLE_DATA_TYPES } from '../variables/DataTypeEditor';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('TSK-704: DataTypeEditor Suite (DUT / UDT Structs & Enums)', () => {
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

  it('renders defined struct and enum types', () => {
    const html = renderClient(<DataTypeEditor />);
    expect(html).toContain('DUT_BottleSpec');
    expect(html).toContain('DUT_MachineState');
    expect(html).toContain('STRUCT');
    expect(html).toContain('ENUM');
  });

  it('renders struct members and generated IEC 61131-3 Structured Text representation', () => {
    const html = renderClient(<DataTypeEditor />);
    expect(html).toContain('Height_mm');
    expect(html).toContain('Diameter_mm');
    expect(html).toContain('Volume_ml');
    expect(html).toContain('IsGlass');
    expect(html).toContain('TYPE DUT_BottleSpec :');
    expect(html).toContain('END_STRUCT');
    expect(html).toContain('END_TYPE');
  });

  it('allows creating a new STRUCT or ENUM definition', () => {
    const onChange = vi.fn();
    renderClient(<DataTypeEditor onChange={onChange} />);

    const buttons = Array.from(container.querySelectorAll('button'));
    const addStructBtn = buttons.find((b) => b.textContent?.includes('+ New STRUCT'));
    expect(addStructBtn).toBeTruthy();

    flushSync(() => {
      addStructBtn?.click();
    });

    expect(onChange).toHaveBeenCalled();
    expect(container.innerHTML).toContain('DUT_Struct_');
  });
});
