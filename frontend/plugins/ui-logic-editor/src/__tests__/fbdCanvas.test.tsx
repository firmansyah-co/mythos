import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { FbdCanvas } from '../fbd/FbdCanvas';
import { FBD_FIXTURE_BLOCKS, FBD_FIXTURE_WIRES } from './fixtures/logicFixtures';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('TSK-703: Function Block Diagram (FBD) Canvas Suite', () => {
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

  it('renders FBD Canvas header, controls, and default logic blocks', () => {
    const html = renderClient(<FbdCanvas blocks={FBD_FIXTURE_BLOCKS} wires={FBD_FIXTURE_WIRES} />);
    expect(html).toContain('IEC 61131-3 Function Block Diagram (FBD) Canvas');
    expect(html).toContain('AND_1');
    expect(html).toContain('TON_1');
    expect(html).toContain('PID_Pressure');
  });

  it('renders typed input and output pins with inversion negation support', () => {
    const html = renderClient(<FbdCanvas blocks={FBD_FIXTURE_BLOCKS} wires={FBD_FIXTURE_WIRES} />);
    expect(html).toContain('IN1');
    expect(html).toContain('IN2');
    expect(html).toContain('OUT');
    expect(html).toContain('BOOL');
    expect(html).toContain('TIME');
    expect(html).toContain('REAL');
  });

  it('renders SVG connection path between output and input pins', () => {
    renderClient(<FbdCanvas blocks={FBD_FIXTURE_BLOCKS} wires={FBD_FIXTURE_WIRES} />);
    const paths = container.querySelectorAll('svg path');
    expect(paths.length).toBeGreaterThan(0);
  });

  it('allows adding a new logic gate block via top palette button', () => {
    const onAddBlock = vi.fn();
    renderClient(<FbdCanvas blocks={FBD_FIXTURE_BLOCKS} wires={FBD_FIXTURE_WIRES} onAddBlock={onAddBlock} />);

    const buttons = Array.from(container.querySelectorAll('button'));
    const addOrBtn = buttons.find((b) => b.textContent?.includes('+ OR'));
    expect(addOrBtn).toBeTruthy();

    flushSync(() => {
      addOrBtn?.click();
    });

    expect(onAddBlock).toHaveBeenCalled();
    expect(container.innerHTML).toContain('OR_');
  });
});
